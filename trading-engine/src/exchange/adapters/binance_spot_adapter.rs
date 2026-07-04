// exchange/adapters/binance_spot_adapter.rs
// Binance 现货交易所适配器实现
// 基于 Spot REST API: /api/v3/...
// 与 BinanceAdapter (合约) 完全独立维护

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use futures_util::{SinkExt, StreamExt};
use hmac::{Hmac, Mac};
use reqwest::Client;
use rust_decimal::Decimal;
use sha2::Sha256;
use std::collections::HashMap;
use std::str::FromStr;
use std::time::Duration;
use tokio::sync::broadcast;
use tokio_tungstenite::{connect_async, tungstenite::Message};
use tracing::{error, info, warn};

use crate::exchange::errors::ExchangeError;
use crate::exchange::traits::{MarketDataProvider, TradingOperations, SymbolPrecision};
use crate::exchange::types::*;
use trading_common::data::types::TickData;

type HmacSha256 = Hmac<Sha256>;

/// Binance 现货配置
#[derive(Debug, Clone)]
pub struct BinanceSpotConfig {
    pub api_key: String,
    pub api_secret: String,
    pub testnet: bool,
    pub recv_window: u64,
    pub timeout: Duration,
}

impl Default for BinanceSpotConfig {
    fn default() -> Self {
        Self {
            api_key: String::new(),
            api_secret: String::new(),
            testnet: true,
            recv_window: 5000,
            timeout: Duration::from_secs(10),
        }
    }
}

/// Binance 现货适配器
pub struct BinanceSpotAdapter {
    config: BinanceSpotConfig,
    client: Client,
    base_url: String,
    ws_url: String,
}

impl BinanceSpotAdapter {
    /// 创建新的 Binance 现货适配器
    pub fn new(config: BinanceSpotConfig) -> Result<Self, ExchangeError> {
        let client = Client::builder()
            .timeout(config.timeout)
            .build()
            .map_err(|e| ExchangeError::NetworkError(format!("Failed to create HTTP client: {}", e)))?;

        let (base_url, ws_url) = if config.testnet {
            (
                "https://testnet.binance.vision".to_string(),
                "wss://testnet.binance.vision/ws".to_string(),
            )
        } else {
            (
                "https://api.binance.com".to_string(),
                "wss://stream.binance.com:9443/ws".to_string(),
            )
        };

        Ok(Self {
            config,
            client,
            base_url,
            ws_url,
        })
    }

    // ===== HTTP 基础设施 =====

    /// 生成 HMAC-SHA256 签名
    fn sign(&self, query_string: &str) -> Result<String, ExchangeError> {
        let mut mac = HmacSha256::new_from_slice(self.config.api_secret.as_bytes())
            .map_err(|e| ExchangeError::SignatureError(format!("Invalid key length: {}", e)))?;
        mac.update(query_string.as_bytes());
        let result = mac.finalize();
        Ok(hex::encode(result.into_bytes()))
    }

    /// 生成带签名的查询字符串
    fn create_signed_query(&self, params: &HashMap<String, String>) -> Result<String, ExchangeError> {
        let mut query_params = params.clone();
        query_params.insert("recvWindow".to_string(), self.config.recv_window.to_string());
        query_params.insert("timestamp".to_string(), Utc::now().timestamp_millis().to_string());

        let query_string: String = query_params
            .iter()
            .map(|(k, v)| format!("{}={}", k, v))
            .collect::<Vec<_>>()
            .join("&");

        let signature = self.sign(&query_string)?;
        Ok(format!("{}&signature={}", query_string, signature))
    }

    /// 发送签名 GET 请求
    async fn send_signed_request(
        &self,
        method: &str,
        endpoint: &str,
        params: &HashMap<String, String>,
    ) -> Result<serde_json::Value, ExchangeError> {
        let query_string = self.create_signed_query(params)?;
        let url = format!("{}{}?{}", self.base_url, endpoint, query_string);

        let response = match method {
            "GET" => self.client.get(&url).header("X-MBX-APIKEY", &self.config.api_key).send().await?,
            "DELETE" => self.client.delete(&url).header("X-MBX-APIKEY", &self.config.api_key).send().await?,
            _ => return Err(ExchangeError::InvalidOrder(format!("Unsupported method: {}", method))),
        };

        let status = response.status();
        let body = response.text().await?;

        if !status.is_success() {
            let error_response: serde_json::Value = serde_json::from_str(&body)
                .unwrap_or_else(|_| serde_json::json!({"msg": body}));
            let code = error_response["code"].as_i64().unwrap_or(-1);
            let message = error_response["msg"].as_str().unwrap_or("Unknown error").to_string();
            return Err(ExchangeError::ApiError { code, message });
        }

        serde_json::from_str(&body).map_err(|e| ExchangeError::ParseError(e.to_string()))
    }

    /// 发送签名 POST 请求 (form-urlencoded)
    async fn send_signed_form_request(
        &self,
        method: &str,
        endpoint: &str,
        params: &HashMap<String, String>,
    ) -> Result<serde_json::Value, ExchangeError> {
        let query_string = self.create_signed_query(params)?;
        let url = format!("{}{}", self.base_url, endpoint);

        let response = match method {
            "POST" => self.client
                .post(&url)
                .header("X-MBX-APIKEY", &self.config.api_key)
                .header("Content-Type", "application/x-www-form-urlencoded")
                .body(query_string)
                .send()
                .await?,
            "PUT" => self.client
                .put(&url)
                .header("X-MBX-APIKEY", &self.config.api_key)
                .header("Content-Type", "application/x-www-form-urlencoded")
                .body(query_string)
                .send()
                .await?,
            _ => return Err(ExchangeError::InvalidOrder(format!("Unsupported method: {}", method))),
        };

        let status = response.status();
        let body = response.text().await?;

        if !status.is_success() {
            let error_response: serde_json::Value = serde_json::from_str(&body)
                .unwrap_or_else(|_| serde_json::json!({"msg": body}));
            let code = error_response["code"].as_i64().unwrap_or(-1);
            let message = error_response["msg"].as_str().unwrap_or("Unknown error").to_string();
            return Err(ExchangeError::ApiError { code, message });
        }

        serde_json::from_str(&body).map_err(|e| ExchangeError::ParseError(e.to_string()))
    }

