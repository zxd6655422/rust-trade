// storage/event_publisher.rs
// 事件发布器 - 通过 Redis LIST 将交易事件推送到 trading-core
//
// 职责：
// - 将 TradingEvent 序列化为 JSON
// - LPUSH 到 Redis 队列 "trading:events:queue"
// - trading-core 使用 BRPOP 消费，转发到 WebSocket 客户端

use redis::aio::ConnectionManager;
use tracing::{debug, warn};

use trading_common::data::event_types::TradingEvent;

/// Redis 队列名称
const EVENT_QUEUE: &str = "trading:events:queue";

/// 事件发布器
#[derive(Clone)]
pub struct EventPublisher {
    redis: ConnectionManager,
}

impl EventPublisher {
    /// 创建新的事件发布器
    pub fn new(redis: ConnectionManager) -> Self {
        Self { redis }
    }

    /// 发布事件到 Redis 队列
    pub async fn publish(&self, event: &TradingEvent) -> Result<(), redis::RedisError> {
        let json = match serde_json::to_string(event) {
            Ok(j) => j,
            Err(e) => {
                warn!("Failed to serialize trading event: {}", e);
                return Ok(());
            }
        };

        let mut conn = self.redis.clone();
        let result: Result<i64, redis::RedisError> = redis::cmd("LPUSH")
            .arg(EVENT_QUEUE)
            .arg(&json)
            .query_async(&mut conn)
            .await;

        match result {
            Ok(queue_len) => {
                debug!(
                    "Published event to {} (queue length: {}): {}",
                    EVENT_QUEUE,
                    queue_len,
                    event.event_type_name()
                );
            }
            Err(e) => {
                warn!("Failed to publish event to Redis: {}", e);
                return Err(e);
            }
        }

        Ok(())
    }
}
