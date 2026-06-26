// exchange/adapters/okx_adapter.rs
// OKX 交易所适配器实现

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

/// OKX 配置
#[derive(Debug, Clone)]
pub struct OkxConfig {
    pub api_key: String,
    pub api_secret: String,
    pub passphrase: String,
    pub simulated: bool,
}

impl Default for OkxConfig {
    fn default() -> Self {
        Self {
            api_key: String::new(),
            api_secret: String::new(),
            passphrase: String::new(),
            simulated: true,
        }
    }
}

/// OKX 适配器
pub struct OkxAdapter {
    config: OkxConfig,
    client: Client,
    base_url: String,
    ws_url: String,
}

impl OkxAdapter {
    /// 创建新的 OKX 适配器
    pub fn new(config: OkxConfig) -> Result<Self, ExchangeError> {
        let client = Client::builder()
            .timeout(Duration::from_secs(10))
            .build()
            .map_err(|e| ExchangeError::NetworkError(format!("Failed to create HTTP client: {}", e)))?;

        let (base_url, ws_url) = if config.simulated {
            (
                "https://www.okx.com".to_string(),
                "wss://wspap.okx.com:8443/ws/v5/public?brokerId=9999".to_string(),
            )
        } else {
            (
                "https://www.okx.com".to_string(),
                "wss://ws.okx.com:8443/ws/v5/public".to_string(),
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
    fn sign(&self, timestamp: &str, method: &str, request_path: &str, body: &str) -> Result<String, ExchangeError> {
        let message = format!("{}{}{}{}", timestamp, method, request_path, body);

        let mut mac = HmacSha256::new_from_slice(self.config.api_secret.as_bytes())
            .map_err(|e| ExchangeError::SignatureError(format!("Invalid key length: {}", e)))?;
        mac.update(message.as_bytes());
        let result = mac.finalize();
        Ok(base64::Engine::encode(
            &base64::engine::general_purpose::STANDARD,
            result.into_bytes(),
        ))
    }

    /// 发送签名请求
    async fn send_signed_request(
        &self,
        method: &str,
        request_path: &str,
        body: &str,
    ) -> Result<serde_json::Value, ExchangeError> {
        let timestamp = Utc::now().format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string();
        let signature = self.sign(&timestamp, method, request_path, body)?;

        let url = format!("{}{}", self.base_url, request_path);

        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert("OK-ACCESS-KEY", self.config.api_key.parse().unwrap());
        headers.insert("OK-ACCESS-SIGN", signature.parse().unwrap());
        headers.insert("OK-ACCESS-TIMESTAMP", timestamp.parse().unwrap());
        headers.insert("OK-ACCESS-PASSPHRASE", self.config.passphrase.parse().unwrap());
        headers.insert("Content-Type", "application/json".parse().unwrap());

        if self.config.simulated {
            headers.insert("x-simulated-trading", "1".parse().unwrap());
        }

        let response = match method {
            "GET" => self.client.get(&url).headers(headers).send().await?,
            "POST" => self.client.post(&url).headers(headers).body(body.to_string()).send().await?,
            "DELETE" => self.client.delete(&url).headers(headers).send().await?,
            _ => return Err(ExchangeError::InvalidOrder(format!("Unsupported method: {}", method))),
        };

        let status = response.status();
        let body = response.text().await?;

        if !status.is_success() {
            let error_response: serde_json::Value = serde_json::from_str(&body)
                .unwrap_or_else(|_| serde_json::json!({"msg": body}));

            let code = error_response["code"].as_str().unwrap_or("-1");
            let message = error_response["msg"]
                .as_str()
                .unwrap_or("Unknown error")
                .to_string();

            return Err(ExchangeError::ApiError {
                code: code.parse().unwrap_or(-1),
                message,
            });
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
}

#[async_trait]
impl Exchange for OkxAdapter {
    async fn subscribe_trades(
        &self,
        symbols: &[String],
        callback: Box<dyn Fn(TickData) + Send + Sync>,
        mut shutdown_rx: broadcast::Receiver<()>,
    ) -> Result<(), ExchangeError> {
        // 构建订阅参数
        let args: Vec<serde_json::Value> = symbols
            .iter()
            .map(|s| {
                serde_json::json!({
                    "channel": "tickers",
                    "instId": s
                })
            })
            .collect();

        let subscribe_msg = serde_json::json!({
            "op": "subscribe",
            "args": args
        });

        info!("Connecting to OKX WebSocket...");

        let (mut ws_stream, _) = connect_async(&self.ws_url)
            .await
            .map_err(|e| ExchangeError::WebSocketError(format!("Connection failed: {}", e)))?;

        info!("Connected to OKX WebSocket");

        // 发送订阅消息
        let msg = Message::Text(subscribe_msg.to_string());
        ws_stream.send(msg).await
            .map_err(|e| ExchangeError::WebSocketError(e.to_string()))?;

        loop {
            tokio::select! {
                msg = ws_stream.next() => {
                    match msg {
                        Some(Ok(Message::Text(text))) => {
                            match serde_json::from_str::<serde_json::Value>(&text) {
                                Ok(data) => {
                                    // 解析行情数据
                                    if let Some(tick) = parse_okx_tick(&data) {
                                        callback(tick);
                                    }
                                }
                                Err(e) => {
                                    warn!("Failed to parse OKX data: {}", e);
                                }
                            }
                        }
                        Some(Ok(Message::Ping(data))) => {
                            let _ = ws_stream.send(Message::Pong(data)).await;
                        }
                        Some(Ok(Message::Close(_))) => {
                            info!("OKX WebSocket connection closed by server");
                            break;
                        }
                        Some(Err(e)) => {
                            error!("OKX WebSocket error: {}", e);
                            return Err(ExchangeError::WebSocketError(e.to_string()));
                        }
                        None => {
                            info!("OKX WebSocket stream ended");
                            break;
                        }
                        _ => {}
                    }
                }
                _ = shutdown_rx.recv() => {
                    info!("Shutdown signal received, closing OKX WebSocket");
                    let _ = ws_stream.close(None).await;
                    break;
                }
            }
        }

        Ok(())
    }

    async fn get_account(&self) -> Result<AccountInfo, ExchangeError> {
        let data = self.send_signed_request("GET", "/api/v5/account/balance", "").await?;

        let balances: Vec<Balance> = data["data"][0]["details"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|b| {
                        let asset = b["ccy"].as_str()?.to_string();
                        let free = b["availBal"].as_str()?.parse::<Decimal>().ok()?;
                        let locked = b["frozenBal"].as_str()?.parse::<Decimal>().ok()?;
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

    async fn get_position(&self, symbol: &str) -> Result<PositionInfo, ExchangeError> {
        let mut params = HashMap::new();
        params.insert("instId".to_string(), symbol.to_string());

        let data = self.send_signed_request("GET", "/api/v5/account/positions", "").await?;

        let positions = data["data"]
            .as_array()
            .ok_or_else(|| ExchangeError::ParseError("Missing positions data".to_string()))?;

        if let Some(pos) = positions.iter().find(|p| p["instId"].as_str() == Some(symbol)) {
            Ok(PositionInfo {
                symbol: symbol.to_string(),
                side: match pos["posSide"].as_str() {
                    Some("long") => PositionSide::Long,
                    Some("short") => PositionSide::Short,
                    _ => PositionSide::None,
                },
                quantity: pos["pos"]
                    .as_str()
                    .and_then(|s| s.parse().ok())
                    .unwrap_or_default(),
                avg_entry_price: pos["avgPx"]
                    .as_str()
                    .and_then(|s| s.parse().ok())
                    .unwrap_or_default(),
                mark_price: pos["markPx"]
                    .as_str()
                    .and_then(|s| s.parse().ok()),
                unrealized_pnl: pos["upl"]
                    .as_str()
                    .and_then(|s| s.parse().ok())
                    .unwrap_or_default(),
                leverage: pos["lever"]
                    .as_str()
                    .and_then(|s| s.parse().ok())
                    .unwrap_or(1),
                margin: pos["margin"]
                    .as_str()
                    .and_then(|s| s.parse().ok())
                    .unwrap_or_default(),
                liquidation_price: pos["liqPx"]
                    .as_str()
                    .and_then(|s| s.parse().ok()),
            })
        } else {
            Ok(PositionInfo {
                symbol: symbol.to_string(),
                side: PositionSide::None,
                quantity: Decimal::ZERO,
                avg_entry_price: Decimal::ZERO,
                mark_price: None,
                unrealized_pnl: Decimal::ZERO,
                leverage: 1,
                margin: Decimal::ZERO,
                liquidation_price: None,
            })
        }
    }

    async fn get_positions(&self) -> Result<Vec<PositionInfo>, ExchangeError> {
        let data = self.send_signed_request("GET", "/api/v5/account/positions", "").await?;

        let positions = data["data"]
            .as_array()
            .ok_or_else(|| ExchangeError::ParseError("Missing positions data".to_string()))?;

        let result = positions
            .iter()
            .filter_map(|pos| {
                let symbol = pos["instId"].as_str()?.to_string();
                Some(PositionInfo {
                    symbol,
                    side: match pos["posSide"].as_str() {
                        Some("long") => PositionSide::Long,
                        Some("short") => PositionSide::Short,
                        _ => PositionSide::None,
                    },
                    quantity: pos["pos"]
                        .as_str()
                        .and_then(|s| s.parse().ok())
                        .unwrap_or_default(),
                    avg_entry_price: pos["avgPx"]
                        .as_str()
                        .and_then(|s| s.parse().ok())
                        .unwrap_or_default(),
                    mark_price: pos["markPx"]
                        .as_str()
                        .and_then(|s| s.parse().ok()),
                    unrealized_pnl: pos["upl"]
                        .as_str()
                        .and_then(|s| s.parse().ok())
                        .unwrap_or_default(),
                    leverage: pos["lever"]
                        .as_str()
                        .and_then(|s| s.parse().ok())
                        .unwrap_or(1),
                    margin: pos["margin"]
                        .as_str()
                        .and_then(|s| s.parse().ok())
                        .unwrap_or_default(),
                    liquidation_price: pos["liqPx"]
                        .as_str()
                        .and_then(|s| s.parse().ok()),
                })
            })
            .collect();

        Ok(result)
    }

    async fn place_order(&self, order: OrderRequest) -> Result<OrderResult, ExchangeError> {
        let body = serde_json::json!({
            "instId": order.symbol,
            "tdMode": "cash",
            "side": order.side.to_string().to_lowercase(),
            "ordType": order.order_type.to_string().to_lowercase(),
            "sz": order.quantity.to_string(),
            "px": order.price.map(|p| p.to_string()),
        });

        let data = self.send_signed_request("POST", "/api/v5/trade/order", &body.to_string()).await?;

        Ok(OrderResult {
            order_id: data["data"][0]["ordId"].as_str().unwrap_or("").to_string(),
            client_order_id: data["data"][0]["clOrdId"].as_str().map(|s| s.to_string()),
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
        let body = serde_json::json!({
            "instId": symbol,
            "ordId": order_id,
        });

        self.send_signed_request("POST", "/api/v5/trade/cancel-order", &body.to_string()).await?;
        Ok(())
    }

    async fn cancel_all_orders(&self, symbol: Option<&str>) -> Result<(), ExchangeError> {
        if let Some(s) = symbol {
            let body = serde_json::json!({
                "instId": s,
            });
            self.send_signed_request("POST", "/api/v5/trade/cancel-batch-orders", &body.to_string()).await?;
        }
        Ok(())
    }

    async fn get_open_orders(&self, symbol: Option<&str>) -> Result<Vec<OrderInfo>, ExchangeError> {
        let mut params = HashMap::new();
        if let Some(s) = symbol {
            params.insert("instId".to_string(), s.to_string());
        }

        let data = self.send_signed_request("GET", "/api/v5/trade/orders-pending", "").await?;

        let orders = data["data"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|o| {
                        let order_id = o["ordId"].as_str()?.to_string();
                        let inst_id = o["instId"].as_str()?.to_string();
                        let side_str = o["side"].as_str()?;
                        let ord_type = o["ordType"].as_str()?;
                        let sz = o["sz"].as_str()?.parse::<Decimal>().ok()?;
                        let acc_fill_sz = o["accFillSz"].as_str()?.parse::<Decimal>().ok()?;
                        let c_time = o["cTime"].as_str()?;
                        let u_time = o["uTime"].as_str()?;

                        Some(OrderInfo {
                            order_id,
                            client_order_id: o["clOrdId"].as_str().map(|s| s.to_string()),
                            symbol: inst_id,
                            side: if side_str == "buy" {
                                OrderSide::Buy
                            } else {
                                OrderSide::Sell
                            },
                            order_type: match ord_type {
                                "market" => OrderType::Market,
                                "limit" => OrderType::Limit,
                                _ => return None,
                            },
                            status: OrderStatus::New,
                            quantity: sz,
                            filled_quantity: acc_fill_sz,
                            remaining_quantity: sz - acc_fill_sz,
                            price: o["px"].as_str().and_then(|p| p.parse().ok()),
                            stop_price: o["stopPx"].as_str().and_then(|p| p.parse().ok()),
                            time_in_force: TimeInForce::Gtc,
                            created_at: DateTime::parse_from_rfc3339(c_time)
                                .ok()?
                                .with_timezone(&Utc),
                            updated_at: DateTime::parse_from_rfc3339(u_time)
                                .ok()?
                                .with_timezone(&Utc),
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();

        Ok(orders)
    }

    async fn get_order(&self, symbol: &str, order_id: &str) -> Result<OrderInfo, ExchangeError> {
        let mut params = HashMap::new();
        params.insert("instId".to_string(), symbol.to_string());
        params.insert("ordId".to_string(), order_id.to_string());

        let data = self.send_signed_request("GET", "/api/v5/trade/order", "").await?;

        let order = data["data"]
            .as_array()
            .and_then(|arr| arr.first())
            .ok_or_else(|| ExchangeError::OrderNotFound(order_id.to_string()))?;

        Ok(OrderInfo {
            order_id: order["ordId"].as_str().unwrap_or("").to_string(),
            client_order_id: order["clOrdId"].as_str().map(|s| s.to_string()),
            symbol: order["instId"].as_str().unwrap_or("").to_string(),
            side: if order["side"].as_str() == Some("buy") {
                OrderSide::Buy
            } else {
                OrderSide::Sell
            },
            order_type: match order["ordType"].as_str() {
                Some("market") => OrderType::Market,
                Some("limit") => OrderType::Limit,
                _ => OrderType::Market,
            },
            status: match order["state"].as_str() {
                Some("live") => OrderStatus::New,
                Some("partially_filled") => OrderStatus::PartiallyFilled,
                Some("filled") => OrderStatus::Filled,
                Some("canceled") => OrderStatus::Canceled,
                _ => OrderStatus::New,
            },
            quantity: order["sz"]
                .as_str()
                .and_then(|s| s.parse().ok())
                .unwrap_or_default(),
            filled_quantity: order["accFillSz"]
                .as_str()
                .and_then(|s| s.parse().ok())
                .unwrap_or_default(),
            remaining_quantity: order["sz"]
                .as_str()
                .and_then(|s| s.parse::<Decimal>().ok())
                .unwrap_or_default()
                - order["accFillSz"]
                    .as_str()
                    .and_then(|s| s.parse::<Decimal>().ok())
                    .unwrap_or_default(),
            price: order["px"].as_str().and_then(|p| p.parse().ok()),
            stop_price: order["stopPx"].as_str().and_then(|p| p.parse().ok()),
            time_in_force: match order["tdMode"].as_str() {
                Some("gtc") => TimeInForce::Gtc,
                Some("ioc") => TimeInForce::Ioc,
                _ => TimeInForce::Gtc,
            },
            created_at: DateTime::parse_from_rfc3339(order["cTime"].as_str().unwrap_or(""))
                .map(|dt| dt.with_timezone(&Utc))
                .unwrap_or_default(),
            updated_at: DateTime::parse_from_rfc3339(order["uTime"].as_str().unwrap_or(""))
                .map(|dt| dt.with_timezone(&Utc))
                .unwrap_or_default(),
        })
    }

    async fn subscribe_user_data(
        &self,
        _order_callback: Box<dyn Fn(OrderUpdate) + Send + Sync>,
        _shutdown_rx: broadcast::Receiver<()>,
    ) -> Result<(), ExchangeError> {
        // OKX 用户数据流需要私有 WebSocket
        // 这里简化实现
        warn!("OKX user data stream not implemented yet");
        Ok(())
    }

    fn exchange_id(&self) -> &str {
        "okx"
    }

    fn is_testnet(&self) -> bool {
        self.config.simulated
    }

    async fn get_server_time(&self) -> Result<DateTime<Utc>, ExchangeError> {
        let data = self.send_public_request("/api/v5/public/time", &HashMap::new()).await?;
        let timestamp = data["data"][0]["ts"]
            .as_str()
            .ok_or_else(|| ExchangeError::ParseError("Missing timestamp".to_string()))?
            .parse::<i64>()
            .map_err(|e| ExchangeError::ParseError(e.to_string()))?;

        DateTime::from_timestamp_millis(timestamp)
            .ok_or_else(|| ExchangeError::ParseError("Invalid timestamp".to_string()))
    }

    async fn get_symbol_precision(&self, symbol: &str) -> Result<SymbolPrecision, ExchangeError> {
        let mut params = HashMap::new();
        params.insert("instId".to_string(), symbol.to_string());

        let data = self.send_public_request("/api/v5/public/instruments", &params).await?;

        let instrument = data["data"]
            .as_array()
            .and_then(|arr| arr.first())
            .ok_or_else(|| ExchangeError::InvalidSymbol(format!("Symbol not found: {}", symbol)))?;

        Ok(SymbolPrecision {
            symbol: symbol.to_string(),
            base_asset_precision: instrument["baseCcy"]
                .as_str()
                .and_then(|_| Some(8))
                .unwrap_or(8),
            quote_asset_precision: instrument["quoteCcy"]
                .as_str()
                .and_then(|_| Some(8))
                .unwrap_or(8),
            min_quantity: instrument["minSz"]
                .as_str()
                .and_then(|s| s.parse().ok())
                .unwrap_or_else(|| Decimal::from(1)),
            max_quantity: Decimal::from(1000000),
            min_notional: Decimal::from(10),
            step_size: Decimal::from(1),
            tick_size: instrument["tickSz"]
                .as_str()
                .and_then(|s| s.parse().ok())
                .unwrap_or_else(|| Decimal::from(1)),
        })
    }
}

/// 解析 OKX 行情数据
fn parse_okx_tick(data: &serde_json::Value) -> Option<TickData> {
    let channel = data["arg"]["channel"].as_str()?;
    if channel != "tickers" {
        return None;
    }

    let ticker = data["data"].as_array()?.first()?;

    let symbol = ticker["instId"].as_str()?;
    let last_price = ticker["last"].as_str()?.parse::<Decimal>().ok()?;
    let timestamp = ticker["ts"]
        .as_str()?
        .parse::<i64>()
        .ok()?;
    let dt = chrono::DateTime::from_timestamp_millis(timestamp)?;

    Some(TickData {
        timestamp: dt,
        symbol: symbol.to_string(),
        price: last_price,
        quantity: ticker["vol24h"]
            .as_str()
            .and_then(|s| s.parse().ok())
            .unwrap_or_default(),
        side: trading_common::data::types::TradeSide::Buy,
        trade_id: timestamp.to_string(),
        is_buyer_maker: false,
    })
}