    /// 发送公开请求 (无需签名)
    async fn send_public_request(
        &self,
        endpoint: &str,
        params: &HashMap<String, String>,
    ) -> Result<serde_json::Value, ExchangeError> {
        let query_string: String = params
            .iter()
            .map(|(k, v)| format!("{}={}", k, v))
            .collect::<Vec<_>>()
            .join("&");

        let url = if query_string.is_empty() {
            format!("{}{}", self.base_url, endpoint)
        } else {
            format!("{}{}?{}", self.base_url, endpoint, query_string)
        };

        let response = self.client.get(&url).send().await?;
        let status = response.status();
        let body = response.text().await?;

        if !status.is_success() {
            return Err(ExchangeError::ApiError {
                code: status.as_u16() as i64,
                message: body,
            });
        }

        serde_json::from_str(&body).map_err(|e| ExchangeError::ParseError(e.to_string()))
    }

    /// 发送仅带 API Key 的请求 (listenKey 等)
    async fn send_apikey_request(
        &self,
        method: &str,
        endpoint: &str,
    ) -> Result<serde_json::Value, ExchangeError> {
        let url = format!("{}{}", self.base_url, endpoint);

        let response = match method {
            "POST" => self.client.post(&url).header("X-MBX-APIKEY", &self.config.api_key).send().await?,
            "PUT" => self.client.put(&url).header("X-MBX-APIKEY", &self.config.api_key).send().await?,
            "DELETE" => self.client.delete(&url).header("X-MBX-APIKEY", &self.config.api_key).send().await?,
            _ => return Err(ExchangeError::InvalidOrder(format!("Unsupported method: {}", method))),
        };

        let status = response.status();
        let body = response.text().await?;

        if !status.is_success() {
            let error_response: serde_json::Value = serde_json::from_str(&body)
                .unwrap_or_else(|_| serde_json::json!({"msg": body}));
            let code = error_response["code"].as_i64().unwrap_or(-1);
            let message = error_response["msg"].as_str().unwrap_or("Unknown error").to_string();
            return Err(ExchangeError::ApiError { code, message });
        }

        if body.is_empty() {
            return Ok(serde_json::json!({}));
        }

        serde_json::from_str(&body).map_err(|e| ExchangeError::ParseError(e.to_string()))
    }

    // ===== 解析辅助函数 =====

    /// 解析现货账户信息 (GET /api/v3/account)
    fn parse_spot_account(&self, data: &serde_json::Value) -> Result<AccountInfo, ExchangeError> {
        let balances: Vec<Balance> = data["balances"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|b| {
                        let asset = b["asset"].as_str()?.to_string();
                        let free = Decimal::from_str(b["free"].as_str()?).ok()?;
                        let locked = Decimal::from_str(b["locked"].as_str()?).ok()?;
                        if free > Decimal::ZERO || locked > Decimal::ZERO {
                            Some(Balance { asset, free, locked })
                        } else {
                            None
                        }
                    })
                    .collect()
            })
            .unwrap_or_default();

        // 现货总权益 = 所有资产 free + locked 之和 (简化)
        let total_equity: Decimal = balances.iter().map(|b| b.free + b.locked).sum();

        Ok(AccountInfo {
            balances,
            total_equity,
            available_balance: total_equity,
            unrealized_pnl: Decimal::ZERO,
            margin_used: Decimal::ZERO,
            margin_ratio: None,
        })
    }

    /// 解析现货订单信息
    fn parse_spot_order(&self, o: &serde_json::Value) -> Option<OrderInfo> {
        let order_id = o["orderId"].as_i64()?.to_string();
        let symbol = o["symbol"].as_str()?.to_string();

        let side = match o["side"].as_str()? {
            "BUY" => OrderSide::Buy,
            "SELL" => OrderSide::Sell,
            _ => return None,
        };

        let order_type = match o["type"].as_str()? {
            "LIMIT" => OrderType::Limit,
            "MARKET" => OrderType::Market,
            "STOP_LOSS" => OrderType::StopLoss,
            "STOP_LOSS_LIMIT" => OrderType::StopLossLimit,
            "TAKE_PROFIT" => OrderType::TakeProfit,
            "TAKE_PROFIT_LIMIT" => OrderType::TakeProfitLimit,
            "LIMIT_MAKER" => OrderType::LimitMaker,
            _ => return None,
        };

        let status = match o["status"].as_str()? {
            "NEW" => OrderStatus::New,
            "PARTIALLY_FILLED" => OrderStatus::PartiallyFilled,
            "FILLED" => OrderStatus::Filled,
            "CANCELED" => OrderStatus::Canceled,
            "PENDING_CANCEL" => OrderStatus::PendingCancel,
            "REJECTED" => OrderStatus::Rejected,
            "EXPIRED" => OrderStatus::Expired,
            _ => return None,
        };

        let time_in_force = match o["timeInForce"].as_str() {
            Some("GTC") => TimeInForce::Gtc,
            Some("IOC") => TimeInForce::Ioc,
            Some("FOK") => TimeInForce::Fok,
            _ => TimeInForce::Gtc,
        };

        Some(OrderInfo {
            order_id,
            client_order_id: o["clientOrderId"].as_str().map(|s| s.to_string()),
            symbol,
            side,
            order_type,
            status,
            quantity: Decimal::from_str(o["origQty"].as_str()?).ok()?,
            filled_quantity: Decimal::from_str(o["executedQty"].as_str().unwrap_or("0")).unwrap_or_default(),
            remaining_quantity: Decimal::from_str(o["origQty"].as_str()?).unwrap_or_default()
                - Decimal::from_str(o["executedQty"].as_str().unwrap_or("0")).unwrap_or_default(),
            price: o["price"].as_str().and_then(|s| Decimal::from_str(s).ok()),
            stop_price: o["stopPrice"].as_str().and_then(|s| Decimal::from_str(s).ok()).filter(|d| *d > Decimal::ZERO),
            time_in_force,
            created_at: DateTime::from_timestamp_millis(o["time"].as_i64()?)?,
            updated_at: DateTime::from_timestamp_millis(o["updateTime"].as_i64()?)?,
        })
    }
}

