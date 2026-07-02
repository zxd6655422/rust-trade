// event/mod.rs
// 事件驱动架构模块

pub mod bus;
pub mod types;

// Re-export commonly used types
pub use bus::{EventBus, EventBusConfig, EventHandler, EventFilter};
pub use types::{
    ConnectionStatus, Event, MarketEvent, OrderSide, OrderStatus, OrderType, SignalType,
    StrategyEvent, SystemEvent, TradingEvent,
};
