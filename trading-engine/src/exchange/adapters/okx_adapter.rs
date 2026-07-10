// exchange/adapters/okx_adapter.rs
// OKX 交易所适配器实现
// API v5: https://www.okx.com/docs-v5/zh/
// 支持现货 (SPOT) 和合约 (SWAP/FUTURES)，通过 instType/instId 区分

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

/// OKX 配置
#[derive(Debug, Clone)]
pub struct OkxConfig {
    pub api_key: String,
    pub api_secret: String,
    pub passphrase: String,
    pub simulated: bool,
    /// 默认 instType: "SPOT" / "SWAP"，用于无 symbol 时的批量操作
    pub default_inst_type: String,
}

impl Default for OkxConfig {
    fn default() -> Self {
        Self {
            api_key: String::new(),
            api_secret: String::new(),
            passphrase: String::new(),
            simulated: true,
            default_inst_type: "SWAP".to_string(),
        }
    }
}

/// OKX 适配器
pub struct OkxAdapter {
    config: OkxConfig,
    client: Client,
    base_url: String,
    ws_public_url: String,
    ws_private_url: String,
}

impl OkxAdapter {
    /// 创建新的 OKX 适配器
    pub fn new(config: OkxConfig) -> Result<Self, ExchangeError> {
        let client = Client::builder()
            .timeout(Duration::from_secs(10))
            .build()
            .map_err(|e| ExchangeError::NetworkError(format!("Failed to create HTTP client: {}", e)))?;

        let (base_url, ws_public_url, ws_private_url) = if config.simulated {
            (
                "https://www.okx.com".to_string(),
                "wss://wspap.okx.com:8443/ws/v5/public?brokerId=9999".to_string(),
                "wss://wspap.okx.com:8443/ws/v5/private?brokerId=9999".to_string(),
            )
        } else {
            (
                "https://www.okx.com".to_string(),
                "wss://ws.okx.com:8443/ws/v5/public".to_string(),
                "wss://ws.okx.com:8443/ws/v5/private".to_string(),
            )
        };

        Ok(Self {
            config,
            client,
            base_url,
            ws_public_url,
            ws_private_url,
        })
    }

    // ===== HTTP 基础设施 =====

    /// 生成 HMAC-SHA256 签名 (base64)
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

    /// 发送签名请求 (JSON body)
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

    // ===== 解析辅助函数 =====

    /// 判断 instId 是现货还是合约
    fn detect_td_mode(inst_id: &str) -> &'static str {
        if inst_id.ends_with("-SWAP") || inst_id.ends_with("-FUTURES") {
            "cross" // 合约默认全仓
        } else {
            "cash" // 现货
        }
    }

    /// 解析订单信息 (通用)
    fn parse_order(o: &serde_json::Value) -> Option<OrderInfo> {
        let order_id = o["ordId"].as_str()?.to_string();
        let inst_id = o["instId"].as_str()?.to_string();

        let side = match o["side"].as_str()? {
            "buy" => OrderSide::Buy,
            "sell" => OrderSide::Sell,
            _ => return None,
        };

        let order_type = match o["ordType"].as_str()? {
            "market" => OrderType::Market,
            "limit" => OrderType::Limit,
            "post_only" => OrderType::Limit,
            "fok" => OrderType::Limit,
            "ioc" => OrderType::Limit,
            "optimal_limit_ioc" => OrderType::Market,
            "conditional" => OrderType::StopLoss,
            _ => OrderType::Market,
        };

        let status = match o["state"].as_str()? {
            "live" => OrderStatus::New,
            "partially_filled" => OrderStatus::PartiallyFilled,
            "filled" => OrderStatus::Filled,
            "canceled" => OrderStatus::Canceled,
            "mmp_canceled" => OrderStatus::Canceled,
            _ => return None,
        };

        let time_in_force = match o["ordType"].as_str() {
            Some("fok") => TimeInForce::Fok,
            Some("ioc") | Some("optimal_limit_ioc") => TimeInForce::Ioc,
            _ => TimeInForce::Gtc,
        };

        let sz = Decimal::from_str(o["sz"].as_str()?).ok()?;
        let acc_fill_sz = Decimal::from_str(o["accFillSz"].as_str().unwrap_or("0")).unwrap_or_default();

        Some(OrderInfo {
            order_id,
            client_order_id: o["clOrdId"].as_str().map(|s| s.to_string()),
            symbol: inst_id,
            side,
            order_type,
            status,
            quantity: sz,
            filled_quantity: acc_fill_sz,
            remaining_quantity: sz - acc_fill_sz,
            price: o["px"].as_str().and_then(|p| Decimal::from_str(p).ok()),
            stop_price: o["stopPx"].as_str().and_then(|p| Decimal::from_str(p).ok()).filter(|d| *d > Decimal::ZERO),
            time_in_force,
            created_at: DateTime::parse_from_rfc3339(o["cTime"].as_str().unwrap_or(""))
                .map(|dt| dt.with_timezone(&Utc))
                .unwrap_or_default(),
            updated_at: DateTime::parse_from_rfc3339(o["uTime"].as_str().unwrap_or(""))
                .map(|dt| dt.with_timezone(&Utc))
                .unwrap_or_default(),
        })
    }

    /// 解析持仓信息
    fn parse_position(pos: &serde_json::Value) -> Option<PositionInfo> {
        let symbol = pos["instId"].as_str()?.to_string();
        let position_amt = Decimal::from_str(pos["pos"].as_str().unwrap_or("0")).unwrap_or_default();

        // 跳过空仓位
        if position_amt == Decimal::ZERO {
            return None;
        }

        let side = match pos["posSide"].as_str() {
            Some("long") => PositionSide::Long,
            Some("short") => PositionSide::Short,
            _ => {
                // net mode: 正数=long, 负数=short
                if position_amt > Decimal::ZERO {
                    PositionSide::Long
                } else {
                    PositionSide::Short
                }
            }
        };

        Some(PositionInfo {
            symbol,
            side,
            quantity: position_amt.abs(),
            avg_entry_price: Decimal::from_str(pos["avgPx"].as_str().unwrap_or("0")).unwrap_or_default(),
            mark_price: Decimal::from_str(pos["markPx"].as_str().unwrap_or("0")).ok().filter(|d| *d > Decimal::ZERO),
            unrealized_pnl: Decimal::from_str(pos["upl"].as_str().unwrap_or("0")).unwrap_or_default(),
            leverage: pos["lever"].as_str().and_then(|s| s.parse().ok()).unwrap_or(1),
            margin: Decimal::from_str(pos["margin"].as_str().unwrap_or("0")).unwrap_or_default(),
            liquidation_price: Decimal::from_str(pos["liqPx"].as_str().unwrap_or("0")).ok().filter(|d| *d > Decimal::ZERO),
        })
    }
}

