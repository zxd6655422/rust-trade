// okx_account.rs
//
// OKX 账户信息提供者实现
// 实现统一的 AccountProvider 接口

use anyhow::{anyhow, Result};
use base64::Engine;
use chrono::Utc;
use reqwest::Client;
use rust_decimal::Decimal;
use serde::Deserialize;
use tracing::{debug, warn};

use trading_common::data::account_types::{
    AccountProvider, AccountSnapshot, AssetBalance, MarginType, PositionInfo, PositionSide,
};
use trading_common::data::types::DataResult;

use crate::okx_client::OkxConfig;

// =================================================================
// OKX 账户信息提供者
// =================================================================

pub struct OkxAccountProvider {
    config: OkxConfig,
    http_client: Client,
}

impl OkxAccountProvider {
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
            return Err(anyhow!("OKX API error: {} - {}", status, body));
        }

        let result = response.json::<T>().await?;
        Ok(result)
    }

    /// 统一 OKX symbol 格式
    /// BTC-USDT-SWAP -> BTCUSDT
    /// BTC-USDT -> BTCUSDT
    fn normalize_okx_symbol(&self, inst_id: &str) -> String {
        let parts: Vec<&str> = inst_id.split('-').collect();
        if parts.len() >= 2 {
            format!("{}{}", parts[0], parts[1])
        } else {
            inst_id.to_string()
        }
    }
}

// =================================================================
// OKX API 响应类型
// =================================================================

/// OKX 通用响应
#[derive(Debug, Clone, Deserialize)]
struct OkxResponse<T> {
    code: String,
    msg: String,
    data: Vec<T>,
}

/// OKX 账户余额响应
#[derive(Debug, Clone, Deserialize)]
struct OkxBalanceData {
    #[serde(rename = "uTime")]
    u_time: String,
    #[serde(rename = "totalEq")]
    total_eq: String,
    #[serde(rename = "adjEq")]
    adj_eq: String,
    #[serde(rename = "availEq")]
    avail_eq: String,
    #[serde(rename = "imr")]
    imr: String,
    #[serde(rename = "mmr")]
    mmr: String,
    #[serde(rename = "mgnRatio")]
    mgn_ratio: String,
    #[serde(rename = "notionalUsd")]
    notional_usd: String,
    details: Vec<OkxBalanceDetail>,
}

/// OKX 资产详情
#[derive(Debug, Clone, Deserialize)]
struct OkxBalanceDetail {
    #[serde(rename = "ccy")]
    ccy: String,
    #[serde(rename = "availBal")]
    avail_bal: String,
    #[serde(rename = "frozenBal")]
    frozen_bal: String,
    #[serde(rename = "eq")]
    eq: String,
    #[serde(rename = "eqUsd")]
    eq_usd: String,
    #[serde(rename = "upl")]
    upl: String,
}

/// OKX 持仓响应
#[derive(Debug, Clone, Deserialize)]
struct OkxPositionData {
    #[serde(rename = "instId")]
    inst_id: String,
    #[serde(rename = "instType")]
    inst_type: String,
    #[serde(rename = "pos")]
    pos: String,
    #[serde(rename = "avgPx")]
    avg_px: String,
    #[serde(rename = "markPx")]
    mark_px: String,
    #[serde(rename = "upl")]
    upl: String,
    #[serde(rename = "lever")]
    lever: String,
    #[serde(rename = "mgnMode")]
    mgn_mode: String,
    #[serde(rename = "posSide")]
    pos_side: String,
    #[serde(rename = "liqPx")]
    liq_px: String,
    #[serde(rename = "notionalUsd")]
    notional_usd: String,
    #[serde(rename = "imr")]
    imr: String,
    #[serde(rename = "mmr")]
    mmr: String,
    #[serde(rename = "margin")]
    margin: String,
}

#[async_trait::async_trait]
impl AccountProvider for OkxAccountProvider {
    async fn get_account_snapshot(&self, market_type: &str) -> DataResult<AccountSnapshot> {
        match market_type {
            "swap" | "futures" => self.get_swap_snapshot().await,
            "spot" => self.get_spot_snapshot().await,
            _ => Err(trading_common::data::types::DataError::Validation(
                format!("Unsupported market_type: {}", market_type),
            )),
        }
    }

    async fn get_asset_balances(&self, market_type: &str) -> DataResult<Vec<AssetBalance>> {
        match market_type {
            "swap" | "futures" => self.get_swap_balances().await,
            "spot" => self.get_spot_balances().await,
            _ => Err(trading_common::data::types::DataError::Validation(
                format!("Unsupported market_type: {}", market_type),
            )),
        }
    }

    async fn get_positions(&self) -> DataResult<Vec<PositionInfo>> {
        self.get_swap_positions().await
    }

