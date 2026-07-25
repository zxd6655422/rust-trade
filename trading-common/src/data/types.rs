use chrono::{DateTime, Datelike, Duration, Timelike, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use thiserror::Error;

// =================================================================
// Core data type: completely corresponds to the tick_data table structure
// =================================================================

/// Trading direction enumeration
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum TradeSide {
    Buy,
    Sell,
}

/// Standard trading data structure - corresponds one-to-one with the tick_data table fields
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct TickData {
    /// UTC timestamp, supports millisecond precision
    pub timestamp: DateTime<Utc>,

    /// Trading pair, such as "BTCUSDT"
    pub symbol: String,

    /// Trading price
    pub price: Decimal,

    /// Trading quantity
    pub quantity: Decimal,

    /// Trading direction
    pub side: TradeSide,

    /// Original transaction ID
    pub trade_id: String,

    /// Whether the buyer is the maker
    pub is_buyer_maker: bool,
}

impl TickData {
    /// New TickData
    pub fn new(
        timestamp: DateTime<Utc>,
        symbol: String,
        price: Decimal,
        quantity: Decimal,
        side: TradeSide,
        trade_id: String,
        is_buyer_maker: bool,
    ) -> Self {
        Self {
            timestamp,
            symbol,
            price,
            quantity,
            side,
            trade_id,
            is_buyer_maker,
        }
    }
}

// =================================================================
// Helper Types
// =================================================================

/// Database statistics
#[derive(Debug, Clone)]
pub struct DbStats {
    pub symbol: Option<String>,
    pub total_records: u64,
    pub earliest_timestamp: Option<DateTime<Utc>>,
    pub latest_timestamp: Option<DateTime<Utc>>,
}

// =================================================================
// TradeSide Implementation for Database Integration
// =================================================================

impl TradeSide {
    /// Convert to database string representation
    pub fn as_db_str(&self) -> &'static str {
        match self {
            TradeSide::Buy => "BUY",
            TradeSide::Sell => "SELL",
        }
    }
}

// =================================================================
// Query parameter type
// =================================================================

/// TickData Query parameters
#[derive(Debug, Clone)]
pub struct TickQuery {
    pub symbol: String,
    pub start_time: Option<DateTime<Utc>>,
    pub end_time: Option<DateTime<Utc>>,
    pub limit: Option<u32>,
    pub trade_side: Option<TradeSide>,
}

impl TickQuery {
    pub fn new(symbol: String) -> Self {
        Self {
            symbol,
            start_time: None,
            end_time: None,
            limit: None,
            trade_side: None,
        }
    }
}

// =================================================================
// Error type definition
// =================================================================