// ===== 行情数据接口 (MarketDataProvider) =====

#[async_trait]
impl MarketDataProvider for OkxAdapter {
    // ===== 元信息 =====

    fn exchange_id(&self) -> &str {
        "okx"
    }

    fn is_testnet(&self) -> bool {
        self.config.simulated
    }

    /// GET /api/v5/public/time - 获取服务器时间
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

    /// GET /api/v5/public/instruments - 获取交易对精度
    async fn get_symbol_precision(&self, symbol: &str) -> Result<SymbolPrecision, ExchangeError> {
        // 自动判断 instType
        let inst_type = if symbol.ends_with("-SWAP") {
            "SWAP"
        } else if symbol.ends_with("-FUTURES") {
            "FUTURES"
        } else {
            "SPOT"
        };

        let endpoint = format!("/api/v5/public/instruments?instType={}&instId={}", inst_type, symbol);
        let data = self.send_public_request(&endpoint, &HashMap::new()).await?;

        let instrument = data["data"]
            .as_array()
            .and_then(|arr| arr.first())
            .ok_or_else(|| ExchangeError::InvalidSymbol(format!("Symbol not found: {}", symbol)))?;

        Ok(SymbolPrecision {
            symbol: symbol.to_string(),
            base_asset_precision: 8,
            quote_asset_precision: 8,
            min_quantity: instrument["minSz"]
                .as_str()
                .and_then(|s| s.parse().ok())
                .unwrap_or_else(|| Decimal::from(1)),
            max_quantity: instrument["maxMktSz"]
                .as_str()
                .and_then(|s| s.parse().ok())
                .unwrap_or_else(|| Decimal::from(1000000)),
            min_notional: Decimal::from(5),
            step_size: instrument["lotSz"]
                .as_str()
                .and_then(|s| s.parse().ok())
                .unwrap_or_else(|| Decimal::from(1)),
            tick_size: instrument["tickSz"]
                .as_str()
                .and_then(|s| s.parse().ok())
                .unwrap_or_else(|| Decimal::from(1)),
        })
    }

    /// GET /api/v5/market/ticker - 获取行情快照
    async fn get_ticker(&self, symbol: &str) -> Result<Ticker, ExchangeError> {
        let mut params = HashMap::new();
        params.insert("instId".to_string(), symbol.to_string());

        let data = self.send_public_request("/api/v5/market/ticker", &params).await?;

        let ticker_data = data["data"].as_array().and_then(|a| a.first())
            .ok_or_else(|| ExchangeError::ApiError {
                code: 0,
                message: "No ticker data".to_string(),
            })?;

        Ok(Ticker {
            symbol: symbol.to_string(),
            last_price: Decimal::from_str(ticker_data["last"].as_str().unwrap_or("0")).unwrap_or_default(),
            bid_price: Decimal::from_str(ticker_data["bidPx"].as_str().unwrap_or("0")).unwrap_or_default(),
            ask_price: Decimal::from_str(ticker_data["askPx"].as_str().unwrap_or("0")).unwrap_or_default(),
            high_price: Decimal::from_str(ticker_data["high24h"].as_str().unwrap_or("0")).unwrap_or_default(),
            low_price: Decimal::from_str(ticker_data["low24h"].as_str().unwrap_or("0")).unwrap_or_default(),
            volume: Decimal::from_str(ticker_data["vol24h"].as_str().unwrap_or("0")).unwrap_or_default(),
            quote_volume: Decimal::from_str(ticker_data["volCcy24h"].as_str().unwrap_or("0")).unwrap_or_default(),
            price_change: Decimal::from_str(ticker_data["last"].as_str().unwrap_or("0")).unwrap_or_default()
                - Decimal::from_str(ticker_data["open24h"].as_str().unwrap_or("0")).unwrap_or_default(),
            price_change_percent: {
                let open = Decimal::from_str(ticker_data["open24h"].as_str().unwrap_or("0")).unwrap_or_default();
                let last = Decimal::from_str(ticker_data["last"].as_str().unwrap_or("0")).unwrap_or_default();
                if open > Decimal::ZERO {
                    ((last - open) / open) * Decimal::from(100)
                } else {
                    Decimal::ZERO
                }
            },
            timestamp: Utc::now(),
        })
    }

