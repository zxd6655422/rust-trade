use chrono::{DateTime, Utc};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use tracing::{debug, error, info, warn};

use crate::exchange::{Exchange, KlineData};
use trading_common::data::types::{OHLCData, Timeframe};

// =================================================================
// Constants
// =================================================================

/// API 限制：Binance 20 req/s, OKX 12 req/s
/// 安全配置：300ms per request = 3.3 req/s
const RATE_LIMIT_MS: u64 = 300;

/// 每批拉取的 K 线数量
const BATCH_SIZE: u32 = 1000;

/// 连续错误最大次数
const MAX_CONSECUTIVE_ERRORS: u32 = 5;

/// 需要存储到数据库的时间框架
const STORED_TIMEFRAMES: &[&str] = &["1m", "5m", "15m", "30m", "1h", "2h", "4h", "1d", "3d", "1w"];

// =================================================================
// Backfill Service
// =================================================================

/// 历史数据回填服务
pub struct BackfillService {
    exchange: Arc<dyn Exchange>,
    repository: Arc<crate::data::repository::TickDataRepository>,
    redis_url: String,
    symbols: Vec<String>,
    start_date: DateTime<Utc>,
}

/// 回填配置
#[derive(Debug, Clone)]
pub struct BackfillConfig {
    pub symbols: Vec<String>,
    pub start_date: DateTime<Utc>,
    pub timeframes: Vec<String>,
    pub incremental: bool,
}

impl BackfillService {
    pub fn new(
        exchange: Arc<dyn Exchange>,
        repository: Arc<crate::data::repository::TickDataRepository>,
        redis_url: String,
        symbols: Vec<String>,
        start_date: DateTime<Utc>,
    ) -> Self {
        Self {
            exchange,
            repository,
            redis_url,
            symbols,
            start_date,
        }
    }

    /// 执行单时间框架回填（1m）
    pub async fn run(&self) {
        info!("🔄 Starting 1m backfill from {}", self.start_date);

        for symbol in &self.symbols {
            if let Err(e) = self.backfill_symbol(symbol).await {
                error!("Backfill failed for {}: {}", symbol, e);
            }
        }

        info!("✅ 1m backfill completed");
    }

    /// 执行多时间框架回填
    pub async fn run_multi_tf(&self, config: &BackfillConfig) {
        info!("🔄 Starting multi-timeframe backfill");
        info!("  Symbols: {:?}", config.symbols);
        info!("  Timeframes: {:?}", config.timeframes);
        info!("  Start date: {}", config.start_date.format("%Y-%m-%d"));

        for symbol in &config.symbols {
            for tf in &config.timeframes {
                info!("[{}] Backfilling {} timeframe...", symbol, tf);

                if tf == "1m" {
                    // 1m 数据使用现有逻辑
                    if let Err(e) = self.backfill_symbol(symbol).await {
                        error!("[{}] 1m backfill failed: {}", symbol, e);
                    }
                } else {
                    // 高时间框架数据
                    if let Err(e) = self.backfill_high_tf(symbol, tf, config).await {
                        error!("[{}] {} backfill failed: {}", symbol, tf, e);
                    }
                }

                // 时间框架之间的间隔
                tokio::time::sleep(Duration::from_secs(1)).await;
            }

            // 交易对之间的间隔
            tokio::time::sleep(Duration::from_secs(5)).await;
        }

        info!("✅ Multi-timeframe backfill completed");
    }