#[derive(Error, Debug)]
pub enum DataError {
    #[error("Database error: {0}")]
    Database(#[from] sqlx::Error),

    #[error("Invalid data format: {0}")]
    InvalidFormat(String),

    #[error("Data not found: {0}")]
    NotFound(String),

    #[error("Validation error: {0}")]
    Validation(String),

    #[error("Serialization error: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("Decimal conversion error: {0}")]
    DecimalConversion(#[from] rust_decimal::Error),

    #[error("Cache error: {0}")]
    Cache(String),

    #[error("Configuration error: {0}")]
    Config(String),
}

pub type DataResult<T> = Result<T, DataError>;

/// Backtest data information for user selection
#[derive(Debug, Clone)]
pub struct BacktestDataInfo {
    pub total_records: u64,
    pub symbols_count: u64,
    pub earliest_time: Option<DateTime<Utc>>,
    pub latest_time: Option<DateTime<Utc>>,
    pub symbol_info: Vec<SymbolDataInfo>,
}

/// Per-symbol data information
#[derive(Debug, Clone)]
pub struct SymbolDataInfo {
    pub symbol: String,
    pub records_count: u64,
    pub earliest_time: Option<DateTime<Utc>>,
    pub latest_time: Option<DateTime<Utc>>,
    pub min_price: Option<Decimal>,
    pub max_price: Option<Decimal>,
    pub total_volume_usd: Decimal,
}

impl BacktestDataInfo {
    /// Get information for a specific symbol
    pub fn get_symbol_info(&self, symbol: &str) -> Option<&SymbolDataInfo> {
        self.symbol_info.iter().find(|info| info.symbol == symbol)
    }

    /// Get available symbols
    pub fn get_available_symbols(&self) -> Vec<String> {
        self.symbol_info
            .iter()
            .map(|info| info.symbol.clone())
            .collect()
    }

    /// Check if has sufficient data for backtesting
    pub fn has_sufficient_data(&self, symbol: &str, min_records: u64) -> bool {
        self.get_symbol_info(symbol)
            .map(|info| info.records_count >= min_records)
            .unwrap_or(false)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LiveStrategyLog {
    pub timestamp: DateTime<Utc>,
    /// 关联的策略信号 ID（全链路追踪）
    pub signal_id: Option<uuid::Uuid>,
    /// 关联的策略实例 ID（可选）
    pub instance_id: Option<uuid::Uuid>,
    pub strategy_id: String,
    pub symbol: String,
    pub current_price: Decimal,
    pub signal_type: String, // BUY/SELL/HOLD
    /// 信号意图: entry(开仓)/exit(平仓)/reverse(反手)
    pub signal_intent: String,
    /// 目标市场: futures/spot/both
    pub market_type: String,
    pub portfolio_value: Decimal,
    pub total_pnl: Decimal,
    pub cache_hit: bool,
    pub processing_time_us: u64,
    // 新增字段
    pub entry_price: Option<Decimal>,
    pub stop_loss: Option<Decimal>,
    pub take_profit: Option<Decimal>,
    pub position_quantity: Option<Decimal>,
    pub unrealized_pnl: Option<Decimal>,
}

/// Time frame for OHLC data
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Timeframe {
    OneMinute,
    FiveMinutes,
    FifteenMinutes,
    ThirtyMinutes,
    OneHour,
    TwoHour,
    FourHour,
    OneDay,
    ThreeDay,
    OneWeek,
}

impl Timeframe {
    pub fn as_duration(&self) -> Duration {
        match self {
            Timeframe::OneMinute => Duration::minutes(1),
            Timeframe::FiveMinutes => Duration::minutes(5),
            Timeframe::FifteenMinutes => Duration::minutes(15),
            Timeframe::ThirtyMinutes => Duration::minutes(30),
            Timeframe::OneHour => Duration::hours(1),
            Timeframe::TwoHour => Duration::hours(2),
            Timeframe::FourHour => Duration::hours(4),
            Timeframe::OneDay => Duration::days(1),
            Timeframe::ThreeDay => Duration::days(3),
            Timeframe::OneWeek => Duration::weeks(1),
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Timeframe::OneMinute => "1m",
            Timeframe::FiveMinutes => "5m",
            Timeframe::FifteenMinutes => "15m",
            Timeframe::ThirtyMinutes => "30m",
            Timeframe::OneHour => "1h",
            Timeframe::TwoHour => "2h",
            Timeframe::FourHour => "4h",
            Timeframe::OneDay => "1d",
            Timeframe::ThreeDay => "3d",
            Timeframe::OneWeek => "1w",
        }
    }

    /// 从字符串解析时间框架
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "1m" => Some(Timeframe::OneMinute),
            "5m" => Some(Timeframe::FiveMinutes),
            "15m" => Some(Timeframe::FifteenMinutes),
            "30m" => Some(Timeframe::ThirtyMinutes),
            "1h" => Some(Timeframe::OneHour),
            "2h" => Some(Timeframe::TwoHour),
            "4h" => Some(Timeframe::FourHour),
            "1d" => Some(Timeframe::OneDay),
            "3d" => Some(Timeframe::ThreeDay),
            "1w" => Some(Timeframe::OneWeek),
            _ => None,
        }
    }

    /// 获取时间框架的级别（用于排序，值越小级别越低）
    pub fn level(&self) -> u8 {
        match self {
            Timeframe::OneMinute => 1,
            Timeframe::FiveMinutes => 2,
            Timeframe::FifteenMinutes => 3,
            Timeframe::ThirtyMinutes => 4,
            Timeframe::OneHour => 5,
            Timeframe::TwoHour => 6,
            Timeframe::FourHour => 7,
            Timeframe::OneDay => 8,
            Timeframe::ThreeDay => 9,
            Timeframe::OneWeek => 10,
        }
    }

    /// 获取该时间框架需要的最小 K 线数量（用于指标预热和缓存完整性检查）
    pub fn min_warmup_bars(&self) -> usize {
        match self {
            Timeframe::OneMinute => 500,
            Timeframe::FiveMinutes => 500,
            Timeframe::FifteenMinutes => 300,
            Timeframe::ThirtyMinutes => 200,
            Timeframe::OneHour => 200,
            Timeframe::TwoHour => 150,
            Timeframe::FourHour => 150,
            Timeframe::OneDay => 100,
            Timeframe::ThreeDay => 50,
            Timeframe::OneWeek => 50,
        }
    }

    /// Get the start of the time window for a given timestamp
    pub fn align_timestamp(&self, timestamp: DateTime<Utc>) -> DateTime<Utc> {
        match self {
            Timeframe::OneMinute => timestamp
                .with_second(0)
                .unwrap()
                .with_nanosecond(0)
                .unwrap(),
            Timeframe::FiveMinutes => {
                let aligned_minute = (timestamp.minute() / 5) * 5;
                timestamp
                    .with_minute(aligned_minute)
                    .unwrap()
                    .with_second(0)
                    .unwrap()
                    .with_nanosecond(0)
                    .unwrap()
            }
            Timeframe::FifteenMinutes => {
                let aligned_minute = (timestamp.minute() / 15) * 15;
                timestamp
                    .with_minute(aligned_minute)
                    .unwrap()
                    .with_second(0)
                    .unwrap()
                    .with_nanosecond(0)
                    .unwrap()
            }
            Timeframe::ThirtyMinutes => {
                let aligned_minute = (timestamp.minute() / 30) * 30;
                timestamp
                    .with_minute(aligned_minute)
                    .unwrap()
                    .with_second(0)
                    .unwrap()
                    .with_nanosecond(0)
                    .unwrap()
            }
            Timeframe::OneHour => timestamp
                .with_minute(0)
                .unwrap()
                .with_second(0)
                .unwrap()
                .with_nanosecond(0)
                .unwrap(),
            Timeframe::TwoHour => {
                let aligned_hour = (timestamp.hour() / 2) * 2;
                timestamp
                    .with_hour(aligned_hour)
                    .unwrap()
                    .with_minute(0)
                    .unwrap()
                    .with_second(0)
                    .unwrap()
                    .with_nanosecond(0)
                    .unwrap()
            }
            Timeframe::FourHour => {
                let aligned_hour = (timestamp.hour() / 4) * 4;
                timestamp
                    .with_hour(aligned_hour)
                    .unwrap()
                    .with_minute(0)
                    .unwrap()
                    .with_second(0)
                    .unwrap()
                    .with_nanosecond(0)
                    .unwrap()
            }
            Timeframe::OneDay => timestamp
                .with_hour(0)
                .unwrap()
                .with_minute(0)
                .unwrap()
                .with_second(0)
                .unwrap()
                .with_nanosecond(0)
                .unwrap(),
            Timeframe::ThreeDay => {
                // 对齐到每月的 1、4、7、10、13、16、19、22、25、28、31 日
                let day = timestamp.day();
                let aligned_day = ((day - 1) / 3) * 3 + 1;
                let aligned_date = timestamp
                    .with_day(aligned_day.min(28)) // 简化处理，避免月份天数问题
                    .unwrap_or(timestamp);
                aligned_date
                    .with_hour(0)
                    .unwrap()
                    .with_minute(0)
                    .unwrap()
                    .with_second(0)
                    .unwrap()
                    .with_nanosecond(0)
                    .unwrap()
            }
            Timeframe::OneWeek => {
                let days_from_monday = timestamp.weekday().num_days_from_monday();
                let week_start = timestamp - Duration::days(days_from_monday as i64);
                week_start
                    .with_hour(0)
                    .unwrap()
                    .with_minute(0)
                    .unwrap()
                    .with_second(0)
                    .unwrap()
                    .with_nanosecond(0)
                    .unwrap()
            }
        }
    }
}

/// OHLC data structure
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct OHLCData {
    pub timestamp: DateTime<Utc>,
    pub symbol: String,
    pub timeframe: Timeframe,
    pub open: Decimal,
    pub high: Decimal,
    pub low: Decimal,
    pub close: Decimal,
    pub volume: Decimal,
    pub trade_count: u64,
}

impl OHLCData {
    pub fn new(
        timestamp: DateTime<Utc>,
        symbol: String,
        timeframe: Timeframe,
        open: Decimal,
        high: Decimal,
        low: Decimal,
        close: Decimal,
        volume: Decimal,
        trade_count: u64,
    ) -> Self {
        Self {
            timestamp,
            symbol,
            timeframe,
            open,
            high,
            low,
            close,
            volume,
            trade_count,
        }
    }

    /// Create OHLC from a collection of tick data
    pub fn from_ticks(
        ticks: &[TickData],
        timeframe: Timeframe,
        window_start: DateTime<Utc>,
    ) -> Option<Self> {
        if ticks.is_empty() {
            return None;
        }

        let symbol = ticks[0].symbol.clone();
        let open = ticks[0].price;
        let mut high = ticks[0].price;
        let mut low = ticks[0].price;
        let close = ticks[ticks.len() - 1].price;
        let mut volume = Decimal::ZERO;

        for tick in ticks {
            if tick.price > high {
                high = tick.price;
            }
            if tick.price < low {
                low = tick.price;
            }
            volume += tick.quantity;
        }

        Some(OHLCData::new(
            window_start,
            symbol,
            timeframe,
            open,
            high,
            low,
            close,
            volume,
            ticks.len() as u64,
        ))
    }
}

// =================================================================
// Unified Order/Trading Types
// =================================================================

/// Order side (buy/sell)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum OrderSide {
    Buy,
    Sell,
}

impl OrderSide {
    pub fn as_str(&self) -> &'static str {
        match self {
            OrderSide::Buy => "BUY",
            OrderSide::Sell => "SELL",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_uppercase().as_str() {
            "BUY" => Some(OrderSide::Buy),
            "SELL" => Some(OrderSide::Sell),
            _ => None,
        }
    }
}

impl std::fmt::Display for OrderSide {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Order type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum OrderType {
    Market,
    Limit,
    StopLoss,
    StopLossLimit,
    TakeProfit,
    TakeProfitLimit,
    LimitMaker,
}

impl OrderType {
    pub fn as_str(&self) -> &'static str {
        match self {
            OrderType::Market => "MARKET",
            OrderType::Limit => "LIMIT",
            OrderType::StopLoss => "STOP_LOSS",
            OrderType::StopLossLimit => "STOP_LOSS_LIMIT",
            OrderType::TakeProfit => "TAKE_PROFIT",
            OrderType::TakeProfitLimit => "TAKE_PROFIT_LIMIT",
            OrderType::LimitMaker => "LIMIT_MAKER",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_uppercase().as_str() {
            "MARKET" => Some(OrderType::Market),
            "LIMIT" => Some(OrderType::Limit),
            "STOP_LOSS" => Some(OrderType::StopLoss),
            "STOP_LOSS_LIMIT" => Some(OrderType::StopLossLimit),
            "TAKE_PROFIT" => Some(OrderType::TakeProfit),
            "TAKE_PROFIT_LIMIT" => Some(OrderType::TakeProfitLimit),
            "LIMIT_MAKER" => Some(OrderType::LimitMaker),
            _ => None,
        }
    }
}

impl std::fmt::Display for OrderType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Order status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum OrderStatus {
    New,
    PartiallyFilled,
    Filled,
    Canceled,
    PendingCancel,
    Rejected,
    Expired,
}

impl OrderStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            OrderStatus::New => "NEW",
            OrderStatus::PartiallyFilled => "PARTIALLY_FILLED",
            OrderStatus::Filled => "FILLED",
            OrderStatus::Canceled => "CANCELED",
            OrderStatus::PendingCancel => "PENDING_CANCEL",
            OrderStatus::Rejected => "REJECTED",
            OrderStatus::Expired => "EXPIRED",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_uppercase().as_str() {
            "NEW" => Some(OrderStatus::New),
            "PARTIALLY_FILLED" => Some(OrderStatus::PartiallyFilled),
            "FILLED" => Some(OrderStatus::Filled),
            "CANCELED" => Some(OrderStatus::Canceled),
            "PENDING_CANCEL" => Some(OrderStatus::PendingCancel),
            "REJECTED" => Some(OrderStatus::Rejected),
            "EXPIRED" => Some(OrderStatus::Expired),
            _ => None,
        }
    }

