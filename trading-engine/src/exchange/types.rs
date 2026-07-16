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
    #[serde(rename = "LIMIT_MAKER")]
    LimitMaker,
    #[serde(rename = "STOP_MARKET")]
    StopMarket,
    #[serde(rename = "TAKE_PROFIT_MARKET")]
    TakeProfitMarket,
    #[serde(rename = "TRAILING_STOP_MARKET")]
    TrailingStopMarket,
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
            OrderType::LimitMaker => write!(f, "LIMIT_MAKER"),
            OrderType::StopMarket => write!(f, "STOP_MARKET"),
            OrderType::TakeProfitMarket => write!(f, "TAKE_PROFIT_MARKET"),
            OrderType::TrailingStopMarket => write!(f, "TRAILING_STOP_MARKET"),
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
    pub positions: Vec<PositionInfo>,
    pub total_equity: Decimal,
    pub available_balance: Decimal,
    pub unrealized_pnl: Decimal,
    pub margin_used: Decimal,
    pub margin_ratio: Option<Decimal>,
    /// 交易所返回的用户唯一标识（Binance: uid, OKX: uid）
    pub uid: Option<String>,
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

// ===== 合约交易扩展类型 =====

/// 保证金模式
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum MarginType {
    #[serde(rename = "ISOLATED")]
    Isolated,
    #[serde(rename = "CROSSED")]
    Crossed,
}

impl std::fmt::Display for MarginType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MarginType::Isolated => write!(f, "ISOLATED"),
            MarginType::Crossed => write!(f, "CROSSED"),
        }
    }
}

/// 持仓模式
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum PositionMode {
    /// 单向持仓模式
    OneWay,
    /// 双向持仓模式
    Hedge,
}

/// 资金费率
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FundingRate {
    pub symbol: String,
    pub funding_rate: Decimal,
    pub funding_time: DateTime<Utc>,
    pub next_funding_time: Option<DateTime<Utc>>,
}

/// K线数据
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Kline {
    pub open_time: DateTime<Utc>,
    pub open: Decimal,
    pub high: Decimal,
    pub low: Decimal,
    pub close: Decimal,
    pub volume: Decimal,
    pub close_time: DateTime<Utc>,
    pub quote_volume: Decimal,
    pub trades_count: u64,
}

/// 订单簿条目
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderBookEntry {
    pub price: Decimal,
    pub quantity: Decimal,
}

/// 订单簿
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderBook {
    pub symbol: String,
    pub bids: Vec<OrderBookEntry>,
    pub asks: Vec<OrderBookEntry>,
    pub last_update_id: u64,
}

/// 标记价格信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarkPrice {
    pub symbol: String,
    pub mark_price: Decimal,
    pub index_price: Decimal,
    pub estimated_settle_price: Option<Decimal>,
    pub last_funding_rate: Decimal,
    pub next_funding_time: DateTime<Utc>,
    pub interest_rate: Decimal,
    pub time: DateTime<Utc>,
}

/// 成交信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TradeInfo {
    pub id: String,
    pub symbol: String,
    pub price: Decimal,
    pub quantity: Decimal,
    pub quote_quantity: Decimal,
    pub commission: Decimal,
    pub commission_asset: String,
    pub time: DateTime<Utc>,
    pub is_buyer: bool,
    pub is_maker: bool,
    pub realized_pnl: Decimal,
}

/// 批量订单请求
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchOrderRequest {
    pub symbol: String,
    pub side: OrderSide,
    pub order_type: OrderType,
    pub quantity: Decimal,
    pub price: Option<Decimal>,
    pub stop_price: Option<Decimal>,
    pub time_in_force: Option<TimeInForce>,
    pub client_order_id: Option<String>,
}

/// 批量订单结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatchOrderResult {
    pub order_id: String,
    pub client_order_id: Option<String>,
    pub symbol: String,
    pub status: OrderStatus,
    pub error_code: Option<i64>,
    pub error_message: Option<String>,
}

/// 合约账户信息 (扩展)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FuturesAccountInfo {
    pub account_info: AccountInfo,
    pub can_trade: bool,
    pub can_withdraw: bool,
    pub fee_tier: u32,
    pub max_withdraw_amount: Decimal,
    pub total_initial_margin: Decimal,
    pub total_maint_margin: Decimal,
    pub total_wallet_balance: Decimal,
    pub total_unrealized_pnl: Decimal,
    pub total_margin_balance: Decimal,
}

// ===== 行情快照类型 =====

/// 行情快照 (Ticker)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Ticker {
    /// 交易对
    pub symbol: String,
    /// 最新价格
    pub last_price: Decimal,
    /// 买一价
    pub bid_price: Decimal,
    /// 卖一价
    pub ask_price: Decimal,
    /// 24h 最高价
    pub high_price: Decimal,
    /// 24h 最低价
    pub low_price: Decimal,
    /// 24h 成交量
    pub volume: Decimal,
    /// 24h 成交额
    pub quote_volume: Decimal,
    /// 24h 价格变化
    pub price_change: Decimal,
    /// 24h 价格变化百分比
    pub price_change_percent: Decimal,
    /// 更新时间
    pub timestamp: DateTime<Utc>,
}

/// 简化成交信息 (用于 public trades)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PublicTrade {
    /// 成交 ID
    pub id: String,
    /// 交易对
    pub symbol: String,
    /// 成交价格
    pub price: Decimal,
    /// 成交数量
    pub quantity: Decimal,
    /// 成交时间
    pub timestamp: DateTime<Utc>,
    /// 是否买方主动
    pub is_buyer_maker: bool,
}

// ===== 条件单类型 (止盈止损) =====

/// 条件单请求（用于交易所端止盈止损）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConditionalOrderRequest {
    /// 交易对
    pub symbol: String,
    /// 订单方向 (BUY/SELL)
    pub side: OrderSide,
    /// 订单类型 (STOP_MARKET / TAKE_PROFIT_MARKET / TRAILING_STOP_MARKET)
    pub order_type: OrderType,
    /// 触发价格
    pub stop_price: Decimal,
    /// 数量（与 close_position 二选一）
    pub quantity: Option<Decimal>,
    /// 触发后是否全部平仓
    pub close_position: bool,
    /// 追踪止损回调比例（仅 TRAILING_STOP_MARKET）
    pub callback_rate: Option<Decimal>,
    /// 触发价格类型（MARK_PRICE / CONTRACT_PRICE）
    pub working_type: Option<String>,
    /// 客户端订单 ID
    pub client_order_id: Option<String>,
}

/// 条件单结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConditionalOrderResult {
    /// 交易所返回的策略订单 ID
    pub strategy_id: String,
    /// 交易对
    pub symbol: String,
    /// 订单方向
    pub side: OrderSide,
    /// 订单类型
    pub order_type: OrderType,
    /// 触发价格
    pub stop_price: Decimal,
    /// 数量
    pub quantity: Option<Decimal>,
    /// 是否全部平仓
    pub close_position: bool,
    /// 状态
    pub status: String,
    /// 创建时间
    pub created_at: DateTime<Utc>,
}

/// 收入记录（已实现盈亏）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IncomeRecord {
    /// 交易对
    pub symbol: String,
    /// 收入类型 (REALIZED_PNL / COMMISSION / FUNDING_FEE 等)
    pub income_type: String,
    /// 收入金额
    pub income: Decimal,
    /// 资产 (USDT 等)
    pub asset: String,
    /// 时间
    pub time: DateTime<Utc>,
    /// 交易 ID
    pub info: Option<String>,
}