    /// 回填单个 symbol 的 1m 数据
    async fn backfill_symbol(&self, symbol: &str) -> Result<(), Box<dyn std::error::Error>> {
        let earliest = self.repository.get_kline_earliest(symbol).await?;
        let latest = self.repository.get_kline_latest(symbol).await?;

        match (earliest, latest) {
            (Some(earliest_ts), Some(latest_ts)) => {
                debug!(
                    "[{}] 已有 1m 数据: {} ~ {}",
                    symbol,
                    earliest_ts.format("%Y-%m-%d %H:%M"),
                    latest_ts.format("%Y-%m-%d %H:%M")
                );

                // 增量更新
                let now = Utc::now();
                let time_since_latest = now.signed_duration_since(latest_ts);

                if time_since_latest.num_hours() > 1 {
                    debug!("[{}] 数据已过期 {} 小时，拉取最新数据", symbol, time_since_latest.num_hours());
                    self.fetch_range(symbol, "1m", latest_ts, now).await?;
                }

                // 检查最近 7 天的间隙
                let gap_check_start = now - chrono::Duration::days(7);
                let check_start = if gap_check_start > latest_ts {
                    gap_check_start
                } else {
                    self.start_date
                };

                self.fill_gaps(symbol, check_start, latest_ts).await?;
            }
            (None, None) => {
                // 首次拉取
                let now = Utc::now();
                info!("[{}] 首次拉取 1m 数据，从 {} 开始", symbol, self.start_date.format("%Y-%m-%d"));
                self.fetch_range(symbol, "1m", self.start_date, now).await?;
            }
            _ => {
                warn!("[{}] 数据状态不一致，跳过", symbol);
            }
        }

        Ok(())
    }

    /// 回填高时间框架数据
    async fn backfill_high_tf(
        &self,
        symbol: &str,
        timeframe: &str,
        config: &BackfillConfig,
    ) -> Result<(), Box<dyn std::error::Error>> {
        // 查询数据库中该时间框架的最新数据
        let latest = self.repository
            .get_high_tf_latest(symbol, timeframe)
            .await?;

        let start = match (config.incremental, latest) {
            (true, Some(ts)) => {
                debug!("[{}] {} 已有数据到 {}", symbol, timeframe, ts.format("%Y-%m-%d"));
                ts
            }
            _ => config.start_date,
        };

        let end = Utc::now();

        // 拉取数据
        self.fetch_range(symbol, timeframe, start, end).await?;

        Ok(())
    }

    /// 分页拉取指定时间范围的 kline 数据并写入数据库
    async fn fetch_range(
        &self,
        symbol: &str,
        timeframe: &str,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> Result<(), Box<dyn std::error::Error>> {
        debug!(
            "[{}] 拉取 {} kline 数据 [{} → {}]",
            symbol,
            timeframe,
            start.format("%Y-%m-%d %H:%M"),
            end.format("%Y-%m-%d %H:%M")
        );

        let mut cursor = start;
        let mut total_fetched: u64 = 0;
        let mut consecutive_errors = 0;

        while cursor < end {
            match self.exchange
                .fetch_klines_with_time(symbol, timeframe, cursor, end, BATCH_SIZE)
                .await
            {
                Ok(klines) => {
                    consecutive_errors = 0;

                    if klines.is_empty() {
                        debug!("  No more {} data available at {}", timeframe, cursor.format("%Y-%m-%d %H:%M"));
                        break;
                    }

                    let count = klines.len();
                    let last_ts = match klines.last() {
                        Some(k) => k.timestamp,
                        None => continue,
                    };

                    // 转换并写入数据库
                    let ohlc_list = klines_to_ohlc(klines, timeframe);

                    if timeframe == "1m" {
                        // 1m 数据写入 kline_1m 表
                        match self.repository.batch_insert_klines(ohlc_list).await {
                            Ok(inserted) => {
                                total_fetched += inserted as u64;
                            }
                            Err(e) => {
                                error!("[{}] 1m kline 插入失败: {}", symbol, e);
                            }
                        }
                    } else {
                        // 高时间框架数据写入对应表
                        match self.repository.batch_insert_high_tf_klines_by_str(&ohlc_list, timeframe).await {
                            Ok(inserted) => {
                                total_fetched += inserted as u64;
                            }
                            Err(e) => {
                                error!("[{}] {} kline 插入失败: {}", symbol, timeframe, e);
                            }
                        }
                    }

                    // 如果返回不足 BATCH_SIZE 条，说明已经到头了
                    if count < BATCH_SIZE as usize {
                        break;
                    }

                    // 移动 cursor
                    cursor = last_ts + get_timeframe_duration(timeframe);

                    // 限速
                    tokio::time::sleep(Duration::from_millis(RATE_LIMIT_MS)).await;
                }
                Err(e) => {
                    consecutive_errors += 1;
                    error!(
                        "  [{}] fetch_klines_with_time failed ({}/{}): {}",
                        symbol, consecutive_errors, MAX_CONSECUTIVE_ERRORS, e
                    );

                    if consecutive_errors >= MAX_CONSECUTIVE_ERRORS {
                        error!("  [{}] Too many consecutive errors, aborting", symbol);
                        return Err(format!("Max consecutive errors reached for {}", symbol).into());
                    }

                    // 指数退避
                    let backoff = 2u64.pow(consecutive_errors - 1);
                    tokio::time::sleep(Duration::from_secs(backoff)).await;
                }
            }
        }

        if total_fetched > 0 {
            info!("✅ [{}] {} 完成，共拉取 {} 条 kline", symbol, timeframe, total_fetched);
        }
        Ok(())
    }

    /// 检测并补齐缺失的 1m 数据
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
            self.fetch_range(symbol, "1m", *gap_start, *gap_end).await?;
        }

