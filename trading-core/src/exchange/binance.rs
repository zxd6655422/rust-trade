// exchange/binance.rs

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use futures_util::{SinkExt, StreamExt};
use rust_decimal::Decimal;
use std::str::FromStr;
use std::time::Duration;
use tokio::time::sleep;
use tokio_tungstenite::{connect_async, tungstenite::Message};
use tracing::{debug, error, info, warn};

use super::{
    errors::ExchangeError,
    traits::Exchange,
    types::{
        BinanceStreamMessage, BinanceSubscribeMessage, BinanceTradeMessage,
        FundingRateData, KlineData, LargeTradeData, LongShortRatioData,
        OpenInterestData, OrderBookData,
    },
    utils::{build_binance_trade_streams, convert_binance_to_tick_data},
};
use trading_common::data::types::TickData;

// Constants
const BINANCE_WS_URL: &str = "wss://stream.binance.com:9443/stream";
const BINANCE_SPOT_URL: &str = "https://api.binance.com";
const BINANCE_FUTURES_URL: &str = "https://fapi.binance.com";
const RECONNECT_DELAY: Duration = Duration::from_secs(5);

/// Binance exchange implementation
pub struct BinanceExchange {
    ws_url: String,
    /// 只有合约的交易对列表
    futures_symbols: Vec<String>,
    /// 共享的 HTTP 客户端（连接池复用）
    http_client: reqwest::Client,
}

impl BinanceExchange {
    /// Create a new Binance exchange instance
    pub fn new() -> Self {
        Self {
            ws_url: BINANCE_WS_URL.to_string(),
            futures_symbols: Vec::new(),
            http_client: reqwest::Client::new(),
        }
    }

    /// Create with futures symbols list
    pub fn with_futures_symbols(futures_symbols: Vec<String>) -> Self {
        Self {
            ws_url: BINANCE_WS_URL.to_string(),
            futures_symbols,
            http_client: reqwest::Client::new(),
        }
    }

    /// 判断是否为合约交易对
    fn is_futures_symbol(&self, symbol: &str) -> bool {
        self.futures_symbols.iter().any(|s| s.eq_ignore_ascii_case(symbol))
    }

    /// 获取 REST API base URL
    fn rest_url(&self, symbol: &str) -> &str {
        if self.is_futures_symbol(symbol) {
            BINANCE_FUTURES_URL
        } else {
            BINANCE_SPOT_URL
        }
    }

    /// 获取 kline API 路径
    fn kline_path(&self, symbol: &str) -> &str {
        if self.is_futures_symbol(symbol) {
            "/fapi/v1/klines"
        } else {
            "/api/v3/klines"
        }
    }

    /// Parse WebSocket message and extract trade data
    fn parse_trade_message(&self, text: &str) -> Result<TickData, ExchangeError> {
        // First try to parse as stream message (combined streams format)
        if let Ok(stream_msg) = serde_json::from_str::<BinanceStreamMessage>(text) {
            return convert_binance_to_tick_data(stream_msg.data);
        }

        // Fallback: try to parse as direct trade message
        if let Ok(trade_msg) = serde_json::from_str::<BinanceTradeMessage>(text) {
            return convert_binance_to_tick_data(trade_msg);
        }

        // Check if it's a subscription confirmation or other control message
        if let Ok(value) = serde_json::from_str::<serde_json::Value>(text) {
            if value.get("result").is_some() || value.get("id").is_some() {
                // This is a subscription confirmation, not an error
                debug!("Received subscription confirmation: {}", text);
                return Err(ExchangeError::ParseError(
                    "Control message, not trade data".to_string(),
                ));
            }
        }

        Err(ExchangeError::ParseError(format!(
            "Unable to parse message: {}",
            text
        )))
    }

    /// Shared kline fetch + parse logic
    async fn do_fetch_klines(&self, url: &str, symbol: &str) -> Result<Vec<KlineData>, ExchangeError> {
        debug!("Fetching klines for {}: {}", symbol, url);

        let client = self.http_client.clone();
        let response = client
            .get(url)
            .send()
            .await
            .map_err(|e| ExchangeError::NetworkError(format!("[{}] HTTP request failed: {}", symbol, e)))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(ExchangeError::NetworkError(format!(
                "[{}] HTTP {} from Binance klines API: {}",
                symbol, status, body
            )));
        }