    /// GET /api/v5/market/tickers - 批量获取行情快照
    async fn get_tickers(&self, symbols: &[String]) -> Result<Vec<Ticker>, ExchangeError> {
        // 根据 symbol 后缀判断 instType
        let inst_type = if symbols.iter().any(|s| s.ends_with("-SWAP") || s.ends_with("-FUTURES")) {
            "SWAP"
        } else {
            "SPOT"
        };
        let mut params = HashMap::new();
        params.insert("instType".to_string(), inst_type.to_string());

        let data = self.send_public_request("/api/v5/market/tickers", &params).await?;

        let all_tickers: Vec<Ticker> = data["data"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|item| {
                        let inst_id = item["instId"].as_str()?;
                        if !symbols.iter().any(|s| s == inst_id) {
                            return None;
                        }
                        Some(Ticker {
                            symbol: inst_id.to_string(),
                            last_price: Decimal::from_str(item["last"].as_str()?).ok()?,
                            bid_price: Decimal::from_str(item["bidPx"].as_str()?).ok()?,
                            ask_price: Decimal::from_str(item["askPx"].as_str()?).ok()?,
                            high_price: Decimal::from_str(item["high24h"].as_str()?).ok()?,
                            low_price: Decimal::from_str(item["low24h"].as_str()?).ok()?,
                            volume: Decimal::from_str(item["vol24h"].as_str()?).ok()?,
                            quote_volume: Decimal::from_str(item["volCcy24h"].as_str()?).ok()?,
                            price_change: {
                                let open = Decimal::from_str(item["open24h"].as_str()?).ok()?;
                                let last = Decimal::from_str(item["last"].as_str()?).ok()?;
                                last - open
                            },
                            price_change_percent: {
                                let open = Decimal::from_str(item["open24h"].as_str()?).ok()?;
                                let last = Decimal::from_str(item["last"].as_str()?).ok()?;
                                if open > Decimal::ZERO {
                                    ((last - open) / open) * Decimal::from(100)
                                } else {
                                    Decimal::ZERO
                                }
                            },
                            timestamp: Utc::now(),
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();

        Ok(all_tickers)
    }

    /// GET /api/v5/public/mark-price - 获取标记价格
    async fn get_mark_price(&self, symbol: &str) -> Result<MarkPrice, ExchangeError> {
        let endpoint = format!("/api/v5/public/mark-price?instId={}", symbol);
        let data = self.send_public_request(&endpoint, &HashMap::new()).await?;

        let item = data["data"].as_array().and_then(|a| a.first())
            .ok_or_else(|| ExchangeError::ParseError("Missing mark price data".to_string()))?;

        Ok(MarkPrice {
            symbol: item["instId"].as_str().unwrap_or(symbol).to_string(),
            mark_price: Decimal::from_str(item["markPx"].as_str().unwrap_or("0")).unwrap_or_default(),
            index_price: Decimal::ZERO, // OKX 需要单独请求 index-tickers
            estimated_settle_price: None,
            last_funding_rate: Decimal::ZERO,
            next_funding_time: DateTime::default(),
            interest_rate: Decimal::ZERO,
            time: DateTime::from_timestamp_millis(
                item["ts"].as_str().and_then(|s| s.parse().ok()).unwrap_or(0)
            ).unwrap_or_default(),
        })
    }

    /// GET /api/v5/public/funding-rate - 获取资金费率
    async fn get_funding_rate(&self, symbol: &str, limit: Option<u32>) -> Result<Vec<FundingRate>, ExchangeError> {
        let mut endpoint = format!("/api/v5/public/funding-rate?instId={}", symbol);
        if let Some(l) = limit {
            endpoint.push_str(&format!("&limit={}", l));
        }
        let data = self.send_public_request(&endpoint, &HashMap::new()).await?;

        let rates = data["data"]
            .as_array()
            .map(|arr| {
                arr.iter().filter_map(|r| {
                    Some(FundingRate {
                        symbol: r["instId"].as_str()?.to_string(),
                        funding_rate: Decimal::from_str(r["fundingRate"].as_str()?).ok()?,
                        funding_time: DateTime::from_timestamp_millis(
                            r["fundingTime"].as_str()?.parse::<i64>().ok()?
                        )?,
                        next_funding_time: DateTime::from_timestamp_millis(
                            r["nextFundingTime"].as_str().and_then(|s| s.parse().ok()).unwrap_or(0)
                        ),
                    })
                }).collect()
            })
            .unwrap_or_default();
        Ok(rates)
    }

    /// GET /api/v5/market/candles - 获取K线数据
    async fn get_klines(&self, symbol: &str, interval: &str, limit: Option<u32>) -> Result<Vec<Kline>, ExchangeError> {
        let bar = match interval {
            "1m" => "1m", "3m" => "3m", "5m" => "5m", "15m" => "15m", "30m" => "30m",
            "1h" => "1H", "2h" => "2H", "4h" => "4H", "6h" => "6H", "8h" => "8H",
            "12h" => "12H", "1d" => "1D", "3d" => "3D", "1w" => "1W", "1M" => "1M",
            _ => "1H",
        };
        let mut endpoint = format!("/api/v5/market/candles?instId={}&bar={}", symbol, bar);
        if let Some(l) = limit {
            endpoint.push_str(&format!("&limit={}", l));
        }
        let data = self.send_public_request(&endpoint, &HashMap::new()).await?;

        let klines = data["data"]
            .as_array()
            .map(|arr| {
                arr.iter().filter_map(|k| {
                    let k_arr = k.as_array()?;
                    if k_arr.len() < 6 { return None; }
                    let open_ts = k_arr[0].as_str()?.parse::<i64>().ok()?;
                    Some(Kline {
                        open_time: DateTime::from_timestamp_millis(open_ts)?,
                        open: Decimal::from_str(k_arr[1].as_str()?).ok()?,
                        high: Decimal::from_str(k_arr[2].as_str()?).ok()?,
                        low: Decimal::from_str(k_arr[3].as_str()?).ok()?,
                        close: Decimal::from_str(k_arr[4].as_str()?).ok()?,
                        volume: Decimal::from_str(k_arr[5].as_str()?).ok()?,
                        close_time: DateTime::from_timestamp_millis(open_ts + 3600000).unwrap_or_default(),
                        quote_volume: if k_arr.len() > 7 {
                            Decimal::from_str(k_arr[7].as_str().unwrap_or("0")).unwrap_or_default()
                        } else {
                            Decimal::ZERO
                        },
                        trades_count: 0,
                    })
                }).collect()
            })
            .unwrap_or_default();
        Ok(klines)
    }

    /// GET /api/v5/market/books - 获取订单簿
    async fn get_order_book(&self, symbol: &str, limit: Option<u32>) -> Result<OrderBook, ExchangeError> {
        let mut endpoint = format!("/api/v5/market/books?instId={}", symbol);
        if let Some(l) = limit {
            endpoint.push_str(&format!("&sz={}", l));
        }
        let data = self.send_public_request(&endpoint, &HashMap::new()).await?;

        let item = data["data"].as_array().and_then(|a| a.first())
            .ok_or_else(|| ExchangeError::ParseError("Missing order book data".to_string()))?;

        let parse_entries = |key: &str| -> Vec<OrderBookEntry> {
            item[key].as_array().map(|arr| {
                arr.iter().filter_map(|e| {
                    let e_arr = e.as_array()?;
                    Some(OrderBookEntry {
                        price: Decimal::from_str(e_arr[0].as_str()?).ok()?,
                        quantity: Decimal::from_str(e_arr[1].as_str()?).ok()?,
                    })
                }).collect()
            }).unwrap_or_default()
        };

        Ok(OrderBook {
            symbol: symbol.to_string(),
            bids: parse_entries("bids"),
            asks: parse_entries("asks"),
            last_update_id: item["ts"].as_str().and_then(|s| s.parse().ok()).unwrap_or(0),
        })
    }

    /// GET /api/v5/market/trades - 获取最近成交
    async fn get_recent_trades(&self, symbol: &str, limit: Option<u32>) -> Result<Vec<PublicTrade>, ExchangeError> {
        let mut params = HashMap::new();
        params.insert("instId".to_string(), symbol.to_string());
        if let Some(l) = limit {
            params.insert("limit".to_string(), l.to_string());
        }

        let data = self.send_public_request("/api/v5/market/trades", &params).await?;

        let trades: Vec<PublicTrade> = data["data"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|t| {
                        Some(PublicTrade {
                            id: t["tradeId"].as_str()?.to_string(),
                            symbol: symbol.to_string(),
                            price: Decimal::from_str(t["px"].as_str()?).ok()?,
                            quantity: Decimal::from_str(t["sz"].as_str()?).ok()?,
                            timestamp: DateTime::from_timestamp_millis(
                                t["ts"].as_str()?.parse::<i64>().ok()?,
                            )?,
                            is_buyer_maker: t["side"].as_str()? == "sell",
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();

        Ok(trades)
    }

    // ===== WebSocket =====

    /// WebSocket public: trades channel (逐笔成交)
    async fn subscribe_trades(
        &self,
        symbols: &[String],
        callback: Box<dyn Fn(TickData) + Send + Sync>,
        mut shutdown_rx: broadcast::Receiver<()>,
    ) -> Result<(), ExchangeError> {
        // 使用 trades channel 获取逐笔成交数据
        let args: Vec<serde_json::Value> = symbols
            .iter()
            .map(|s| {
                serde_json::json!({
                    "channel": "trades",
                    "instId": s
                })
            })
            .collect();

        let subscribe_msg = serde_json::json!({
            "op": "subscribe",
            "args": args
        });

        info!("Connecting to OKX public WebSocket: {}", self.ws_public_url);

        let (mut ws_stream, _) = connect_async(&self.ws_public_url)
            .await
            .map_err(|e| ExchangeError::WebSocketError(format!("Connection failed: {}", e)))?;

        info!("Connected to OKX public WebSocket");

        // 发送订阅消息
        ws_stream.send(Message::Text(subscribe_msg.to_string())).await
            .map_err(|e| ExchangeError::WebSocketError(e.to_string()))?;

        let mut ping_interval = tokio::time::interval(Duration::from_secs(30));

        loop {
            tokio::select! {
                _ = ping_interval.tick() => {
                    if let Err(e) = ws_stream.send(Message::Text("ping".to_string())).await {
                        error!("Failed to send ping: {}", e);
                        break;
                    }
                }
                msg = ws_stream.next() => {
                    match msg {
                        Some(Ok(Message::Text(text))) => {
                            if text == "pong" { continue; }
                            match serde_json::from_str::<serde_json::Value>(&text) {
                                Ok(data) => {
                                    if let Some(tick) = parse_okx_trade(&data) {
                                        callback(tick);
                                    }
                                }
                                Err(e) => warn!("Failed to parse OKX data: {}", e),
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
}

// ===== 交易操作接口 (TradingOperations) =====

#[async_trait]
impl TradingOperations for OkxAdapter {
    // ===== 账户接口 =====

    /// GET /api/v5/account/balance - 获取账户信息
    async fn get_account(&self) -> Result<AccountInfo, ExchangeError> {
        let data = self.send_signed_request("GET", "/api/v5/account/balance", "").await?;

        let detail = data["data"].as_array().and_then(|a| a.first());

        let balances: Vec<Balance> = detail
            .and_then(|d| d["details"].as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|b| {
                        let asset = b["ccy"].as_str()?.to_string();
                        let free = Decimal::from_str(b["availBal"].as_str().unwrap_or("0")).ok()?;
                        let frozen = Decimal::from_str(b["frozenBal"].as_str().unwrap_or("0")).ok()?;
                        if free > Decimal::ZERO || frozen > Decimal::ZERO {
                            Some(Balance {
                                asset,
                                free,
                                locked: frozen,
                            })
                        } else {
                            None
                        }
                    })
                    .collect()
            })
            .unwrap_or_default();

        let total_equity: Decimal = detail
            .and_then(|d| d["totalEq"].as_str())
            .and_then(|s| Decimal::from_str(s).ok())
            .unwrap_or_else(|| balances.iter().map(|b| b.free + b.locked).sum());

        let available: Decimal = detail
            .and_then(|d| d["availEq"].as_str())
            .and_then(|s| Decimal::from_str(s).ok())
            .unwrap_or(total_equity);

        let unrealized_pnl: Decimal = detail
            .and_then(|d| d["upl"].as_str())
            .and_then(|s| Decimal::from_str(s).ok())
            .unwrap_or_default();

        let margin_used: Decimal = detail
            .and_then(|d| d["imr"].as_str())
            .and_then(|s| Decimal::from_str(s).ok())
            .unwrap_or_default();

        let margin_ratio: Option<Decimal> = detail
            .and_then(|d| d["mgnRatio"].as_str())
            .and_then(|s| Decimal::from_str(s).ok());

        Ok(AccountInfo {
            balances,
            total_equity,
            available_balance: available,
            unrealized_pnl,
            margin_used,
            margin_ratio,
        })
    }

    /// GET /api/v5/account/balance - 获取合约账户信息
    async fn get_futures_account(&self) -> Result<FuturesAccountInfo, ExchangeError> {
        let data = self.send_signed_request("GET", "/api/v5/account/balance", "").await?;

        let account_info = self.get_account().await?;

        let detail = data["data"].as_array().and_then(|a| a.first());

        Ok(FuturesAccountInfo {
            account_info,
            can_trade: true,
            can_withdraw: true,
            fee_tier: 0,
            max_withdraw_amount: detail
                .and_then(|d| d["maxWithdraw"].as_str()?.parse().ok())
                .unwrap_or_default(),
            total_initial_margin: detail
                .and_then(|d| d["imr"].as_str()?.parse().ok())
                .unwrap_or_default(),
            total_maint_margin: detail
                .and_then(|d| d["mmr"].as_str()?.parse().ok())
                .unwrap_or_default(),
            total_wallet_balance: detail
                .and_then(|d| d["totalEq"].as_str()?.parse().ok())
                .unwrap_or_default(),
            total_unrealized_pnl: detail
                .and_then(|d| d["upl"].as_str()?.parse().ok())
                .unwrap_or_default(),
            total_margin_balance: detail
                .and_then(|d| d["adjEq"].as_str()?.parse().ok())
                .unwrap_or_default(),
        })
    }

    /// GET /api/v5/account/positions - 获取单个持仓
    async fn get_position(&self, symbol: &str) -> Result<PositionInfo, ExchangeError> {
        let endpoint = format!("/api/v5/account/positions?instId={}", symbol);
        let data = self.send_signed_request("GET", &endpoint, "").await?;

        let positions = data["data"]
            .as_array()
            .ok_or_else(|| ExchangeError::ParseError("Missing positions data".to_string()))?;

        for pos in positions {
            if let Some(info) = Self::parse_position(pos) {
                if info.symbol == symbol {
                    return Ok(info);
                }
            }
        }

        // 空仓位
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

    /// GET /api/v5/account/positions - 获取所有持仓
    async fn get_positions(&self) -> Result<Vec<PositionInfo>, ExchangeError> {
        let data = self.send_signed_request("GET", "/api/v5/account/positions", "").await?;

        let positions = data["data"]
            .as_array()
            .ok_or_else(|| ExchangeError::ParseError("Missing positions data".to_string()))?;

        Ok(positions.iter().filter_map(|pos| Self::parse_position(pos)).collect())
    }

    // ===== 订单接口 =====

    /// POST /api/v5/trade/order - 下单
    async fn place_order(&self, order: OrderRequest) -> Result<OrderResult, ExchangeError> {
        // 自动检测 tdMode
        let td_mode = Self::detect_td_mode(&order.symbol);

        let mut body = serde_json::json!({
            "instId": order.symbol,
            "tdMode": td_mode,
            "side": order.side.to_string().to_lowercase(),
            "ordType": order.order_type.to_string().to_lowercase(),
            "sz": order.quantity.to_string(),
        });

        if let Some(price) = order.price {
            body["px"] = serde_json::json!(price.to_string());
        }

        if let Some(client_id) = &order.client_order_id {
            body["clOrdId"] = serde_json::json!(client_id);
        }

        let data = self.send_signed_request("POST", "/api/v5/trade/order", &body.to_string()).await?;

        let result = data["data"].as_array().and_then(|a| a.first());

        let s_code = result.and_then(|r| r["sCode"].as_str()).unwrap_or("-1");
        if s_code != "0" {
            let s_msg = result.and_then(|r| r["sMsg"].as_str()).unwrap_or("Unknown error");
            return Err(ExchangeError::ApiError {
                code: s_code.parse().unwrap_or(-1),
                message: s_msg.to_string(),
            });
        }

        Ok(OrderResult {
            order_id: result.and_then(|r| r["ordId"].as_str()).unwrap_or("").to_string(),
            client_order_id: result.and_then(|r| r["clOrdId"].as_str()).map(|s| s.to_string()),
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

    /// POST /api/v5/trade/cancel-order - 撤单
    async fn cancel_order(&self, symbol: &str, order_id: &str) -> Result<(), ExchangeError> {
        let body = serde_json::json!({
            "instId": symbol,
            "ordId": order_id,
        });

        let data = self.send_signed_request("POST", "/api/v5/trade/cancel-order", &body.to_string()).await?;

        let result = data["data"].as_array().and_then(|a| a.first());
        let s_code = result.and_then(|r| r["sCode"].as_str()).unwrap_or("-1");
        if s_code != "0" {
            let s_msg = result.and_then(|r| r["sMsg"].as_str()).unwrap_or("Cancel failed");
            return Err(ExchangeError::ApiError {
                code: s_code.parse().unwrap_or(-1),
                message: s_msg.to_string(),
            });
        }

        Ok(())
    }

    /// POST /api/v5/trade/mass-cancel - 撤销所有订单
    async fn cancel_all_orders(&self, symbol: Option<&str>) -> Result<(), ExchangeError> {
        if let Some(s) = symbol {
            // 撤销指定交易对的所有订单
            let body = serde_json::json!({
                "instId": s,
            });
            self.send_signed_request("POST", "/api/v5/trade/mass-cancel", &body.to_string()).await?;
        } else {
            // 使用配置的默认 instType
            let body = serde_json::json!({
                "instType": self.config.default_inst_type,
            });
            self.send_signed_request("POST", "/api/v5/trade/mass-cancel", &body.to_string()).await?;
        }
        Ok(())
    }

    /// GET /api/v5/trade/orders-pending - 获取未成交订单
    async fn get_open_orders(&self, symbol: Option<&str>) -> Result<Vec<OrderInfo>, ExchangeError> {
        let mut endpoint = "/api/v5/trade/orders-pending".to_string();
        if let Some(s) = symbol {
            endpoint = format!("{}?instId={}", endpoint, s);
        }

        let data = self.send_signed_request("GET", &endpoint, "").await?;

        let orders = data["data"]
            .as_array()
            .map(|arr| arr.iter().filter_map(|o| Self::parse_order(o)).collect())
            .unwrap_or_default();

        Ok(orders)
    }

    /// GET /api/v5/trade/order - 获取订单详情
    async fn get_order(&self, symbol: &str, order_id: &str) -> Result<OrderInfo, ExchangeError> {
        let endpoint = format!("/api/v5/trade/order?instId={}&ordId={}", symbol, order_id);
        let data = self.send_signed_request("GET", &endpoint, "").await?;

        let order = data["data"]
            .as_array()
            .and_then(|arr| arr.first())
            .ok_or_else(|| ExchangeError::OrderNotFound(order_id.to_string()))?;

        Self::parse_order(order)
            .ok_or_else(|| ExchangeError::OrderNotFound(order_id.to_string()))
    }

    /// GET /api/v5/trade/orders-history - 获取所有订单 (历史)
    async fn get_all_orders(&self, symbol: &str, limit: Option<u32>) -> Result<Vec<OrderInfo>, ExchangeError> {
        let inst_type = if symbol.ends_with("-SWAP") {
            "SWAP"
        } else if symbol.ends_with("-FUTURES") {
            "FUTURES"
        } else {
            "SPOT"
        };

        let mut endpoint = format!("/api/v5/trade/orders-history-archive?instType={}&instId={}", inst_type, symbol);
        if let Some(l) = limit {
            endpoint.push_str(&format!("&limit={}", l));
        }

        let data = self.send_signed_request("GET", &endpoint, "").await?;

        let orders = data["data"]
            .as_array()
            .map(|arr| arr.iter().filter_map(|o| Self::parse_order(o)).collect())
            .unwrap_or_default();

        Ok(orders)
    }

    /// GET /api/v5/trade/fills - 获取成交历史
    async fn get_trade_history(&self, symbol: &str, limit: Option<u32>) -> Result<Vec<TradeInfo>, ExchangeError> {
        let mut endpoint = format!("/api/v5/trade/fills?instId={}", symbol);
        if let Some(l) = limit {
            endpoint.push_str(&format!("&limit={}", l));
        }
        let data = self.send_signed_request("GET", &endpoint, "").await?;

        let trades = data["data"]
            .as_array()
            .map(|arr| {
                arr.iter().filter_map(|t| {
                    Some(TradeInfo {
                        id: t["tradeId"].as_str()?.to_string(),
                        symbol: t["instId"].as_str()?.to_string(),
                        price: Decimal::from_str(t["fillPx"].as_str()?).ok()?,
                        quantity: Decimal::from_str(t["fillSz"].as_str()?).ok()?,
                        quote_quantity: Decimal::ZERO,
                        commission: Decimal::from_str(t["fee"].as_str().unwrap_or("0")).unwrap_or_default(),
                        commission_asset: t["feeCcy"].as_str().unwrap_or("").to_string(),
                        time: DateTime::parse_from_rfc3339(t["ts"].as_str().unwrap_or(""))
                            .map(|dt| dt.with_timezone(&Utc))
                            .unwrap_or_default(),
                        is_buyer: t["side"].as_str() == Some("buy"),
                        is_maker: t["execType"].as_str() == Some("M"),
                        realized_pnl: Decimal::from_str(t["fillPnl"].as_str().unwrap_or("0")).unwrap_or_default(),
                    })
                }).collect()
            })
            .unwrap_or_default();
        Ok(trades)
    }

    /// POST /api/v5/trade/batch-orders - 原生批量下单 (最多 20 个)
    async fn batch_place_orders(&self, orders: Vec<BatchOrderRequest>) -> Result<Vec<BatchOrderResult>, ExchangeError> {
        if orders.is_empty() {
            return Ok(Vec::new());
        }

        // OKX 批量下单最多 20 个
        let mut all_results = Vec::new();

        for chunk in orders.chunks(20) {
            let batch: Vec<serde_json::Value> = chunk.iter().map(|o| {
                let td_mode = Self::detect_td_mode(&o.symbol);
                let mut order = serde_json::json!({
                    "instId": o.symbol,
                    "tdMode": td_mode,
                    "side": o.side.to_string().to_lowercase(),
                    "ordType": o.order_type.to_string().to_lowercase(),
                    "sz": o.quantity.to_string(),
                });
                if let Some(price) = o.price {
                    order["px"] = serde_json::json!(price.to_string());
                }
                if let Some(cid) = &o.client_order_id {
                    order["clOrdId"] = serde_json::json!(cid);
                }
                order
            }).collect();

            let body = serde_json::to_string(&batch)
                .map_err(|e| ExchangeError::ParseError(e.to_string()))?;

            let data = self.send_signed_request("POST", "/api/v5/trade/batch-orders", &body).await?;

            if let Some(results) = data["data"].as_array() {
                for (i, r) in results.iter().enumerate() {
                    let s_code = r["sCode"].as_str().unwrap_or("-1");
                    all_results.push(BatchOrderResult {
                        order_id: r["ordId"].as_str().unwrap_or("").to_string(),
                        client_order_id: r["clOrdId"].as_str().map(|s| s.to_string()),
                        symbol: chunk.get(i).map(|o| o.symbol.clone()).unwrap_or_default(),
                        status: if s_code == "0" { OrderStatus::New } else { OrderStatus::Rejected },
                        error_code: if s_code != "0" { s_code.parse().ok() } else { None },
                        error_message: if s_code != "0" { r["sMsg"].as_str().map(|s| s.to_string()) } else { None },
                    });
                }
            }
        }

        Ok(all_results)
    }

    /// POST /api/v5/trade/cancel-batch-orders - 原生批量撤单 (最多 20 个)
    async fn batch_cancel_orders(&self, symbol: &str, order_ids: Vec<String>) -> Result<Vec<BatchOrderResult>, ExchangeError> {
        if order_ids.is_empty() {
            return Ok(Vec::new());
        }

        let mut all_results = Vec::new();

        for chunk in order_ids.chunks(20) {
            let batch: Vec<serde_json::Value> = chunk.iter().map(|oid| {
                serde_json::json!({
                    "instId": symbol,
                    "ordId": oid,
                })
            }).collect();

            let body = serde_json::to_string(&batch)
                .map_err(|e| ExchangeError::ParseError(e.to_string()))?;

            let data = self.send_signed_request("POST", "/api/v5/trade/cancel-batch-orders", &body).await?;

            if let Some(results) = data["data"].as_array() {
                for (i, r) in results.iter().enumerate() {
                    let s_code = r["sCode"].as_str().unwrap_or("-1");
                    all_results.push(BatchOrderResult {
                        order_id: r["ordId"].as_str().unwrap_or("").to_string(),
                        client_order_id: r["clOrdId"].as_str().map(|s| s.to_string()),
                        symbol: symbol.to_string(),
                        status: if s_code == "0" { OrderStatus::Canceled } else { OrderStatus::Rejected },
                        error_code: if s_code != "0" { s_code.parse().ok() } else { None },
                        error_message: if s_code != "0" { r["sMsg"].as_str().map(|s| s.to_string()) } else { None },
                    });
                }
            }
        }

        Ok(all_results)
    }

    // ===== 合约配置接口 =====

    /// POST /api/v5/account/set-leverage - 设置杠杆
    async fn set_leverage(&self, symbol: &str, leverage: u32) -> Result<(), ExchangeError> {
        let mgn_mode = if symbol.ends_with("-SWAP") || symbol.ends_with("-FUTURES") {
            "cross"
        } else {
            "isolated"
        };

        let body = serde_json::json!({
            "instId": symbol,
            "lever": leverage.to_string(),
            "mgnMode": mgn_mode,
        });
        self.send_signed_request("POST", "/api/v5/account/set-leverage", &body.to_string()).await?;
        Ok(())
    }

    /// POST /api/v5/account/set-isolated-mode - 设置保证金模式
    async fn set_margin_type(&self, symbol: &str, margin_type: MarginType) -> Result<(), ExchangeError> {
        // OKX 的保证金模式通过 tdMode 在下单时指定
        // 这里设置 isolated mode 的自动转账行为
        let body = serde_json::json!({
            "instId": symbol,
            "mgnMode": match margin_type {
                MarginType::Isolated => "isolated",
                MarginType::Crossed => "cross",
            },
        });
        // 注意: OKX 没有直接的 set-margin-type 端点
        // 保证金模式在下单时通过 tdMode 指定
        // 这里尝试调用，如果失败则忽略
        match self.send_signed_request("POST", "/api/v5/account/set-leverage", &body.to_string()).await {
            Ok(_) => Ok(()),
            Err(ExchangeError::ApiError { code: _, message }) => {
                // 忽略 "already set" 类型的错误
                if message.contains("already") || message.contains("51004") {
                    Ok(())
                } else {
                    Err(ExchangeError::ApiError { code: -1, message })
                }
            }
            Err(e) => Err(e),
        }
    }

    // ===== 用户数据流 (WebSocket private) =====

    /// WebSocket private: orders channel (订单更新)
    async fn subscribe_user_data(
        &self,
        order_callback: Box<dyn Fn(OrderUpdate) + Send + Sync>,
        mut shutdown_rx: broadcast::Receiver<()>,
    ) -> Result<(), ExchangeError> {
        info!("Connecting to OKX private WebSocket: {}", self.ws_private_url);

        let (mut ws_stream, _) = connect_async(&self.ws_private_url)
            .await
            .map_err(|e| ExchangeError::WebSocketError(format!("Connection failed: {}", e)))?;

        info!("Connected to OKX private WebSocket");

        // 登录认证
        let timestamp = (Utc::now().timestamp()).to_string();
        let sign = self.sign(&timestamp, "GET", "/users/self/verify", "")?;

        let login_msg = serde_json::json!({
            "op": "login",
            "args": [{
                "apiKey": self.config.api_key,
                "passphrase": self.config.passphrase,
                "timestamp": timestamp,
                "sign": sign
            }]
        });

        ws_stream.send(Message::Text(login_msg.to_string())).await
            .map_err(|e| ExchangeError::WebSocketError(format!("Login send failed: {}", e)))?;

        // 等待登录响应
        let mut logged_in = false;
        let mut ping_interval = tokio::time::interval(Duration::from_secs(30));

        loop {
            tokio::select! {
                _ = ping_interval.tick() => {
                    if let Err(e) = ws_stream.send(Message::Text("ping".to_string())).await {
                        error!("Failed to send ping: {}", e);
                        break;
                    }
                }
                msg = ws_stream.next() => {
                    match msg {
                        Some(Ok(Message::Text(text))) => {
                            if text == "pong" { continue; }

                            match serde_json::from_str::<serde_json::Value>(&text) {
                                Ok(data) => {
                                    // 处理登录响应
                                    if data["op"].as_str() == Some("login") {
                                        if data["data"][0]["code"].as_str() == Some("0") {
                                            info!("OKX private WebSocket login succeeded");
                                            logged_in = true;

                                            // 订阅 orders channel
                                            let subscribe_msg = serde_json::json!({
                                                "op": "subscribe",
                                                "args": [{"channel": "orders", "instType": "ANY"}]
                                            });
                                            ws_stream.send(Message::Text(subscribe_msg.to_string())).await
                                                .map_err(|e| ExchangeError::WebSocketError(e.to_string()))?;
                                            info!("Subscribed to OKX orders channel");
                                        } else {
                                            let msg = data["data"][0]["msg"].as_str().unwrap_or("Login failed");
                                            return Err(ExchangeError::AuthenticationError(msg.to_string()));
                                        }
                                        continue;
                                    }

                                    // 处理订单更新
                                    if !logged_in { continue; }

                                    if let Some(arg) = data.get("arg") {
                                        if arg["channel"].as_str() == Some("orders") {
                                            if let Some(orders) = data["data"].as_array() {
                                                for order_data in orders {
                                                    if let Some(update) = parse_okx_order_update(order_data) {
                                                        order_callback(update);
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                                Err(e) => warn!("Failed to parse OKX private data: {}", e),
                            }
                        }
                        Some(Ok(Message::Ping(data))) => {
                            let _ = ws_stream.send(Message::Pong(data)).await;
                        }
                        Some(Ok(Message::Close(_))) => {
                            info!("OKX private WebSocket closed");
                            break;
                        }
                        Some(Err(e)) => {
                            error!("OKX private WebSocket error: {}", e);
                            return Err(ExchangeError::WebSocketError(e.to_string()));
                        }
                        None => {
                            info!("OKX private WebSocket stream ended");
                            break;
                        }
                        _ => {}
                    }
                }
                _ = shutdown_rx.recv() => {
                    info!("Shutdown signal received, closing OKX private WebSocket");
                    let _ = ws_stream.close(None).await;
                    break;
                }
            }
        }

        Ok(())
    }
}

// ===== WebSocket 数据解析 =====

/// 解析 OKX trades channel 推送数据 (逐笔成交)
fn parse_okx_trade(data: &serde_json::Value) -> Option<TickData> {
    let arg = data.get("arg")?;
    if arg["channel"].as_str()? != "trades" {
        return None;
    }

    let trades = data["data"].as_array()?;
    let trade = trades.first()?;

    let symbol = trade["instId"].as_str()?;
    let price = Decimal::from_str(trade["px"].as_str()?).ok()?;
    let quantity = Decimal::from_str(trade["sz"].as_str()?).ok()?;
    let trade_id = trade["tradeId"].as_str()?.to_string();
    let timestamp = trade["ts"].as_str()?.parse::<i64>().ok()?;
    let dt = DateTime::from_timestamp_millis(timestamp)?;
    let is_buyer_maker = trade["side"].as_str()? == "sell";

    Some(TickData {
        timestamp: dt,
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

/// 解析 OKX orders channel 推送数据 (订单更新)
fn parse_okx_order_update(data: &serde_json::Value) -> Option<OrderUpdate> {
    let symbol = data["instId"].as_str()?.to_string();

    let side = match data["side"].as_str()? {
        "buy" => OrderSide::Buy,
        "sell" => OrderSide::Sell,
        _ => return None,
    };

    let order_type = match data["ordType"].as_str()? {
        "market" => OrderType::Market,
        "limit" => OrderType::Limit,
        "post_only" => OrderType::Limit,
        "fok" => OrderType::Limit,
        "ioc" => OrderType::Limit,
        "conditional" => OrderType::StopLoss,
        _ => OrderType::Market,
    };

    let status = match data["state"].as_str()? {
        "live" => OrderStatus::New,
        "partially_filled" => OrderStatus::PartiallyFilled,
        "filled" => OrderStatus::Filled,
        "canceled" => OrderStatus::Canceled,
        "mmp_canceled" => OrderStatus::Canceled,
        _ => return None,
    };

    let ts = data["uTime"].as_str()
        .and_then(|s| s.parse::<i64>().ok())
        .unwrap_or_else(|| Utc::now().timestamp_millis());

    Some(OrderUpdate {
        order_id: data["ordId"].as_str()?.to_string(),
        client_order_id: data["clOrdId"].as_str().map(|s| s.to_string()),
        symbol,
        side,
        order_type,
        status,
        quantity: Decimal::from_str(data["sz"].as_str()?).ok()?,
        filled_quantity: Decimal::from_str(data["accFillSz"].as_str().unwrap_or("0")).unwrap_or_default(),
        price: Decimal::from_str(data["px"].as_str().unwrap_or("0")).ok(),
        avg_price: Decimal::from_str(data["avgPx"].as_str().unwrap_or("0")).ok().filter(|d| *d > Decimal::ZERO),
        commission: Decimal::from_str(data["fee"].as_str().unwrap_or("0")).ok(),
        commission_asset: data["feeCcy"].as_str().map(|s| s.to_string()),
        timestamp: DateTime::from_timestamp_millis(ts).unwrap_or_default(),
    })
}
