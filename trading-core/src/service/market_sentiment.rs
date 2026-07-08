// service/market_sentiment.rs
//
// 市场情绪数据采集服务
// 负责采集：资金费率、持仓量、多空比
// 存储：PostgreSQL + Redis

use std::sync::Arc;
use std::time::Duration;
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use tracing::{debug, error, info, warn};

use crate::exchange::{Exchange, FundingRateData, OpenInterestData, LongShortRatioData};
use redis::aio::ConnectionManager;

// =================================================================
// Redis data structures
// =================================================================

/// 资金费率 Redis 存储格式
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FundingRateRedis {
    pub fr: f64,   // funding_rate
    pub ft: i64,   // funding_time (millis)
    pub mp: f64,   // mark_price
}

/// 持仓量 Redis 存储格式
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OpenInterestRedis {
    pub oi: f64,   // open_interest
    pub ov: f64,   // open_value
    pub ts: i64,   // timestamp (millis)
}

/// 多空比 Redis 存储格式
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LongShortRatioRedis {
    pub lr: f64,   // long_ratio
    pub sr: f64,   // short_ratio
    pub r: f64,    // ratio
    pub ts: i64,   // timestamp (millis)
}

// =================================================================
// 市场情绪采集服务
// =================================================================

/// Decimal 转 f64（用于 Redis 缓存，精度损失可接受）
fn decimal_to_f64(d: Decimal) -> f64 {
    d.to_string().parse::<f64>().unwrap_or(0.0)
}

pub struct MarketSentimentCollector {
    exchange: Arc<dyn Exchange>,
    repo: Arc<crate::data::repository::TickDataRepository>,
    redis_conn: ConnectionManager,
    symbols: Vec<String>,
}

impl MarketSentimentCollector {
    pub fn new(
        exchange: Arc<dyn Exchange>,
        repo: Arc<crate::data::repository::TickDataRepository>,
        redis_conn: ConnectionManager,
        symbols: Vec<String>,
    ) -> Self {
        Self {
            exchange,
            repo,
            redis_conn,
            symbols,
        }
    }

    /// 启动市场情绪数据采集
    ///
    /// 采集频率：
    /// - 资金费率：每1小时
    /// - 持仓量：每1分钟
    /// - 多空比：每5分钟
    ///
    /// 设计：每种采集在独立的 tokio task 中运行，互不阻塞
    pub async fn run(&self) {
        info!("📊 Market sentiment collector started");

        // 启动时立即采集一次
        self.collect_all().await;

        // 每种采集类型在独立 task 中运行，避免串行阻塞
        let exchange_f = self.exchange.clone();
        let repo_f = self.repo.clone();
        let redis_f = self.redis_conn.clone();
        let symbols_f = self.symbols.clone();

        let exchange_o = self.exchange.clone();
        let repo_o = self.repo.clone();
        let redis_o = self.redis_conn.clone();
        let symbols_o = self.symbols.clone();

        let exchange_l = self.exchange.clone();
        let repo_l = self.repo.clone();
        let redis_l = self.redis_conn.clone();
        let symbols_l = self.symbols.clone();

        // 资金费率采集 task
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(3600));
            interval.tick().await; // skip first
            let mut consecutive_failures: u32 = 0;

