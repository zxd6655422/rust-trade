use chrono::{DateTime, Utc};
use std::sync::Arc;
use std::time::Duration;
use tracing::{debug, error, info, warn};

use crate::exchange::{Exchange, KlineData};
use trading_common::data::types::{OHLCData, Timeframe};
use trading_common::data::repository::TickDataRepository;

/// 历史数据回填服务
pub struct BackfillService {
    exchange: Arc<dyn Exchange>,
    repository: Arc<TickDataRepository>,
    symbols: Vec<String>,
    start_date: DateTime<Utc>,
}

impl BackfillService {
    pub fn new(
        exchange: Arc<dyn Exchange>,
        repository: Arc<TickDataRepository>,
        symbols: Vec<String>,
        start_date: DateTime<Utc>,
    ) -> Self {
        Self {
            exchange,
            repository,
            symbols,
            start_date,
        }
    }

    /// 执行回填：拉取历史数据 + 补齐缺失
    pub async fn run(&self) {
        info!("🔄 Starting backfill from {}", self.start_date);

        for symbol in &self.symbols {
            if let Err(e) = self.backfill_symbol(symbol).await {
                error!("Backfill failed for {}: {}", symbol, e);
            }
        }

        info!("✅ Backfill completed");
    }

    /// 对单个 symbol 执行回填
    /// 优化: 增量更新，只拉取新数据
    async fn backfill_symbol(&self, symbol: &str) -> Result<(), Box<dyn std::error::Error>> {
        // 1. 查询数据库中已有的最早/最新时间
        let earliest = self.repository.get_kline_earliest(symbol).await?;
        let latest = self.repository.get_kline_latest(symbol).await?;

        match (earliest, latest) {
            (Some(earliest_ts), Some(latest_ts)) => {
                debug!(
                    "[{}] 已有数据: {} ~ {}",
                    symbol,
                    earliest_ts.format("%Y-%m-%d %H:%M"),
                    latest_ts.format("%Y-%m-%d %H:%M")
                );

                // 优化: 只在首次时拉取历史，后续只补齐间隙
                let now = Utc::now();
                let time_since_latest = now.signed_duration_since(latest_ts);

                // 如果最新数据超过 1 小时，只拉取最近的数据
                if time_since_latest.num_hours() > 1 {
                    debug!(
                        "[{}] 数据已过期 {} 小时，拉取最近数据",
                        symbol,
                        time_since_latest.num_hours()
                    );
                    self.fetch_range(symbol, latest_ts, now).await?;
                } else {
                    debug!("[{}] 数据较新 ({} 分钟前)，跳过", symbol, time_since_latest.num_minutes());
                }

                // 只检查最近 7 天的间隙（而不是全部历史）
                let gap_check_start = now - chrono::Duration::days(7);
                let check_start = if gap_check_start > latest_ts {
                    gap_check_start
                } else {
                    self.start_date
                };

                debug!("[{}] 检查最近 7 天数据间隙...", symbol);
                self.fill_gaps(symbol, check_start, latest_ts).await?;
            }
            (None, None) => {
                // 数据库无数据，从 start_date 拉到现在
                let now = Utc::now();
                info!(
                    "[{}] 首次拉取，从 {} 开始",
                    symbol,
                    self.start_date.format("%Y-%m-%d")
                );
                self.fetch_range(symbol, self.start_date, now).await?;
            }
            _ => {
                warn!("[{}] 数据状态不一致，跳过", symbol);
            }
        }

        Ok(())
    }

