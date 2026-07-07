//! 交易所 API 模块
//!
//! 调用交易所 API 获取账户信息、交易对精度等

use anyhow::{anyhow, Result};
use reqwest::Client;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tracing::{debug, error, info, warn};

// =================================================================
// 交易所配置
// =================================================================

/// 交易所配置
#[derive(Debug, Clone)]
pub struct ExchangeApiConfig {
    pub api_key: String,
    pub api_secret: String,
    pub base_url: String,
    pub testnet: bool,
}

impl ExchangeApiConfig {
    /// 从环境变量加载 Binance 配置
    pub fn binance_from_env() -> Result<Self> {
        let api_key = std::env::var("BINANCE_API_KEY")
            .map_err(|_| anyhow!("BINANCE_API_KEY not set"))?;
        let api_secret = std::env::var("BINANCE_API_SECRET")
            .map_err(|_| anyhow!("BINANCE_API_SECRET not set"))?;
        let testnet = std::env::var("BINANCE_TESTNET")
            .unwrap_or_else(|_| "false".to_string())
            .parse()
            .unwrap_or(false);

        let base_url = if testnet {
            "https://testnet.binancefuture.com".to_string()
        } else {
            "https://fapi.binance.com".to_string()
        };

        Ok(Self {
            api_key,
            api_secret,
            base_url,
            testnet,
        })
    }
}

// =================================================================
// 交易所 API 响应类型
// =================================================================

/// 交易对精度信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SymbolPrecision {
    pub symbol: String,
    pub base_asset_precision: u32,
    pub quote_asset_precision: u32,
    pub min_quantity: Decimal,
    pub max_quantity: Decimal,
    pub min_notional: Decimal,
    pub step_size: Decimal,
    pub tick_size: Decimal,
}

/// 账户余额信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountBalance {
    pub asset: String,
    pub free: Decimal,
    pub locked: Decimal,
    pub total: Decimal,
}

/// 持仓信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PositionInfo {
    pub symbol: String,
    pub position_amt: Decimal,
    pub entry_price: Decimal,
    pub mark_price: Decimal,
    pub unrealized_pnl: Decimal,
    pub leverage: u32,
    pub margin_type: String,
}

// =================================================================
// 交易所 API 客户端
// =================================================================

/// 交易所 API 客户端
pub struct ExchangeClient {
    config: ExchangeApiConfig,
    http_client: Client,
}

impl ExchangeClient {
    pub fn new(config: ExchangeApiConfig) -> Self {
        Self {
            config,
            http_client: Client::new(),
        }
    }

    /// 生成签名
    fn sign(&self, params: &HashMap<String, String>) -> String {
        use hmac::{Hmac, Mac};
        use sha2::Sha256;

        let query_string: String = params
            .iter()
            .map(|(k, v)| format!("{}={}", k, v))
            .collect::<Vec<_>>()
            .join("&");

        let mut mac = Hmac::<Sha256>::new_from_slice(self.config.api_secret.as_bytes())
            .expect("HMAC can take key of any size");
        mac.update(query_string.as_bytes());
        let result = mac.finalize();
        hex::encode(result.into_bytes())
    }

    /// 发送签名请求
    async fn signed_request<T: for<'de> Deserialize<'de>>(
        &self,
        method: &str,
        path: &str,
        params: HashMap<String, String>,
    ) -> Result<T> {
        let timestamp = chrono::Utc::now().timestamp_millis().to_string();
        let mut signed_params = params.clone();
        signed_params.insert("timestamp".to_string(), timestamp);

        let signature = self.sign(&signed_params);
        signed_params.insert("signature".to_string(), signature);

        let url = format!("{}{}", self.config.base_url, path);

        let response = match method {
            "GET" => {
                self.http_client
                    .get(&url)
                    .header("X-MBX-APIKEY", &self.config.api_key)
                    .query(&signed_params)
                    .send()
                    .await?
            }
            "POST" => {
                self.http_client
                    .post(&url)
                    .header("X-MBX-APIKEY", &self.config.api_key)
                    .form(&signed_params)
                    .send()
                    .await?
            }
            "DELETE" => {
                self.http_client
                    .delete(&url)
                    .header("X-MBX-APIKEY", &self.config.api_key)
                    .query(&signed_params)
                    .send()
                    .await?
            }
            _ => return Err(anyhow!("Unsupported HTTP method: {}", method)),
        };

        if !response.status().is_success() {
            let status = response.status();
            let body = response.text().await?;
            error!("API error: {} - {}", status, body);
            return Err(anyhow!("API error: {} - {}", status, body));
        }

        let result = response.json::<T>().await?;
        Ok(result)
    }