    /// Whether the order is in a terminal state
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            OrderStatus::Filled | OrderStatus::Canceled | OrderStatus::Rejected | OrderStatus::Expired
        )
    }

    /// Whether the order is active (can still be filled)
    pub fn is_active(&self) -> bool {
        matches!(self, OrderStatus::New | OrderStatus::PartiallyFilled)
    }
}

impl std::fmt::Display for OrderStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Time in force policy
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum TimeInForce {
    /// Good Till Cancel
    Gtc,
    /// Immediate or Cancel
    Ioc,
    /// Fill or Kill
    Fok,
}

impl TimeInForce {
    pub fn as_str(&self) -> &'static str {
        match self {
            TimeInForce::Gtc => "GTC",
            TimeInForce::Ioc => "IOC",
            TimeInForce::Fok => "FOK",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_uppercase().as_str() {
            "GTC" => Some(TimeInForce::Gtc),
            "IOC" => Some(TimeInForce::Ioc),
            "FOK" => Some(TimeInForce::Fok),
            _ => None,
        }
    }
}

impl std::fmt::Display for TimeInForce {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Signal type for strategy decisions
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SignalType {
    Buy,
    Sell,
    Hold,
}

impl SignalType {
    pub fn as_str(&self) -> &'static str {
        match self {
            SignalType::Buy => "BUY",
            SignalType::Sell => "SELL",
            SignalType::Hold => "HOLD",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_uppercase().as_str() {
            "BUY" => Some(SignalType::Buy),
            "SELL" => Some(SignalType::Sell),
            "HOLD" => Some(SignalType::Hold),
            _ => None,
        }
    }
}

impl std::fmt::Display for SignalType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

// =================================================================
// Order Data Structures
// =================================================================

/// Order request for placing orders
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

/// Order result after placing
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

/// Order information (query result)
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

/// Order update from WebSocket
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

// =================================================================
// Account & Position Types
// =================================================================

/// Asset balance
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Balance {
    pub asset: String,
    pub free: Decimal,
    pub locked: Decimal,
}

/// Account information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountInfo {
    pub balances: Vec<Balance>,
    pub total_equity: Decimal,
    pub available_balance: Decimal,
    pub unrealized_pnl: Decimal,
    pub margin_used: Decimal,
    pub margin_ratio: Option<Decimal>,
}