/// MarketDataProvider 实现 - 只读市场数据接口
#[async_trait]
impl MarketDataProvider for BinanceSpotAdapter {
    // ===== 元信息 =====

    fn exchange_id(&self) -> &str {
        "binance-spot"
    }

    fn is_testnet(&self) -> bool {
        self.config.testnet
    }

    /// GET /api/v3/time
    async fn get_server_time(&self) -> Result<DateTime<Utc>, ExchangeError> {
        let data = self.send_public_request("/api/v3/time", &HashMap::new()).await?;
        let timestamp = data["serverTime"]
            .as_i64()
            .ok_or_else(|| ExchangeError::ParseError("Missing serverTime".to_string()))?;

        DateTime::from_timestamp_millis(timestamp)
            .ok_or_else(|| ExchangeError::ParseError("Invalid timestamp".to_string()))
    }

    /// GET /api/v3/exchangeInfo - 现货交易对精度
    async fn get_symbol_precision(&self, symbol: &str) -> Result<SymbolPrecision, ExchangeError> {
        let mut params = HashMap::new();
        params.insert("symbol".to_string(), symbol.to_string());

        let data = self.send_public_request("/api/v3/exchangeInfo", &params).await?;

        let symbols = data["symbols"]
            .as_array()
            .ok_or_else(|| ExchangeError::ParseError("Missing symbols array".to_string()))?;

        let symbol_info = symbols
            .iter()
            .find(|s| s["symbol"].as_str() == Some(symbol))
            .ok_or_else(|| ExchangeError::InvalidSymbol(format!("Symbol not found: {}", symbol)))?;

        Ok(SymbolPrecision {
            symbol: symbol.to_string(),
            base_asset_precision: symbol_info["baseAssetPrecision"].as_u64().unwrap_or(8) as u32,
            quote_asset_precision: symbol_info["quoteAssetPrecision"].as_u64().unwrap_or(8) as u32,
            min_quantity: symbol_info["filters"]
                .as_array()
                .and_then(|filters| {
                    filters.iter()
                        .find(|f| f["filterType"].as_str() == Some("LOT_SIZE"))
                        .and_then(|f| f["minQty"].as_str()?.parse().ok())
                })
                .unwrap_or_else(|| Decimal::from(1)),
            max_quantity: symbol_info["filters"]
                .as_array()
                .and_then(|filters| {
                    filters.iter()
                        .find(|f| f["filterType"].as_str() == Some("LOT_SIZE"))
                        .and_then(|f| f["maxQty"].as_str()?.parse().ok())
                })
                .unwrap_or_else(|| Decimal::from(1000000)),
            min_notional: symbol_info["filters"]
                .as_array()
                .and_then(|filters| {
                    // Spot 用 MIN_NOTIONAL 或 NOTIONAL
                    filters.iter()
                        .find(|f| f["filterType"].as_str() == Some("NOTIONAL")
                            || f["filterType"].as_str() == Some("MIN_NOTIONAL"))
                        .and_then(|f| f["minNotional"].as_str()?.parse().ok())
                })
                .unwrap_or_else(|| Decimal::from(5)),
            step_size: symbol_info["filters"]
                .as_array()
                .and_then(|filters| {
                    filters.iter()
                        .find(|f| f["filterType"].as_str() == Some("LOT_SIZE"))
                        .and_then(|f| f["stepSize"].as_str()?.parse().ok())
                })
                .unwrap_or_else(|| Decimal::from(1)),
            tick_size: symbol_info["filters"]
                .as_array()
                .and_then(|filters| {
                    filters.iter()
                        .find(|f| f["filterType"].as_str() == Some("PRICE_FILTER"))
                        .and_then(|f| f["tickSize"].as_str()?.parse().ok())
                })
                .unwrap_or_else(|| Decimal::from(1)),
        })
    }

    // ===== 行情数据接口 =====

