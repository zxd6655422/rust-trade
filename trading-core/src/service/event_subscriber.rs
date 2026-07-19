// service/event_subscriber.rs
// Redis 事件订阅器 - 订阅 trading-engine 发布的交易事件
//
// 职责：
// - 订阅 Redis channel "trading:events"
// - 反序列化 TradingEvent
// - 通过 broadcast channel 转发给 WebSocket 客户端
//
// 实现方式：使用 Redis LIST + BRPOP 作为消息队列（比 Pub/Sub 更可靠）

use redis::aio::ConnectionManager;
use tokio::sync::broadcast;
use tracing::{debug, error, info, warn};

use trading_common::data::event_types::TradingEvent;

/// Redis 队列名称（与 trading-engine 侧一致）
const EVENT_QUEUE: &str = "trading:events:queue";

/// 使用 Redis LIST 的事件订阅器
pub struct PubSubEventSubscriber {
    redis: ConnectionManager,
    event_tx: broadcast::Sender<TradingEvent>,
}

impl PubSubEventSubscriber {
    /// 创建新的事件订阅器
    pub fn new(redis: ConnectionManager, event_tx: broadcast::Sender<TradingEvent>) -> Self {
        Self { redis, event_tx }
    }

    /// 启动事件订阅（后台循环）
    pub async fn start(&self) {
        info!("🔔 Event subscriber starting on queue: {}", EVENT_QUEUE);

        loop {
            match self.poll_once().await {
                Ok(Some(event)) => {
                    debug!("Received event: {}", event.event_type_name());
                    // 广播到 WebSocket 客户端
                    if let Err(e) = self.event_tx.send(event) {
                        // 没有订阅者时不报错
                        debug!("No event subscribers: {}", e);
                    }
                }
                Ok(None) => {
                    // 没有消息，短暂休眠
                    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
                }
                Err(e) => {
                    warn!("Event subscriber error: {}", e);
                    tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
                }
            }
        }
    }

    /// 轮询一次 Redis 队列
    async fn poll_once(&self) -> Result<Option<TradingEvent>, redis::RedisError> {
        let mut conn = self.redis.clone();

        // 使用 BRPOP 等待消息（带超时）
        let result: Option<(String, String)> = redis::cmd("BRPOP")
            .arg(EVENT_QUEUE)
            .arg(0.1) // 100ms 超时
            .query_async(&mut conn)
            .await?;

        match result {
            Some((_, payload)) => {
                match serde_json::from_str::<TradingEvent>(&payload) {
                    Ok(event) => Ok(Some(event)),
                    Err(e) => {
                        warn!("Failed to deserialize trading event: {}", e);
                        Ok(None)
                    }
                }
            }
            None => Ok(None),
        }
    }
}