    /// 获取交易对精度信息
    pub async fn get_symbol_precision(&self, symbol: &str) -> Result<SymbolPrecision> {
        #[derive(Deserialize)]
        struct ExchangeInfo {
            symbols: Vec<SymbolInfo>,
        }

        #[derive(Deserialize)]
        struct SymbolInfo {
            symbol: String,
            #[serde(rename = "baseAssetPrecision")]
            base_asset_precision: u32,
            #[serde(rename = "quoteAssetPrecision")]
            quote_asset_precision: u32,
            filters: Vec<Filter>,
        }

        #[derive(Deserialize)]
        struct Filter {
            #[serde(rename = "filterType")]
            filter_type: String,
            #[serde(rename = "minQty")]
            min_qty: Option<String>,
            #[serde(rename = "maxQty")]
            max_qty: Option<String>,
            #[serde(rename = "minNotional")]
            min_notional: Option<String>,
            #[serde(rename = "stepSize")]
            step_size: Option<String>,
            #[serde(rename = "tickSize")]
            tick_size: Option<String>,
        }

        let url = format!("{}/fapi/v1/exchangeInfo", self.config.base_url);
        let response = self.http_client.get(&url).send().await?;

        if !response.status().is_success() {
            return Err(anyhow!("Failed to get exchange info"));
        }

        let exchange_info: ExchangeInfo = response.json().await?;

        let symbol_info = exchange_info
            .symbols
            .iter()
            .find(|s| s.symbol == symbol)
            .ok_or_else(|| anyhow!("Symbol not found: {}", symbol))?;

        let mut min_quantity = Decimal::ZERO;
        let mut max_quantity = Decimal::ZERO;
        let mut min_notional = Decimal::ZERO;
        let mut step_size = Decimal::ZERO;
        let mut tick_size = Decimal::ZERO;

        for filter in &symbol_info.filters {
            match filter.filter_type.as_str() {
                "LOT_SIZE" => {
                    if let Some(min) = &filter.min_qty {
                        min_quantity = min.parse().unwrap_or(Decimal::ZERO);
                    }
                    if let Some(max) = &filter.max_qty {
                        max_quantity = max.parse().unwrap_or(Decimal::ZERO);
                    }
                    if let Some(step) = &filter.step_size {
                        step_size = step.parse().unwrap_or(Decimal::ZERO);
                    }
                }
                "MIN_NOTIONAL" => {
                    if let Some(min) = &filter.min_notional {
                        min_notional = min.parse().unwrap_or(Decimal::ZERO);
                    }
                }
                "PRICE_FILTER" => {
                    if let Some(tick) = &filter.tick_size {
                        tick_size = tick.parse().unwrap_or(Decimal::ZERO);
                    }
                }
                _ => {}
            }
        }

        Ok(SymbolPrecision {
            symbol: symbol.to_string(),
            base_asset_precision: symbol_info.base_asset_precision,
            quote_asset_precision: symbol_info.quote_asset_precision,
            min_quantity,
            max_quantity,
            min_notional,
            step_size,
            tick_size,
        })
    }

    /// 获取合约账户余额
    pub async fn get_futures_balance(&self) -> Result<Vec<AccountBalance>> {
        #[derive(Deserialize)]
        struct BalanceResponse {
            assets: Vec<AssetBalance>,
        }

        #[derive(Deserialize)]
        struct AssetBalance {
            asset: String,
            #[serde(rename = "availableBalance")]
            available_balance: String,
            #[serde(rename = "crossWalletBalance")]
            cross_wallet_balance: String,
        }

        let params = HashMap::new();
        let response: BalanceResponse = self
            .signed_request("GET", "/fapi/v2/balance", params)
            .await?;

        let balances: Vec<AccountBalance> = response
            .assets
            .iter()
            .map(|a| {
                let free: Decimal = a.available_balance.parse().unwrap_or(Decimal::ZERO);
                let total: Decimal = a.cross_wallet_balance.parse().unwrap_or(Decimal::ZERO);
                AccountBalance {
                    asset: a.asset.clone(),
                    free,
                    locked: total - free,
                    total,
                }
            })
            .collect();

        Ok(balances)
    }

    /// 获取 USDT 可用余额
    pub async fn get_usdt_balance(&self) -> Result<Decimal> {
        let balances = self.get_futures_balance().await?;

        let usdt_balance = balances
            .iter()
            .find(|b| b.asset == "USDT")
            .map(|b| b.free)
            .unwrap_or(Decimal::ZERO);

        Ok(usdt_balance)
    }

    /// 获取持仓信息
    pub async fn get_positions(&self) -> Result<Vec<PositionInfo>> {
        #[derive(Deserialize)]
        struct PositionResponse {
            symbol: String,
            #[serde(rename = "positionAmt")]
            position_amt: String,
            #[serde(rename = "entryPrice")]
            entry_price: String,
            #[serde(rename = "markPrice")]
            mark_price: String,
            #[serde(rename = "unRealizedProfit")]
            unrealized_pnl: String,
            leverage: String,
            #[serde(rename = "marginType")]
            margin_type: String,
        }

        let params = HashMap::new();
        let response: Vec<PositionResponse> = self
            .signed_request("GET", "/fapi/v2/positionRisk", params)
            .await?;

        let positions: Vec<PositionInfo> = response
            .iter()
            .map(|p| {
                let position_amt: Decimal = p.position_amt.parse().unwrap_or(Decimal::ZERO);
                let entry_price: Decimal = p.entry_price.parse().unwrap_or(Decimal::ZERO);
                let mark_price: Decimal = p.mark_price.parse().unwrap_or(Decimal::ZERO);
                let unrealized_pnl: Decimal = p.unrealized_pnl.parse().unwrap_or(Decimal::ZERO);
                let leverage: u32 = p.leverage.parse().unwrap_or(1);

                PositionInfo {
                    symbol: p.symbol.clone(),
                    position_amt,
                    entry_price,
                    mark_price,
                    unrealized_pnl,
                    leverage,
                    margin_type: p.margin_type.clone(),
                }
            })
            .collect();

        Ok(positions)
    }

