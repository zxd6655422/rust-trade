//! OKX 交易所 API 客户端
//!
//! 实现 OKX V5 API 接口

use anyhow::{anyhow, Result};
use base64::Engine;
use reqwest::Client;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tracing::{debug, error, info, warn};

// =================================================================
// OKX 配置
// =================================================================

/// OKX API 配置
#[derive(Debug, Clone)]
pub struct OkxConfig {
    pub api_key: String,
    pub api_secret: String,
    pub passphrase: String,
    pub base_url: String,
    pub testnet: bool,
}

impl OkxConfig {
    /// 从环境变量加载 OKX 配置
    pub fn from_env() -> Result<Self> {
        let api_key = std::env::var("OKX_API_KEY")
            .map_err(|_| anyhow!("OKX_API_KEY not set"))?;
        let api_secret = std::env::var("OKX_API_SECRET")
            .map_err(|_| anyhow!("OKX_API_SECRET not set"))?;
        let passphrase = std::env::var("OKX_PASSPHRASE")
            .map_err(|_| anyhow!("OKX_PASSPHRASE not set"))?;
        let testnet = std::env::var("OKX_TESTNET")
            .unwrap_or_else(|_| "false".to_string())
            .parse()
            .unwrap_or(false);

        let base_url = if testnet {
            "https://www.okx.com".to_string() // OKX 没有单独的测试网 URL
        } else {
            "https://www.okx.com".to_string()
        };

        Ok(Self {
            api_key,
            api_secret,
            passphrase,
            base_url,
            testnet,
        })
    }
}

// =================================================================
// OKX API 响应类型
// =================================================================

/// OKX 通用响应
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OkxResponse<T> {
    pub code: String,
    pub msg: String,
    pub data: Vec<T>,
}

/// OKX 账户余额
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OkxBalance {
    #[serde(rename = "totalEq")]
    pub total_eq: String,
    #[serde(rename = "adjEq")]
    pub adj_eq: String,
    #[serde(rename = "availBal")]
    pub avail_bal: String,
}

/// OKX 账户详情
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OkxAccountDetail {
    pub acctId: String,
    pub totalEq: String,
    pub adjEq: String,
}

/// OKX 持仓信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OkxPosition {
    pub instId: String,
    pub pos: String,
    #[serde(rename = "avgPx")]
    pub avg_px: String,
    #[serde(rename = "upl")]
    pub upl: String,
    pub lever: String,
    #[serde(rename = "mgnMode")]
    pub mgn_mode: String,
}

/// OKX 订单信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OkxOrder {
    #[serde(rename = "ordId")]
    pub ord_id: String,
    pub instId: String,
    pub side: String,
    #[serde(rename = "ordType")]
    pub ord_type: String,
    pub sz: String,
    pub px: String,
    pub state: String,
    #[serde(rename = "fillSz")]
    pub fill_sz: String,
    #[serde(rename = "avgPx")]
    pub avg_px: String,
    #[serde(rename = "cTime")]
    pub c_time: String,
}

/// OKX 交易对信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OkxInstrument {
    #[serde(rename = "instId")]
    pub inst_id: String,
    #[serde(rename = "baseCcy")]
    pub base_ccy: String,
    #[serde(rename = "quoteCcy")]
    pub quote_ccy: String,
    #[serde(rename = "lotSz")]
    pub lot_sz: String,
    #[serde(rename = "minSz")]
    pub min_sz: String,
    #[serde(rename = "tickSz")]
    pub tick_sz: String,
}

// =================================================================
// OKX 客户端
// =================================================================

/// OKX API 客户端
pub struct OkxClient {
    config: OkxConfig,
    http_client: Client,
}

impl OkxClient {
    pub fn new(config: OkxConfig) -> Self {
        Self {
            config,
            http_client: Client::new(),
        }
    }

    /// 生成签名
    fn sign(&self, timestamp: &str, method: &str, path: &str, body: &str) -> String {
        use hmac::{Hmac, Mac};
        use sha2::Sha256;

        let message = format!("{}{}{}{}", timestamp, method, path, body);
        let mut mac = Hmac::<Sha256>::new_from_slice(self.config.api_secret.as_bytes())
            .expect("HMAC can take key of any size");
        mac.update(message.as_bytes());
        let result = mac.finalize();
        base64::engine::general_purpose::STANDARD.encode(result.into_bytes())
    }