        let raw: Vec<Vec<serde_json::Value>> = response
            .json()
            .await
            .map_err(|e| ExchangeError::ParseError(format!("Failed to parse klines response: {}", e)))?;

        let mut klines = Vec::with_capacity(raw.len());

        for item in &raw {
            if item.len() < 12 {
                return Err(ExchangeError::ParseError(
                    "Kline array has fewer than 12 elements".to_string(),
                ));
            }

            let open_time_ms = item[0]
                .as_i64()
                .ok_or_else(|| ExchangeError::ParseError("Invalid open_time".to_string()))?;

            let timestamp = DateTime::from_timestamp_millis(open_time_ms)
                .ok_or_else(|| ExchangeError::ParseError("Invalid timestamp".to_string()))?;

            let open = Decimal::from_str(
                item[1].as_str().ok_or_else(|| ExchangeError::ParseError("Invalid open".to_string()))?,
            )
            .map_err(|e| ExchangeError::ParseError(format!("Invalid open price: {}", e)))?;

            let high = Decimal::from_str(
                item[2].as_str().ok_or_else(|| ExchangeError::ParseError("Invalid high".to_string()))?,
            )
            .map_err(|e| ExchangeError::ParseError(format!("Invalid high price: {}", e)))?;

            let low = Decimal::from_str(
                item[3].as_str().ok_or_else(|| ExchangeError::ParseError("Invalid low".to_string()))?,
            )
            .map_err(|e| ExchangeError::ParseError(format!("Invalid low price: {}", e)))?;

            let close = Decimal::from_str(
                item[4].as_str().ok_or_else(|| ExchangeError::ParseError("Invalid close".to_string()))?,
            )
            .map_err(|e| ExchangeError::ParseError(format!("Invalid close price: {}", e)))?;

            let volume = Decimal::from_str(
                item[5].as_str().ok_or_else(|| ExchangeError::ParseError("Invalid volume".to_string()))?,
            )
            .map_err(|e| ExchangeError::ParseError(format!("Invalid volume: {}", e)))?;

            let trade_count = item[8]
                .as_u64()
                .ok_or_else(|| ExchangeError::ParseError("Invalid trade count".to_string()))?;

            klines.push(KlineData {
                timestamp,
                symbol: symbol.to_string(),
                open,
                high,
                low,
                close,
                volume,
                trade_count,
            });
        }

