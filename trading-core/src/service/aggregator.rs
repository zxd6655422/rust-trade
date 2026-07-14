use std::sync::Arc;
use chrono::{DateTime, Duration, Utc};
use redis::aio::ConnectionManager;
use tracing::{debug, info, warn};

use trading_common::data::repository::TickDataRepository;
use trading_common::data::types::{DataError, DataResult, Timeframe};
use crate::redis_writer;

/// 高时间框架聚合器
///
/// 从 kline_1m 表聚合生成 5m/15m/30m/1h/2h/4h/1d/3d/1w K线数据。
/// 替代从交易所 API 单独拉取高TF数据的方式，实现：
/// - 实时性：1m 数据写入后立即可聚合
/// - 一致性：所有时间框架数据源自同一份 1m 数据
/// - 效率：无需额外 API 调用
pub struct HighTfAggregator {
    repo: Arc<TickDataRepository>,
    redis_conn: ConnectionManager,
}

/// 聚合结果
#[derive(Debug)]
pub struct AggregationResult {
    pub timeframe: String,
    pub rows_affected: i64,
}

impl HighTfAggregator {
    pub fn new(repo: Arc<TickDataRepository>, redis_conn: ConnectionManager) -> Self {
        Self { repo, redis_conn }
    }

    /// 增量聚合：只聚合最近N分钟的1m数据
    ///
    /// 在每轮轮询后调用，开销极小（只查最新几行1m数据）
    pub async fn aggregate_incremental(
        &mut self,
        symbol: &str,
        lookback_minutes: i64,
    ) -> DataResult<Vec<AggregationResult>> {
        let end = Utc::now();
        let start = end - Duration::minutes(lookback_minutes);

        self.aggregate_range(symbol, start, end).await
    }

