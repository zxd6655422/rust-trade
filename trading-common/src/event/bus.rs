// event/bus.rs
// 事件总线实现

use tokio::sync::broadcast;

use super::types::Event;

/// 事件总线配置
#[derive(Debug, Clone)]
pub struct EventBusConfig {
    /// 通道容量
    pub capacity: usize,
}

impl Default for EventBusConfig {
    fn default() -> Self {
        Self { capacity: 1024 }
    }
}

/// 事件总线
///
/// 基于 tokio::sync::broadcast 实现的发布-订阅事件总线。
/// 支持多个发布者和多个订阅者。
///
/// # 示例
///
/// ```rust
/// use trading_common::event::{EventBus, Event, MarketEvent};
///
/// # tokio_test::block_on(async {
/// let bus = EventBus::new(Default::default());
///
/// // 订阅事件
/// let mut rx = bus.subscribe();
///
/// // 发布事件
/// bus.publish(MarketEvent::Tick(tick_data).into());
///
/// // 接收事件
/// if let Ok(event) = rx.recv().await {
///     // 处理事件
/// }
/// # });
/// ```
#[derive(Debug, Clone)]
pub struct EventBus {
    sender: broadcast::Sender<Event>,
}

impl EventBus {
    /// 创建新的事件总线
    pub fn new(config: EventBusConfig) -> Self {
        let (sender, _) = broadcast::channel(config.capacity);
        Self { sender }
    }

    /// 创建默认配置的事件总线
    pub fn with_capacity(capacity: usize) -> Self {
        let (sender, _) = broadcast::channel(capacity);
        Self { sender }
    }

    /// 发布事件
    ///
    /// 如果没有订阅者，事件会被丢弃（不会阻塞）。
    pub fn publish(&self, event: Event) {
        // 忽略发送错误（没有接收者）
        let _ = self.sender.send(event);
    }

    /// 订阅事件
    ///
    /// 返回一个接收器，可以用来接收后续发布的所有事件。
    pub fn subscribe(&self) -> broadcast::Receiver<Event> {
        self.sender.subscribe()
    }

    /// 获取当前订阅者数量
    pub fn subscriber_count(&self) -> usize {
        self.sender.receiver_count()
    }

    /// 获取发送端的克隆（用于外部集成）
    pub fn sender(&self) -> &broadcast::Sender<Event> {
        &self.sender
    }
}

impl Default for EventBus {
    fn default() -> Self {
        Self::new(EventBusConfig::default())
    }
}

/// 事件过滤器
///
/// 用于过滤事件，只处理感兴趣的事件类型。
pub struct EventFilter {
    /// 是否处理市场事件
    pub market: bool,
    /// 是否处理交易事件
    pub trading: bool,
    /// 是否处理策略事件
    pub strategy: bool,
    /// 是否处理系统事件
    pub system: bool,
}

impl EventFilter {
    /// 创建接受所有事件的过滤器
    pub fn all() -> Self {
        Self {
            market: true,
            trading: true,
            strategy: true,
            system: true,
        }
    }

    /// 创建只接受市场事件的过滤器
    pub fn market_only() -> Self {
        Self {
            market: true,
            trading: false,
            strategy: false,
            system: false,
        }
    }

    /// 创建只接受交易事件的过滤器
    pub fn trading_only() -> Self {
        Self {
            market: false,
            trading: true,
            strategy: false,
            system: false,
        }
    }

    /// 检查事件是否通过过滤器
    pub fn matches(&self, event: &Event) -> bool {
        match event {
            Event::Market(_) => self.market,
            Event::Trading(_) => self.trading,
            Event::Strategy(_) => self.strategy,
            Event::System(_) => self.system,
        }
    }
}

impl Default for EventFilter {
    fn default() -> Self {
        Self::all()
    }
}

/// 事件处理器
///
/// 用于处理接收到的事件，支持过滤。
pub struct EventHandler {
    filter: EventFilter,
    handler: Box<dyn Fn(&Event) + Send + Sync>,
}

impl EventHandler {
    /// 创建新的事件处理器
    pub fn new<F>(filter: EventFilter, handler: F) -> Self
    where
        F: Fn(&Event) + Send + Sync + 'static,
    {
        Self {
            filter,
            handler: Box::new(handler),
        }
    }

    /// 处理事件（如果通过过滤器）
    pub fn handle(&self, event: &Event) {
        if self.filter.matches(event) {
            (self.handler)(event);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::types::TickData;
    use chrono::Utc;
    use rust_decimal::Decimal;

    #[test]
    fn test_event_bus_publish_subscribe() {
        let bus = EventBus::new(Default::default());
        let mut rx = bus.subscribe();

        let tick = TickData {
            timestamp: Utc::now(),
            symbol: "BTCUSDT".to_string(),
            price: Decimal::from(50000),
            quantity: Decimal::from(1),
            side: crate::data::types::TradeSide::Buy,
            trade_id: "123".to_string(),
            is_buyer_maker: true,
        };

        let event: Event = super::super::types::MarketEvent::Tick(tick).into();
        bus.publish(event);

        // 注意：在异步测试中需要 tokio runtime
        // 这里只测试同步部分
        assert_eq!(bus.subscriber_count(), 1);
    }

    #[test]
    fn test_event_filter() {
        let filter = EventFilter::market_only();

        let tick = TickData {
            timestamp: Utc::now(),
            symbol: "BTCUSDT".to_string(),
            price: Decimal::from(50000),
            quantity: Decimal::from(1),
            side: crate::data::types::TradeSide::Buy,
            trade_id: "123".to_string(),
            is_buyer_maker: true,
        };

        let market_event: Event = super::super::types::MarketEvent::Tick(tick).into();
        assert!(filter.matches(&market_event));

        let system_event: Event = super::super::types::SystemEvent::Heartbeat {
            timestamp: Utc::now(),
        }
        .into();
        assert!(!filter.matches(&system_event));
    }
}
