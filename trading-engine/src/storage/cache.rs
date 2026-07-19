// storage/cache.rs
// Redis 缓存管理

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use redis::aio::ConnectionManager;
use redis::Client;
use tracing::{debug, info};

use crate::config::CacheConfig;

/// Redis 缓存
#[derive(Clone)]
pub struct RedisCache {
    manager: ConnectionManager,
    ttl_seconds: u64,
    max_ticks_per_symbol: usize,
}

impl RedisCache {
    /// 获取 Redis 连接管理器（用于创建 EventPublisher 等）
    pub fn manager(&self) -> &ConnectionManager {
        &self.manager
    }

    /// 创建新的 Redis 缓存连接
    pub async fn new(config: &CacheConfig) -> Result<Self, redis::RedisError> {
        info!("Connecting to Redis...");

        let client = Client::open(config.url.as_str())?;
        let manager = ConnectionManager::new(client).await?;

        info!("Redis connection established");

        Ok(Self {
            manager,
            ttl_seconds: config.ttl_seconds,
            max_ticks_per_symbol: config.max_ticks_per_symbol,
        })
    }

    /// 保存实时价格
    pub async fn set_price(
        &self,
        symbol: &str,
        price: Decimal,
    ) -> Result<(), redis::RedisError> {
        let mut conn = self.manager.clone();
        let key = format!("price:{}", symbol);
        let value = price.to_string();

        redis::cmd("SET")
            .arg(&key)
            .arg(&value)
            .arg("EX")
            .arg(self.ttl_seconds)
            .query_async::<_, redis::Value>(&mut conn)
            .await?;

        debug!("Price cached: {} = {}", symbol, price);
        Ok(())
    }

    /// 获取实时价格
    pub async fn get_price(&self, symbol: &str) -> Result<Option<Decimal>, redis::RedisError> {
        let mut conn = self.manager.clone();
        let key = format!("price:{}", symbol);

        let value: Option<String> = redis::cmd("GET")
            .arg(&key)
            .query_async(&mut conn)
            .await?;

        Ok(value.and_then(|v| v.parse().ok()))
    }

    /// 保存持仓信息
    pub async fn set_position(
        &self,
        exchange: &str,
        symbol: &str,
        quantity: Decimal,
        avg_entry_price: Decimal,
    ) -> Result<(), redis::RedisError> {
        let mut conn = self.manager.clone();
        let key = format!("position:{}:{}", exchange, symbol);
        let value = serde_json::json!({
            "quantity": quantity.to_string(),
            "avg_entry_price": avg_entry_price.to_string(),
            "updated_at": Utc::now().to_rfc3339(),
        });

        redis::cmd("SET")
            .arg(&key)
            .arg(value.to_string())
            .arg("EX")
            .arg(self.ttl_seconds * 2)  // 持仓信息保留更长时间
            .query_async::<_, redis::Value>(&mut conn)
            .await?;

        debug!("Position cached: {}:{} = {}", exchange, symbol, quantity);
        Ok(())
    }

    /// 获取持仓信息
    pub async fn get_position(
        &self,
        exchange: &str,
        symbol: &str,
    ) -> Result<Option<PositionCache>, redis::RedisError> {
        let mut conn = self.manager.clone();
        let key = format!("position:{}:{}", exchange, symbol);

        let value: Option<String> = redis::cmd("GET")
            .arg(&key)
            .query_async(&mut conn)
            .await?;

        if let Some(json_str) = value {
            if let Ok(data) = serde_json::from_str::<serde_json::Value>(&json_str) {
                return Ok(Some(PositionCache {
                    symbol: symbol.to_string(),
                    quantity: data["quantity"]
                        .as_str()
                        .and_then(|s| s.parse().ok())
                        .unwrap_or_default(),
                    avg_entry_price: data["avg_entry_price"]
                        .as_str()
                        .and_then(|s| s.parse().ok())
                        .unwrap_or_default(),
                    updated_at: data["updated_at"]
                        .as_str()
                        .and_then(|s| s.parse().ok())
                        .unwrap_or_default(),
                }));
            }
        }

        Ok(None)
    }

    /// 保存订单状态
    pub async fn set_order_status(
        &self,
        order_id: &str,
        status: &str,
    ) -> Result<(), redis::RedisError> {
        let mut conn = self.manager.clone();
        let key = format!("order:{}", order_id);

        redis::cmd("SET")
            .arg(&key)
            .arg(status)
            .arg("EX")
            .arg(self.ttl_seconds * 6)  // 订单状态保留更长时间
            .query_async::<_, redis::Value>(&mut conn)
            .await?;

        debug!("Order status cached: {} = {}", order_id, status);
        Ok(())
    }

    /// 获取订单状态
    pub async fn get_order_status(&self, order_id: &str) -> Result<Option<String>, redis::RedisError> {
        let mut conn = self.manager.clone();
        let key = format!("order:{}", order_id);

        let value: Option<String> = redis::cmd("GET")
            .arg(&key)
            .query_async(&mut conn)
            .await?;

        Ok(value)
    }

