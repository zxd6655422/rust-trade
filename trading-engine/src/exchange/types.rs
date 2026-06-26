// exchange/types.rs
// 交易所通用类型定义

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

// ===== 订单相关类型 =====

/// 订单方向
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum OrderSide {
    #[serde(rename = "BUY")]
    Buy,
    #[serde(rename = "SELL")]
    Sell,
}

impl std::fmt::Display for OrderSide {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OrderSide::Buy => write!(f, "BUY"),
            OrderSide::Sell => write!(f, "SELL"),
        }
    }
}

/// 订单类型
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum OrderType {
    #[serde(rename = "MARKET")]
    Market,
    #[serde(rename = "LIMIT")]
    Limit,
    #[serde(rename = "STOP_LOSS")]
    StopLoss,
    #[serde(rename = "STOP_LOSS_LIMIT")]
    StopLossLimit,
    #[serde(rename = "TAKE_PROFIT")]
    TakeProfit,
    #[serde(rename = "TAKE_PROFIT_LIMIT")]
    TakeProfitLimit,
}

impl std::fmt::Display for OrderType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OrderType::Market => write!(f, "MARKET"),
            OrderType::Limit => write!(f, "LIMIT"),
            OrderType::StopLoss => write!(f, "STOP_LOSS"),
            OrderType::StopLossLimit => write!(f, "STOP_LOSS_LIMIT"),
            OrderType::TakeProfit => write!(f, "TAKE_PROFIT"),
            OrderType::TakeProfitLimit => write!(f, "TAKE_PROFIT_LIMIT"),
        }
    }
}

/// 订单状态
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum OrderStatus {
    #[serde(rename = "NEW")]
    New,
    #[serde(rename = "PARTIALLY_FILLED")]
    PartiallyFilled,
    #[serde(rename = "FILLED")]
    Filled,
    #[serde(rename = "CANCELED")]
    Canceled,
    #[serde(rename = "PENDING_CANCEL")]
    PendingCancel,
    #[serde(rename = "REJECTED")]
    Rejected,
    #[serde(rename = "EXPIRED")]
    Expired,
}

impl std::fmt::Display for OrderStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            OrderStatus::New => write!(f, "NEW"),
            OrderStatus::PartiallyFilled => write!(f, "PARTIALLY_FILLED"),
            OrderStatus::Filled => write!(f, "FILLED"),
            OrderStatus::Canceled => write!(f, "CANCELED"),
            OrderStatus::PendingCancel => write!(f, "PENDING_CANCEL"),
            OrderStatus::Rejected => write!(f, "REJECTED"),
            OrderStatus::Expired => write!(f, "EXPIRED"),
        }
    }
}

/// 有效期类型
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum TimeInForce {
    #[serde(rename = "GTC")]
    Gtc, // Good Till Cancel
    #[serde(rename = "IOC")]
    Ioc, // Immediate or Cancel
    #[serde(rename = "FOK")]
    Fok, // Fill or Kill
}

impl std::fmt::Display for TimeInForce {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TimeInForce::Gtc => write!(f, "GTC"),
            TimeInForce::Ioc => write!(f, "IOC"),
            TimeInForce::Fok => write!(f, "FOK"),
        }
    }
}

/// 持仓方向
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum PositionSide {
    #[serde(rename = "LONG")]
    Long,
    #[serde(rename = "SHORT")]
    Short,
    #[serde(rename = "NONE")]
    None,
}

// ===== 请求/响应类型 =====

/// 订单请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderRequest {
    pub symbol: String,
    pub side: OrderSide,
    pub order_type: OrderType,
    pub quantity: Decimal,
    pub price: Option<Decimal>,
    pub stop_price: Option<Decimal>,
    pub time_in_force: Option<TimeInForce>,
    pub client_order_id: Option<String>,
}

/// 订单结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderResult {
    pub order_id: String,
    pub client_order_id: Option<String>,
    pub symbol: String,
    pub side: OrderSide,
    pub order_type: OrderType,
    pub status: OrderStatus,
    pub quantity: Decimal,
    pub filled_quantity: Decimal,
    pub price: Option<Decimal>,
    pub avg_price: Option<Decimal>,
    pub commission: Option<Decimal>,
    pub commission_asset: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// 订单信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderInfo {
    pub order_id: String,
    pub client_order_id: Option<String>,
    pub symbol: String,
    pub side: OrderSide,
    pub order_type: OrderType,
    pub status: OrderStatus,
    pub quantity: Decimal,
    pub filled_quantity: Decimal,
    pub remaining_quantity: Decimal,
    pub price: Option<Decimal>,
    pub stop_price: Option<Decimal>,
    pub time_in_force: TimeInForce,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// 订单更新 (WebSocket 推送)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderUpdate {
    pub order_id: String,
    pub client_order_id: Option<String>,
    pub symbol: String,
    pub side: OrderSide,
    pub order_type: OrderType,
    pub status: OrderStatus,
    pub quantity: Decimal,
    pub filled_quantity: Decimal,
    pub price: Option<Decimal>,
    pub avg_price: Option<Decimal>,
    pub commission: Option<Decimal>,
    pub commission_asset: Option<String>,
    pub timestamp: DateTime<Utc>,
}

/// 账户余额
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Balance {
    pub asset: String,
    pub free: Decimal,
    pub locked: Decimal,
}

/// 账户信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountInfo {
    pub balances: Vec<Balance>,
    pub total_equity: Decimal,
    pub available_balance: Decimal,
    pub unrealized_pnl: Decimal,
    pub margin_used: Decimal,
    pub margin_ratio: Option<Decimal>,
}

/// 持仓信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PositionInfo {
    pub symbol: String,
    pub side: PositionSide,
    pub quantity: Decimal,
    pub avg_entry_price: Decimal,
    pub mark_price: Option<Decimal>,
    pub unrealized_pnl: Decimal,
    pub leverage: u32,
    pub margin: Decimal,
    pub liquidation_price: Option<Decimal>,
}

/// 交易所时间
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExchangeTime {
    pub server_time: DateTime<Utc>,
    pub local_time: DateTime<Utc>,
    pub offset_ms: i64,
}