    /// 分页拉取指定时间范围的 kline 数据并写入数据库
    /// 优化: 使用更大的 batch size 和更短的限速
    async fn fetch_range(
        &self,
        symbol: &str,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        debug!(
            "[{}] 拉取 kline 数据 [{} → {}]",
            symbol,
            start.format("%Y-%m-%d %H:%M"),
            end.format("%Y-%m-%d %H:%M")
        );

        let mut cursor = start;
        let mut total_fetched: u64 = 0;
        let mut consecutive_errors = 0;
        const MAX_CONSECUTIVE_ERRORS: u32 = 5;

        // Binance 实际支持 1500，但用 1000 更稳定
        const BATCH_SIZE: u32 = 1000;
        // API 限制: Binance 20 req/s, OKX 12 req/s
        // 安全配置: 200ms per request = 5 req/s per symbol
        // 多 symbol 并发时总速率 = 5 * N，留 4x 安全边际
        const RATE_LIMIT_MS: u64 = 200;

        while cursor < end {
            match self
                .exchange
                .fetch_klines_with_time(symbol, "1m", cursor, end, BATCH_SIZE)
                .await
            {
                Ok(klines) => {
                    consecutive_errors = 0; // 重置错误计数

                    if klines.is_empty() {
                        info!("  No more data available at {}", cursor.format("%Y-%m-%d %H:%M"));
                        break;
                    }

                    let count = klines.len();
                    let last_ts = klines.last().unwrap().timestamp;

                    // 转换为 OHLCData 并写入
                    let ohlc_list = klines_to_ohlc(klines);
                    match self.repository.batch_insert_klines(ohlc_list).await {
                        Ok(inserted) => {
                            total_fetched += inserted as u64;
                        }
                        Err(e) => {
                            error!("[{}] kline 插入失败: {}", symbol, e);
                        }
                    }

                    // 如果返回不足 BATCH_SIZE 条，说明已经到头了
                    if count < BATCH_SIZE as usize {
                        break;
                    }

                    // 移动 cursor 到最后一条之后（+1 分钟）
                    cursor = last_ts + chrono::Duration::minutes(1);

                    // 限速
                    tokio::time::sleep(Duration::from_millis(RATE_LIMIT_MS)).await;
                }
                Err(e) => {
                    consecutive_errors += 1;
                    error!("  [{}] fetch_klines_with_time failed ({}/{}): {}", symbol, consecutive_errors, MAX_CONSECUTIVE_ERRORS, e);

                    if consecutive_errors >= MAX_CONSECUTIVE_ERRORS {
                        error!("  [{}] Too many consecutive errors, aborting backfill", symbol);
                        return Err(format!("Max consecutive errors reached for {}", symbol).into());
                    }

                    // 指数退避: 1s, 2s, 4s, 8s, 16s
                    let backoff = 2u64.pow(consecutive_errors - 1);
                    tokio::time::sleep(Duration::from_secs(backoff)).await;
                }
            }
        }

        if total_fetched > 0 {
            info!("✅ [{}] 完成，共拉取 {} 条 kline", symbol, total_fetched);
        }
        Ok(())
    }

    /// 检测并补齐缺失的 kline 数据
    async fn fill_gaps(
        &self,
        symbol: &str,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let gaps = self.repository.find_kline_gaps(symbol, start, end).await?;

        if gaps.is_empty() {
            debug!("[{}] 无数据间隙", symbol);
            return Ok(());
        }

        info!("[{}] 发现 {} 个数据间隙，正在补齐...", symbol, gaps.len());

        for (gap_start, gap_end) in &gaps {
            debug!(
                "[{}] 补齐间隙: {} → {}",
                symbol,
                gap_start.format("%Y-%m-%d %H:%M"),
                gap_end.format("%Y-%m-%d %H:%M")
            );
            self.fetch_range(symbol, *gap_start, *gap_end).await?;
        }

        debug!("[{}] 数据间隙补齐完成", symbol);
        Ok(())
    }
}

/// 将 KlineData 转换为 OHLCData
fn klines_to_ohlc(klines: Vec<KlineData>) -> Vec<OHLCData> {
    klines
        .into_iter()
        .map(|k| OHLCData {
            timestamp: k.timestamp,
            symbol: k.symbol,
            timeframe: Timeframe::OneMinute,
            open: k.open,
            high: k.high,
            low: k.low,
            close: k.close,
            volume: k.volume,
            trade_count: k.trade_count,
        })
        .collect()
}