        debug!("Fetched {} klines for {}", klines.len(), symbol);
        Ok(klines)
    }

    /// Handle WebSocket connection with reconnection logic
    async fn handle_websocket_connection(
        &self,
        symbols: &[String],
        callback: Box<dyn Fn(TickData) + Send + Sync>,
        mut shutdown_rx: tokio::sync::broadcast::Receiver<()>,
    ) -> Result<(), ExchangeError> {
        let streams = build_binance_trade_streams(symbols)?;
        info!(
            "Connecting to Binance WebSocket with {} streams",
            streams.len()
        );

        let mut reconnect_attempts = 0;
        const MAX_RECONNECT_ATTEMPTS: u32 = 10;

        loop {
            // Check for shutdown signal before each connection attempt
            if shutdown_rx.try_recv().is_ok() {
                info!("Shutdown signal received, stopping WebSocket connection attempts");
                return Ok(());
            }

            match self
                .connect_and_subscribe(&streams, &callback, shutdown_rx.resubscribe())
                .await
            {
                Ok(()) => {
                    info!(
                        "WebSocket connection ended normally - checking if shutdown was requested"
                    );

                    // If connection ended normally, it's likely due to shutdown signal
                    // Exit the reconnection loop
                    return Ok(());
                }
                Err(e) => {
                    reconnect_attempts += 1;
                    error!(
                        "WebSocket connection failed (attempt {}): {}",
                        reconnect_attempts, e
                    );

                    if reconnect_attempts >= MAX_RECONNECT_ATTEMPTS {
                        return Err(ExchangeError::NetworkError(format!(
                            "Max reconnection attempts ({}) exceeded",
                            MAX_RECONNECT_ATTEMPTS
                        )));
                    }

                    warn!("Attempting to reconnect in {:?}...", RECONNECT_DELAY);

                    // Wait for reconnect delay or shutdown signal
                    tokio::select! {
                        _ = sleep(RECONNECT_DELAY) => {
                            // Continue to retry
                            continue;
                        }
                        _ = shutdown_rx.recv() => {
                            info!("Shutdown signal received during reconnect delay");
                            return Ok(());
                        }
                    }
                }
            }
        }
    }

    /// Connect to WebSocket and handle subscription
    async fn connect_and_subscribe(
        &self,
        streams: &[String],
        callback: &Box<dyn Fn(TickData) + Send + Sync>,
        mut shutdown_rx: tokio::sync::broadcast::Receiver<()>,
    ) -> Result<(), ExchangeError> {
        // Establish WebSocket connection
        // tokio-tungstenite respects system proxy settings via environment variables
        let (ws_stream, _) = connect_async(&self.ws_url)
            .await
            .map_err(|e| ExchangeError::WebSocketError(format!("Failed to connect: {}", e)))?;

        debug!("WebSocket connected to {}", self.ws_url);

        let (mut write, mut read) = ws_stream.split();

        // Send subscription message
        let subscribe_msg = BinanceSubscribeMessage::new(streams.to_vec());
        let subscribe_json = serde_json::to_string(&subscribe_msg).map_err(|e| {
            ExchangeError::ParseError(format!("Failed to serialize subscription: {}", e))
        })?;

        write
            .send(Message::Text(subscribe_json))
            .await
            .map_err(|e| {
                ExchangeError::WebSocketError(format!("Failed to send subscription: {}", e))
            })?;

        info!("Subscription sent for {} streams", streams.len());

        // Message processing loop
        loop {
            tokio::select! {
                msg = read.next() => {
                    match msg {
                        Some(Ok(Message::Text(text))) => {
                            match self.parse_trade_message(&text) {
                                Ok(tick_data) => callback(tick_data),
                                Err(e) => warn!("Parse error: {}", e),
                            }
                        }
                        Some(Ok(Message::Ping(ping))) => {
                            write.send(Message::Pong(ping)).await?;
                        }
                        Some(Ok(Message::Close(_))) => {
                            info!("WebSocket closed by server");
                            break;
                        }
                        Some(Err(e)) => {
                            return Err(ExchangeError::WebSocketError(e.to_string()));
                        }
                        None => {
                            info!("WebSocket stream ended");
                            break;
                        }
                        _ => continue,
                    }
                }
                _ = shutdown_rx.recv() => {
                    info!("Shutdown signal received, closing WebSocket gracefully");
                    // Send Close frame to server
                    if let Err(e) = write.send(Message::Close(None)).await {
                        warn!("Failed to send close frame: {}", e);
                    }
                    break;
                }
            }
        }

        Ok(())
    }
}

#[async_trait]
impl Exchange for BinanceExchange {
    async fn subscribe_trades(
        &self,
        symbols: &[String],
        callback: Box<dyn Fn(TickData) + Send + Sync>,
        shutdown_rx: tokio::sync::broadcast::Receiver<()>,
    ) -> Result<(), ExchangeError> {
        if symbols.is_empty() {
            return Err(ExchangeError::InvalidSymbol(
                "No symbols provided".to_string(),
            ));
        }

        info!(
            "Starting Binance trade subscription for symbols: {:?}",
            symbols
        );

        // This will run indefinitely with reconnection logic
        self.handle_websocket_connection(symbols, callback, shutdown_rx.resubscribe())
            .await
    }

    async fn fetch_klines(
        &self,
        symbol: &str,
        interval: &str,
        limit: u32,
    ) -> Result<Vec<KlineData>, ExchangeError> {
        let base_url = self.rest_url(symbol);
        let path = self.kline_path(symbol);
        let url = format!(
            "{}{}?symbol={}&interval={}&limit={}",
            base_url, path, symbol, interval, limit
        );
        self.do_fetch_klines(&url, symbol).await
    }

