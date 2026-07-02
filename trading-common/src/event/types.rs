// event/types.rs
// 事件类型定义

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use crate::data::types::{OHLCData, TickData, Timeframe};

// ===== 市场事件 =====

/// 市场数据事件
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MarketEvent {
    /// 实时成交数据
    Tick(TickData),
    /// K线数据（完成的）
    Kline(OHLCData),
    /// 订单簿更新
    OrderBookUpdate {
        symbol: String,
        bids: Vec<(Decimal, Decimal)>,
        asks: Vec<(Decimal, Decimal)>,
        timestamp: DateTime<Utc>,
    },
    /// 标记价格更新
    MarkPriceUpdate {
        symbol: String,
        mark_price: Decimal,
        index_price: Decimal,
        timestamp: DateTime<Utc>,
    },
}

// ===== 交易事件 =====

/// 订单状态
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum OrderStatus {
    New,
    PartiallyFilled,
    Filled,
    Canceled,
    Rejected,
    Expired,
}

/// 订单方向
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum OrderSide {
    Buy,
    Sell,
}

/// 订单类型
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum OrderType {
    Market,
    Limit,
    StopLoss,
    TakeProfit,
}

/// 交易事件
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TradingEvent {
    /// 订单已提交
    OrderPlaced {
        order_id: String,
        symbol: String,
        side: OrderSide,
        order_type: OrderType,
        quantity: Decimal,
        price: Option<Decimal>,
        timestamp: DateTime<Utc>,
    },
    /// 订单已成交（全部或部分）
    OrderFilled {
        order_id: String,
        symbol: String,
        side: OrderSide,
        filled_quantity: Decimal,
        filled_price: Decimal,
        commission: Decimal,
        timestamp: DateTime<Utc>,
    },
    /// 订单已取消
    OrderCancelled {
        order_id: String,
        symbol: String,
        timestamp: DateTime<Utc>,
    },
    /// 订单被拒绝
    OrderRejected {
        order_id: String,
        symbol: String,
        reason: String,
        timestamp: DateTime<Utc>,
    },
    /// 持仓变化
    PositionChanged {
        symbol: String,
        side: OrderSide,
        quantity: Decimal,
        avg_price: Decimal,
        unrealized_pnl: Decimal,
        timestamp: DateTime<Utc>,
    },
}

// ===== 策略事件 =====

/// 信号类型
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SignalType {
    Buy,
    Sell,
    Hold,
}

/// 策略事件
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum StrategyEvent {
    /// 策略产生信号
    SignalGenerated {
        strategy_id: String,
        symbol: String,
        signal: SignalType,
        price: Decimal,
        timestamp: DateTime<Utc>,
    },
    /// 策略启动
    StrategyStarted {
        strategy_id: String,
        timestamp: DateTime<Utc>,
    },
    /// 策略停止
    StrategyStopped {
        strategy_id: String,
        reason: String,
        timestamp: DateTime<Utc>,
    },
    /// 策略参数更新
    ParametersUpdated {
        strategy_id: String,
        parameters: std::collections::HashMap<String, String>,
        timestamp: DateTime<Utc>,
    },
}

// ===== 系统事件 =====

/// 连接状态
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ConnectionStatus {
    Connected,
    Disconnected,
    Reconnecting,
    Error(String),
}

/// 系统事件
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SystemEvent {
    /// 系统错误
    Error {
        module: String,
        message: String,
        timestamp: DateTime<Utc>,
    },
    /// 心跳
    Heartbeat {
        timestamp: DateTime<Utc>,
    },
    /// 连接状态变化
    ConnectionChanged {
        exchange: String,
        status: ConnectionStatus,
        timestamp: DateTime<Utc>,
    },
    /// 数据采集状态
    DataCollectionStatus {
        symbol: String,
        timeframe: Timeframe,
        last_timestamp: DateTime<Utc>,
        gap_detected: bool,
        timestamp: DateTime<Utc>,
    },
}

// ===== 统一事件类型 =====

/// 统一事件枚举
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Event {
    Market(MarketEvent),
    Trading(TradingEvent),
    Strategy(StrategyEvent),
    System(SystemEvent),
}

impl From<MarketEvent> for Event {
    fn from(e: MarketEvent) -> Self {
        Event::Market(e)
    }
}

impl From<TradingEvent> for Event {
    fn from(e: TradingEvent) -> Self {
        Event::Trading(e)
    }
}

impl From<StrategyEvent> for Event {
    fn from(e: StrategyEvent) -> Self {
        Event::Strategy(e)
    }
}

impl From<SystemEvent> for Event {
    fn from(e: SystemEvent) -> Self {
        Event::System(e)
    }
}

impl Event {
    /// 获取事件时间戳
    pub fn timestamp(&self) -> DateTime<Utc> {
        match self {
            Event::Market(e) => match e {
                MarketEvent::Tick(tick) => tick.timestamp,
                MarketEvent::Kline(kline) => kline.timestamp,
                MarketEvent::OrderBookUpdate { timestamp, .. } => *timestamp,
                MarketEvent::MarkPriceUpdate { timestamp, .. } => *timestamp,
            },
            Event::Trading(e) => match e {
                TradingEvent::OrderPlaced { timestamp, .. } => *timestamp,
                TradingEvent::OrderFilled { timestamp, .. } => *timestamp,
                TradingEvent::OrderCancelled { timestamp, .. } => *timestamp,
                TradingEvent::OrderRejected { timestamp, .. } => *timestamp,
                TradingEvent::PositionChanged { timestamp, .. } => *timestamp,
            },
            Event::Strategy(e) => match e {
                StrategyEvent::SignalGenerated { timestamp, .. } => *timestamp,
                StrategyEvent::StrategyStarted { timestamp, .. } => *timestamp,
                StrategyEvent::StrategyStopped { timestamp, .. } => *timestamp,
                StrategyEvent::ParametersUpdated { timestamp, .. } => *timestamp,
            },
            Event::System(e) => match e {
                SystemEvent::Error { timestamp, .. } => *timestamp,
                SystemEvent::Heartbeat { timestamp, .. } => *timestamp,
                SystemEvent::ConnectionChanged { timestamp, .. } => *timestamp,
                SystemEvent::DataCollectionStatus { timestamp, .. } => *timestamp,
            },
        }
    }

    /// 获取事件类别名称
    pub fn category(&self) -> &'static str {
        match self {
            Event::Market(_) => "market",
            Event::Trading(_) => "trading",
            Event::Strategy(_) => "strategy",
            Event::System(_) => "system",
        }
    }
}