    fn normalize_symbol(&self, raw_symbol: &str) -> String {
        self.normalize_okx_symbol(raw_symbol)
    }
}

// =================================================================
// OKX 合约账户实现
// =================================================================

impl OkxAccountProvider {
    /// 获取合约账户快照
    async fn get_swap_snapshot(&self) -> DataResult<AccountSnapshot> {
        let path = "/api/v5/account/balance?instType=SWAP";
        let response: OkxResponse<OkxBalanceData> = self
            .request("GET", path, None)
            .await
            .map_err(|e| trading_common::data::types::DataError::Database(
                sqlx::Error::Protocol(e.to_string())
            ))?;

        let data = response.data.first()
            .ok_or_else(|| trading_common::data::types::DataError::NotFound("No account data".to_string()))?;

        let total_equity: Decimal = data.total_eq.parse().unwrap_or(Decimal::ZERO);
        let adj_eq: Decimal = data.adj_eq.parse().unwrap_or(Decimal::ZERO);
        let imr: Decimal = data.imr.parse().unwrap_or(Decimal::ZERO);
        let mmr: Decimal = data.mmr.parse().unwrap_or(Decimal::ZERO);
        let mgn_ratio: Decimal = data.mgn_ratio.parse().unwrap_or(Decimal::ZERO);

        // 计算可用余额和冻结余额
        let mut available = Decimal::ZERO;
        let mut frozen = Decimal::ZERO;
        let mut total_upl = Decimal::ZERO;

        for detail in &data.details {
            let avail: Decimal = detail.avail_bal.parse().unwrap_or(Decimal::ZERO);
            let frz: Decimal = detail.frozen_bal.parse().unwrap_or(Decimal::ZERO);
            let upl: Decimal = detail.upl.parse().unwrap_or(Decimal::ZERO);

            available += avail;
            frozen += frz;
            total_upl += upl;
        }

        // 获取持仓数量
        let positions = self.get_swap_positions().await.unwrap_or_default();
        let position_count = positions.len() as i32;

        Ok(AccountSnapshot {
            exchange: "okx".to_string(),
            market_type: "swap".to_string(),
            snapshot_at: Utc::now(),
            total_equity,
            total_balance: total_equity - total_upl,  // 总余额 = 总权益 - 未实现盈亏
            available_balance: available,
            frozen_balance: frozen,
            unrealized_pnl: total_upl,
            initial_margin: Some(imr),
            maint_margin: Some(mmr),
            margin_ratio: if mgn_ratio > Decimal::ZERO { Some(mgn_ratio) } else { None },
            position_count,
            raw_data: None,
        })
    }

    /// 获取合约资产余额
    async fn get_swap_balances(&self) -> DataResult<Vec<AssetBalance>> {
        let path = "/api/v5/account/balance?instType=SWAP";
        let response: OkxResponse<OkxBalanceData> = self
            .request("GET", path, None)
            .await
            .map_err(|e| trading_common::data::types::DataError::Database(
                sqlx::Error::Protocol(e.to_string())
            ))?;

        let data = match response.data.first() {
            Some(d) => d,
            None => return Ok(Vec::new()),
        };

        let balances: Vec<AssetBalance> = data.details.iter()
            .filter(|d| {
                let eq: Decimal = d.eq.parse().unwrap_or(Decimal::ZERO);
                eq > Decimal::ZERO
            })
            .map(|d| {
                let total: Decimal = d.eq.parse().unwrap_or(Decimal::ZERO);
                let available: Decimal = d.avail_bal.parse().unwrap_or(Decimal::ZERO);
                let frozen: Decimal = d.frozen_bal.parse().unwrap_or(Decimal::ZERO);
                let upl: Decimal = d.upl.parse().unwrap_or(Decimal::ZERO);
                let eq_usd: Decimal = d.eq_usd.parse().unwrap_or(Decimal::ZERO);

                AssetBalance {
                    exchange: "okx".to_string(),
                    market_type: "swap".to_string(),
                    asset: d.ccy.clone(),
                    snapshot_at: Utc::now(),
                    total,
                    available,
                    frozen,
                    unrealized_pnl: upl,
                    usd_value: Some(eq_usd),
                }
            })
            .collect();

        Ok(balances)
    }

