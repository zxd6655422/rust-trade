//! 订单状态同步模块
//!
//! 轮询交易所订单状态，更新 trades 表

use anyhow::Result;
use chrono::Utc;
use rust_decimal::Decimal;
use sqlx::PgPool;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

use crate::exchange::{ExchangeClient, ExchangeApiConfig};

/// 订单同步器
pub struct OrderSync {
    pool: PgPool,
    exchange_client: Option<ExchangeClient>,
    /// 同步间隔（秒）
    sync_interval_secs: u64,
}

impl OrderSync {
    pub fn new(pool: PgPool, sync_interval_secs: u64) -> Self {
        // 尝试从环境变量创建交易所客户端
        let exchange_client = match ExchangeApiConfig::binance_from_env() {
            Ok(api_config) => Some(ExchangeClient::new(api_config)),
            Err(e) => {
                warn!("Failed to create exchange client for order sync: {}", e);
                None
            }
        };

        Self {
            pool,
            exchange_client,
            sync_interval_secs,
        }
    }

    /// 启动订单同步任务
    pub async fn start(self: Arc<Self>) {
        info!("Starting order sync task (interval: {}s)", self.sync_interval_secs);

        let mut interval = tokio::time::interval(Duration::from_secs(self.sync_interval_secs));

        loop {
            interval.tick().await;

            if let Err(e) = self.sync_pending_orders().await {
                error!("Order sync error: {}", e);
            }
        }
    }

    /// 同步待处理订单
    async fn sync_pending_orders(&self) -> Result<()> {
        let client = match &self.exchange_client {
            Some(client) => client,
            None => return Ok(()),
        };

        // 查询所有待处理订单
        let pending_orders = self.get_pending_orders().await?;

        if pending_orders.is_empty() {
            return Ok(());
        }

        debug!("Syncing {} pending orders", pending_orders.len());

        for order in pending_orders {
            match self.sync_single_order(client, &order).await {
                Ok(updated) => {
                    if updated {
                        debug!("Order {} updated", order.order_id);
                    }
                }
                Err(e) => {
                    warn!("Failed to sync order {}: {}", order.order_id, e);
                }
            }
        }

        Ok(())
    }