            loop {
                interval.tick().await;
                let mut any_failed = false;
                for symbol in &symbols_f {
                    if let Err(e) = collect_funding_rate(&exchange_f, &repo_f, &mut redis_f.clone(), symbol).await {
                        warn!("[{}] Funding rate collection failed: {}", symbol, e);
                        any_failed = true;
                    }
                }
                // 退避机制：连续失败时增大采集间隔
                if any_failed {
                    consecutive_failures = consecutive_failures.saturating_add(1);
                    let backoff = Duration::from_secs(300 * 2u64.pow(consecutive_failures.min(4)));
                    warn!("Funding rate: {} consecutive failures, backing off {:?}", consecutive_failures, backoff);
                    tokio::time::sleep(backoff).await;
                } else {
                    consecutive_failures = 0;
                }
            }
        });

        // 持仓量采集 task
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(60));
            interval.tick().await;
            let mut consecutive_failures: u32 = 0;

            loop {
                interval.tick().await;
                let mut any_failed = false;
                for symbol in &symbols_o {
                    if let Err(e) = collect_open_interest(&exchange_o, &repo_o, &mut redis_o.clone(), symbol).await {
                        warn!("[{}] Open interest collection failed: {}", symbol, e);
                        any_failed = true;
                    }
                }
                if any_failed {
                    consecutive_failures = consecutive_failures.saturating_add(1);
                    let backoff = Duration::from_secs(60 * 2u64.pow(consecutive_failures.min(4)));
                    warn!("Open interest: {} consecutive failures, backing off {:?}", consecutive_failures, backoff);
                    tokio::time::sleep(backoff).await;
                } else {
                    consecutive_failures = 0;
                }
            }
        });

        // 多空比采集 task
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(300));
            interval.tick().await;
            let mut consecutive_failures: u32 = 0;

            loop {
                interval.tick().await;
                let mut any_failed = false;
                for symbol in &symbols_l {
                    if let Err(e) = collect_long_short_ratio(&exchange_l, &repo_l, &mut redis_l.clone(), symbol).await {
                        warn!("[{}] Long/short ratio collection failed: {}", symbol, e);
                        any_failed = true;
                    }
                }
                if any_failed {
                    consecutive_failures = consecutive_failures.saturating_add(1);
                    let backoff = Duration::from_secs(300 * 2u64.pow(consecutive_failures.min(4)));
                    warn!("Long/short ratio: {} consecutive failures, backing off {:?}", consecutive_failures, backoff);
                    tokio::time::sleep(backoff).await;
                } else {
                    consecutive_failures = 0;
                }
            }
        });
    }

    /// 启动时全量采集一次
    async fn collect_all(&self) {
        info!("📊 Initial market sentiment collection...");
        for symbol in &self.symbols {
            let _ = collect_funding_rate(&self.exchange, &self.repo, &mut self.redis_conn.clone(), symbol).await;
            let _ = collect_open_interest(&self.exchange, &self.repo, &mut self.redis_conn.clone(), symbol).await;
            let _ = collect_long_short_ratio(&self.exchange, &self.repo, &mut self.redis_conn.clone(), symbol).await;
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        info!("📊 Initial market sentiment collection completed");
    }
}

// =================================================================
// 独立采集函数（每个函数使用单一 conn clone）
// =================================================================

/// 采集资金费率
async fn collect_funding_rate(
    exchange: &Arc<dyn Exchange>,
    repo: &Arc<crate::data::repository::TickDataRepository>,
    conn: &mut ConnectionManager,
    symbol: &str,
) -> Result<(), String> {
    let data = exchange.fetch_funding_rate(symbol).await
        .map_err(|e| format!("fetch_funding_rate: {}", e))?;

    // 写入 PostgreSQL
    repo.insert_funding_rate(
        symbol,
        data.funding_rate,
        data.funding_time,
        data.mark_price,
    ).await.map_err(|e| format!("insert_funding_rate: {}", e))?;

    // 写入 Redis（使用单一 conn，三步操作原子性更好）
    let redis_data = FundingRateRedis {
        fr: decimal_to_f64(data.funding_rate),
        ft: data.funding_time.timestamp_millis(),
        mp: data.mark_price.map(decimal_to_f64).unwrap_or(0.0),
    };

    let key = format!("funding_rate:{}", symbol);
    let json = serde_json::to_string(&redis_data).map_err(|e| e.to_string())?;

    // 使用 pipeline 将三步操作合并，减少竞态窗口
    let mut pipe = redis::pipe();
    pipe.cmd("ZADD").arg(&key).arg(redis_data.ft).arg(&json);
    pipe.cmd("ZREMRANGEBYRANK").arg(&key).arg(0).arg(-721);
    pipe.cmd("EXPIRE").arg(&key).arg(604800u32);
    pipe.query_async::<_, ()>(conn).await
        .map_err(|e| format!("Redis pipeline: {}", e))?;

    debug!("[{}] Funding rate: {} (time: {})", symbol, data.funding_rate, data.funding_time.format("%Y-%m-%d %H:%M"));

    Ok(())
}