    /// 下单
    pub async fn place_order(
        &self,
        symbol: &str,
        side: &str,
        order_type: &str,
        quantity: Decimal,
        price: Option<Decimal>,
        stop_price: Option<Decimal>,
    ) -> Result<OrderResult> {
        #[derive(Deserialize)]
        struct OrderResponse {
            #[serde(rename = "orderId")]
            order_id: u64,
            symbol: String,
            status: String,
            #[serde(rename = "clientOrderId")]
            client_order_id: String,
        }

        let mut params = HashMap::new();
        params.insert("symbol".to_string(), symbol.to_string());
        params.insert("side".to_string(), side.to_string());
        params.insert("type".to_string(), order_type.to_string());
        params.insert("quantity".to_string(), quantity.to_string());

        if let Some(p) = price {
            params.insert("price".to_string(), p.to_string());
        }

        if let Some(sp) = stop_price {
            params.insert("stopPrice".to_string(), sp.to_string());
        }

        let response: OrderResponse = self
            .signed_request("POST", "/fapi/v1/order", params)
            .await?;

        Ok(OrderResult {
            order_id: response.order_id.to_string(),
            symbol: response.symbol,
            status: response.status,
            client_order_id: response.client_order_id,
        })
    }

    /// 查询订单状态
    pub async fn get_order_status(
        &self,
        symbol: &str,
        order_id: &str,
    ) -> Result<OrderStatus> {
        let mut params = HashMap::new();
        params.insert("symbol".to_string(), symbol.to_string());
        params.insert("orderId".to_string(), order_id.to_string());

        let response: OrderStatus = self
            .signed_request("GET", "/fapi/v1/order", params)
            .await?;

        Ok(response)
    }

    /// 查询所有未成交订单
    pub async fn get_open_orders(&self, symbol: Option<&str>) -> Result<Vec<OrderStatus>> {
        let mut params = HashMap::new();
        if let Some(s) = symbol {
            params.insert("symbol".to_string(), s.to_string());
        }

        let response: Vec<OrderStatus> = self
            .signed_request("GET", "/fapi/v1/openOrders", params)
            .await?;

        Ok(response)
    }

    /// 撤单
    pub async fn cancel_order(&self, symbol: &str, order_id: &str) -> Result<()> {
        let mut params = HashMap::new();
        params.insert("symbol".to_string(), symbol.to_string());
        params.insert("orderId".to_string(), order_id.to_string());

        let _: serde_json::Value = self
            .signed_request("DELETE", "/fapi/v1/order", params)
            .await?;

        Ok(())
    }

    /// 获取现货账户余额
    pub async fn get_spot_balance(&self) -> Result<Vec<SpotBalance>> {
        #[derive(Deserialize)]
        struct SpotAccountResponse {
            balances: Vec<SpotBalance>,
        }

        let params = HashMap::new();
        let response: SpotAccountResponse = self
            .signed_request("GET", "/api/v3/account", params)
            .await?;

        // 只返回有余额的资产
        let balances: Vec<SpotBalance> = response
            .balances
            .into_iter()
            .filter(|b| b.free > Decimal::ZERO || b.locked > Decimal::ZERO)
            .collect();

        Ok(balances)
    }

    /// 获取现货 USDT 可用余额
    pub async fn get_spot_usdt_balance(&self) -> Result<Decimal> {
        let balances = self.get_spot_balance().await?;

        let usdt_balance = balances
            .iter()
            .find(|b| b.asset == "USDT")
            .map(|b| b.free)
            .unwrap_or(Decimal::ZERO);

        Ok(usdt_balance)
    }
}

/// 订单结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderResult {
    pub order_id: String,
    pub symbol: String,
    pub status: String,
    pub client_order_id: String,
}

/// 订单状态
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderStatus {
    pub order_id: u64,
    pub symbol: String,
    pub status: String,
    pub side: String,
    pub order_type: String,
    pub price: Decimal,
    pub avg_price: Decimal,
    pub quantity: Decimal,
    pub executed_qty: Decimal,
    pub cummulative_quote_qty: Decimal,
    pub time: Option<i64>,
    pub update_time: Option<i64>,
}

/// 账户余额信息（现货）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpotBalance {
    pub asset: String,
    pub free: Decimal,
    pub locked: Decimal,
}