    /// GET /api/v3/ticker/24hr - 获取行情快照
    async fn get_ticker(&self, symbol: &str) -> Result<Ticker, ExchangeError> {
        let mut params = HashMap::new();
        params.insert("symbol".to_string(), symbol.to_string());

        let data = self.send_public_request("/api/v3/ticker/24hr", &params).await?;

        Ok(Ticker {
            symbol: data["symbol"].as_str().unwrap_or(symbol).to_string(),
            last_price: Decimal::from_str(data["lastPrice"].as_str().unwrap_or("0")).unwrap_or_default(),
            bid_price: Decimal::from_str(data["bidPrice"].as_str().unwrap_or("0")).unwrap_or_default(),
            ask_price: Decimal::from_str(data["askPrice"].as_str().unwrap_or("0")).unwrap_or_default(),
            high_price: Decimal::from_str(data["highPrice"].as_str().unwrap_or("0")).unwrap_or_default(),
            low_price: Decimal::from_str(data["lowPrice"].as_str().unwrap_or("0")).unwrap_or_default(),
            volume: Decimal::from_str(data["volume"].as_str().unwrap_or("0")).unwrap_or_default(),
            quote_volume: Decimal::from_str(data["quoteVolume"].as_str().unwrap_or("0")).unwrap_or_default(),
            price_change: Decimal::from_str(data["priceChange"].as_str().unwrap_or("0")).unwrap_or_default(),
            price_change_percent: Decimal::from_str(data["priceChangePercent"].as_str().unwrap_or("0")).unwrap_or_default(),
            timestamp: Utc::now(),
        })
    }

