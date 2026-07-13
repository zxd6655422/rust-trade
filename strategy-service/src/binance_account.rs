// binance_account.rs
//
// Binance 账户信息提供者实现
// 实现统一的 AccountProvider 接口

use anyhow::{anyhow, Result};
use chrono::Utc;
use reqwest::Client;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tracing::{debug, warn};

use trading_common::data::account_types::{
    AccountProvider, AccountSnapshot, AssetBalance, MarginType, PositionInfo, PositionSide,
};
use trading_common::data::types::DataResult;

use crate::exchange::ExchangeApiConfig;

// =================================================================
// Binance 账户信息提供者
// =================================================================

pub struct BinanceAccountProvider {
    config: ExchangeApiConfig,
    http_client: Client,
}

impl BinanceAccountProvider {
    pub fn new(config: ExchangeApiConfig) -> Self {
        Self {
            config,
            http_client: Client::new(),
        }
    }

    /// 发送签名请求
    async fn signed_request<T: for<'de> Deserialize<'de>>(
        &self,
        method: &str,
        path: &str,
        params: HashMap<String, String>,
    ) -> Result<T> {
        use hmac::{Hmac, Mac};
        use sha2::Sha256;

        type HmacSha256 = Hmac<Sha256>;

        let timestamp = chrono::Utc::now().timestamp_millis().to_string();
        let mut all_params = params.clone();
        all_params.insert("timestamp".to_string(), timestamp.clone());

        // 构建查询字符串
        let query_string: String = all_params
            .iter()
            .map(|(k, v)| format!("{}={}", k, v))
            .collect::<Vec<_>>()
            .join("&");

        // 计算签名
        let mut mac = HmacSha256::new_from_slice(self.config.api_secret.as_bytes())
            .map_err(|e| anyhow!("HMAC error: {}", e))?;
        mac.update(query_string.as_bytes());
        let signature = hex::encode(mac.finalize().into_bytes());

        let url = format!("{}{}?{}&signature={}", self.config.base_url, path, query_string, signature);

        let response = match method {
            "GET" => {
                self.http_client
                    .get(&url)
                    .header("X-MBX-APIKEY", &self.config.api_key)
                    .send()
                    .await?
            }
            _ => return Err(anyhow!("Unsupported method: {}", method)),
        };

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await.unwrap_or_default();
            return Err(anyhow!("HTTP {}: {}", status, body));
        }

        let result: T = response.json().await?;
        Ok(result)
    }
}

#[async_trait::async_trait]
impl AccountProvider for BinanceAccountProvider {
    async fn get_account_snapshot(&self, market_type: &str) -> DataResult<AccountSnapshot> {
        match market_type {
            "futures" => self.get_futures_snapshot().await,
            "spot" => self.get_spot_snapshot().await,
            _ => Err(trading_common::data::types::DataError::Validation(
                format!("Unsupported market_type: {}", market_type),
            )),
        }
    }

    async fn get_asset_balances(&self, market_type: &str) -> DataResult<Vec<AssetBalance>> {
        match market_type {
            "futures" => self.get_futures_balances().await,
            "spot" => self.get_spot_balances().await,
            _ => Err(trading_common::data::types::DataError::Validation(
                format!("Unsupported market_type: {}", market_type),
            )),
        }
    }

    async fn get_positions(&self) -> DataResult<Vec<PositionInfo>> {
        self.get_futures_positions().await
    }

    fn normalize_symbol(&self, raw_symbol: &str) -> String {
        // Binance 格式已经是统一格式
        raw_symbol.to_uppercase()
    }
}

// =================================================================
// Binance 合约账户实现
// =================================================================

