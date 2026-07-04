// exchange/adapters/bybit_adapter.rs
// Bybit 交易所适配器实现
// 支持 Bybit V5 API (统一账户)
// 文档: https://bybit-exchange.github.io/docs/v5/

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
use crate::exchange::traits::{Exchange, SymbolPrecision};
use crate::exchange::types::*;
use trading_common::data::types::TickData;

type HmacSha256 = Hmac<Sha256>;

/// Bybit 配置
#[derive(Debug, Clone)]
pub struct BybitConfig {
    pub api_key: String,
    pub api_secret: String,
    pub testnet: bool,
    pub recv_window: u64,
    pub timeout: Duration,
}

impl Default for BybitConfig {
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

/// Bybit 适配器
pub struct BybitAdapter {
    config: BybitConfig,
    client: Client,
    base_url: String,
    ws_url: String,
}

impl BybitAdapter {
    /// 创建新的 Bybit 适配器
    pub fn new(config: BybitConfig) -> Result<Self, ExchangeError> {
        let client = Client::builder()
            .timeout(config.timeout)
            .build()
            .map_err(|e| ExchangeError::NetworkError(format!("Failed to create HTTP client: {}", e)))?;

        // Bybit V5 API URLs
        let (base_url, ws_url) = if config.testnet {
            (
                "https://api-testnet.bybit.com".to_string(),
                "wss://stream-testnet.bybit.com/v5/public/linear".to_string(),
            )
        } else {
            (
                "https://api.bybit.com".to_string(),
                "wss://stream.bybit.com/v5/public/linear".to_string(),
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
    fn sign(&self, timestamp: &str, params: &str) -> Result<String, ExchangeError> {
        let sign_str = format!("{}{}{}{}", timestamp, self.config.api_key, self.config.recv_window, params);
        let mut mac = HmacSha256::new_from_slice(self.config.api_secret.as_bytes())
            .map_err(|e| ExchangeError::SignatureError(format!("Invalid key length: {}", e)))?;
        mac.update(sign_str.as_bytes());
        let result = mac.finalize();
        Ok(hex::encode(result.into_bytes()))
    }

    /// 发送签名请求
    async fn send_signed_request(
        &self,
        method: &str,
        endpoint: &str,
        params: &HashMap<String, String>,
    ) -> Result<serde_json::Value, ExchangeError> {
        let timestamp = Utc::now().timestamp_millis().to_string();
        let query_string: String = params
            .iter()
            .map(|(k, v)| format!("{}={}", k, v))
            .collect::<Vec<_>>()
            .join("&");

        let signature = self.sign(&timestamp, &query_string)?;
        let url = format!("{}{}", self.base_url, endpoint);

        let response = match method {
            "GET" => {
                let full_url = if query_string.is_empty() {
                    url
                } else {
                    format!("{}?{}", url, query_string)
                };
                self.client
                    .get(&full_url)
                    .header("X-BAPI-API-KEY", &self.config.api_key)
                    .header("X-BAPI-TIMESTAMP", &timestamp)
                    .header("X-BAPI-RECV-WINDOW", self.config.recv_window.to_string())
                    .header("X-BAPI-SIGN", &signature)
                    .send()
                    .await?
            }
            "POST" => {
                self.client
                    .post(&url)
                    .header("X-BAPI-API-KEY", &self.config.api_key)
                    .header("X-BAPI-TIMESTAMP", &timestamp)
                    .header("X-BAPI-RECV-WINDOW", self.config.recv_window.to_string())
                    .header("X-BAPI-SIGN", &signature)
                    .header("Content-Type", "application/json")
                    .json(&serde_json::json!({
                        "params": params
                    }))
                    .send()
                    .await?
            }
            _ => return Err(ExchangeError::InvalidOrder(format!("Unsupported method: {}", method))),
        };

        let status = response.status();
        let body = response.text().await?;

        if !status.is_success() {
            let error_response: serde_json::Value = serde_json::from_str(&body)
                .unwrap_or_else(|_| serde_json::json!({"retMsg": body}));

            let code = error_response["retCode"].as_i64().unwrap_or(-1);
            let message = error_response["retMsg"]
                .as_str()
                .unwrap_or("Unknown error")
                .to_string();

            return Err(ExchangeError::ApiError { code, message });
        }

        serde_json::from_str(&body).map_err(|e| ExchangeError::ParseError(e.to_string()))
    }

    /// 发送公共请求 (无需签名)
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
            return Err(ExchangeError::NetworkError(format!("HTTP {}: {}", status, body)));
        }

        serde_json::from_str(&body).map_err(|e| ExchangeError::ParseError(e.to_string()))
    }
}

// 注意: Exchange trait 的完整实现需要较多代码
// 这里提供骨架实现，具体方法需要根据 Bybit V5 API 文档填充
// 文档: https://bybit-exchange.github.io/docs/v5/

// TODO: 实现 Exchange trait 的所有方法
// - subscribe_trades: WebSocket 订阅行情
// - get_account: GET /v5/account/wallet-balance
// - get_position: GET /v5/position/list
// - place_order: POST /v5/order/create
// - cancel_order: POST /v5/order/cancel
// - get_open_orders: GET /v5/order/realtime
// - subscribe_user_data: WebSocket 订阅用户数据流
// 等等