        debug!("[{}] 数据间隙补齐完成", symbol);
        Ok(())
    }

    /// 刷新 Redis 缓存（从数据库加载）
    ///
    /// 注意：此方法需要在 main.rs 中调用，因为 redis_writer 在 crate 级别
    pub async fn get_klines_for_cache_refresh(
        &self,
        symbol: &str,
        timeframe: &str,
        cache_size: u32,
    ) -> Result<Vec<OHLCData>, Box<dyn std::error::Error>> {
        let klines = if timeframe == "1m" {
            self.repository.get_klines(symbol, cache_size).await?
        } else {
            self.repository.get_high_tf_klines(symbol, timeframe, cache_size).await?
        };
        Ok(klines)
    }

    /// 获取需要刷新缓存的时间框架列表
    pub fn get_stored_timeframes(&self) -> &[&str] {
        STORED_TIMEFRAMES
    }
}

/// 将 KlineData 转换为 OHLCData
fn klines_to_ohlc(klines: Vec<KlineData>, timeframe: &str) -> Vec<OHLCData> {
    let tf = Timeframe::from_str(timeframe).unwrap_or(Timeframe::OneMinute);

    klines
        .into_iter()
        .map(|k| OHLCData {
            timestamp: k.timestamp,
            symbol: k.symbol,
            timeframe: tf,
            open: k.open,
            high: k.high,
            low: k.low,
            close: k.close,
            volume: k.volume,
            trade_count: k.trade_count,
        })
        .collect()
}

/// 获取时间框架对应的 Duration
fn get_timeframe_duration(tf: &str) -> chrono::Duration {
    match tf {
        "1m" => chrono::Duration::minutes(1),
        "3m" => chrono::Duration::minutes(3),
        "5m" => chrono::Duration::minutes(5),
        "15m" => chrono::Duration::minutes(15),
        "30m" => chrono::Duration::minutes(30),
        "45m" => chrono::Duration::minutes(45),
        "1h" => chrono::Duration::hours(1),
        "2h" => chrono::Duration::hours(2),
        "4h" => chrono::Duration::hours(4),
        "6h" => chrono::Duration::hours(6),
        "8h" => chrono::Duration::hours(8),
        "12h" => chrono::Duration::hours(12),
        "1d" => chrono::Duration::days(1),
        "3d" => chrono::Duration::days(3),
        "1w" => chrono::Duration::weeks(1),
        _ => chrono::Duration::minutes(1),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_timeframe_duration() {
        assert_eq!(get_timeframe_duration("1m"), chrono::Duration::minutes(1));
        assert_eq!(get_timeframe_duration("4h"), chrono::Duration::hours(4));
        assert_eq!(get_timeframe_duration("1d"), chrono::Duration::days(1));
    }
}