/// 采集持仓量
async fn collect_open_interest(
    exchange: &Arc<dyn Exchange>,
    repo: &Arc<crate::data::repository::TickDataRepository>,
    conn: &mut ConnectionManager,
    symbol: &str,
) -> Result<(), String> {
    let data = exchange.fetch_open_interest(symbol).await
        .map_err(|e| format!("fetch_open_interest: {}", e))?;

    // 写入 PostgreSQL
    repo.insert_open_interest(
        symbol,
        data.open_interest,
        data.open_value,
        data.timestamp,
    ).await.map_err(|e| format!("insert_open_interest: {}", e))?;

    // 写入 Redis
    let redis_data = OpenInterestRedis {
        oi: decimal_to_f64(data.open_interest),
        ov: data.open_value.map(decimal_to_f64).unwrap_or(0.0),
        ts: data.timestamp.timestamp_millis(),
    };

    let key = format!("open_interest:{}", symbol);
    let json = serde_json::to_string(&redis_data).map_err(|e| e.to_string())?;

    let mut pipe = redis::pipe();
    pipe.cmd("ZADD").arg(&key).arg(redis_data.ts).arg(&json);
    pipe.cmd("ZREMRANGEBYRANK").arg(&key).arg(0).arg(-43201);
    pipe.cmd("EXPIRE").arg(&key).arg(604800u32);
    pipe.query_async::<_, ()>(conn).await
        .map_err(|e| format!("Redis pipeline: {}", e))?;

    debug!("[{}] Open interest: {} (value: {:?})", symbol, data.open_interest, data.open_value);

    Ok(())
}

/// 采集多空比
async fn collect_long_short_ratio(
    exchange: &Arc<dyn Exchange>,
    repo: &Arc<crate::data::repository::TickDataRepository>,
    conn: &mut ConnectionManager,
    symbol: &str,
) -> Result<(), String> {
    let data_list = exchange.fetch_long_short_ratio(symbol, "5m", 1).await
        .map_err(|e| format!("fetch_long_short_ratio: {}", e))?;

    for data in &data_list {
        // 写入 PostgreSQL
        repo.insert_long_short_ratio(
            symbol,
            data.long_ratio,
            data.short_ratio,
            data.ratio,
            data.timestamp,
        ).await.map_err(|e| format!("insert_long_short_ratio: {}", e))?;

        // 写入 Redis
        let redis_data = LongShortRatioRedis {
            lr: decimal_to_f64(data.long_ratio),
            sr: decimal_to_f64(data.short_ratio),
            r: decimal_to_f64(data.ratio),
            ts: data.timestamp.timestamp_millis(),
        };

        let key = format!("long_short_ratio:{}", symbol);
        let json = serde_json::to_string(&redis_data).map_err(|e| e.to_string())?;

        let mut pipe = redis::pipe();
        pipe.cmd("ZADD").arg(&key).arg(redis_data.ts).arg(&json);
        pipe.cmd("ZREMRANGEBYRANK").arg(&key).arg(0).arg(-8641);
        pipe.cmd("EXPIRE").arg(&key).arg(604800u32);
        pipe.query_async::<_, ()>(conn).await
            .map_err(|e| format!("Redis pipeline: {}", e))?;

        debug!("[{}] Long/short ratio: {} (long: {}, short: {})", symbol, data.ratio, data.long_ratio, data.short_ratio);
    }

    Ok(())
}
