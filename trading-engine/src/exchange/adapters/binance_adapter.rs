// exchange/adapters/binance_adapter.rs
// Binance USDⓈ-M 合约交易所适配器实现
// 基于 schema.yaml 中的 /fapi/v1/... 和 /fapi/v2/... 接口

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

/// Binance USDⓈ-M 合约配置
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

/// Binance USDⓈ-M 合约适配器
pub struct BinanceAdapter {
    config: BinanceConfig,
    client: Client,
    base_url: String,
    ws_url: String,
}

impl BinanceAdapter {
    /// 创建新的 Binance 合约适配器
    pub fn new(config: BinanceConfig) -> Result<Self, ExchangeError> {
        let client = Client::builder()
            .timeout(config.timeout)
            .build()
            .map_err(|e| ExchangeError::NetworkError(format!("Failed to create HTTP client: {}", e)))?;

        // USDⓈ-M Futures: fapi.binance.com / testnet.binancefuture.com
        let (base_url, ws_url) = if config.testnet {
            (
                "https://testnet.binancefuture.com".to_string(),
                "wss://testnet.binancefuture.com/ws".to_string(),
            )
        } else {
            (
                "https://fapi.binance.com".to_string(),
                "wss://fstream.binance.com/ws".to_string(),
            )
        };

        Ok(Self {
            config,
            client,
            base_url,
            ws_url,
        })
    }

    /// 根据 Binance 错误码分类错误，提供友好的错误提示
    fn classify_error(&self, code: i64, message: String) -> ExchangeError {
        match code {
            // API Key 权限不足（只读 key 尝试写操作）
            -2015 => ExchangeError::PermissionDenied(format!(
                "{} (当前 API Key 为只读权限，无法执行交易操作。请在 Binance 后台开启交易权限，或仅使用只读功能)",
                message
            )),
            // API Key 格式无效
            -2014 => ExchangeError::AuthenticationError(format!(
                "API Key 格式无效: {} (请检查 BINANCE_API_KEY 配置)", message
            )),
            // 签名无效
            -1022 => ExchangeError::SignatureError(format!(
                "签名验证失败: {} (请检查 BINANCE_API_SECRET 配置)", message
            )),
            // 时间戳超出 recvWindow
            -1021 => ExchangeError::AuthenticationError(format!(
                "时间戳超出允许范围: {} (请检查系统时间是否准确)", message
            )),
            // IP 不在白名单
            -2016 => ExchangeError::PermissionDenied(format!(
                "IP 地址不在 API Key 白名单中: {} (请在 Binance 后台添加当前服务器 IP)", message
            )),
            // 请求频率限制
            -1003 => ExchangeError::RateLimitExceeded,
            // 其他错误
            _ => ExchangeError::ApiError { code, message },
        }
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

    /// 发送签名请求 (带 API Key header)
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
            let message = error_response["msg"]
                .as_str()
                .unwrap_or("Unknown error")
                .to_string();

            return Err(self.classify_error(code, message));
        }