    async fn fetch_klines_with_time(
        &self,
        symbol: &str,
        interval: &str,
        start_time: DateTime<Utc>,
        end_time: DateTime<Utc>,
        limit: u32,
    ) -> Result<Vec<KlineData>, ExchangeError> {
        let base_url = self.rest_url(symbol);
        let path = self.kline_path(symbol);
        let start_ms = start_time.timestamp_millis();
        let end_ms = end_time.timestamp_millis();
        let url = format!(
            "{}{}?symbol={}&interval={}&startTime={}&endTime={}&limit={}",
            base_url, path, symbol, interval, start_ms, end_ms, limit
        );
        self.do_fetch_klines(&url, symbol).await
    }

    /// Fetch K-line data for multiple timeframes with rate limiting
    ///
    /// Binance API limit: 20 requests/second
    /// We use 300ms delay between requests for safety
    async fn fetch_klines_multi_tf(
        &self,
        symbol: &str,
        timeframes: &[&str],
        start_time: DateTime<Utc>,
        end_time: DateTime<Utc>,
        limit: u32,
    ) -> Result<std::collections::HashMap<String, Vec<KlineData>>, ExchangeError> {
        use std::collections::HashMap;

        let mut result = HashMap::new();
        const RATE_LIMIT_MS: u64 = 300; // 300ms between requests

        for (i, &tf) in timeframes.iter().enumerate() {
            // Rate limiting (skip delay for first request)
            if i > 0 {
                tokio::time::sleep(std::time::Duration::from_millis(RATE_LIMIT_MS)).await;
            }

            match self.fetch_klines_with_time(symbol, tf, start_time, end_time, limit).await {
                Ok(klines) => {
                    tracing::debug!(
                        "[{}] Fetched {} {} klines",
                        symbol,
                        klines.len(),
                        tf
                    );
                    result.insert(tf.to_string(), klines);
                }
                Err(e) => {
                    // 单个 timeframe 失败不影响其他 timeframe
                    // 调用方通过检查 HashMap 中是否存在对应 key 来判断是否成功
                    tracing::warn!(
                        "[{}] Failed to fetch {} klines (will be missing from result): {}",
                        symbol,
                        tf,
                        e
                    );
                }
            }
        }

        Ok(result)
    }

    async fn fetch_funding_rate(
        &self,
        symbol: &str,
    ) -> Result<FundingRateData, ExchangeError> {
        let base_url = self.rest_url(symbol);
        // limit=1 获取最新一条资金费率
        // 资金费率每8小时结算一次（00:00, 08:00, 16:00 UTC）
        // 如果恰好在结算时间点调用，可能取到上一周期的数据
        // 对于采集服务（每小时调用一次）来说，这不会造成问题
        let url = format!("{}/fapi/v1/fundingRate?symbol={}&limit=1", base_url, symbol);

        let client = self.http_client.clone();
        let response = client.get(&url).send().await
            .map_err(|e| ExchangeError::NetworkError(format!("[{}] HTTP failed: {}", symbol, e)))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(ExchangeError::NetworkError(format!("[{}] HTTP {}: {}", symbol, status, body)));
        }

        let raw: Vec<serde_json::Value> = response.json().await
            .map_err(|e| ExchangeError::ParseError(format!("Parse funding rate: {}", e)))?;

        let item = raw.first()
            .ok_or_else(|| ExchangeError::ParseError(format!("[{}] No funding rate data", symbol)))?;

        let funding_rate = item["fundingRate"].as_str()
            .ok_or_else(|| ExchangeError::ParseError("Missing fundingRate".to_string()))?
            .parse::<Decimal>()
            .map_err(|e| ExchangeError::ParseError(format!("Invalid fundingRate: {}", e)))?;

        let funding_time_ms = item["fundingTime"].as_i64()
            .ok_or_else(|| ExchangeError::ParseError("Missing fundingTime".to_string()))?;

        let funding_time = DateTime::from_timestamp_millis(funding_time_ms)
            .ok_or_else(|| ExchangeError::ParseError("Invalid fundingTime".to_string()))?;

        let mark_price = item["markPrice"].as_str()
            .and_then(|s| s.parse::<Decimal>().ok());

