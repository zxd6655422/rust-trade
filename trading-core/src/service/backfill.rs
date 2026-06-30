use chrono::{DateTime, Utc};
use std::sync::Arc;
use std::time::Duration;
use tracing::{error, info, warn};

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
    async fn backfill_symbol(&self, symbol: &str) -> Result<(), Box<dyn std::error::Error>> {
        info!("📊 Backfilling {} from {}", symbol, self.start_date);

        // 1. 查询数据库中已有的最早/最新时间
        let earliest = self.repository.get_kline_earliest(symbol).await?;
        let latest = self.repository.get_kline_latest(symbol).await?;

        match (earliest, latest) {
            (Some(earliest_ts), Some(latest_ts)) => {
                info!(
                    "  DB has data: {} ~ {}",
                    earliest_ts.format("%Y-%m-%d %H:%M"),
                    latest_ts.format("%Y-%m-%d %H:%M")
                );

                // 拉取 start_date → earliest 之间的历史数据
                if self.start_date < earliest_ts {
                    info!(
                        "  Backfilling history: {} → {}",
                        self.start_date.format("%Y-%m-%d %H:%M"),
                        earliest_ts.format("%Y-%m-%d %H:%M")
                    );
                    self.fetch_range(symbol, self.start_date, earliest_ts).await?;
                }

                // 检测并补齐缺失
                info!("  Checking for gaps...");
                self.fill_gaps(symbol, self.start_date, latest_ts).await?;
            }
            (None, None) => {
                // 数据库无数据，从 start_date 拉到现在
                let now = Utc::now();
                info!(
                    "  No existing data, fetching: {} → {}",
                    self.start_date.format("%Y-%m-%d %H:%M"),
                    now.format("%Y-%m-%d %H:%M")
                );
                self.fetch_range(symbol, self.start_date, now).await?;
            }
            _ => {
                // 不可能的状态（有 earliest 没 latest 或反之），忽略
                warn!("  Inconsistent kline data state for {}, skipping", symbol);
            }
        }

        info!("✅ Backfill completed for {}", symbol);
        Ok(())
    }

    /// 分页拉取指定时间范围的 kline 数据并写入数据库
    async fn fetch_range(
        &self,
        symbol: &str,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let mut cursor = start;
        let mut total_fetched: u64 = 0;

        while cursor < end {
            // 每次拉取 1000 根（Binance 最大限制）
            match self
                .exchange
                .fetch_klines_with_time(symbol, "1m", cursor, end, 1000)
                .await
            {
                Ok(klines) => {
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
                            if total_fetched % 10000 < 1000 {
                                info!(
                                    "  Progress: {} klines fetched, cursor at {}",
                                    total_fetched,
                                    cursor.format("%Y-%m-%d %H:%M")
                                );
                            }
                        }
                        Err(e) => {
                            error!("  Failed to insert klines: {}", e);
                        }
                    }

                    // 如果返回不足 1000 条，说明已经到头了
                    if count < 1000 {
                        break;
                    }

                    // 移动 cursor 到最后一条之后（+1 分钟）
                    cursor = last_ts + chrono::Duration::minutes(1);

                    // 限速：每次请求后 sleep 100ms
                    tokio::time::sleep(Duration::from_millis(100)).await;
                }
                Err(e) => {
                    error!("  fetch_klines_with_time failed: {}", e);
                    // 出错后等久一点再重试
                    tokio::time::sleep(Duration::from_secs(5)).await;
                    // 不 break，重试同一段
                }
            }
        }

        info!("  Fetched total {} klines for {}", total_fetched, symbol);
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
            info!("  No gaps found for {}", symbol);
            return Ok(());
        }

        info!("  Found {} gaps for {}, filling...", gaps.len(), symbol);

        for (gap_start, gap_end) in &gaps {
            info!(
                "    Filling gap: {} → {}",
                gap_start.format("%Y-%m-%d %H:%M"),
                gap_end.format("%Y-%m-%d %H:%M")
            );
            self.fetch_range(symbol, *gap_start, *gap_end).await?;
        }

        info!("  Gaps filled for {}", symbol);
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