/// Binance 合约账户响应
#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct FuturesAccountResponse {
    total_wallet_balance: String,
    total_unrealized_profit: String,
    total_margin_balance: String,
    available_balance: String,
    total_initial_margin: String,
    total_maint_margin: String,
    max_withdraw_amount: String,
    #[serde(default)]
    total_cross_wallet_balance: String,
    assets: Vec<FuturesAssetResponse>,
    positions: Vec<FuturesPositionResponse>,
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct FuturesAssetResponse {
    asset: String,
    wallet_balance: String,
    unrealized_profit: String,
    margin_balance: String,
    available_balance: String,
    cross_wallet_balance: String,
    cross_un_pnl: String,
}

/// Binance /fapi/v2/account 中的持仓
/// 注意：字段名与 /fapi/v2/positionRisk 不同（如 unrealizedProfit vs unRealizedProfit）
#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct FuturesPositionResponse {
    symbol: String,
    #[serde(default)]
    position_amt: String,
    #[serde(default)]
    entry_price: String,
    /// positionAmt=0 时 Binance 可能不返回此字段
    #[serde(default)]
    mark_price: String,
    /// /fapi/v2/account 返回 "unrealizedProfit"（小写 r）
    /// /fapi/v2/positionRisk 返回 "unRealizedProfit"（大写 R）
    #[serde(alias = "unRealizedProfit")]
    #[serde(default)]
    unrealized_profit: String,
    #[serde(default)]
    leverage: String,
    #[serde(default)]
    margin_type: String,
    #[serde(default)]
    position_side: String,
    #[serde(default)]
    liquidation_price: String,
    #[serde(default)]
    notional: String,
    #[serde(default)]
    initial_margin: String,
    #[serde(default)]
    maint_margin: String,
    #[serde(default)]
    break_even_price: String,
    #[serde(default)]
    isolated_wallet: String,
    #[serde(default)]
    isolated_margin: String,
    #[serde(default)]
    max_notional: String,
    #[serde(default)]
    update_time: i64,
}

impl BinanceAccountProvider {
    /// 获取合约账户快照
    async fn get_futures_snapshot(&self) -> DataResult<AccountSnapshot> {
        let params = HashMap::new();
        let response: FuturesAccountResponse = self
            .signed_request("GET", "/fapi/v2/account", params)
            .await
            .map_err(|e| trading_common::data::types::DataError::Database(
                sqlx::Error::Protocol(e.to_string())
            ))?;

        let total_equity = response.total_margin_balance.parse().unwrap_or(Decimal::ZERO);
        let total_balance = response.total_wallet_balance.parse().unwrap_or(Decimal::ZERO);
        let available = response.available_balance.parse().unwrap_or(Decimal::ZERO);
        let unrealized_pnl = response.total_unrealized_profit.parse().unwrap_or(Decimal::ZERO);
        let initial_margin = response.total_initial_margin.parse().ok();
        let maint_margin = response.total_maint_margin.parse().ok();

        // 直接用 API 返回的全仓钱包余额和可用余额计算冻结
        // frozen = crossWalletBalance - availableBalance
        let cross_wallet: Decimal = response.total_cross_wallet_balance.parse().unwrap_or(Decimal::ZERO);
        let frozen = if cross_wallet > Decimal::ZERO {
            (cross_wallet - available).max(Decimal::ZERO)
        } else {
            // fallback: total_balance - available - unrealized_pnl
            (total_balance - available - unrealized_pnl).max(Decimal::ZERO)
        };

        // 计算保证金率
        let margin_ratio = if let (Some(imr), Some(mmr)) = (initial_margin, maint_margin) {
            if mmr > Decimal::ZERO {
                Some(imr / mmr)
            } else {
                None
            }
        } else {
            None
        };

        // 计算持仓数量（有持仓的）
        let position_count = response.positions.iter()
            .filter(|p| p.position_amt.parse::<Decimal>().unwrap_or(Decimal::ZERO) != Decimal::ZERO)
            .count() as i32;

        // 存储原始响应用于调试
        let raw_data = serde_json::to_value(&response).ok();

        Ok(AccountSnapshot {
            exchange: "binance".to_string(),
            market_type: "futures".to_string(),
            snapshot_at: Utc::now(),
            total_equity,
            total_balance,
            available_balance: available,
            frozen_balance: frozen,
            unrealized_pnl,
            initial_margin,
            maint_margin,
            margin_ratio,
            position_count,
            raw_data,
        })
    }