        serde_json::from_str(&body).map_err(|e| ExchangeError::ParseError(e.to_string()))
    }

    /// 发送签名请求 (POST form-urlencoded, 用于下单/设置等)
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
                .body(query_string.clone())
                .send()
                .await?,
            "PUT" => self.client
                .put(&url)
                .header("X-MBX-APIKEY", &self.config.api_key)
                .header("Content-Type", "application/x-www-form-urlencoded")
                .body(query_string.clone())
                .send()
                .await?,
            "DELETE" => self.client
                .delete(&format!("{}?{}", url, query_string))
                .header("X-MBX-APIKEY", &self.config.api_key)
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
            let message = error_response["msg"]
                .as_str()
                .unwrap_or("Unknown error")
                .to_string();

            return Err(self.classify_error(code, message));
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

    /// 发送仅带 API Key 的请求 (无需签名, 如 listenKey)
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
            return Err(self.classify_error(code, message));
        }

        if body.is_empty() {
            return Ok(serde_json::json!({}));
        }

        serde_json::from_str(&body).map_err(|e| ExchangeError::ParseError(e.to_string()))
    }

    /// 解析合约账户信息 (GET /fapi/v2/account)
    fn parse_futures_account(&self, data: &serde_json::Value) -> Result<AccountInfo, ExchangeError> {
        let balances: Vec<Balance> = data["assets"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|a| {
                        let asset = a["asset"].as_str()?.to_string();
                        let wallet_balance = Decimal::from_str(a["walletBalance"].as_str()?).ok()?;
                        let available = Decimal::from_str(a["availableBalance"].as_str().unwrap_or("0")).ok()?;
                        if wallet_balance > Decimal::ZERO {
                            Some(Balance {
                                asset,
                                free: available,
                                locked: wallet_balance - available,
                            })
                        } else {
                            None
                        }
                    })
                    .collect()
            })
            .unwrap_or_default();

        let total_wallet: Decimal = Decimal::from_str(
            data["totalWalletBalance"].as_str().unwrap_or("0")
        ).unwrap_or_default();
        let available: Decimal = Decimal::from_str(
            data["availableBalance"].as_str().unwrap_or("0")
        ).unwrap_or_default();
        let unrealized_pnl: Decimal = Decimal::from_str(
            data["totalUnrealizedProfit"].as_str().unwrap_or("0")
        ).unwrap_or_default();
        let margin_used: Decimal = Decimal::from_str(
            data["totalMaintMargin"].as_str().unwrap_or("0")
        ).unwrap_or_default();

        Ok(AccountInfo {
            balances,
            total_equity: total_wallet + unrealized_pnl,
            available_balance: available,
            unrealized_pnl,
            margin_used,
            margin_ratio: None,
        })
    }

    /// 解析持仓信息 (GET /fapi/v2/positionRisk)
    fn parse_position(&self, pos: &serde_json::Value) -> Option<PositionInfo> {
        let symbol = pos["symbol"].as_str()?.to_string();
        let position_amt = Decimal::from_str(pos["positionAmt"].as_str()?).ok()?;

        // 跳过空仓位
        if position_amt == Decimal::ZERO {
            return None;
        }

        let side = if position_amt > Decimal::ZERO {
            PositionSide::Long
        } else {
            PositionSide::Short
        };

        Some(PositionInfo {
            symbol,
            side,
            quantity: position_amt.abs(),
            avg_entry_price: Decimal::from_str(pos["entryPrice"].as_str().unwrap_or("0")).unwrap_or_default(),
            mark_price: Decimal::from_str(pos["markPrice"].as_str().unwrap_or("0")).ok(),
            unrealized_pnl: Decimal::from_str(pos["unRealizedProfit"].as_str().unwrap_or("0")).unwrap_or_default(),
            leverage: pos["leverage"].as_str().and_then(|s| s.parse().ok()).unwrap_or(1),
            margin: Decimal::from_str(pos["isolatedMargin"].as_str().unwrap_or("0")).unwrap_or_default(),
            liquidation_price: Decimal::from_str(pos["liquidationPrice"].as_str().unwrap_or("0")).ok().filter(|d| *d > Decimal::ZERO),
        })
    }

    /// 解析订单信息
    fn parse_order_info(&self, o: &serde_json::Value) -> Option<OrderInfo> {
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
            "STOP" => OrderType::StopLoss,
            "STOP_MARKET" => OrderType::StopLoss,
            "TAKE_PROFIT" => OrderType::TakeProfit,
            "TAKE_PROFIT_MARKET" => OrderType::TakeProfit,
            "TRAILING_STOP_MARKET" => OrderType::StopLoss,
            _ => OrderType::Market,
        };

        let status = match o["status"].as_str()? {
            "NEW" => OrderStatus::New,
            "PARTIALLY_FILLED" => OrderStatus::PartiallyFilled,
            "FILLED" => OrderStatus::Filled,
            "CANCELED" => OrderStatus::Canceled,
            "PENDING_CANCEL" => OrderStatus::PendingCancel,
            "REJECTED" => OrderStatus::Rejected,
            "EXPIRED" => OrderStatus::Expired,
            _ => OrderStatus::New,
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
impl MarketDataProvider for BinanceAdapter {
    fn exchange_id(&self) -> &str {
        "binance"
    }

    fn is_testnet(&self) -> bool {
        self.config.testnet
    }

    async fn get_server_time(&self) -> Result<DateTime<Utc>, ExchangeError> {
        let data = self.send_public_request("/fapi/v1/time", &HashMap::new()).await?;
        let timestamp = data["serverTime"]
            .as_i64()
            .ok_or_else(|| ExchangeError::ParseError("Missing serverTime".to_string()))?;

        DateTime::from_timestamp_millis(timestamp)
            .ok_or_else(|| ExchangeError::ParseError("Invalid timestamp".to_string()))
    }

    async fn get_symbol_precision(&self, symbol: &str) -> Result<SymbolPrecision, ExchangeError> {
        let mut params = HashMap::new();
        params.insert("symbol".to_string(), symbol.to_string());

        let data = self.send_public_request("/fapi/v1/exchangeInfo", &params).await?;

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
            quote_asset_precision: symbol_info["quotePrecision"].as_u64().unwrap_or(8) as u32,
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
                    filters.iter()
                        .find(|f| f["filterType"].as_str() == Some("MIN_NOTIONAL"))
                        .and_then(|f| f["notional"].as_str()?.parse().ok())
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

    async fn get_ticker(&self, symbol: &str) -> Result<Ticker, ExchangeError> {
        let mut params = HashMap::new();
        params.insert("symbol".to_string(), symbol.to_string());

        let data = self.send_public_request("/fapi/v1/ticker/24hr", &params).await?;

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

    async fn get_tickers(&self, symbols: &[String]) -> Result<Vec<Ticker>, ExchangeError> {
        let params = HashMap::new();
        let data = self.send_public_request("/fapi/v1/ticker/24hr", &params).await?;

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

    async fn get_mark_price(&self, symbol: &str) -> Result<MarkPrice, ExchangeError> {
        let mut params = HashMap::new();
        params.insert("symbol".to_string(), symbol.to_string());

        let data = self.send_public_request("/fapi/v1/premiumIndex", &params).await?;

        Ok(MarkPrice {
            symbol: data["symbol"].as_str().unwrap_or(symbol).to_string(),
            mark_price: Decimal::from_str(data["markPrice"].as_str().unwrap_or("0")).unwrap_or_default(),
            index_price: Decimal::from_str(data["indexPrice"].as_str().unwrap_or("0")).unwrap_or_default(),
            estimated_settle_price: data["estimatedSettlePrice"].as_str().and_then(|s| Decimal::from_str(s).ok()),
            last_funding_rate: Decimal::from_str(data["lastFundingRate"].as_str().unwrap_or("0")).unwrap_or_default(),
            next_funding_time: DateTime::from_timestamp_millis(data["nextFundingTime"].as_i64().unwrap_or(0)).unwrap_or_default(),
            interest_rate: Decimal::from_str(data["interestRate"].as_str().unwrap_or("0")).unwrap_or_default(),
            time: DateTime::from_timestamp_millis(data["time"].as_i64().unwrap_or(0)).unwrap_or_default(),
        })
    }

    async fn get_funding_rate(&self, symbol: &str, limit: Option<u32>) -> Result<Vec<FundingRate>, ExchangeError> {
        let mut params = HashMap::new();
        params.insert("symbol".to_string(), symbol.to_string());
        if let Some(l) = limit {
            params.insert("limit".to_string(), l.to_string());
        }

        let data = self.send_public_request("/fapi/v1/fundingRate", &params).await?;

        let rates = data
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|r| {
                        Some(FundingRate {
                            symbol: r["symbol"].as_str()?.to_string(),
                            funding_rate: Decimal::from_str(r["fundingRate"].as_str()?).ok()?,
                            funding_time: DateTime::from_timestamp_millis(r["fundingTime"].as_i64()?)?,
                            next_funding_time: None,
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();

        Ok(rates)
    }

    async fn get_klines(&self, symbol: &str, interval: &str, limit: Option<u32>) -> Result<Vec<Kline>, ExchangeError> {
        let mut params = HashMap::new();
        params.insert("symbol".to_string(), symbol.to_string());
        params.insert("interval".to_string(), interval.to_string());
        if let Some(l) = limit {
            params.insert("limit".to_string(), l.to_string());
        }

        let data = self.send_public_request("/fapi/v1/klines", &params).await?;

        let klines = data
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|k| {
                        let k_arr = k.as_array()?;
                        if k_arr.len() < 12 {
                            return None;
                        }
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

    async fn get_order_book(&self, symbol: &str, limit: Option<u32>) -> Result<OrderBook, ExchangeError> {
        let mut params = HashMap::new();
        params.insert("symbol".to_string(), symbol.to_string());
        if let Some(l) = limit {
            params.insert("limit".to_string(), l.to_string());
        }

        let data = self.send_public_request("/fapi/v1/depth", &params).await?;

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

    async fn get_recent_trades(&self, symbol: &str, limit: Option<u32>) -> Result<Vec<PublicTrade>, ExchangeError> {
        let mut params = HashMap::new();
        params.insert("symbol".to_string(), symbol.to_string());
        if let Some(l) = limit {
            params.insert("limit".to_string(), l.to_string());
        }

        let data = self.send_public_request("/fapi/v1/trades", &params).await?;

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
                            is_buyer_maker: t["maker"].as_bool()?,
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();

        Ok(trades)
    }

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

        info!("Connecting to Binance Futures WebSocket: {}", url);

        let (mut ws_stream, _) = connect_async(&url)
            .await
            .map_err(|e| ExchangeError::WebSocketError(format!("Connection failed: {}", e)))?;

        info!("Connected to Binance Futures WebSocket");

        loop {
            tokio::select! {
                msg = ws_stream.next() => {
                    match msg {
                        Some(Ok(Message::Text(text))) => {
                            match serde_json::from_str::<serde_json::Value>(&text) {
                                Ok(data) => {
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
}

/// TradingOperations 实现 - 认证交易操作接口
#[async_trait]
impl TradingOperations for BinanceAdapter {
    async fn get_account(&self) -> Result<AccountInfo, ExchangeError> {
        let params = HashMap::new();
        let data = self.send_signed_request("GET", "/fapi/v2/account", &params).await?;
        self.parse_futures_account(&data)
    }

    async fn get_futures_account(&self) -> Result<FuturesAccountInfo, ExchangeError> {
        let params = HashMap::new();
        let data = self.send_signed_request("GET", "/fapi/v2/account", &params).await?;

        let account_info = self.parse_futures_account(&data)?;

        Ok(FuturesAccountInfo {
            account_info,
            can_trade: data["canTrade"].as_bool().unwrap_or(false),
            can_withdraw: data["canWithdraw"].as_bool().unwrap_or(false),
            fee_tier: data["feeTier"].as_u64().unwrap_or(0) as u32,
            max_withdraw_amount: Decimal::from_str(data["maxWithdrawAmount"].as_str().unwrap_or("0")).unwrap_or_default(),
            total_initial_margin: Decimal::from_str(data["totalInitialMargin"].as_str().unwrap_or("0")).unwrap_or_default(),
            total_maint_margin: Decimal::from_str(data["totalMaintMargin"].as_str().unwrap_or("0")).unwrap_or_default(),
            total_wallet_balance: Decimal::from_str(data["totalWalletBalance"].as_str().unwrap_or("0")).unwrap_or_default(),
            total_unrealized_pnl: Decimal::from_str(data["totalUnrealizedProfit"].as_str().unwrap_or("0")).unwrap_or_default(),
            total_margin_balance: Decimal::from_str(data["totalMarginBalance"].as_str().unwrap_or("0")).unwrap_or_default(),
        })
    }

    async fn get_position(&self, symbol: &str) -> Result<PositionInfo, ExchangeError> {
        let mut params = HashMap::new();
        params.insert("symbol".to_string(), symbol.to_string());

        let data = self.send_signed_request("GET", "/fapi/v2/positionRisk", &params).await?;

        let positions = data.as_array()
            .ok_or_else(|| ExchangeError::ParseError("Expected array response".to_string()))?;

        for pos in positions {
            if let Some(info) = self.parse_position(pos) {
                if info.symbol == symbol {
                    return Ok(info);
                }
            }
        }

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

    async fn get_positions(&self) -> Result<Vec<PositionInfo>, ExchangeError> {
        let params = HashMap::new();
        let data = self.send_signed_request("GET", "/fapi/v2/positionRisk", &params).await?;

        let positions = data.as_array()
            .ok_or_else(|| ExchangeError::ParseError("Expected array response".to_string()))?;

        Ok(positions.iter().filter_map(|pos| self.parse_position(pos)).collect())
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

        params.insert("newOrderRespType".to_string(), "RESULT".to_string());

        let data = self.send_signed_form_request("POST", "/fapi/v1/order", &params).await?;

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
            avg_price: Decimal::from_str(data["avgPrice"].as_str().unwrap_or("0")).ok().filter(|d| *d > Decimal::ZERO),
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

        self.send_signed_request("DELETE", "/fapi/v1/order", &params).await?;
        Ok(())
    }

    async fn cancel_all_orders(&self, symbol: Option<&str>) -> Result<(), ExchangeError> {
        if let Some(s) = symbol {
            let mut params = HashMap::new();
            params.insert("symbol".to_string(), s.to_string());
            self.send_signed_request("DELETE", "/fapi/v1/allOpenOrders", &params).await?;
        } else {
            return Err(ExchangeError::InvalidOrder("Symbol is required for cancel_all_orders".to_string()));
        }
        Ok(())
    }

    async fn get_open_orders(&self, symbol: Option<&str>) -> Result<Vec<OrderInfo>, ExchangeError> {
        let mut params = HashMap::new();
        if let Some(s) = symbol {
            params.insert("symbol".to_string(), s.to_string());
        }

        let data = self.send_signed_request("GET", "/fapi/v1/openOrders", &params).await?;

        let orders = data
            .as_array()
            .map(|arr| arr.iter().filter_map(|o| self.parse_order_info(o)).collect())
            .unwrap_or_default();

        Ok(orders)
    }

    async fn get_order(&self, symbol: &str, order_id: &str) -> Result<OrderInfo, ExchangeError> {
        let mut params = HashMap::new();
        params.insert("symbol".to_string(), symbol.to_string());
        params.insert("orderId".to_string(), order_id.to_string());

        let data = self.send_signed_request("GET", "/fapi/v1/order", &params).await?;

        self.parse_order_info(&data)
            .ok_or_else(|| ExchangeError::OrderNotFound(order_id.to_string()))
    }

    async fn get_all_orders(&self, symbol: &str, limit: Option<u32>) -> Result<Vec<OrderInfo>, ExchangeError> {
        let mut params = HashMap::new();
        params.insert("symbol".to_string(), symbol.to_string());
        if let Some(l) = limit {
            params.insert("limit".to_string(), l.to_string());
        }

        let data = self.send_signed_request("GET", "/fapi/v1/allOrders", &params).await?;

        let orders = data
            .as_array()
            .map(|arr| arr.iter().filter_map(|o| self.parse_order_info(o)).collect())
            .unwrap_or_default();

        Ok(orders)
    }

    async fn get_trade_history(&self, symbol: &str, limit: Option<u32>) -> Result<Vec<TradeInfo>, ExchangeError> {
        let mut params = HashMap::new();
        params.insert("symbol".to_string(), symbol.to_string());
        if let Some(l) = limit {
            params.insert("limit".to_string(), l.to_string());
        }

        let data = self.send_signed_request("GET", "/fapi/v1/userTrades", &params).await?;

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
                            is_buyer: t["buyer"].as_bool().unwrap_or(false),
                            is_maker: t["maker"].as_bool().unwrap_or(false),
                            realized_pnl: Decimal::from_str(t["realizedPnl"].as_str().unwrap_or("0")).unwrap_or_default(),
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();

        Ok(trades)
    }

    async fn batch_place_orders(&self, orders: Vec<BatchOrderRequest>) -> Result<Vec<BatchOrderResult>, ExchangeError> {
        if orders.is_empty() {
            return Ok(Vec::new());
        }

        if orders.len() > 5 {
            return Err(ExchangeError::InvalidOrder("Batch order limit is 5".to_string()));
        }

        let symbol = orders[0].symbol.clone();
        let mut params = HashMap::new();
        params.insert("symbol".to_string(), symbol.clone());

        let batch: Vec<serde_json::Value> = orders.iter().enumerate().map(|(_i, o)| {
            let mut order = serde_json::json!({
                "symbol": o.symbol,
                "side": o.side.to_string(),
                "type": o.order_type.to_string(),
                "quantity": o.quantity.to_string(),
            });
            if let Some(price) = o.price {
                order["price"] = serde_json::json!(price.to_string());
            }
            if let Some(stop_price) = o.stop_price {
                order["stopPrice"] = serde_json::json!(stop_price.to_string());
            }
            if let Some(tif) = &o.time_in_force {
                order["timeInForce"] = serde_json::json!(tif.to_string());
            }
            if let Some(cid) = &o.client_order_id {
                order["newClientOrderId"] = serde_json::json!(cid);
            }
            order
        }).collect();

        let mut query_params = HashMap::new();
        query_params.insert("symbol".to_string(), symbol);
        let batch_json = serde_json::to_string(&batch).map_err(|e| ExchangeError::ParseError(e.to_string()))?;
        query_params.insert("batchOrders".to_string(), batch_json);

        let data = self.send_signed_form_request("POST", "/fapi/v1/batchOrders", &query_params).await?;

        let results = data.as_array()
            .map(|arr| {
                arr.iter().map(|r| {
                    BatchOrderResult {
                        order_id: r["orderId"].as_i64().unwrap_or(0).to_string(),
                        client_order_id: r["clientOrderId"].as_str().map(|s| s.to_string()),
                        symbol: r["symbol"].as_str().unwrap_or("").to_string(),
                        status: match r["status"].as_str() {
                            Some("NEW") => OrderStatus::New,
                            Some("FILLED") => OrderStatus::Filled,
                            _ => OrderStatus::New,
                        },
                        error_code: r["code"].as_i64(),
                        error_message: r["msg"].as_str().map(|s| s.to_string()),
                    }
                }).collect()
            })
            .unwrap_or_default();

        Ok(results)
    }

    async fn batch_cancel_orders(&self, symbol: &str, order_ids: Vec<String>) -> Result<Vec<BatchOrderResult>, ExchangeError> {
        if order_ids.is_empty() {
            return Ok(Vec::new());
        }

        let mut query_params = HashMap::new();
        query_params.insert("symbol".to_string(), symbol.to_string());
        let ids_json = serde_json::to_string(&order_ids).map_err(|e| ExchangeError::ParseError(e.to_string()))?;
        query_params.insert("orderIdList".to_string(), ids_json);

        let data = self.send_signed_form_request("DELETE", "/fapi/v1/batchOrders", &query_params).await?;

        let results = data.as_array()
            .map(|arr| {
                arr.iter().map(|r| {
                    BatchOrderResult {
                        order_id: r["orderId"].as_i64().unwrap_or(0).to_string(),
                        client_order_id: r["clientOrderId"].as_str().map(|s| s.to_string()),
                        symbol: r["symbol"].as_str().unwrap_or(symbol).to_string(),
                        status: match r["status"].as_str() {
                            Some("CANCELED") => OrderStatus::Canceled,
                            _ => OrderStatus::Canceled,
                        },
                        error_code: r["code"].as_i64(),
                        error_message: r["msg"].as_str().map(|s| s.to_string()),
                    }
                }).collect()
            })
            .unwrap_or_default();

        Ok(results)
    }

    async fn set_leverage(&self, symbol: &str, leverage: u32) -> Result<(), ExchangeError> {
        let mut params = HashMap::new();
        params.insert("symbol".to_string(), symbol.to_string());
        params.insert("leverage".to_string(), leverage.to_string());

        self.send_signed_form_request("POST", "/fapi/v1/leverage", &params).await?;
        info!("Set leverage for {} to {}x", symbol, leverage);
        Ok(())
    }

    async fn set_margin_type(&self, symbol: &str, margin_type: MarginType) -> Result<(), ExchangeError> {
        let mut params = HashMap::new();
        params.insert("symbol".to_string(), symbol.to_string());
        params.insert("marginType".to_string(), margin_type.to_string());

        match self.send_signed_form_request("POST", "/fapi/v1/marginType", &params).await {
            Ok(_) => {
                info!("Set margin type for {} to {:?}", symbol, margin_type);
                Ok(())
            }
            Err(ExchangeError::ApiError { code: -4046, .. }) => {
                info!("Margin type for {} already set to {:?}", symbol, margin_type);
                Ok(())
            }
            Err(e) => Err(e),
        }
    }

    async fn subscribe_user_data(
        &self,
        order_callback: Box<dyn Fn(OrderUpdate) + Send + Sync>,
        mut shutdown_rx: broadcast::Receiver<()>,
    ) -> Result<(), ExchangeError> {
        let data = self.send_apikey_request("POST", "/fapi/v1/listenKey").await?;
        let listen_key = data["listenKey"]
            .as_str()
            .ok_or_else(|| ExchangeError::ParseError("Missing listenKey".to_string()))?
            .to_string();

        info!("Got listenKey: {}...", &listen_key[..8.min(listen_key.len())]);

        let ws_url = format!("{}/{}", self.ws_url, listen_key);
        let (mut ws_stream, _) = connect_async(&ws_url)
            .await
            .map_err(|e| ExchangeError::WebSocketError(format!("Connection failed: {}", e)))?;

        info!("Connected to Binance user data stream");

        let client = self.client.clone();
        let base_url = self.base_url.clone();
        let api_key = self.config.api_key.clone();
        let keepalive_handle = tokio::spawn(async move {
            let mut interval = tokio::time::interval(Duration::from_secs(30 * 60));
            loop {
                interval.tick().await;
                let url = format!("{}/fapi/v1/listenKey", base_url);
                match client.put(&url).header("X-MBX-APIKEY", &api_key).send().await {
                    Ok(resp) => {
                        if resp.status().is_success() {
                            info!("listenKey keepalive succeeded");
                        } else {
                            warn!("listenKey keepalive failed: {}", resp.status());
                        }
                    }
                    Err(e) => {
                        warn!("listenKey keepalive error: {}", e);
                    }
                }
            }
        });

        loop {
            tokio::select! {
                msg = ws_stream.next() => {
                    match msg {
                        Some(Ok(Message::Text(text))) => {
                            match serde_json::from_str::<serde_json::Value>(&text) {
                                Ok(data) => {
                                    let event_type = data["e"].as_str().unwrap_or("");
                                    match event_type {
                                        "ORDER_TRADE_UPDATE" => {
                                            if let Some(update) = parse_order_update(&data) {
                                                order_callback(update);
                                            }
                                        }
                                        "ACCOUNT_UPDATE" => {
                                            // 解析账户更新：余额和持仓变化
                                            if let Some(account_data) = data.get("a") {
                                                let balances = account_data["B"].as_array();
                                                let positions = account_data["P"].as_array();
                                                let balance_changes: Vec<String> = balances
                                                    .map(|b| b.iter().filter_map(|item| {
                                                        let asset = item["a"].as_str()?;
                                                        let change = item["bc"].as_str()?;
                                                        if change != "0" {
                                                            Some(format!("{}: {}", asset, change))
                                                        } else { None }
                                                    }).collect())
                                                    .unwrap_or_default();
                                                let pos_changes: Vec<String> = positions
                                                    .map(|p| p.iter().filter_map(|item| {
                                                        let sym = item["s"].as_str()?;
                                                        let amount = item["pa"].as_str()?;
                                                        let pnl = item["up"].as_str()?;
                                                        Some(format!("{} amt={} pnl={}", sym, amount, pnl))
                                                    }).collect())
                                                    .unwrap_or_default();
                                                info!(
                                                    "Account update: balances=[{}], positions=[{}]",
                                                    balance_changes.join(", "),
                                                    pos_changes.join(", ")
                                                );
                                            }
                                        }
                                        "MARGIN_CALL" => {
                                            warn!("Margin call received: {:?}", data);
                                        }
                                        _ => {
                                            info!("User data event: {}", event_type);
                                        }
                                    }
                                }
                                Err(e) => {
                                    warn!("Failed to parse user data: {}", e);
                                }
                            }
                        }
                        Some(Ok(Message::Ping(data))) => {
                            let _ = ws_stream.send(Message::Pong(data)).await;
                        }
                        Some(Ok(Message::Close(_))) => {
                            info!("User data WebSocket closed");
                            break;
                        }
                        Some(Err(e)) => {
                            error!("User data WebSocket error: {}", e);
                            return Err(ExchangeError::WebSocketError(e.to_string()));
                        }
                        None => {
                            info!("User data WebSocket stream ended");
                            break;
                        }
                        _ => {}
                    }
                }
                _ = shutdown_rx.recv() => {
                    info!("Shutdown signal received, closing user data stream");
                    let _ = ws_stream.close(None).await;
                    break;
                }
            }
        }

        keepalive_handle.abort();
        let _ = self.send_apikey_request("DELETE", "/fapi/v1/listenKey").await;

        Ok(())
    }
}

/// 解析交易数据 (WebSocket @trade stream)
fn parse_trade_data(data: &serde_json::Value) -> Option<TickData> {
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

/// 解析订单更新 (WebSocket ORDER_TRADE_UPDATE 事件)
fn parse_order_update(data: &serde_json::Value) -> Option<OrderUpdate> {
    let order = data.get("o")?;

    let symbol = order["s"].as_str()?.to_string();
    let side = match order["S"].as_str()? {
        "BUY" => OrderSide::Buy,
        "SELL" => OrderSide::Sell,
        _ => return None,
    };
    let order_type = match order["o"].as_str()? {
        "LIMIT" => OrderType::Limit,
        "MARKET" => OrderType::Market,
        "STOP" => OrderType::StopLoss,
        "STOP_MARKET" => OrderType::StopLoss,
        "TAKE_PROFIT" => OrderType::TakeProfit,
        "TAKE_PROFIT_MARKET" => OrderType::TakeProfit,
        "TRAILING_STOP_MARKET" => OrderType::StopLoss,
        _ => OrderType::Market,
    };
    let status = match order["X"].as_str()? {
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
        order_id: order["i"].as_i64()?.to_string(),
        client_order_id: order["c"].as_str().map(|s| s.to_string()),
        symbol,
        side,
        order_type,
        status,
        quantity: Decimal::from_str(order["q"].as_str()?).ok()?,
        filled_quantity: Decimal::from_str(order["z"].as_str().unwrap_or("0")).unwrap_or_default(),
        price: Decimal::from_str(order["p"].as_str().unwrap_or("0")).ok(),
        avg_price: Decimal::from_str(order["ap"].as_str().unwrap_or("0")).ok().filter(|d| *d > Decimal::ZERO),
        commission: Decimal::from_str(order["n"].as_str().unwrap_or("0")).ok(),
        commission_asset: order["N"].as_str().map(|s| s.to_string()),
        timestamp: DateTime::from_timestamp_millis(data["T"].as_i64()?)?,
    })
}