        Ok(FundingRateData {
            symbol: symbol.to_string(),
            funding_rate,
            funding_time,
            mark_price,
        })
    }

    async fn fetch_funding_rate_history(
        &self,
        symbol: &str,
        start_time: Option<DateTime<Utc>>,
        end_time: Option<DateTime<Utc>>,
        limit: u32,
    ) -> Result<Vec<FundingRateData>, ExchangeError> {
        let base_url = self.rest_url(symbol);
        let mut url = format!("{}/fapi/v1/fundingRate?symbol={}&limit={}", base_url, symbol, limit);

        if let Some(start) = start_time {
            url.push_str(&format!("&startTime={}", start.timestamp_millis()));
        }
        if let Some(end) = end_time {
            url.push_str(&format!("&endTime={}", end.timestamp_millis()));
        }

        let client = self.http_client.clone();
        let response = client.get(&url).send().await
            .map_err(|e| ExchangeError::NetworkError(format!("[{}] HTTP failed: {}", symbol, e)))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(ExchangeError::NetworkError(format!("[{}] HTTP {}: {}", symbol, status, body)));
        }

        let raw: Vec<serde_json::Value> = response.json().await
            .map_err(|e| ExchangeError::ParseError(format!("Parse funding rate history: {}", e)))?;

        let mut result = Vec::new();
        for item in &raw {
            // 解析失败时跳过该条记录，不产生虚假数据
            let Some(funding_rate_str) = item["fundingRate"].as_str() else {
                warn!("[{}] Skipping funding rate record: missing fundingRate", symbol);
                continue;
            };
            let Ok(funding_rate) = funding_rate_str.parse::<Decimal>() else {
                warn!("[{}] Skipping funding rate record: invalid fundingRate: {}", symbol, funding_rate_str);
                continue;
            };
            let Some(funding_time_ms) = item["fundingTime"].as_i64() else {
                warn!("[{}] Skipping funding rate record: missing fundingTime", symbol);
                continue;
            };
            let Some(funding_time) = DateTime::from_timestamp_millis(funding_time_ms) else {
                warn!("[{}] Skipping funding rate record: invalid fundingTime: {}", symbol, funding_time_ms);
                continue;
            };
            let mark_price = item["markPrice"].as_str()
                .and_then(|s| s.parse::<Decimal>().ok());

            result.push(FundingRateData {
                symbol: symbol.to_string(),
                funding_rate,
                funding_time,
                mark_price,
            });
        }

        Ok(result)
    }

    async fn fetch_open_interest(
        &self,
        symbol: &str,
    ) -> Result<OpenInterestData, ExchangeError> {
        let base_url = self.rest_url(symbol);
        let url = format!("{}/fapi/v1/openInterest?symbol={}", base_url, symbol);

        let client = self.http_client.clone();
        let response = client.get(&url).send().await
            .map_err(|e| ExchangeError::NetworkError(format!("[{}] HTTP failed: {}", symbol, e)))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(ExchangeError::NetworkError(format!("[{}] HTTP {}: {}", symbol, status, body)));
        }

        let raw: serde_json::Value = response.json().await
            .map_err(|e| ExchangeError::ParseError(format!("Parse open interest: {}", e)))?;

        let open_interest = raw["openInterest"].as_str()
            .ok_or_else(|| ExchangeError::ParseError("Missing openInterest".to_string()))?
            .parse::<Decimal>()
            .map_err(|e| ExchangeError::ParseError(format!("Invalid openInterest: {}", e)))?;

        // 计算持仓价值 (openInterest * markPrice)
        let open_value = raw["sumOpenInterestValue"].as_str()
            .and_then(|s| s.parse::<Decimal>().ok());

        Ok(OpenInterestData {
            symbol: symbol.to_string(),
            open_interest,
            open_value,
            // Binance API 不返回 timestamp 字段，使用调用时间
            // 如果 API 调用有延迟，记录的时间戳会略有偏差（通常 <1秒）
            timestamp: Utc::now(),
        })
    }

    async fn fetch_long_short_ratio(
        &self,
        symbol: &str,
        period: &str,
        limit: u32,
    ) -> Result<Vec<LongShortRatioData>, ExchangeError> {
        let base_url = self.rest_url(symbol);
        let url = format!(
            "{}/futures/data/topLongShortAccountRatio?symbol={}&period={}&limit={}",
            base_url, symbol, period, limit
        );

        let client = self.http_client.clone();
        let response = client.get(&url).send().await
            .map_err(|e| ExchangeError::NetworkError(format!("[{}] HTTP failed: {}", symbol, e)))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(ExchangeError::NetworkError(format!("[{}] HTTP {}: {}", symbol, status, body)));
        }

        let raw: Vec<serde_json::Value> = response.json().await
            .map_err(|e| ExchangeError::ParseError(format!("Parse long short ratio: {}", e)))?;

        let mut result = Vec::new();
        for item in &raw {
            // 解析失败时跳过该条记录，不产生虚假数据
            let Some(long_ratio_str) = item["longAccount"].as_str() else {
                warn!("[{}] Skipping long/short ratio record: missing longAccount", symbol);
                continue;
            };
            let Some(short_ratio_str) = item["shortAccount"].as_str() else {
                warn!("[{}] Skipping long/short ratio record: missing shortAccount", symbol);
                continue;
            };
            let Some(ratio_str) = item["longShortRatio"].as_str() else {
                warn!("[{}] Skipping long/short ratio record: missing longShortRatio", symbol);
                continue;
            };
            let Some(timestamp_ms) = item["timestamp"].as_i64() else {
                warn!("[{}] Skipping long/short ratio record: missing timestamp", symbol);
                continue;
            };

            let Ok(long_ratio) = long_ratio_str.parse::<Decimal>() else {
                warn!("[{}] Skipping: invalid longAccount: {}", symbol, long_ratio_str);
                continue;
            };
            let Ok(short_ratio) = short_ratio_str.parse::<Decimal>() else {
                warn!("[{}] Skipping: invalid shortAccount: {}", symbol, short_ratio_str);
                continue;
            };
            let Ok(ratio) = ratio_str.parse::<Decimal>() else {
                warn!("[{}] Skipping: invalid longShortRatio: {}", symbol, ratio_str);
                continue;
            };
            let Some(timestamp) = DateTime::from_timestamp_millis(timestamp_ms) else {
                warn!("[{}] Skipping: invalid timestamp: {}", symbol, timestamp_ms);
                continue;
            };

            result.push(LongShortRatioData {
                symbol: symbol.to_string(),
                long_ratio,
                short_ratio,
                ratio,
                timestamp,
            });
        }

        Ok(result)
    }

    async fn fetch_order_book(
        &self,
        symbol: &str,
        limit: u32,
    ) -> Result<OrderBookData, ExchangeError> {
        let base_url = self.rest_url(symbol);
        let url = format!("{}/fapi/v1/depth?symbol={}&limit={}", base_url, symbol, limit);

        let client = self.http_client.clone();
        let response = client.get(&url).send().await
            .map_err(|e| ExchangeError::NetworkError(format!("[{}] HTTP failed: {}", symbol, e)))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(ExchangeError::NetworkError(format!("[{}] HTTP {}: {}", symbol, status, body)));
        }

        let raw: serde_json::Value = response.json().await
            .map_err(|e| ExchangeError::ParseError(format!("Parse order book: {}", e)))?;

        let parse_levels = |key: &str| -> Vec<(Decimal, Decimal)> {
            raw[key].as_array()
                .map(|levels| {
                    levels.iter()
                        .filter_map(|level| {
                            let price = level[0].as_str()?.parse::<Decimal>().ok()?;
                            let qty = level[1].as_str()?.parse::<Decimal>().ok()?;
                            Some((price, qty))
                        })
                        .collect()
                })
                .unwrap_or_default()
        };

        Ok(OrderBookData {
            symbol: symbol.to_string(),
            bids: parse_levels("bids"),
            asks: parse_levels("asks"),
            timestamp: Utc::now(),
        })
    }

    async fn fetch_large_trades(
        &self,
        symbol: &str,
        min_quote_qty: Decimal,
        limit: u32,
    ) -> Result<Vec<LargeTradeData>, ExchangeError> {
        let base_url = self.rest_url(symbol);
        let url = format!("{}/fapi/v1/aggTrades?symbol={}&limit={}", base_url, symbol, limit);

        let client = self.http_client.clone();
        let response = client.get(&url).send().await
            .map_err(|e| ExchangeError::NetworkError(format!("[{}] HTTP failed: {}", symbol, e)))?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(ExchangeError::NetworkError(format!("[{}] HTTP {}: {}", symbol, status, body)));
        }

        let raw: Vec<serde_json::Value> = response.json().await
            .map_err(|e| ExchangeError::ParseError(format!("Parse agg trades: {}", e)))?;

        let mut result = Vec::new();
        for item in &raw {
            let price = item["p"].as_str()
                .and_then(|s| s.parse::<Decimal>().ok())
                .unwrap_or_default();
            let quantity = item["q"].as_str()
                .and_then(|s| s.parse::<Decimal>().ok())
                .unwrap_or_default();
            let quote_qty = price * quantity;

            // 过滤大单
            if quote_qty < min_quote_qty {
                continue;
            }

            let side = if item["m"].as_bool().unwrap_or(false) { "SELL" } else { "BUY" };
            let timestamp_ms = item["T"].as_i64().unwrap_or(0);
            let timestamp = DateTime::from_timestamp_millis(timestamp_ms)
                .unwrap_or_default();

            result.push(LargeTradeData {
                symbol: symbol.to_string(),
                price,
                quantity,
                quote_qty,
                side: side.to_string(),
                timestamp,
            });
        }

        Ok(result)
    }
}

