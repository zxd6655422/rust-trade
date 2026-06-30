// exchange/traits.rs

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
}