    /// 获取合约资产余额
    async fn get_futures_balances(&self) -> DataResult<Vec<AssetBalance>> {
        let params = HashMap::new();
        let response: FuturesAccountResponse = self
            .signed_request("GET", "/fapi/v2/account", params)
            .await
            .map_err(|e| trading_common::data::types::DataError::Database(
                sqlx::Error::Protocol(e.to_string())
            ))?;

        let balances: Vec<AssetBalance> = response.assets.iter()
            .filter(|a| {
                let total: Decimal = a.wallet_balance.parse().unwrap_or(Decimal::ZERO);
                total > Decimal::ZERO
            })
            .map(|a| {
                let total: Decimal = a.wallet_balance.parse().unwrap_or(Decimal::ZERO);
                let available: Decimal = a.available_balance.parse().unwrap_or(Decimal::ZERO);
                let upl: Decimal = a.unrealized_profit.parse().unwrap_or(Decimal::ZERO);
                let margin: Decimal = a.margin_balance.parse().unwrap_or(Decimal::ZERO);

                AssetBalance {
                    exchange: "binance".to_string(),
                    market_type: "futures".to_string(),
                    asset: a.asset.clone(),
                    snapshot_at: Utc::now(),
                    total,
                    available,
                    frozen: total - available - upl,
                    unrealized_pnl: upl,
                    usd_value: Some(margin),
                }
            })
            .collect();

        Ok(balances)
    }

    /// 获取合约持仓
    async fn get_futures_positions(&self) -> DataResult<Vec<PositionInfo>> {
        let params = HashMap::new();
        let response: FuturesAccountResponse = self
            .signed_request("GET", "/fapi/v2/account", params)
            .await
            .map_err(|e| trading_common::data::types::DataError::Database(
                sqlx::Error::Protocol(e.to_string())
            ))?;

        let positions: Vec<PositionInfo> = response.positions.iter()
            .filter(|p| {
                let amt: Decimal = p.position_amt.parse().unwrap_or(Decimal::ZERO);
                amt != Decimal::ZERO
            })
            .map(|p| {
                let position_amt: Decimal = p.position_amt.parse().unwrap_or(Decimal::ZERO);
                let entry_price: Decimal = p.entry_price.parse().unwrap_or(Decimal::ZERO);
                let mark_price: Decimal = p.mark_price.parse().unwrap_or(Decimal::ZERO);
                let upl: Decimal = p.unrealized_profit.parse().unwrap_or(Decimal::ZERO);
                let leverage: u32 = p.leverage.parse().unwrap_or(1);
                let liq_price: Decimal = p.liquidation_price.parse().unwrap_or(Decimal::ZERO);
                let notional: Decimal = p.notional.parse().unwrap_or(Decimal::ZERO);
                let im: Decimal = p.initial_margin.parse().unwrap_or(Decimal::ZERO);
                let mm: Decimal = p.maint_margin.parse().unwrap_or(Decimal::ZERO);
                let break_even: Decimal = p.break_even_price.parse().unwrap_or(Decimal::ZERO);
                let isolated_wallet: Decimal = p.isolated_wallet.parse().unwrap_or(Decimal::ZERO);

                let raw_data = serde_json::to_value(p).ok();

                PositionInfo {
                    exchange: "binance".to_string(),
                    symbol: p.symbol.clone(),
                    raw_symbol: p.symbol.clone(),
                    snapshot_at: Utc::now(),
                    position_side: PositionSide::from_str(&p.position_side),
                    position_amt,
                    entry_price,
                    mark_price,
                    unrealized_pnl: upl,
                    leverage,
                    margin_type: MarginType::from_str(&p.margin_type),
                    initial_margin: im,
                    maint_margin: mm,
                    liquidation_price: if liq_price > Decimal::ZERO { Some(liq_price) } else { None },
                    notional,
                    break_even_price: Some(break_even),
                    isolated_wallet: Some(isolated_wallet),
                    raw_data,
                }
            })
            .collect();

        Ok(positions)
    }
}