    /// 保存风控状态
    pub async fn set_risk_state(
        &self,
        state: &RiskStateCache,
    ) -> Result<(), redis::RedisError> {
        let mut conn = self.manager.clone();
        let key = "risk:state";
        let value = serde_json::json!({
            "daily_pnl": state.daily_pnl.to_string(),
            "peak_equity": state.peak_equity.to_string(),
            "daily_trade_count": state.daily_trade_count,
            "circuit_breaker_until": state.circuit_breaker_until.map(|dt| dt.to_rfc3339()),
            "updated_at": Utc::now().to_rfc3339(),
        });

        redis::cmd("SET")
            .arg(&key)
            .arg(value.to_string())
            .query_async::<_, redis::Value>(&mut conn)
            .await?;

        debug!("Risk state cached");
        Ok(())
    }

    /// 获取风控状态
    pub async fn get_risk_state(&self) -> Result<Option<RiskStateCache>, redis::RedisError> {
        let mut conn = self.manager.clone();
        let key = "risk:state";

        let value: Option<String> = redis::cmd("GET")
            .arg(&key)
            .query_async(&mut conn)
            .await?;

        if let Some(json_str) = value {
            if let Ok(data) = serde_json::from_str::<serde_json::Value>(&json_str) {
                return Ok(Some(RiskStateCache {
                    daily_pnl: data["daily_pnl"]
                        .as_str()
                        .and_then(|s| s.parse().ok())
                        .unwrap_or_default(),
                    peak_equity: data["peak_equity"]
                        .as_str()
                        .and_then(|s| s.parse().ok())
                        .unwrap_or_default(),
                    daily_trade_count: data["daily_trade_count"]
                        .as_i64()
                        .unwrap_or(0) as u32,
                    circuit_breaker_until: data["circuit_breaker_until"]
                        .as_str()
                        .and_then(|s| s.parse().ok()),
                }));
            }
        }

        Ok(None)
    }

    /// 保存 Tick 数据到列表
    pub async fn push_tick(
        &self,
        symbol: &str,
        price: Decimal,
        quantity: Decimal,
    ) -> Result<(), redis::RedisError> {
        let mut conn = self.manager.clone();
        let key = format!("ticks:{}", symbol);
        let value = serde_json::json!({
            "price": price.to_string(),
            "quantity": quantity.to_string(),
            "timestamp": Utc::now().to_rfc3339(),
        });

        redis::cmd("LPUSH")
            .arg(&key)
            .arg(value.to_string())
            .query_async::<_, redis::Value>(&mut conn)
            .await?;

        // 保持列表大小 (使用配置的最大 tick 数量)
        redis::cmd("LTRIM")
            .arg(&key)
            .arg(0)
            .arg((self.max_ticks_per_symbol - 1) as i64)
            .query_async::<_, redis::Value>(&mut conn)
            .await?;

        // 设置过期时间
        redis::cmd("EXPIRE")
            .arg(&key)
            .arg(self.ttl_seconds * 10)
            .query_async::<_, redis::Value>(&mut conn)
            .await?;

        Ok(())
    }

    /// 获取最近的 Tick 数据
    pub async fn get_recent_ticks(
        &self,
        symbol: &str,
        count: usize,
    ) -> Result<Vec<TickCache>, redis::RedisError> {
        let mut conn = self.manager.clone();
        let key = format!("ticks:{}", symbol);

        let values: Vec<String> = redis::cmd("LRANGE")
            .arg(&key)
            .arg(0)
            .arg((count - 1) as i64)
            .query_async(&mut conn)
            .await?;

        let ticks = values
            .iter()
            .filter_map(|json_str| {
                serde_json::from_str::<serde_json::Value>(json_str)
                    .ok()
                    .and_then(|data| {
                        Some(TickCache {
                            price: data["price"]
                                .as_str()
                                .and_then(|s| s.parse().ok())
                                .unwrap_or_default(),
                            quantity: data["quantity"]
                                .as_str()
                                .and_then(|s| s.parse().ok())
                                .unwrap_or_default(),
                            timestamp: data["timestamp"]
                                .as_str()
                                .and_then(|s| s.parse().ok())
                                .unwrap_or_default(),
                        })
                    })
            })
            .collect();

        Ok(ticks)
    }

    /// 清除缓存
    pub async fn clear(&self, pattern: &str) -> Result<(), redis::RedisError> {
        let mut conn = self.manager.clone();

        // 获取匹配的键
        let keys: Vec<String> = redis::cmd("KEYS")
            .arg(pattern)
            .query_async(&mut conn)
            .await?;

        if !keys.is_empty() {
            redis::cmd("DEL")
                .arg(&keys)
                .query_async::<_, redis::Value>(&mut conn)
                .await?;

            info!("Cleared {} keys matching {}", keys.len(), pattern);
        }

        Ok(())
    }
}

/// 持仓缓存
#[derive(Debug, Clone)]
pub struct PositionCache {
    pub symbol: String,
    pub quantity: Decimal,
    pub avg_entry_price: Decimal,
    pub updated_at: DateTime<Utc>,
}

/// 风控状态缓存
#[derive(Debug, Clone)]
pub struct RiskStateCache {
    pub daily_pnl: Decimal,
    pub peak_equity: Decimal,
    pub daily_trade_count: u32,
    pub circuit_breaker_until: Option<DateTime<Utc>>,
}

/// Tick 数据缓存
#[derive(Debug, Clone)]
pub struct TickCache {
    pub price: Decimal,
    pub quantity: Decimal,
    pub timestamp: DateTime<Utc>,
}
