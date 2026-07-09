// engine/signal_poller.rs
//
// 信号轮询器：从 strategy_signals 表读取待执行信号，通过 OrderManager 执行交易

use std::sync::Arc;
use std::time::Duration;
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use sqlx::{PgPool, Row};
use tokio::time::interval;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

use trading_common::backtest::strategy::Signal;

use crate::order::OrderManager;

/// 信号记录（从 strategy_signals 表读取）
#[derive(Debug, Clone)]
struct SignalRecord {
    pub id: Uuid,
    pub symbol: String,
    pub strategy_id: String,
    pub direction: String,
    pub entry_price: Decimal,
    pub overall_confidence: Decimal,
    pub entry_allowed: bool,
    pub status: String,
    pub created_at: DateTime<Utc>,
}

/// 信号轮询器配置
pub struct SignalPollerConfig {
    /// 轮询间隔（毫秒）
    pub poll_interval_ms: u64,
    /// 每次最多获取的信号数
    pub batch_size: i64,
    /// 信号过期时间（小时）
    pub signal_expire_hours: i64,
    /// 默认下单数量（当信号没有指定数量时）
    pub default_quantity: Decimal,
}

impl Default for SignalPollerConfig {
    fn default() -> Self {
        Self {
            poll_interval_ms: 5000,
            batch_size: 10,
            signal_expire_hours: 1,
            default_quantity: Decimal::from(100), // 默认 100 USDT 等值
        }
    }
}

/// 信号轮询器
///
/// 定时从 strategy_signals 表获取待执行信号，
/// 转换为交易引擎的 Signal 格式，通过 OrderManager 执行
pub struct SignalPoller {
    pool: PgPool,
    order_manager: Arc<OrderManager>,
    config: SignalPollerConfig,
}

impl SignalPoller {
    pub fn new(
        pool: PgPool,
        order_manager: Arc<OrderManager>,
        config: SignalPollerConfig,
    ) -> Self {
        Self {
            pool,
            order_manager,
            config,
        }
    }

    /// 启动轮询循环
    pub async fn start(self: Arc<Self>) {
        info!(
            "Signal poller started (interval: {}ms, batch: {})",
            self.config.poll_interval_ms, self.config.batch_size
        );

        let mut poll_interval = interval(Duration::from_millis(self.config.poll_interval_ms));
        let mut expire_interval = interval(Duration::from_secs(3600)); // 每小时清理过期信号

        loop {
            tokio::select! {
                _ = poll_interval.tick() => {
                    if let Err(e) = self.poll_and_execute().await {
                        error!("Signal poll error: {}", e);
                    }
                }
                _ = expire_interval.tick() => {
                    if let Err(e) = self.expire_old_signals().await {
                        warn!("Failed to expire old signals: {}", e);
                    }
                }
            }
        }
    }

    /// 轮询并执行信号
    async fn poll_and_execute(&self) -> Result<(), String> {
        let signals = self.get_pending_signals().await?;

        if signals.is_empty() {
            return Ok(());
        }

        debug!("Found {} pending signals", signals.len());

        for record in signals {
            let signal = self.convert_signal(&record);
            let signal_id = record.id;

            info!(
                "Executing signal: {} {} @ {} (confidence: {}, strategy: {})",
                record.direction, record.symbol, record.entry_price,
                record.overall_confidence, record.strategy_id
            );

            match self.order_manager.execute_signal(signal).await {
                Ok(result) => {
                    info!(
                        "Signal executed: {} -> order_id={}",
                        signal_id, result.order_id
                    );
                    self.mark_signal_executed(signal_id, &result.order_id).await;
                }
                Err(e) => {
                    warn!("Signal execution failed: {} - {}", signal_id, e);
                    self.mark_signal_rejected(signal_id, &e.to_string()).await;
                }
            }
        }

        Ok(())
    }

    /// 获取待执行信号
    async fn get_pending_signals(&self) -> Result<Vec<SignalRecord>, String> {
        let rows = sqlx::query(
            "SELECT id, symbol, strategy_id, direction, entry_price, \
                    overall_confidence, entry_allowed, status, created_at \
             FROM strategy_signals \
             WHERE status='pending' AND entry_allowed=true \
             ORDER BY created_at DESC LIMIT $1"
        )
        .bind(self.config.batch_size)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| format!("Failed to query signals: {}", e))?;

        let records: Vec<SignalRecord> = rows.iter().map(|r| SignalRecord {
            id: r.get::<Uuid, _>("id"),
            symbol: r.get::<String, _>("symbol"),
            strategy_id: r.get::<String, _>("strategy_id"),
            direction: r.get::<String, _>("direction"),
            entry_price: r.get::<Decimal, _>("entry_price"),
            overall_confidence: r.get::<Decimal, _>("overall_confidence"),
            entry_allowed: r.get::<bool, _>("entry_allowed"),
            status: r.get::<String, _>("status"),
            created_at: r.get::<DateTime<Utc>, _>("created_at"),
        }).collect();

        Ok(records)
    }

    /// 标记信号为已执行
    async fn mark_signal_executed(&self, signal_id: Uuid, order_id: &str) {
        if let Err(e) = sqlx::query(
            "UPDATE strategy_signals SET status='executed', closed_reason=$2 \
             WHERE id=$1 AND status='pending'"
        )
        .bind(signal_id)
        .bind(order_id)
        .execute(&self.pool)
        .await
        {
            warn!("Failed to mark signal as executed: {}", e);
        }
    }

    /// 标记信号为已拒绝
    async fn mark_signal_rejected(&self, signal_id: Uuid, reason: &str) {
        if let Err(e) = sqlx::query(
            "UPDATE strategy_signals SET status='rejected', closed_reason=$2 \
             WHERE id=$1 AND status='pending'"
        )
        .bind(signal_id)
        .bind(reason)
        .execute(&self.pool)
        .await
        {
            warn!("Failed to mark signal as rejected: {}", e);
        }
    }

    /// 将数据库信号转换为交易引擎的 Signal 格式
    fn convert_signal(&self, record: &SignalRecord) -> Signal {
        let direction = record.direction.to_lowercase();
        let entry_price = record.entry_price;
        let symbol = record.symbol.clone();
        let quantity = self.config.default_quantity;

        if direction == "bullish" || direction == "buy" {
            Signal::Buy {
                symbol,
                quantity,
                entry_price,
            }
        } else if direction == "bearish" || direction == "sell" {
            Signal::Sell {
                symbol,
                quantity,
                entry_price,
            }
        } else {
            warn!("Unknown signal direction: {}, treating as Hold", direction);
            Signal::Hold
        }
    }

    /// 清理过期信号
    async fn expire_old_signals(&self) -> Result<(), String> {
        let result = sqlx::query(
            "UPDATE strategy_signals SET status='expired', closed_reason='expired', \
             closed_at=NOW(), close_price=entry_price, actual_return_pct=0 \
             WHERE status='pending' AND created_at < NOW() - INTERVAL '1 hour' * $1"
        )
        .bind(self.config.signal_expire_hours)
        .execute(&self.pool)
        .await
        .map_err(|e| format!("Failed to expire signals: {}", e))?;

        let count = result.rows_affected();
        if count > 0 {
            info!("Expired {} old signals", count);
        }

        Ok(())
    }
}
