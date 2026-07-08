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
    /// - 资金费率：每1小时（结算周期8小时，但每小时检查一次新数据）
    /// - 持仓量：每1分钟（随K线poll一起）
    /// - 多空比：每5分钟
    pub async fn run(&self) {
        info!("📊 Market sentiment collector started");

        // 启动时立即采集一次
        self.collect_all().await;

        // 定时采集
        let mut funding_interval = tokio::time::interval(Duration::from_secs(3600));  // 1小时
        let mut oi_interval = tokio::time::interval(Duration::from_secs(60));          // 1分钟
        let mut lsr_interval = tokio::time::interval(Duration::from_secs(300));        // 5分钟

        // 跳过首次tick
        funding_interval.tick().await;
        oi_interval.tick().await;
        lsr_interval.tick().await;

        loop {
            tokio::select! {
                _ = funding_interval.tick() => {
                    for symbol in &self.symbols {
                        if let Err(e) = self.collect_funding_rate(symbol).await {
                            warn!("[{}] Funding rate collection failed: {}", symbol, e);
                        }
                    }
                }
                _ = oi_interval.tick() => {
                    for symbol in &self.symbols {
                        if let Err(e) = self.collect_open_interest(symbol).await {
                            warn!("[{}] Open interest collection failed: {}", symbol, e);
                        }
                    }
                }
                _ = lsr_interval.tick() => {
                    for symbol in &self.symbols {
                        if let Err(e) = self.collect_long_short_ratio(symbol).await {
                            warn!("[{}] Long/short ratio collection failed: {}", symbol, e);
                        }
                    }
                }
            }
        }
    }

    /// 启动时全量采集一次
    async fn collect_all(&self) {
        info!("📊 Initial market sentiment collection...");
        for symbol in &self.symbols {
            let _ = self.collect_funding_rate(symbol).await;
            let _ = self.collect_open_interest(symbol).await;
            let _ = self.collect_long_short_ratio(symbol).await;
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        info!("📊 Initial market sentiment collection completed");
    }

    /// 采集资金费率
    async fn collect_funding_rate(&self, symbol: &str) -> Result<(), String> {
        let data = self.exchange.fetch_funding_rate(symbol).await
            .map_err(|e| format!("fetch_funding_rate: {}", e))?;

        // 写入 PostgreSQL
        self.repo.insert_funding_rate(
            symbol,
            data.funding_rate,
            data.funding_time,
            data.mark_price,
        ).await.map_err(|e| format!("insert_funding_rate: {}", e))?;

        // 写入 Redis
        let redis_data = FundingRateRedis {
            fr: data.funding_rate.to_string().parse::<f64>().unwrap_or(0.0),
            ft: data.funding_time.timestamp_millis(),
            mp: data.mark_price.map(|p| p.to_string().parse::<f64>().unwrap_or(0.0)).unwrap_or(0.0),
        };

        let key = format!("funding_rate:{}", symbol);
        let json = serde_json::to_string(&redis_data).map_err(|e| e.to_string())?;

        redis::cmd("ZADD")
            .arg(&key)
            .arg(redis_data.ft)
            .arg(&json)
            .query_async::<_, ()>(&mut self.redis_conn.clone())
            .await
            .map_err(|e| format!("Redis ZADD: {}", e))?;

        // 裁剪到720条（约30天）
        redis::cmd("ZREMRANGEBYRANK")
            .arg(&key)
            .arg(0)
            .arg(-721)
            .query_async::<_, ()>(&mut self.redis_conn.clone())
            .await
            .map_err(|e| format!("Redis ZREMRANGEBYRANK: {}", e))?;

        // TTL 7天
        redis::cmd("EXPIRE")
            .arg(&key)
            .arg(604800)
            .query_async::<_, ()>(&mut self.redis_conn.clone())
            .await
            .map_err(|e| format!("Redis EXPIRE: {}", e))?;

        debug!("[{}] Funding rate: {} (time: {})", symbol, data.funding_rate, data.funding_time.format("%Y-%m-%d %H:%M"));

        Ok(())
    }

    /// 采集持仓量
    async fn collect_open_interest(&self, symbol: &str) -> Result<(), String> {
        let data = self.exchange.fetch_open_interest(symbol).await
            .map_err(|e| format!("fetch_open_interest: {}", e))?;

        // 写入 PostgreSQL
        self.repo.insert_open_interest(
            symbol,
            data.open_interest,
            data.open_value,
            data.timestamp,
        ).await.map_err(|e| format!("insert_open_interest: {}", e))?;

        // 写入 Redis
        let redis_data = OpenInterestRedis {
            oi: data.open_interest.to_string().parse::<f64>().unwrap_or(0.0),
            ov: data.open_value.map(|v| v.to_string().parse::<f64>().unwrap_or(0.0)).unwrap_or(0.0),
            ts: data.timestamp.timestamp_millis(),
        };

        let key = format!("open_interest:{}", symbol);
        let json = serde_json::to_string(&redis_data).map_err(|e| e.to_string())?;

        redis::cmd("ZADD")
            .arg(&key)
            .arg(redis_data.ts)
            .arg(&json)
            .query_async::<_, ()>(&mut self.redis_conn.clone())
            .await
            .map_err(|e| format!("Redis ZADD: {}", e))?;

        // 裁剪到43200条（30天 × 1440条/天）
        redis::cmd("ZREMRANGEBYRANK")
            .arg(&key)
            .arg(0)
            .arg(-43201)
            .query_async::<_, ()>(&mut self.redis_conn.clone())
            .await
            .map_err(|e| format!("Redis ZREMRANGEBYRANK: {}", e))?;

        // TTL 7天
        redis::cmd("EXPIRE")
            .arg(&key)
            .arg(604800)
            .query_async::<_, ()>(&mut self.redis_conn.clone())
            .await
            .map_err(|e| format!("Redis EXPIRE: {}", e))?;

        debug!("[{}] Open interest: {} (value: {:?})", symbol, data.open_interest, data.open_value);

        Ok(())
    }

    /// 采集多空比
    async fn collect_long_short_ratio(&self, symbol: &str) -> Result<(), String> {
        let data_list = self.exchange.fetch_long_short_ratio(symbol, "5m", 1).await
            .map_err(|e| format!("fetch_long_short_ratio: {}", e))?;

        for data in &data_list {
            // 写入 PostgreSQL
            self.repo.insert_long_short_ratio(
                symbol,
                data.long_ratio,
                data.short_ratio,
                data.ratio,
                data.timestamp,
            ).await.map_err(|e| format!("insert_long_short_ratio: {}", e))?;

            // 写入 Redis
            let redis_data = LongShortRatioRedis {
                lr: data.long_ratio.to_string().parse::<f64>().unwrap_or(0.0),
                sr: data.short_ratio.to_string().parse::<f64>().unwrap_or(0.0),
                r: data.ratio.to_string().parse::<f64>().unwrap_or(0.0),
                ts: data.timestamp.timestamp_millis(),
            };

            let key = format!("long_short_ratio:{}", symbol);
            let json = serde_json::to_string(&redis_data).map_err(|e| e.to_string())?;

            redis::cmd("ZADD")
                .arg(&key)
                .arg(redis_data.ts)
                .arg(&json)
                .query_async::<_, ()>(&mut self.redis_conn.clone())
                .await
                .map_err(|e| format!("Redis ZADD: {}", e))?;

            // 裁剪到8640条（30天 × 288条/天）
            redis::cmd("ZREMRANGEBYRANK")
                .arg(&key)
                .arg(0)
                .arg(-8641)
                .query_async::<_, ()>(&mut self.redis_conn.clone())
                .await
                .map_err(|e| format!("Redis ZREMRANGEBYRANK: {}", e))?;

            // TTL 7天
            redis::cmd("EXPIRE")
                .arg(&key)
                .arg(604800)
                .query_async::<_, ()>(&mut self.redis_conn.clone())
                .await
                .map_err(|e| format!("Redis EXPIRE: {}", e))?;

            debug!("[{}] Long/short ratio: {} (long: {}, short: {})", symbol, data.ratio, data.long_ratio, data.short_ratio);
        }

        Ok(())
    }
}
