// exchange/adapters/redis_datasource.rs
// Redis 数据源适配器

use chrono::Utc;
use rust_decimal::Decimal;
use std::sync::Arc;
use tokio::sync::{broadcast, Mutex};
use tokio::time::{interval, Duration};
use tracing::{debug, info, warn};

use crate::exchange::errors::ExchangeError;
use crate::exchange::traits::Exchange;
use crate::storage::RedisCache;
use trading_common::data::types::{TickData, TradeSide};

/// Redis 数据源配置
#[derive(Debug, Clone)]
pub struct RedisDataSourceConfig {
    /// 轮询间隔 (毫秒)
    pub poll_interval_ms: u64,
    /// 是否启用
    pub enabled: bool,
}

impl Default for RedisDataSourceConfig {
    fn default() -> Self {
        Self {
            poll_interval_ms: 100,
            enabled: true,
        }
    }
}

/// Redis 数据源适配器
/// 从 Redis 读取实时行情数据，作为 WebSocket 的备用数据源
pub struct RedisDataSource {
    cache: Arc<RedisCache>,
    config: RedisDataSourceConfig,
    symbols: Vec<String>,
}

impl RedisDataSource {
    /// 创建新的 Redis 数据源
    pub fn new(cache: Arc<RedisCache>, config: RedisDataSourceConfig) -> Self {
        Self {
            cache,
            config,
            symbols: Vec::new(),
        }
    }

    /// 启动行情轮询
    pub async fn start_polling(
        &self,
        symbols: &[String],
        callback: Box<dyn Fn(TickData) + Send + Sync>,
        shutdown_rx: broadcast::Receiver<()>,
    ) -> Result<(), ExchangeError> {
        if !self.config.enabled {
            info!("Redis data source is disabled");
            return Ok(());
        }

        let cache = self.cache.clone();
        let symbols = symbols.to_vec();
        let poll_interval = Duration::from_millis(self.config.poll_interval_ms);
        let symbols_clone = symbols.clone();

        tokio::spawn(async move {
            let mut interval = interval(poll_interval);
            let mut shutdown_rx = shutdown_rx;
            let mut last_prices: std::collections::HashMap<String, Decimal> =
                std::collections::HashMap::new();

            loop {
                tokio::select! {
                    _ = interval.tick() => {
                        for symbol in &symbols {
                            match cache.get_price(symbol).await {
                                Ok(Some(price)) => {
                                    // 只在价格变化时触发回调
                                    let should_notify = match last_prices.get(symbol) {
                                        Some(last_price) => *last_price != price,
                                        None => true,
                                    };

                                    if should_notify {
                                        last_prices.insert(symbol.clone(), price);

                                        let tick = TickData {
                                            symbol: symbol.clone(),
                                            price,
                                            quantity: Decimal::ZERO, // Redis 不存储数量
                                            timestamp: Utc::now(),
                                            side: TradeSide::Buy, // 默认值
                                            trade_id: format!("redis-{}", Utc::now().timestamp_millis()),
                                            is_buyer_maker: false,
                                        };

                                        callback(tick);
                                        debug!("Redis price update: {} = {}", symbol, price);
                                    }
                                }
                                Ok(None) => {
                                    debug!("No price in cache for {}", symbol);
                                }
                                Err(e) => {
                                    warn!("Failed to get price from Redis for {}: {}", symbol, e);
                                }
                            }
                        }
                    }
                    _ = shutdown_rx.recv() => {
                        info!("Redis data source polling stopped");
                        break;
                    }
                }
            }
        });

        info!(
            "Redis data source started polling for {:?} every {}ms",
            symbols_clone, self.config.poll_interval_ms
        );

        Ok(())
    }

    /// 获取最新价格
    pub async fn get_latest_price(&self, symbol: &str) -> Result<Option<Decimal>, ExchangeError> {
        self.cache
            .get_price(symbol)
            .await
            .map_err(|e| ExchangeError::Unknown(e.to_string()))
    }

    /// 获取多个交易对的最新价格
    pub async fn get_latest_prices(
        &self,
        symbols: &[String],
    ) -> Result<std::collections::HashMap<String, Decimal>, ExchangeError> {
        let mut prices = std::collections::HashMap::new();

        for symbol in symbols {
            if let Ok(Some(price)) = self.cache.get_price(symbol).await {
                prices.insert(symbol.clone(), price);
            }
        }

        Ok(prices)
    }
}

/// 混合数据源
/// 结合 WebSocket 和 Redis 数据源，提供高可用的行情数据
pub struct HybridDataSource {
    primary: Arc<dyn Exchange + Send + Sync>,
    fallback: Arc<RedisDataSource>,
    use_fallback: Arc<Mutex<bool>>,
}

impl HybridDataSource {
    /// 创建混合数据源
    pub fn new(primary: Arc<dyn Exchange + Send + Sync>, fallback: Arc<RedisDataSource>) -> Self {
        Self {
            primary,
            fallback,
            use_fallback: Arc::new(Mutex::new(false)),
        }
    }

    /// 启用备用数据源
    pub async fn enable_fallback(&self) {
        let mut use_fallback = self.use_fallback.lock().await;
        *use_fallback = true;
        info!("Fallback data source enabled");
    }

    /// 禁用备用数据源
    pub async fn disable_fallback(&self) {
        let mut use_fallback = self.use_fallback.lock().await;
        *use_fallback = false;
        info!("Fallback data source disabled");
    }

    /// 获取最新价格 (优先使用主数据源)
    pub async fn get_latest_price(&self, symbol: &str) -> Result<Decimal, ExchangeError> {
        // 首先尝试主数据源
        // 这里需要实现从 WebSocket 获取最新价格的逻辑
        // 暂时直接使用 Redis
        self.fallback
            .get_latest_price(symbol)
            .await?
            .ok_or_else(|| ExchangeError::Unknown(format!("No price available for {}", symbol)))
    }
}