    /// 获取待处理订单
    async fn get_pending_orders(&self) -> Result<Vec<PendingOrder>> {
        let orders = sqlx::query_as::<_, PendingOrder>(
            r#"
            SELECT id, symbol, order_id, side, quantity, exchange, market_type
            FROM trades
            WHERE order_status IN ('pending', 'partially_filled')
              AND order_id IS NOT NULL
              AND created_at > NOW() - INTERVAL '24 hours'
            ORDER BY created_at DESC
            LIMIT 100
            "#
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(orders)
    }

    /// 同步单个订单
    async fn sync_single_order(
        &self,
        client: &ExchangeClient,
        order: &PendingOrder,
    ) -> Result<bool> {
        // 查询订单状态
        let order_status = client
            .get_order_status(&order.symbol, &order.order_id)
            .await?;

        // 检查状态是否变化
        let new_status = match order_status.status.as_str() {
            "NEW" => "pending",
            "PARTIALLY_FILLED" => "partially_filled",
            "FILLED" => "filled",
            "CANCELED" => "cancelled",
            "REJECTED" => "rejected",
            "EXPIRED" => "expired",
            _ => return Ok(false),
        };

        // 更新订单状态
        let affected = sqlx::query(
            r#"
            UPDATE trades
            SET order_status = $1,
                price = $2,
                metadata = COALESCE(metadata, '{}'::jsonb) || $3::jsonb
            WHERE id = $4 AND order_status != $1
            "#
        )
        .bind(new_status)
        .bind(order_status.avg_price)
        .bind(serde_json::json!({
            "executed_qty": order_status.executed_qty,
            "cummulative_quote_qty": order_status.cummulative_quote_qty,
            "exchange_status": order_status.status,
            "last_sync": Utc::now(),
        }))
        .bind(order.id)
        .execute(&self.pool)
        .await?;

        if affected.rows_affected() > 0 {
            info!(
                "Order {} status updated: {} (executed: {})",
                order.order_id,
                new_status,
                order_status.executed_qty
            );

            // 如果订单已成交，更新持仓
            if new_status == "filled" {
                self.update_position_after_fill(order, &order_status).await?;
            }

            return Ok(true);
        }

        Ok(false)
    }

    /// 订单成交后更新持仓
    async fn update_position_after_fill(
        &self,
        order: &PendingOrder,
        order_status: &crate::exchange::OrderStatus,
    ) -> Result<()> {
        let side = order.side.as_str();
        let quantity = order_status.executed_qty;
        let price = order_status.avg_price;

        // 检查是否已有持仓
        let existing_position = sqlx::query_scalar::<_, Decimal>(
            "SELECT COALESCE(quantity, 0) FROM positions WHERE symbol = $1"
        )
        .bind(&order.symbol)
        .fetch_optional(&self.pool)
        .await?;

        match existing_position {
            Some(current_qty) => {
                if side == "BUY" {
                    // 买入：增加持仓
                    let new_qty = current_qty + quantity;
                    let new_avg_price = if current_qty > Decimal::ZERO {
                        // 计算新的平均价格
                        let current_value = current_qty * self.get_avg_price(&order.symbol).await?;
                        let new_value = quantity * price;
                        (current_value + new_value) / new_qty
                    } else {
                        price
                    };

                    sqlx::query(
                        r#"
                        UPDATE positions
                        SET quantity = $1,
                            avg_entry_price = $2,
                            updated_at = NOW()
                        WHERE symbol = $3
                        "#
                    )
                    .bind(new_qty)
                    .bind(new_avg_price)
                    .bind(&order.symbol)
                    .execute(&self.pool)
                    .await?;

                    info!(
                        "Position updated: {} qty={} avg_price={}",
                        order.symbol, new_qty, new_avg_price
                    );
                } else {
                    // 卖出：减少持仓
                    let new_qty = (current_qty - quantity).max(Decimal::ZERO);

                    if new_qty == Decimal::ZERO {
                        // 平仓
                        sqlx::query("DELETE FROM positions WHERE symbol = $1")
                            .bind(&order.symbol)
                            .execute(&self.pool)
                            .await?;

                        info!("Position closed: {}", order.symbol);
                    } else {
                        sqlx::query(
                            r#"
                            UPDATE positions
                            SET quantity = $1,
                                updated_at = NOW()
                            WHERE symbol = $2
                            "#
                        )
                        .bind(new_qty)
                        .bind(&order.symbol)
                        .execute(&self.pool)
                        .await?;

                        info!(
                            "Position reduced: {} qty={}",
                            order.symbol, new_qty
                        );
                    }
                }
            }
            None => {
                // 新建持仓
                sqlx::query(
                    r#"
                    INSERT INTO positions (symbol, side, quantity, avg_entry_price, exchange, market_type, opened_at, updated_at)
                    VALUES ($1, $2, $3, $4, $5, $6, NOW(), NOW())
                    "#
                )
                .bind(&order.symbol)
                .bind(if side == "BUY" { "LONG" } else { "SHORT" })
                .bind(quantity)
                .bind(price)
                .bind(&order.exchange)
                .bind(&order.market_type)
                .execute(&self.pool)
                .await?;

                info!(
                    "New position created: {} {} qty={} price={}",
                    order.symbol, side, quantity, price
                );
            }
        }

        Ok(())
    }

    /// 获取持仓平均价格
    async fn get_avg_price(&self, symbol: &str) -> Result<Decimal> {
        let avg_price = sqlx::query_scalar::<_, Decimal>(
            "SELECT COALESCE(avg_entry_price, 0) FROM positions WHERE symbol = $1"
        )
        .bind(symbol)
        .fetch_optional(&self.pool)
        .await?
        .unwrap_or(Decimal::ZERO);

        Ok(avg_price)
    }
}

/// 待处理订单
#[derive(Debug, sqlx::FromRow)]
struct PendingOrder {
    id: uuid::Uuid,
    symbol: String,
    order_id: String,
    side: String,
    quantity: Decimal,
    exchange: String,
    market_type: String,
}
