// exchange/adapters/binance_adapter.rs
// Binance 交易所适配器实现

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use futures_util::{SinkExt, StreamExt};
use hmac::{Hmac, Mac};
use reqwest::Client;
use rust_decimal::Decimal;
use sha2::Sha256;
use std::collections::HashMap;
use std::time::Duration;
use tokio::sync::broadcast;
use tokio_tungstenite::{connect_async, tungstenite::Message};
use tracing::{error, info, warn};

use crate::exchange::errors::ExchangeError;
use crate::exchange::traits::{Exchange, SymbolPrecision};
use crate::exchange::types::*;
use trading_common::data::types::TickData;

type HmacSha256 = Hmac<Sha256>;

/// Binance 配置
#[derive(Debug, Clone)]
pub struct BinanceConfig {
    pub api_key: String,
    pub api_secret: String,
    pub testnet: bool,
    pub recv_window: u64,
    pub timeout: Duration,
}

impl Default for BinanceConfig {
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

/// Binance 适配器
pub struct BinanceAdapter {
    config: BinanceConfig,
    client: Client,
    base_url: String,
    ws_url: String,
}

impl BinanceAdapter {
    /// 创建新的 Binance 适配器
    pub fn new(config: BinanceConfig) -> Result<Self, ExchangeError> {
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

    /// 发送签名请求
    async fn send_signed_request(
        &self,
        method: &str,
        endpoint: &str,
        params: &HashMap<String, String>,
    ) -> Result<serde_json::Value, ExchangeError> {
        let query_string = self.create_signed_query(params)?;
        let url = format!("{}{}?{}", self.base_url, endpoint, query_string);

        let response = match method {
            "GET" => self.client.get(&url).send().await?,
            "POST" => self.client.post(&url).send().await?,
            "DELETE" => self.client.delete(&url).send().await?,
            _ => return Err(ExchangeError::InvalidOrder(format!("Unsupported method: {}", method))),
        };

        let status = response.status();
        let body = response.text().await?;

        if !status.is_success() {
            let error_response: serde_json::Value = serde_json::from_str(&body)
                .unwrap_or_else(|_| serde_json::json!({"msg": body}));

            let code = error_response["code"].as_i64().unwrap_or(-1);
            let message = error_response["msg"]
                .as_str()
                .unwrap_or("Unknown error")
                .to_string();

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

    /// 解析账户信息响应
    fn parse_account_response(&self, data: &serde_json::Value) -> Result<AccountInfo, ExchangeError> {
        let balances: Vec<Balance> = data["balances"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|b| {
                        let asset = b["asset"].as_str()?.to_string();
                        let free = b["free"].as_str()?.parse::<Decimal>().ok()?;
                        let locked = b["locked"].as_str()?.parse::<Decimal>().ok()?;
                        if free > Decimal::ZERO || locked > Decimal::ZERO {
                            Some(Balance {
                                asset,
                                free,
                                locked,
                            })
                        } else {
                            None
                        }
                    })
                    .collect()
            })
            .unwrap_or_default();

        // 计算总权益 (简化版本，实际需要获取价格)
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
}

#[async_trait]
impl Exchange for BinanceAdapter {
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

        info!("Connecting to Binance WebSocket: {}", url);

        let (mut ws_stream, _) = connect_async(&url)
            .await
            .map_err(|e| ExchangeError::WebSocketError(format!("Connection failed: {}", e)))?;

        info!("Connected to Binance WebSocket");

        loop {
            tokio::select! {
                msg = ws_stream.next() => {
                    match msg {
                        Some(Ok(Message::Text(text))) => {
                            match serde_json::from_str::<serde_json::Value>(&text) {
                                Ok(data) => {
                                    // 解析交易数据
                                    if let Some(trade) = parse_trade_data(&data) {
                                        callback(trade);
                                    }
                                }
                                Err(e) => {
                                    warn!("Failed to parse trade data: {}", e);
                                }
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

    async fn get_account(&self) -> Result<AccountInfo, ExchangeError> {
        let params = HashMap::new();
        let data = self.send_signed_request("GET", "/api/v3/account", &params).await?;
        self.parse_account_response(&data)
    }

    async fn get_position(&self, symbol: &str) -> Result<PositionInfo, ExchangeError> {
        // Binance Spot 不直接支持持仓查询，需要从余额计算
        let account = self.get_account().await?;
        let base_asset = symbol.replace("USDT", "").replace("BUSD", "");

        let balance = account
            .balances
            .iter()
            .find(|b| b.asset == base_asset)
            .cloned()
            .unwrap_or(Balance {
                asset: base_asset.clone(),
                free: Decimal::ZERO,
                locked: Decimal::ZERO,
            });

        let quantity = balance.free + balance.locked;

        Ok(PositionInfo {
            symbol: symbol.to_string(),
            side: if quantity > Decimal::ZERO {
                PositionSide::Long
            } else {
                PositionSide::None
            },
            quantity,
            avg_entry_price: Decimal::ZERO, // 需要从交易历史计算
            mark_price: None,
            unrealized_pnl: Decimal::ZERO,
            leverage: 1,
            margin: Decimal::ZERO,
            liquidation_price: None,
        })
    }

    async fn get_positions(&self) -> Result<Vec<PositionInfo>, ExchangeError> {
        let account = self.get_account().await?;
        let mut positions = Vec::new();

        for balance in &account.balances {
            if balance.free > Decimal::ZERO || balance.locked > Decimal::ZERO {
                let symbol = format!("{}USDT", balance.asset);
                positions.push(PositionInfo {
                    symbol,
                    side: PositionSide::Long,
                    quantity: balance.free + balance.locked,
                    avg_entry_price: Decimal::ZERO,
                    mark_price: None,
                    unrealized_pnl: Decimal::ZERO,
                    leverage: 1,
                    margin: Decimal::ZERO,
                    liquidation_price: None,
                });
            }
        }

        Ok(positions)
    }

    async fn place_order(&self, order: OrderRequest) -> Result<OrderResult, ExchangeError> {
        let mut params = HashMap::new();
        params.insert("symbol".to_string(), order.symbol.clone());
        params.insert("side".to_string(), order.side.to_string());
        params.insert("type".to_string(), order.order_type.to_string());
        params.insert("quantity".to_string(), order.quantity.to_string());

        if let Some(price) = order.price {
            params.insert("price".to_string(), price.to_string());
        }

        if let Some(stop_price) = order.stop_price {
            params.insert("stopPrice".to_string(), stop_price.to_string());
        }

        if let Some(time_in_force) = order.time_in_force {
            params.insert("timeInForce".to_string(), time_in_force.to_string());
        }

        if let Some(client_order_id) = order.client_order_id {
            params.insert("newClientOrderId".to_string(), client_order_id);
        }

        let data = self.send_signed_request("POST", "/api/v3/order", &params).await?;

        Ok(OrderResult {
            order_id: data["orderId"].as_str().unwrap_or("").to_string(),
            client_order_id: data["clientOrderId"].as_str().map(|s| s.to_string()),
            symbol: order.symbol,
            side: order.side,
            order_type: order.order_type,
            status: OrderStatus::New,
            quantity: order.quantity,
            filled_quantity: Decimal::ZERO,
            price: order.price,
            avg_price: None,
            commission: None,
            commission_asset: None,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        })
    }

    async fn cancel_order(&self, symbol: &str, order_id: &str) -> Result<(), ExchangeError> {
        let mut params = HashMap::new();
        params.insert("symbol".to_string(), symbol.to_string());
        params.insert("orderId".to_string(), order_id.to_string());

        self.send_signed_request("DELETE", "/api/v3/order", &params)
            .await?;

        Ok(())
    }

    async fn cancel_all_orders(&self, symbol: Option<&str>) -> Result<(), ExchangeError> {
        let mut params = HashMap::new();
        if let Some(s) = symbol {
            params.insert("symbol".to_string(), s.to_string());
        }

        self.send_signed_request("DELETE", "/api/v3/openOrders", &params)
            .await?;

        Ok(())
    }

    async fn get_open_orders(&self, symbol: Option<&str>) -> Result<Vec<OrderInfo>, ExchangeError> {
        let mut params = HashMap::new();
        if let Some(s) = symbol {
            params.insert("symbol".to_string(), s.to_string());
        }

        let data = self.send_signed_request("GET", "/api/v3/openOrders", &params).await?;

        let orders = data
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|o| {
                        Some(OrderInfo {
                            order_id: o["orderId"].as_str()?.to_string(),
                            client_order_id: o["clientOrderId"].as_str().map(|s| s.to_string()),
                            symbol: o["symbol"].as_str()?.to_string(),
                            side: if o["side"].as_str()? == "BUY" {
                                OrderSide::Buy
                            } else {
                                OrderSide::Sell
                            },
                            order_type: match o["type"].as_str()? {
                                "MARKET" => OrderType::Market,
                                "LIMIT" => OrderType::Limit,
                                _ => return None,
                            },
                            status: OrderStatus::New,
                            quantity: o["origQty"].as_str()?.parse().ok()?,
                            filled_quantity: o["executedQty"].as_str()?.parse().ok()?,
                            remaining_quantity: o["origQty"]
                                .as_str()?
                                .parse::<Decimal>()
                                .ok()?
                                - o["executedQty"].as_str()?.parse::<Decimal>().ok()?,
                            price: o["price"].as_str()?.parse().ok(),
                            stop_price: o["stopPrice"].as_str()?.parse().ok(),
                            time_in_force: match o["timeInForce"].as_str()? {
                                "GTC" => TimeInForce::Gtc,
                                "IOC" => TimeInForce::Ioc,
                                "FOK" => TimeInForce::Fok,
                                _ => return None,
                            },
                            created_at: chrono::DateTime::from_timestamp_millis(
                                o["time"].as_i64()?,
                            )?,
                            updated_at: chrono::DateTime::from_timestamp_millis(
                                o["updateTime"].as_i64()?,
                            )?,
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();

        Ok(orders)
    }

    async fn get_order(&self, symbol: &str, order_id: &str) -> Result<OrderInfo, ExchangeError> {
        let mut params = HashMap::new();
        params.insert("symbol".to_string(), symbol.to_string());
        params.insert("orderId".to_string(), order_id.to_string());

        let data = self.send_signed_request("GET", "/api/v3/order", &params).await?;

        Ok(OrderInfo {
            order_id: data["orderId"].as_str().unwrap_or("").to_string(),
            client_order_id: data["clientOrderId"].as_str().map(|s| s.to_string()),
            symbol: data["symbol"].as_str().unwrap_or("").to_string(),
            side: if data["side"].as_str() == Some("BUY") {
                OrderSide::Buy
            } else {
                OrderSide::Sell
            },
            order_type: match data["type"].as_str() {
                Some("MARKET") => OrderType::Market,
                Some("LIMIT") => OrderType::Limit,
                _ => OrderType::Market,
            },
            status: match data["status"].as_str() {
                Some("NEW") => OrderStatus::New,
                Some("PARTIALLY_FILLED") => OrderStatus::PartiallyFilled,
                Some("FILLED") => OrderStatus::Filled,
                Some("CANCELED") => OrderStatus::Canceled,
                Some("REJECTED") => OrderStatus::Rejected,
                _ => OrderStatus::New,
            },
            quantity: data["origQty"]
                .as_str()
                .and_then(|s| s.parse().ok())
                .unwrap_or_default(),
            filled_quantity: data["executedQty"]
                .as_str()
                .and_then(|s| s.parse().ok())
                .unwrap_or_default(),
            remaining_quantity: data["origQty"]
                .as_str()
                .and_then(|s| s.parse::<Decimal>().ok())
                .unwrap_or_default()
                - data["executedQty"]
                    .as_str()
                    .and_then(|s| s.parse::<Decimal>().ok())
                    .unwrap_or_default(),
            price: data["price"].as_str().and_then(|s| s.parse().ok()),
            stop_price: data["stopPrice"].as_str().and_then(|s| s.parse().ok()),
            time_in_force: match data["timeInForce"].as_str() {
                Some("GTC") => TimeInForce::Gtc,
                Some("IOC") => TimeInForce::Ioc,
                Some("FOK") => TimeInForce::Fok,
                _ => TimeInForce::Gtc,
            },
            created_at: chrono::DateTime::from_timestamp_millis(
                data["time"].as_i64().unwrap_or(0),
            )
            .unwrap_or_default(),
            updated_at: chrono::DateTime::from_timestamp_millis(
                data["updateTime"].as_i64().unwrap_or(0),
            )
            .unwrap_or_default(),
        })
    }

    async fn subscribe_user_data(
        &self,
        _order_callback: Box<dyn Fn(OrderUpdate) + Send + Sync>,
        _shutdown_rx: broadcast::Receiver<()>,
    ) -> Result<(), ExchangeError> {
        // Binance 用户数据流需要 listenKey
        // 这里简化实现，实际需要：
        // 1. POST /api/v3/userDataStream 获取 listenKey
        // 2. 连接 WebSocket wss://stream.binance.com:9443/ws/<listenKey>
        // 3. 定期 PUT /api/v3/userDataStream 保持连接
        warn!("Binance user data stream not implemented yet");
        Ok(())
    }

    fn exchange_id(&self) -> &str {
        "binance"
    }

    fn is_testnet(&self) -> bool {
        self.config.testnet
    }

    async fn get_server_time(&self) -> Result<DateTime<Utc>, ExchangeError> {
        let data = self.send_public_request("/api/v3/time", &HashMap::new()).await?;
        let timestamp = data["serverTime"]
            .as_i64()
            .ok_or_else(|| ExchangeError::ParseError("Missing serverTime".to_string()))?;

        DateTime::from_timestamp_millis(timestamp)
            .ok_or_else(|| ExchangeError::ParseError("Invalid timestamp".to_string()))
    }

    async fn get_symbol_precision(&self, symbol: &str) -> Result<SymbolPrecision, ExchangeError> {
        let mut params = HashMap::new();
        params.insert("symbol".to_string(), symbol.to_string());

        let data = self
            .send_public_request("/api/v3/exchangeInfo", &params)
            .await?;

        let symbols = data["symbols"]
            .as_array()
            .ok_or_else(|| ExchangeError::ParseError("Missing symbols array".to_string()))?;

        let symbol_info = symbols
            .iter()
            .find(|s| s["symbol"].as_str() == Some(symbol))
            .ok_or_else(|| ExchangeError::InvalidSymbol(format!("Symbol not found: {}", symbol)))?;

        Ok(SymbolPrecision {
            symbol: symbol.to_string(),
            base_asset_precision: symbol_info["baseAssetPrecision"]
                .as_u64()
                .unwrap_or(8) as u32,
            quote_asset_precision: symbol_info["quoteAssetPrecision"]
                .as_u64()
                .unwrap_or(8) as u32,
            min_quantity: symbol_info["filters"]
                .as_array()
                .and_then(|filters| {
                    filters
                        .iter()
                        .find(|f| f["filterType"].as_str() == Some("LOT_SIZE"))
                        .and_then(|f| f["minQty"].as_str()?.parse().ok())
                })
                .unwrap_or_else(|| Decimal::from(1)),
            max_quantity: symbol_info["filters"]
                .as_array()
                .and_then(|filters| {
                    filters
                        .iter()
                        .find(|f| f["filterType"].as_str() == Some("LOT_SIZE"))
                        .and_then(|f| f["maxQty"].as_str()?.parse().ok())
                })
                .unwrap_or_else(|| Decimal::from(1000000)),
            min_notional: symbol_info["filters"]
                .as_array()
                .and_then(|filters| {
                    filters
                        .iter()
                        .find(|f| f["filterType"].as_str() == Some("NOTIONAL"))
                        .and_then(|f| f["minNotional"].as_str()?.parse().ok())
                })
                .unwrap_or_else(|| Decimal::from(10)),
            step_size: symbol_info["filters"]
                .as_array()
                .and_then(|filters| {
                    filters
                        .iter()
                        .find(|f| f["filterType"].as_str() == Some("LOT_SIZE"))
                        .and_then(|f| f["stepSize"].as_str()?.parse().ok())
                })
                .unwrap_or_else(|| Decimal::from(1)),
            tick_size: symbol_info["filters"]
                .as_array()
                .and_then(|filters| {
                    filters
                        .iter()
                        .find(|f| f["filterType"].as_str() == Some("PRICE_FILTER"))
                        .and_then(|f| f["tickSize"].as_str()?.parse().ok())
                })
                .unwrap_or_else(|| Decimal::from(1)),
        })
    }
}

/// 解析交易数据
fn parse_trade_data(data: &serde_json::Value) -> Option<TickData> {
    let symbol = data["s"].as_str()?;
    let price = data["p"].as_str()?.parse::<Decimal>().ok()?;
    let quantity = data["q"].as_str()?.parse::<Decimal>().ok()?;
    let trade_id = data["t"].as_i64()?.to_string();
    let timestamp = chrono::DateTime::from_timestamp_millis(data["T"].as_i64()?)?;
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
