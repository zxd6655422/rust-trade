// exchange/traits.rs

use std::collections::HashMap;

use super::{types::KlineData, ExchangeError};
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use trading_common::data::types::TickData;

/// Main exchange interface that all exchange implementations must follow
#[async_trait]
pub trait Exchange: Send + Sync {
    /// Subscribe to real-time trade data streams
    async fn subscribe_trades(
        &self,
        symbols: &[String],
        callback: Box<dyn Fn(TickData) + Send + Sync>,
        shutdown_rx: tokio::sync::broadcast::Receiver<()>,
    ) -> Result<(), ExchangeError>;

    /// Fetch K-line data via REST API (latest)
    async fn fetch_klines(
        &self,
        symbol: &str,
        interval: &str,
        limit: u32,
    ) -> Result<Vec<KlineData>, ExchangeError>;

    /// Fetch K-line data with time range (for backfill)
    async fn fetch_klines_with_time(
        &self,
        symbol: &str,
        interval: &str,
        start_time: DateTime<Utc>,
        end_time: DateTime<Utc>,
        limit: u32,
    ) -> Result<Vec<KlineData>, ExchangeError>;

    /// Fetch K-line data for multiple timeframes (for multi-TF backfill)
    ///
    /// # Arguments
    /// * `symbol` - Trading pair
    /// * `timeframes` - List of timeframe strings (e.g., ["4h", "1d", "1w"])
    /// * `start_time` - Start time for data range
    /// * `end_time` - End time for data range
    /// * `limit` - Max records per request
    ///
    /// # Returns
    /// HashMap with timeframe as key and kline data as value
    async fn fetch_klines_multi_tf(
        &self,
        symbol: &str,
        timeframes: &[&str],
        start_time: DateTime<Utc>,
        end_time: DateTime<Utc>,
        limit: u32,
    ) -> Result<HashMap<String, Vec<KlineData>>, ExchangeError> {
        // Default implementation: fetch sequentially
        let mut result = HashMap::new();

        for &tf in timeframes {
            match self.fetch_klines_with_time(symbol, tf, start_time, end_time, limit).await {
                Ok(klines) => {
                    result.insert(tf.to_string(), klines);
                }
                Err(e) => {
                    tracing::warn!("Failed to fetch {} klines for {}: {}", tf, symbol, e);
                }
            }
        }

        Ok(result)
    }
}