    /// 发送请求
    async fn request<T: for<'de> Deserialize<'de>>(
        &self,
        method: &str,
        path: &str,
        body: Option<&str>,
    ) -> Result<T> {
        let timestamp = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string();
        let signature = self.sign(&timestamp, method, path, body.unwrap_or(""));

        let url = format!("{}{}", self.config.base_url, path);

        let mut request = match method {
            "GET" => self.http_client.get(&url),
            "POST" => self.http_client.post(&url),
            _ => return Err(anyhow!("Unsupported HTTP method: {}", method)),
        };

        request = request
            .header("OK-ACCESS-KEY", &self.config.api_key)
            .header("OK-ACCESS-SIGN", &signature)
            .header("OK-ACCESS-TIMESTAMP", &timestamp)
            .header("OK-ACCESS-PASSPHRASE", &self.config.passphrase);

        if let Some(body_content) = body {
            request = request
                .header("Content-Type", "application/json")
                .body(body_content.to_string());
        }

        let response = request.send().await?;

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await?;
            error!("OKX API error: {} - {}", status, body);
            return Err(anyhow!("OKX API error: {} - {}", status, body));
        }

        let result = response.json::<T>().await?;
        Ok(result)
    }

    /// 获取账户余额
    pub async fn get_balance(&self) -> Result<Decimal> {
        let response: OkxResponse<OkxAccountDetail> = self
            .request("GET", "/api/v5/account/balance", None)
            .await?;

        if let Some(account) = response.data.first() {
            let total_eq: Decimal = account.totalEq.parse().unwrap_or(Decimal::ZERO);
            Ok(total_eq)
        } else {
            Ok(Decimal::ZERO)
        }
    }

    /// 获取持仓信息
    pub async fn get_positions(&self) -> Result<Vec<OkxPosition>> {
        let response: OkxResponse<OkxPosition> = self
            .request("GET", "/api/v5/account/positions", None)
            .await?;

        Ok(response.data)
    }

    /// 获取订单信息
    pub async fn get_order(&self, inst_id: &str, ord_id: &str) -> Result<OkxOrder> {
        let path = format!("/api/v5/trade/order?instId={}&ordId={}", inst_id, ord_id);
        let response: OkxResponse<OkxOrder> = self
            .request("GET", &path, None)
            .await?;

        response.data.into_iter().next()
            .ok_or_else(|| anyhow!("Order not found"))
    }

    /// 获取未成交订单
    pub async fn get_open_orders(&self, inst_id: Option<&str>) -> Result<Vec<OkxOrder>> {
        let path = if let Some(id) = inst_id {
            format!("/api/v5/trade/orders-pending?instId={}", id)
        } else {
            "/api/v5/trade/orders-pending".to_string()
        };

        let response: OkxResponse<OkxOrder> = self
            .request("GET", &path, None)
            .await?;

        Ok(response.data)
    }

    /// 下单
    pub async fn place_order(
        &self,
        inst_id: &str,
        side: &str,
        order_type: &str,
        size: &str,
        price: Option<&str>,
        reduce_only: bool,
    ) -> Result<String> {
        let mut order = serde_json::json!({
            "instId": inst_id,
            "tdMode": "cross",
            "side": side,
            "ordType": order_type,
            "sz": size,
        });

        if let Some(px) = price {
            order["px"] = serde_json::Value::String(px.to_string());
        }

        if reduce_only {
            order["reduceOnly"] = serde_json::Value::Bool(true);
        }

        let body = serde_json::to_string(&order)?;

        #[derive(Deserialize)]
        struct OrderResponse {
            #[serde(rename = "ordId")]
            ord_id: String,
        }

        let response: OkxResponse<OrderResponse> = self
            .request("POST", "/api/v5/trade/order", Some(&body))
            .await?;

        response.data.into_iter().next()
            .map(|r| r.ord_id)
            .ok_or_else(|| anyhow!("No order ID returned"))
    }

    /// 撤单
    pub async fn cancel_order(&self, inst_id: &str, ord_id: &str) -> Result<()> {
        let body = serde_json::json!({
            "instId": inst_id,
            "ordId": ord_id,
        });

        let body_str = serde_json::to_string(&body)?;

        let _: OkxResponse<serde_json::Value> = self
            .request("POST", "/api/v5/trade/cancel-order", Some(&body_str))
            .await?;

        Ok(())
    }

    /// 获取交易对信息
    pub async fn get_instruments(&self, inst_type: &str) -> Result<Vec<OkxInstrument>> {
        let path = format!("/api/v5/public/instruments?instType={}", inst_type);
        let response: OkxResponse<OkxInstrument> = self
            .request("GET", &path, None)
            .await?;

        Ok(response.data)
    }
}