/// Position side
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum PositionSide {
    Long,
    Short,
    None,
}

impl PositionSide {
    pub fn as_str(&self) -> &'static str {
        match self {
            PositionSide::Long => "LONG",
            PositionSide::Short => "SHORT",
            PositionSide::None => "NONE",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_uppercase().as_str() {
            "LONG" => Some(PositionSide::Long),
            "SHORT" => Some(PositionSide::Short),
            "NONE" => Some(PositionSide::None),
            _ => None,
        }
    }
}

impl std::fmt::Display for PositionSide {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Margin type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum MarginType {
    Isolated,
    Crossed,
}

impl MarginType {
    pub fn as_str(&self) -> &'static str {
        match self {
            MarginType::Isolated => "ISOLATED",
            MarginType::Crossed => "CROSSED",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s.to_uppercase().as_str() {
            "ISOLATED" => Some(MarginType::Isolated),
            "CROSSED" => Some(MarginType::Crossed),
            _ => None,
        }
    }
}

impl std::fmt::Display for MarginType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Position mode
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum PositionMode {
    /// One-way position mode
    OneWay,
    /// Hedge position mode (dual side)
    Hedge,
}

// =================================================================
// Market Data Types
// =================================================================

/// Ticker (24h price summary)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Ticker {
    pub symbol: String,
    pub last_price: Decimal,
    pub bid_price: Decimal,
    pub ask_price: Decimal,
    pub high_price: Decimal,
    pub low_price: Decimal,
    pub volume: Decimal,
    pub quote_volume: Decimal,
    pub price_change: Decimal,
    pub price_change_percent: Decimal,
    pub timestamp: DateTime<Utc>,
}

/// Order book entry
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderBookEntry {
    pub price: Decimal,
    pub quantity: Decimal,
}

/// Order book snapshot
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderBook {
    pub symbol: String,
    pub bids: Vec<OrderBookEntry>,
    pub asks: Vec<OrderBookEntry>,
    pub last_update_id: u64,
}

/// Funding rate information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FundingRate {
    pub symbol: String,
    pub funding_rate: Decimal,
    pub funding_time: DateTime<Utc>,
    pub next_funding_time: Option<DateTime<Utc>>,
}

/// Mark price information
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

/// Kline (candlestick) data
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

/// Public trade information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PublicTrade {
    pub id: String,
    pub symbol: String,
    pub price: Decimal,
    pub quantity: Decimal,
    pub timestamp: DateTime<Utc>,
    pub is_buyer_maker: bool,
}

/// Trade information (account trade)
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

/// Position information
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

/// Exchange server time
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExchangeTime {
    pub server_time: DateTime<Utc>,
    pub local_time: DateTime<Utc>,
    pub offset_ms: i64,
}

/// Futures account information
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