// =================================================================
// Binance 现货账户实现
// =================================================================

/// Binance 现货账户响应
#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct SpotAccountResponse {
    maker_commission: i64,
    taker_commission: i64,
    can_trade: bool,
    can_withdraw: bool,
    can_deposit: bool,
    account_type: String,
    balances: Vec<SpotBalanceResponse>,
}

#[derive(Deserialize, Serialize)]
struct SpotBalanceResponse {
    asset: String,
    free: String,
    locked: String,
}

impl BinanceAccountProvider {
    /// 获取现货账户快照
    async fn get_spot_snapshot(&self) -> DataResult<AccountSnapshot> {
        let params = HashMap::new();
        let response: SpotAccountResponse = self
            .signed_request("GET", "/api/v3/account", params)
            .await
            .map_err(|e| trading_common::data::types::DataError::Database(
                sqlx::Error::Protocol(e.to_string())
            ))?;

        let mut total_balance = Decimal::ZERO;
        let mut available = Decimal::ZERO;
        let mut frozen = Decimal::ZERO;

        for balance in &response.balances {
            let free: Decimal = balance.free.parse().unwrap_or(Decimal::ZERO);
            let locked: Decimal = balance.locked.parse().unwrap_or(Decimal::ZERO);
            let total = free + locked;

            if total > Decimal::ZERO {
                total_balance += total;
                if balance.asset == "USDT" {
                    available += free;
                    frozen += locked;
                }
            }
        }

        // 现货没有未实现盈亏（除非计算持仓的浮动盈亏）
        let unrealized_pnl = Decimal::ZERO;

        let raw_data = serde_json::to_value(&response).ok();

        Ok(AccountSnapshot {
            exchange: "binance".to_string(),
            market_type: "spot".to_string(),
            snapshot_at: Utc::now(),
            total_equity: total_balance,  // 现货总余额就是总权益
            total_balance,
            available_balance: available,
            frozen_balance: frozen,
            unrealized_pnl,
            initial_margin: None,
            maint_margin: None,
            margin_ratio: None,
            position_count: 0,
            raw_data,
        })
    }

    /// 获取现货资产余额
    async fn get_spot_balances(&self) -> DataResult<Vec<AssetBalance>> {
        let params = HashMap::new();
        let response: SpotAccountResponse = self
            .signed_request("GET", "/api/v3/account", params)
            .await
            .map_err(|e| trading_common::data::types::DataError::Database(
                sqlx::Error::Protocol(e.to_string())
            ))?;

        let balances: Vec<AssetBalance> = response.balances.iter()
            .filter(|b| {
                let free: Decimal = b.free.parse().unwrap_or(Decimal::ZERO);
                let locked: Decimal = b.locked.parse().unwrap_or(Decimal::ZERO);
                free + locked > Decimal::ZERO
            })
            .map(|b| {
                let free: Decimal = b.free.parse().unwrap_or(Decimal::ZERO);
                let locked: Decimal = b.locked.parse().unwrap_or(Decimal::ZERO);

                AssetBalance {
                    exchange: "binance".to_string(),
                    market_type: "spot".to_string(),
                    asset: b.asset.clone(),
                    snapshot_at: Utc::now(),
                    total: free + locked,
                    available: free,
                    frozen: locked,
                    unrealized_pnl: Decimal::ZERO,
                    usd_value: None,  // 需要价格数据才能计算
                }
            })
            .collect();

        Ok(balances)
    }
}