    /// GET /api/v3/ticker/24hr - 批量获取行情快照
    async fn get_tickers(&self, symbols: &[String]) -> Result<Vec<Ticker>, ExchangeError> {
        let params = HashMap::new();
        let data = self.send_public_request("/api/v3/ticker/24hr", &params).await?;

        let all_tickers: Vec<Ticker> = data
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|item| {
                        let sym = item["symbol"].as_str()?;
                        if !symbols.iter().any(|s| s == sym) {
                            return None;
                        }
                        Some(Ticker {
                            symbol: sym.to_string(),
                            last_price: Decimal::from_str(item["lastPrice"].as_str()?).ok()?,
                            bid_price: Decimal::from_str(item["bidPrice"].as_str()?).ok()?,
                            ask_price: Decimal::from_str(item["askPrice"].as_str()?).ok()?,
                            high_price: Decimal::from_str(item["highPrice"].as_str()?).ok()?,
                            low_price: Decimal::from_str(item["lowPrice"].as_str()?).ok()?,
                            volume: Decimal::from_str(item["volume"].as_str()?).ok()?,
                            quote_volume: Decimal::from_str(item["quoteVolume"].as_str()?).ok()?,
                            price_change: Decimal::from_str(item["priceChange"].as_str()?).ok()?,
                            price_change_percent: Decimal::from_str(item["priceChangePercent"].as_str()?).ok()?,
                            timestamp: Utc::now(),
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();

        Ok(all_tickers)
    }

    async fn get_mark_price(&self, _symbol: &str) -> Result<MarkPrice, ExchangeError> {
        Err(ExchangeError::ConfigError("Mark price not available for Spot".to_string()))
    }

    async fn get_funding_rate(&self, _symbol: &str, _limit: Option<u32>) -> Result<Vec<FundingRate>, ExchangeError> {
        Err(ExchangeError::ConfigError("Funding rate not available for Spot".to_string()))
    }

    /// GET /api/v3/klines - K线数据
    async fn get_klines(&self, symbol: &str, interval: &str, limit: Option<u32>) -> Result<Vec<Kline>, ExchangeError> {
        let mut params = HashMap::new();
        params.insert("symbol".to_string(), symbol.to_string());
        params.insert("interval".to_string(), interval.to_string());
        if let Some(l) = limit {
            params.insert("limit".to_string(), l.to_string());
        }

        let data = self.send_public_request("/api/v3/klines", &params).await?;

        let klines = data
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|k| {
                        let k_arr = k.as_array()?;
                        if k_arr.len() < 12 { return None; }
                        Some(Kline {
                            open_time: DateTime::from_timestamp_millis(k_arr[0].as_i64()?)?,
                            open: Decimal::from_str(k_arr[1].as_str()?).ok()?,
                            high: Decimal::from_str(k_arr[2].as_str()?).ok()?,
                            low: Decimal::from_str(k_arr[3].as_str()?).ok()?,
                            close: Decimal::from_str(k_arr[4].as_str()?).ok()?,
                            volume: Decimal::from_str(k_arr[5].as_str()?).ok()?,
                            close_time: DateTime::from_timestamp_millis(k_arr[6].as_i64()?)?,
                            quote_volume: Decimal::from_str(k_arr[7].as_str()?).ok()?,
                            trades_count: k_arr[8].as_u64().unwrap_or(0),
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();

        Ok(klines)
    }

    /// GET /api/v3/depth - 订单簿深度
    async fn get_order_book(&self, symbol: &str, limit: Option<u32>) -> Result<OrderBook, ExchangeError> {
        let mut params = HashMap::new();
        params.insert("symbol".to_string(), symbol.to_string());
        if let Some(l) = limit {
            params.insert("limit".to_string(), l.to_string());
        }

        let data = self.send_public_request("/api/v3/depth", &params).await?;

        let parse_entries = |arr: &serde_json::Value| -> Vec<OrderBookEntry> {
            arr.as_array()
                .map(|a| {
                    a.iter()
                        .filter_map(|entry| {
                            let e = entry.as_array()?;
                            Some(OrderBookEntry {
                                price: Decimal::from_str(e[0].as_str()?).ok()?,
                                quantity: Decimal::from_str(e[1].as_str()?).ok()?,
                            })
                        })
                        .collect()
                })
                .unwrap_or_default()
        };

        Ok(OrderBook {
            symbol: symbol.to_string(),
            bids: parse_entries(&data["bids"]),
            asks: parse_entries(&data["asks"]),
            last_update_id: data["lastUpdateId"].as_u64().unwrap_or(0),
        })
    }

    /// GET /api/v3/trades - 获取最近成交
    async fn get_recent_trades(&self, symbol: &str, limit: Option<u32>) -> Result<Vec<PublicTrade>, ExchangeError> {
        let mut params = HashMap::new();
        params.insert("symbol".to_string(), symbol.to_string());
        if let Some(l) = limit {
            params.insert("limit".to_string(), l.to_string());
        }

        let data = self.send_public_request("/api/v3/trades", &params).await?;

        let trades: Vec<PublicTrade> = data
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|t| {
                        Some(PublicTrade {
                            id: t["id"].as_i64()?.to_string(),
                            symbol: symbol.to_string(),
                            price: Decimal::from_str(t["price"].as_str()?).ok()?,
                            quantity: Decimal::from_str(t["qty"].as_str()?).ok()?,
                            timestamp: DateTime::from_timestamp_millis(t["time"].as_i64()?)?,
                            is_buyer_maker: t["isBuyerMaker"].as_bool()?,
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();

        Ok(trades)
    }

    // ===== WebSocket 行情接口 =====

    /// WebSocket: {symbol}@trade
    async fn subscribe_trades(
        &self,
        symbols: &[String],
        callback: Box<dyn Fn(TickData) + Send + Sync>,
        mut shutdown_rx: broadcast::Receiver<()>,
    ) -> Result<(), ExchangeError> {
        let streams: Vec<String> = symbols
            .iter()
            .map(|s| format!("{}@trade", s.to_lowercase()))
            .collect();

        let stream_name = streams.join("/");
        let url = format!("{}/{}", self.ws_url, stream_name);

        info!("Connecting to Binance Spot WebSocket: {}", url);

        let (mut ws_stream, _) = connect_async(&url)
            .await
            .map_err(|e| ExchangeError::WebSocketError(format!("Connection failed: {}", e)))?;

        info!("Connected to Binance Spot WebSocket");

        loop {
            tokio::select! {
                msg = ws_stream.next() => {
                    match msg {
                        Some(Ok(Message::Text(text))) => {
                            match serde_json::from_str::<serde_json::Value>(&text) {
                                Ok(data) => {
                                    if let Some(trade) = parse_spot_trade_data(&data) {
                                        callback(trade);
                                    }
                                }
                                Err(e) => warn!("Failed to parse trade data: {}", e),
                            }
                        }
                        Some(Ok(Message::Ping(data))) => {
                            let _ = ws_stream.send(Message::Pong(data)).await;
                        }
                        Some(Ok(Message::Close(_))) => {
                            info!("WebSocket connection closed by server");
                            break;
                        }
                        Some(Err(e)) => {
                            error!("WebSocket error: {}", e);
                            return Err(ExchangeError::WebSocketError(e.to_string()));
                        }
                        None => {
                            info!("WebSocket stream ended");
                            break;
                        }
                        _ => {}
                    }
                }
                _ = shutdown_rx.recv() => {
                    info!("Shutdown signal received, closing WebSocket");
                    let _ = ws_stream.close(None).await;
                    break;
                }
            }
        }

        Ok(())
    }
}

/// TradingOperations 实现 - 交易操作接口
#[async_trait]
impl TradingOperations for BinanceSpotAdapter {
    // ===== 账户接口 =====

    /// GET /api/v3/account - 现货账户信息
    async fn get_account(&self) -> Result<AccountInfo, ExchangeError> {
        let params = HashMap::new();
        let data = self.send_signed_request("GET", "/api/v3/account", &params).await?;
        self.parse_spot_account(&data)
    }

    async fn get_futures_account(&self) -> Result<FuturesAccountInfo, ExchangeError> {
        Err(ExchangeError::ConfigError("Not a futures account".to_string()))
    }

    /// 现货无持仓概念，从余额推算
    async fn get_position(&self, symbol: &str) -> Result<PositionInfo, ExchangeError> {
        let account = self.get_account().await?;
        let base_asset = symbol.replace("USDT", "").replace("BUSD", "").replace("FDUSD", "");

        let balance = account.balances.iter().find(|b| b.asset == base_asset);
        let quantity = balance.map(|b| b.free + b.locked).unwrap_or(Decimal::ZERO);

        Ok(PositionInfo {
            symbol: symbol.to_string(),
            side: if quantity > Decimal::ZERO { PositionSide::Long } else { PositionSide::None },
            quantity,
            avg_entry_price: Decimal::ZERO,
            mark_price: None,
            unrealized_pnl: Decimal::ZERO,
            leverage: 1,
            margin: Decimal::ZERO,
            liquidation_price: None,
        })
    }

    /// 现货: 遍历非零余额作为 "持仓"
    async fn get_positions(&self) -> Result<Vec<PositionInfo>, ExchangeError> {
        let account = self.get_account().await?;

        let positions = account.balances.iter().map(|b| {
            let quantity = b.free + b.locked;
            PositionInfo {
                symbol: format!("{}USDT", b.asset),
                side: if quantity > Decimal::ZERO { PositionSide::Long } else { PositionSide::None },
                quantity,
                avg_entry_price: Decimal::ZERO,
                mark_price: None,
                unrealized_pnl: Decimal::ZERO,
                leverage: 1,
                margin: Decimal::ZERO,
                liquidation_price: None,
            }
        }).collect();

        Ok(positions)
    }

    // ===== 订单接口 =====

    /// POST /api/v3/order - 现货下单
    async fn place_order(&self, order: OrderRequest) -> Result<OrderResult, ExchangeError> {
        let mut params = HashMap::new();
        params.insert("symbol".to_string(), order.symbol.clone());
        params.insert("side".to_string(), order.side.to_string());
        params.insert("type".to_string(), order.order_type.to_string());

        // MARKET 买单可以用 quoteOrderQty (按 USDT 金额买)
        // 其他类型用 quantity
        params.insert("quantity".to_string(), order.quantity.to_string());

        if let Some(price) = order.price {
            params.insert("price".to_string(), price.to_string());
        }

        if let Some(stop_price) = order.stop_price {
            params.insert("stopPrice".to_string(), stop_price.to_string());
        }

        if let Some(time_in_force) = order.time_in_force {
            params.insert("timeInForce".to_string(), time_in_force.to_string());
        } else if order.order_type == OrderType::Limit {
            // LIMIT 订单必须指定 timeInForce
            params.insert("timeInForce".to_string(), "GTC".to_string());
        }

        if let Some(client_order_id) = order.client_order_id {
            params.insert("newClientOrderId".to_string(), client_order_id);
        }

        params.insert("newOrderRespType".to_string(), "RESULT".to_string());

        let data = self.send_signed_form_request("POST", "/api/v3/order", &params).await?;

        Ok(OrderResult {
            order_id: data["orderId"].as_i64().unwrap_or(0).to_string(),
            client_order_id: data["clientOrderId"].as_str().map(|s| s.to_string()),
            symbol: data["symbol"].as_str().unwrap_or(&order.symbol).to_string(),
            side: order.side,
            order_type: order.order_type,
            status: match data["status"].as_str() {
                Some("NEW") => OrderStatus::New,
                Some("FILLED") => OrderStatus::Filled,
                Some("PARTIALLY_FILLED") => OrderStatus::PartiallyFilled,
                Some("CANCELED") => OrderStatus::Canceled,
                Some("REJECTED") => OrderStatus::Rejected,
                Some("EXPIRED") => OrderStatus::Expired,
                _ => OrderStatus::New,
            },
            quantity: order.quantity,
            filled_quantity: Decimal::from_str(data["executedQty"].as_str().unwrap_or("0")).unwrap_or_default(),
            price: order.price,
            avg_price: calculate_avg_price(&data),
            commission: extract_commission(&data),
            commission_asset: data["fills"].as_array().and_then(|f| f.first()).and_then(|f| f["commissionAsset"].as_str()).map(|s| s.to_string()),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        })
    }

    /// DELETE /api/v3/order - 撤单
    async fn cancel_order(&self, symbol: &str, order_id: &str) -> Result<(), ExchangeError> {
        let mut params = HashMap::new();
        params.insert("symbol".to_string(), symbol.to_string());
        params.insert("orderId".to_string(), order_id.to_string());

        self.send_signed_request("DELETE", "/api/v3/order", &params).await?;
        Ok(())
    }

    /// DELETE /api/v3/openOrders - 撤销所有未成交订单
    async fn cancel_all_orders(&self, symbol: Option<&str>) -> Result<(), ExchangeError> {
        if let Some(s) = symbol {
            let mut params = HashMap::new();
            params.insert("symbol".to_string(), s.to_string());
            self.send_signed_request("DELETE", "/api/v3/openOrders", &params).await?;
        } else {
            return Err(ExchangeError::InvalidOrder("Symbol is required for cancel_all_orders".to_string()));
        }
        Ok(())
    }

    /// GET /api/v3/openOrders - 当前未成交订单
    async fn get_open_orders(&self, symbol: Option<&str>) -> Result<Vec<OrderInfo>, ExchangeError> {
        let mut params = HashMap::new();
        if let Some(s) = symbol {
            params.insert("symbol".to_string(), s.to_string());
        }

        let data = self.send_signed_request("GET", "/api/v3/openOrders", &params).await?;

        let orders = data
            .as_array()
            .map(|arr| arr.iter().filter_map(|o| self.parse_spot_order(o)).collect())
            .unwrap_or_default();

        Ok(orders)
    }

    /// GET /api/v3/order - 查询订单
    async fn get_order(&self, symbol: &str, order_id: &str) -> Result<OrderInfo, ExchangeError> {
        let mut params = HashMap::new();
        params.insert("symbol".to_string(), symbol.to_string());
        params.insert("orderId".to_string(), order_id.to_string());

        let data = self.send_signed_request("GET", "/api/v3/order", &params).await?;

        self.parse_spot_order(&data)
            .ok_or_else(|| ExchangeError::OrderNotFound(order_id.to_string()))
    }

    /// GET /api/v3/allOrders - 所有订单
    async fn get_all_orders(&self, symbol: &str, limit: Option<u32>) -> Result<Vec<OrderInfo>, ExchangeError> {
        let mut params = HashMap::new();
        params.insert("symbol".to_string(), symbol.to_string());
        if let Some(l) = limit {
            params.insert("limit".to_string(), l.to_string());
        }

        let data = self.send_signed_request("GET", "/api/v3/allOrders", &params).await?;

        let orders = data
            .as_array()
            .map(|arr| arr.iter().filter_map(|o| self.parse_spot_order(o)).collect())
            .unwrap_or_default();

        Ok(orders)
    }

    /// GET /api/v3/myTrades - 成交历史
    async fn get_trade_history(&self, symbol: &str, limit: Option<u32>) -> Result<Vec<TradeInfo>, ExchangeError> {
        let mut params = HashMap::new();
        params.insert("symbol".to_string(), symbol.to_string());
        if let Some(l) = limit {
            params.insert("limit".to_string(), l.to_string());
        }

        let data = self.send_signed_request("GET", "/api/v3/myTrades", &params).await?;

        let trades = data
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|t| {
                        Some(TradeInfo {
                            id: t["id"].as_i64()?.to_string(),
                            symbol: t["symbol"].as_str()?.to_string(),
                            price: Decimal::from_str(t["price"].as_str()?).ok()?,
                            quantity: Decimal::from_str(t["qty"].as_str()?).ok()?,
                            quote_quantity: Decimal::from_str(t["quoteQty"].as_str().unwrap_or("0")).unwrap_or_default(),
                            commission: Decimal::from_str(t["commission"].as_str().unwrap_or("0")).unwrap_or_default(),
                            commission_asset: t["commissionAsset"].as_str().unwrap_or("").to_string(),
                            time: DateTime::from_timestamp_millis(t["time"].as_i64()?)?,
                            is_buyer: t["isBuyer"].as_bool().unwrap_or(false),
                            is_maker: t["isMaker"].as_bool().unwrap_or(false),
                            realized_pnl: Decimal::ZERO, // 现货无 realizedPnl
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();

        Ok(trades)
    }

    /// 现货无批量下单端点，逐个调用
    async fn batch_place_orders(&self, orders: Vec<BatchOrderRequest>) -> Result<Vec<BatchOrderResult>, ExchangeError> {
        let mut results = Vec::new();
        for order in orders {
            let result = self.place_order(OrderRequest {
                symbol: order.symbol.clone(),
                side: order.side,
                order_type: order.order_type,
                quantity: order.quantity,
                price: order.price,
                stop_price: order.stop_price,
                time_in_force: order.time_in_force,
                client_order_id: order.client_order_id,
            }).await;

            match result {
                Ok(r) => results.push(BatchOrderResult {
                    order_id: r.order_id,
                    client_order_id: r.client_order_id,
                    symbol: r.symbol,
                    status: r.status,
                    error_code: None,
                    error_message: None,
                }),
                Err(e) => results.push(BatchOrderResult {
                    order_id: String::new(),
                    client_order_id: None,
                    symbol: order.symbol,
                    status: OrderStatus::Rejected,
                    error_code: Some(-1),
                    error_message: Some(e.to_string()),
                }),
            }
        }
        Ok(results)
    }

    /// 现货无批量撤单端点，逐个调用
    async fn batch_cancel_orders(&self, symbol: &str, order_ids: Vec<String>) -> Result<Vec<BatchOrderResult>, ExchangeError> {
        let mut results = Vec::new();
        for order_id in order_ids {
            match self.cancel_order(symbol, &order_id).await {
                Ok(_) => results.push(BatchOrderResult {
                    order_id: order_id.clone(),
                    client_order_id: None,
                    symbol: symbol.to_string(),
                    status: OrderStatus::Canceled,
                    error_code: None,
                    error_message: None,
                }),
                Err(e) => results.push(BatchOrderResult {
                    order_id: order_id.clone(),
                    client_order_id: None,
                    symbol: symbol.to_string(),
                    status: OrderStatus::Rejected,
                    error_code: Some(-1),
                    error_message: Some(e.to_string()),
                }),
            }
        }
        Ok(results)
    }

    // ===== 合约交易接口 (现货不支持) =====

    async fn set_leverage(&self, _symbol: &str, _leverage: u32) -> Result<(), ExchangeError> {
        Err(ExchangeError::ConfigError("Spot trading does not support leverage".to_string()))
    }

    async fn set_margin_type(&self, _symbol: &str, _margin_type: MarginType) -> Result<(), ExchangeError> {
        Err(ExchangeError::ConfigError("Spot trading does not support margin type".to_string()))
    }

    // ===== 用户数据流 =====

    /// POST /api/v3/listenKey → WebSocket executionReport
    async fn subscribe_user_data(
        &self,
        order_callback: Box<dyn Fn(OrderUpdate) + Send + Sync>,
        mut shutdown_rx: broadcast::Receiver<()>,
    ) -> Result<(), ExchangeError> {
        // 1. 创建 listenKey
        let data = self.send_apikey_request("POST", "/api/v3/listenKey").await?;
        let listen_key = data["listenKey"]
            .as_str()
            .ok_or_else(|| ExchangeError::ParseError("Missing listenKey".to_string()))?
            .to_string();

        info!("Got Spot listenKey: {}...", &listen_key[..8.min(listen_key.len())]);

        // 2. 连接 WebSocket
        let ws_url = format!("{}/{}", self.ws_url, listen_key);
        let (mut ws_stream, _) = connect_async(&ws_url)
            .await
            .map_err(|e| ExchangeError::WebSocketError(format!("Connection failed: {}", e)))?;

        info!("Connected to Binance Spot user data stream");

        // 3. 定期延长 listenKey (每 30 分钟)
        let client = self.client.clone();
        let base_url = self.base_url.clone();
        let api_key = self.config.api_key.clone();
        let keepalive_handle = tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(30 * 60));
            loop {
                interval.tick().await;
                let url = format!("{}/api/v3/listenKey", base_url);
                match client.put(&url).header("X-MBX-APIKEY", &api_key).send().await {
                    Ok(resp) if resp.status().is_success() => info!("Spot listenKey keepalive succeeded"),
                    Ok(resp) => warn!("Spot listenKey keepalive failed: {}", resp.status()),
                    Err(e) => warn!("Spot listenKey keepalive error: {}", e),
                }
            }
        });

        // 4. 处理消息
        loop {
            tokio::select! {
                msg = ws_stream.next() => {
                    match msg {
                        Some(Ok(Message::Text(text))) => {
                            match serde_json::from_str::<serde_json::Value>(&text) {
                                Ok(data) => {
                                    let event_type = data["e"].as_str().unwrap_or("");
                                    match event_type {
                                        "executionReport" => {
                                            if let Some(update) = parse_spot_order_update(&data) {
                                                order_callback(update);
                                            }
                                        }
                                        "outboundAccountPosition" => {
                                            info!("Spot account position update received");
                                        }
                                        _ => {
                                            info!("Spot user data event: {}", event_type);
                                        }
                                    }
                                }
                                Err(e) => warn!("Failed to parse spot user data: {}", e),
                            }
                        }
                        Some(Ok(Message::Ping(data))) => {
                            let _ = ws_stream.send(Message::Pong(data)).await;
                        }
                        Some(Ok(Message::Close(_))) => {
                            info!("Spot user data WebSocket closed");
                            break;
                        }
                        Some(Err(e)) => {
                            error!("Spot user data WebSocket error: {}", e);
                            return Err(ExchangeError::WebSocketError(e.to_string()));
                        }
                        None => {
                            info!("Spot user data WebSocket stream ended");
                            break;
                        }
                        _ => {}
                    }
                }
                _ = shutdown_rx.recv() => {
                    info!("Shutdown signal received, closing Spot user data stream");
                    let _ = ws_stream.close(None).await;
                    break;
                }
            }
        }

        keepalive_handle.abort();
        let _ = self.send_apikey_request("DELETE", "/api/v3/listenKey").await;

        Ok(())
    }
}

// ===== 辅助函数 =====

/// 解析现货 WebSocket 交易数据
fn parse_spot_trade_data(data: &serde_json::Value) -> Option<TickData> {
    let symbol = data["s"].as_str()?;
    let price = data["p"].as_str()?.parse::<Decimal>().ok()?;
    let quantity = data["q"].as_str()?.parse::<Decimal>().ok()?;
    let trade_id = data["t"].as_i64()?.to_string();
    let timestamp = DateTime::from_timestamp_millis(data["T"].as_i64()?)?;
    let is_buyer_maker = data["m"].as_bool()?;

    Some(TickData {
        timestamp,
        symbol: symbol.to_string(),
        price,
        quantity,
        side: if is_buyer_maker {
            trading_common::data::types::TradeSide::Sell
        } else {
            trading_common::data::types::TradeSide::Buy
        },
        trade_id,
        is_buyer_maker,
    })
}

/// 解析现货 WebSocket executionReport 事件
fn parse_spot_order_update(data: &serde_json::Value) -> Option<OrderUpdate> {
    let symbol = data["s"].as_str()?.to_string();
    let side = match data["S"].as_str()? {
        "BUY" => OrderSide::Buy,
        "SELL" => OrderSide::Sell,
        _ => return None,
    };
    let order_type = match data["o"].as_str()? {
        "LIMIT" => OrderType::Limit,
        "MARKET" => OrderType::Market,
        "STOP_LOSS" => OrderType::StopLoss,
        "STOP_LOSS_LIMIT" => OrderType::StopLossLimit,
        "TAKE_PROFIT" => OrderType::TakeProfit,
        "TAKE_PROFIT_LIMIT" => OrderType::TakeProfitLimit,
        "LIMIT_MAKER" => OrderType::LimitMaker,
        _ => return None,
    };
    let status = match data["X"].as_str()? {
        "NEW" => OrderStatus::New,
        "PARTIALLY_FILLED" => OrderStatus::PartiallyFilled,
        "FILLED" => OrderStatus::Filled,
        "CANCELED" => OrderStatus::Canceled,
        "PENDING_CANCEL" => OrderStatus::PendingCancel,
        "REJECTED" => OrderStatus::Rejected,
        "EXPIRED" => OrderStatus::Expired,
        _ => return None,
    };

    Some(OrderUpdate {
        order_id: data["i"].as_i64()?.to_string(),
        client_order_id: data["c"].as_str().map(|s| s.to_string()),
        symbol,
        side,
        order_type,
        status,
        quantity: Decimal::from_str(data["q"].as_str()?).ok()?,
        filled_quantity: Decimal::from_str(data["z"].as_str().unwrap_or("0")).unwrap_or_default(),
        price: Decimal::from_str(data["p"].as_str().unwrap_or("0")).ok(),
        avg_price: Decimal::from_str(data["L"].as_str().unwrap_or("0")).ok().filter(|d| *d > Decimal::ZERO),
        commission: Decimal::from_str(data["n"].as_str().unwrap_or("0")).ok(),
        commission_asset: data["N"].as_str().map(|s| s.to_string()),
        timestamp: DateTime::from_timestamp_millis(data["T"].as_i64()?)?,
    })
}

/// 从订单响应中计算平均价格
fn calculate_avg_price(data: &serde_json::Value) -> Option<Decimal> {
    let executed_qty = Decimal::from_str(data["executedQty"].as_str().unwrap_or("0")).ok()?;
    let cummulative_quote = Decimal::from_str(data["cummulativeQuoteQty"].as_str().unwrap_or("0")).ok()?;

    if executed_qty > Decimal::ZERO {
        Some(cummulative_quote / executed_qty)
    } else {
        None
    }
}

/// 从订单响应 fills 中提取总手续费
fn extract_commission(data: &serde_json::Value) -> Option<Decimal> {
    let fills = data["fills"].as_array()?;
    let total: Decimal = fills.iter()
        .filter_map(|f| Decimal::from_str(f["commission"].as_str()?).ok())
        .sum();
    if total > Decimal::ZERO { Some(total) } else { None }
}
