// exchange/traits.rs

use std::collections::HashMap;

use super::types::*;
use super::ExchangeError;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
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

    /// Fetch latest funding rate for a symbol
    async fn fetch_funding_rate(
        &self,
        symbol: &str,
    ) -> Result<FundingRateData, ExchangeError>;

    /// Fetch funding rate history
    async fn fetch_funding_rate_history(
        &self,
        symbol: &str,
        start_time: Option<DateTime<Utc>>,
        end_time: Option<DateTime<Utc>>,
        limit: u32,
    ) -> Result<Vec<FundingRateData>, ExchangeError>;

    /// Fetch current open interest
    async fn fetch_open_interest(
        &self,
        symbol: &str,
    ) -> Result<OpenInterestData, ExchangeError>;

    /// Fetch long/short ratio
    async fn fetch_long_short_ratio(
        &self,
        symbol: &str,
        period: &str,  // "5m", "15m", "30m", "1h"
        limit: u32,
    ) -> Result<Vec<LongShortRatioData>, ExchangeError>;

    /// Fetch order book depth (top N levels)
    async fn fetch_order_book(
        &self,
        symbol: &str,
        limit: u32,  // 5, 10, 20, 50, 100, 500, 1000
    ) -> Result<OrderBookData, ExchangeError>;

    /// Fetch recent trades and filter for large ones
    async fn fetch_large_trades(
        &self,
        symbol: &str,
        min_quote_qty: Decimal,  // Minimum USDT value to qualify as "large"
        limit: u32,
    ) -> Result<Vec<LargeTradeData>, ExchangeError>;
}