    /// 聚合指定时间范围的1m数据到所有高TF
    ///
    /// 用于：
    /// - 增量更新（lookback = 2-10分钟）
    /// - 恢复追赶（lookback = 数小时/数天）
    /// - 间隙填补
    pub async fn aggregate_range(
        &mut self,
        symbol: &str,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> DataResult<Vec<AggregationResult>> {
        // 使用 PostgreSQL 函数一次性聚合所有时间框架
        let raw_results = self.repo.aggregate_all_timeframes(symbol, start, end).await?;

        let results: Vec<AggregationResult> = raw_results.into_iter()
            .map(|(tf, count)| AggregationResult { timeframe: tf, rows_affected: count })
            .collect();

        // 聚合完成后，将有更新的时间框架同步到 Redis
        for result in &results {
            if result.rows_affected > 0 {
                if let Err(e) = self.sync_to_redis(symbol, &result.timeframe).await {
                    warn!("[{}] Redis 同步失败 ({}): {}", symbol, result.timeframe, e);
                }
            }
        }

        Ok(results)
    }

    /// 将单个时间框架的最新数据从 PG 同步到 Redis
    async fn sync_to_redis(
        &mut self,
        symbol: &str,
        timeframe_str: &str,
    ) -> DataResult<()> {
        let tf = Timeframe::from_str(timeframe_str)
            .ok_or_else(|| DataError::Validation(format!("Invalid timeframe: {}", timeframe_str)))?;

        // 读取最新的 N 条数据
        let cache_size = redis_writer::get_cache_size(&tf) as u32;
        let klines = self.repo.get_high_tf_klines(symbol, timeframe_str, cache_size).await?;

        if klines.is_empty() {
            return Ok(());
        }

        // 写入 Redis
        redis_writer::load_cache_from_db(&mut self.redis_conn, symbol, &tf, &klines).await
            .map_err(|e| DataError::Cache(format!("Redis write failed: {}", e)))?;

        debug!("[{}] Redis 同步完成 {}: {} 条", symbol, timeframe_str, klines.len());
        Ok(())
    }

    /// 检测并填补数据间隙
    ///
    /// 检查两种间隙：
    /// 1. 末尾间隙：最新数据到当前时间之间的缺失
    /// 2. 内部间隙：相邻K线之间的缺失（中间断档）
    pub async fn detect_and_fill_gaps(
        &mut self,
        symbol: &str,
    ) -> DataResult<Vec<AggregationResult>> {
        let now = Utc::now();
        let mut all_results = Vec::new();

        // 检查每个时间框架
        let timeframes = vec![
            ("5m", 5i64), ("15m", 15), ("30m", 30),
            ("1h", 60), ("2h", 120), ("4h", 240),
            ("1d", 1440), ("3d", 4320), ("1w", 10080),
        ];

        for (tf_str, tf_minutes) in timeframes {
            // === 检查1：末尾间隙（最新数据到当前时间）===
            if let Some(latest) = self.repo.get_high_tf_latest(symbol, tf_str).await? {
                let gap = now.signed_duration_since(latest);

                if gap.num_minutes() > tf_minutes * 2 {
                    info!(
                        "[{}] {} 末尾间隙: {} 分钟 (最新: {})",
                        symbol, tf_str, gap.num_minutes(),
                        latest.format("%Y-%m-%d %H:%M")
                    );
                    let results = self.aggregate_range(symbol, latest, now).await?;
                    all_results.extend(results);
                }
            }

            // === 检查2：内部间隙（相邻K线之间的断档）===
            // 只检查最近24小时的数据，避免历史数据中的正常间隙被误报
            let internal_gaps = self.repo.detect_high_tf_gaps(
                symbol, tf_str, tf_minutes, 24,
            ).await?;

            for (gap_start, gap_end) in internal_gaps {
                info!(
                    "[{}] {} 内部间隙: {} ~ {} ({} 分钟)",
                    symbol, tf_str,
                    gap_start.format("%H:%M"),
                    gap_end.format("%H:%M"),
                    gap_end.signed_duration_since(gap_start).num_minutes()
                );
                let results = self.aggregate_range(symbol, gap_start, gap_end).await?;
                all_results.extend(results);
            }
        }

        Ok(all_results)
    }

    /// 服务停止后的恢复聚合
    ///
    /// 检测自上次聚合以来经过了多长时间，一次性补齐所有缺失数据
    pub async fn recover_after_downtime(
        &mut self,
        symbol: &str,
        downtime_threshold_minutes: i64,
    ) -> DataResult<bool> {
        let now = Utc::now();

        // 检查 5m 数据的最新时间戳（最小的时间框架，最能反映数据新鲜度）
        let latest_5m = self.repo.get_high_tf_latest(symbol, "5m").await?;

        match latest_5m {
            Some(latest) => {
                let downtime = now.signed_duration_since(latest);

                if downtime.num_minutes() > downtime_threshold_minutes {
                    info!(
                        "[{}] 检测到停机 {} 分钟，开始恢复聚合 ({} → {})",
                        symbol,
                        downtime.num_minutes(),
                        latest.format("%Y-%m-%d %H:%M"),
                        now.format("%Y-%m-%d %H:%M")
                    );

                    // 分批聚合，避免单次查询范围过大
                    // 每次最多聚合 1 天的数据
                    let mut cursor = latest;
                    let batch_size = Duration::days(1);

                    while cursor < now {
                        let batch_end = std::cmp::min(cursor + batch_size, now);
                        let results = self.aggregate_range(symbol, cursor, batch_end).await?;

                        for r in &results {
                            if r.rows_affected > 0 {
                                debug!(
                                    "[{}] 恢复聚合 {} ~ {}: {} = {} 行",
                                    symbol,
                                    cursor.format("%m-%d %H:%M"),
                                    batch_end.format("%m-%d %H:%M"),
                                    r.timeframe,
                                    r.rows_affected
                                );
                            }
                        }

                        cursor = batch_end;
                    }

                    info!("[{}] 恢复聚合完成", symbol);
                    return Ok(true);
                }

                Ok(false) // 无需恢复
            }
            None => {
                // 没有数据，需要首次聚合
                info!("[{}] 无高TF数据，执行首次聚合", symbol);
                let lookback = Duration::days(30); // 默认回填30天
                let start = now - lookback;
                self.aggregate_range(symbol, start, now).await?;
                Ok(true)
            }
        }
    }
}