    /// 获取合约持仓
    async fn get_swap_positions(&self) -> DataResult<Vec<PositionInfo>> {
        let path = "/api/v5/account/positions?instType=SWAP";
        let response: OkxResponse<OkxPositionData> = self
            .request("GET", path, None)
            .await
            .map_err(|e| trading_common::data::types::DataError::Database(
                sqlx::Error::Protocol(e.to_string())
            ))?;

        let positions: Vec<PositionInfo> = response.data.iter()
            .filter(|p| {
                let pos: Decimal = p.pos.parse().unwrap_or(Decimal::ZERO);
                pos != Decimal::ZERO
            })
            .map(|p| {
                let position_amt: Decimal = p.pos.parse().unwrap_or(Decimal::ZERO);
                let entry_price: Decimal = p.avg_px.parse().unwrap_or(Decimal::ZERO);
                let mark_price: Decimal = p.mark_px.parse().unwrap_or(Decimal::ZERO);
                let upl: Decimal = p.upl.parse().unwrap_or(Decimal::ZERO);
                let leverage: u32 = p.lever.parse().unwrap_or(1);
                let liq_price: Decimal = p.liq_px.parse().unwrap_or(Decimal::ZERO);
                let notional: Decimal = p.notional_usd.parse().unwrap_or(Decimal::ZERO);
                let im: Decimal = p.imr.parse().unwrap_or(Decimal::ZERO);
                let mm: Decimal = p.mmr.parse().unwrap_or(Decimal::ZERO);

                PositionInfo {
                    exchange: "okx".to_string(),
                    symbol: self.normalize_okx_symbol(&p.inst_id),
                    raw_symbol: p.inst_id.clone(),
                    snapshot_at: Utc::now(),
                    position_side: PositionSide::from_str(&p.pos_side),
                    position_amt,
                    entry_price,
                    mark_price,
                    unrealized_pnl: upl,
                    leverage,
                    margin_type: MarginType::from_str(&p.mgn_mode),
                    initial_margin: im,
                    maint_margin: mm,
                    liquidation_price: if liq_price > Decimal::ZERO { Some(liq_price) } else { None },
                    notional,
                    raw_data: None,
                }
            })
            .collect();

        Ok(positions)
    }
}

// =================================================================
// OKX 现货账户实现
// =================================================================

impl OkxAccountProvider {
    /// 获取现货账户快照
    async fn get_spot_snapshot(&self) -> DataResult<AccountSnapshot> {
        let path = "/api/v5/account/balance?instType=SPOT";
        let response: OkxResponse<OkxBalanceData> = self
            .request("GET", path, None)
            .await
            .map_err(|e| trading_common::data::types::DataError::Database(
                sqlx::Error::Protocol(e.to_string())
            ))?;

        let data = response.data.first()
            .ok_or_else(|| trading_common::data::types::DataError::NotFound("No account data".to_string()))?;

        let total_equity: Decimal = data.total_eq.parse().unwrap_or(Decimal::ZERO);

        // 计算可用余额和冻结余额
        let mut available = Decimal::ZERO;
        let mut frozen = Decimal::ZERO;

        for detail in &data.details {
            let avail: Decimal = detail.avail_bal.parse().unwrap_or(Decimal::ZERO);
            let frz: Decimal = detail.frozen_bal.parse().unwrap_or(Decimal::ZERO);

            available += avail;
            frozen += frz;
        }

        Ok(AccountSnapshot {
            exchange: "okx".to_string(),
            market_type: "spot".to_string(),
            snapshot_at: Utc::now(),
            total_equity,
            total_balance: total_equity,
            available_balance: available,
            frozen_balance: frozen,
            unrealized_pnl: Decimal::ZERO,
            initial_margin: None,
            maint_margin: None,
            margin_ratio: None,
            position_count: 0,
            raw_data: None,
        })
    }

    /// 获取现货资产余额
    async fn get_spot_balances(&self) -> DataResult<Vec<AssetBalance>> {
        let path = "/api/v5/account/balance?instType=SPOT";
        let response: OkxResponse<OkxBalanceData> = self
            .request("GET", path, None)
            .await
            .map_err(|e| trading_common::data::types::DataError::Database(
                sqlx::Error::Protocol(e.to_string())
            ))?;

        let data = match response.data.first() {
            Some(d) => d,
            None => return Ok(Vec::new()),
        };

        let balances: Vec<AssetBalance> = data.details.iter()
            .filter(|d| {
                let eq: Decimal = d.eq.parse().unwrap_or(Decimal::ZERO);
                eq > Decimal::ZERO
            })
            .map(|d| {
                let total: Decimal = d.eq.parse().unwrap_or(Decimal::ZERO);
                let available: Decimal = d.avail_bal.parse().unwrap_or(Decimal::ZERO);
                let frozen: Decimal = d.frozen_bal.parse().unwrap_or(Decimal::ZERO);
                let eq_usd: Decimal = d.eq_usd.parse().unwrap_or(Decimal::ZERO);

                AssetBalance {
                    exchange: "okx".to_string(),
                    market_type: "spot".to_string(),
                    asset: d.ccy.clone(),
                    snapshot_at: Utc::now(),
                    total,
                    available,
                    frozen,
                    unrealized_pnl: Decimal::ZERO,
                    usd_value: Some(eq_usd),
                }
            })
            .collect();

        Ok(balances)
    }
}