impl Default for BinanceExchange {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal::Decimal;
    use std::str::FromStr;
    use trading_common::data::types::TradeSide;

    #[test]
    fn test_parse_trade_message() {
        let exchange = BinanceExchange::new();

        // Test combined stream message format
        let stream_msg = r#"{
            "stream": "btcusdt@trade",
            "data": {
                "e": "trade",
                "E": 1672515782136,
                "s": "BTCUSDT",
                "t": 12345,
                "p": "50000.00",
                "q": "0.001",
                "b": 88,
                "a": 50,
                "T": 1672515782136,
                "m": false,
                "M": true
            }
        }"#;

        let tick_data = exchange.parse_trade_message(stream_msg).unwrap();

        assert_eq!(tick_data.symbol, "BTCUSDT");
        assert_eq!(tick_data.price, Decimal::from_str("50000.00").unwrap());
        assert_eq!(tick_data.quantity, Decimal::from_str("0.001").unwrap());
        assert_eq!(tick_data.side, TradeSide::Buy); // is_buyer_maker = false -> Buy
        assert_eq!(tick_data.trade_id, "12345");
        assert!(!tick_data.is_buyer_maker);
    }

    #[test]
    fn test_parse_direct_trade_message() {
        let exchange = BinanceExchange::new();

        // Test direct trade message format
        let trade_msg = r#"{
            "e": "trade",
            "E": 1672515782136,
            "s": "ETHUSDT",
            "t": 67890,
            "p": "3000.50",
            "q": "0.1",
            "b": 88,
            "a": 50,
            "T": 1672515782136,
            "m": true,
            "M": true
        }"#;

        let tick_data = exchange.parse_trade_message(trade_msg).unwrap();

        assert_eq!(tick_data.symbol, "ETHUSDT");
        assert_eq!(tick_data.price, Decimal::from_str("3000.50").unwrap());
        assert_eq!(tick_data.side, TradeSide::Sell); // is_buyer_maker = true -> Sell
        assert!(tick_data.is_buyer_maker);
    }

    #[test]
    fn test_parse_subscription_confirmation() {
        let exchange = BinanceExchange::new();

        let confirmation_msg = r#"{
            "result": null,
            "id": 1
        }"#;

        let result = exchange.parse_trade_message(confirmation_msg);
        assert!(result.is_err());

        // Should be a parse error indicating it's a control message
        if let Err(ExchangeError::ParseError(msg)) = result {
            assert!(msg.contains("Control message"));
        } else {
            panic!("Expected ParseError with control message indication");
        }
    }
}
