// storage/stop_order_repository.rs
// 止损止盈订单持久化仓储

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use sqlx::PgPool;
use uuid::Uuid;

/// 止损止盈订单记录
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct StopOrderRecord {
    pub id: Uuid,
    pub exchange: String,
    pub market_type: String,
    pub symbol: String,
    pub side: String,
    pub quantity: Decimal,
    pub entry_price: Decimal,
    pub stop_loss_price: Option<Decimal>,
    pub take_profit_price: Option<Decimal>,
    pub trailing_stop_pct: Option<Decimal>,
    pub exchange_sl_order_id: Option<String>,
    pub exchange_tp_order_id: Option<String>,
    pub status: String,
    pub triggered_at: Option<DateTime<Utc>>,
    pub triggered_reason: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// 止损止盈订单仓储
pub struct StopOrderRepository {
    pool: PgPool,
}

impl StopOrderRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// 创建止损止盈订单
    pub async fn create(
        &self,
        exchange: &str,
        market_type: &str,
        symbol: &str,
        side: &str,
        quantity: Decimal,
        entry_price: Decimal,
        stop_loss_price: Option<Decimal>,
        take_profit_price: Option<Decimal>,
        trailing_stop_pct: Option<Decimal>,
    ) -> Result<StopOrderRecord, sqlx::Error> {
        let record = sqlx::query_as::<_, StopOrderRecord>(
            r#"
            INSERT INTO stop_orders (
                exchange, market_type, symbol, side, quantity, entry_price,
                stop_loss_price, take_profit_price, trailing_stop_pct, status
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, 'active')
            RETURNING *
            "#,
        )
        .bind(exchange)
        .bind(market_type)
        .bind(symbol)
        .bind(side)
        .bind(quantity)
        .bind(entry_price)
        .bind(stop_loss_price)
        .bind(take_profit_price)
        .bind(trailing_stop_pct)
        .fetch_one(&self.pool)
        .await?;

        Ok(record)
    }

    /// 更新交易所条件单 ID
    pub async fn update_exchange_order_ids(
        &self,
        id: Uuid,
        exchange_sl_order_id: Option<&str>,
        exchange_tp_order_id: Option<&str>,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            UPDATE stop_orders
            SET exchange_sl_order_id = COALESCE($2, exchange_sl_order_id),
                exchange_tp_order_id = COALESCE($3, exchange_tp_order_id),
                updated_at = NOW()
            WHERE id = $1
            "#,
        )
        .bind(id)
        .bind(exchange_sl_order_id)
        .bind(exchange_tp_order_id)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// 更新状态（触发/取消）
    pub async fn update_status(
        &self,
        id: Uuid,
        status: &str,
        triggered_reason: Option<&str>,
    ) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            UPDATE stop_orders
            SET status = $2,
                triggered_reason = $3,
                triggered_at = CASE WHEN $2 = 'triggered' THEN NOW() ELSE triggered_at END,
                updated_at = NOW()
            WHERE id = $1
            "#,
        )
        .bind(id)
        .bind(status)
        .bind(triggered_reason)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// 获取活跃的止损止盈订单
    pub async fn get_active_orders(
        &self,
        exchange: &str,
        market_type: &str,
    ) -> Result<Vec<StopOrderRecord>, sqlx::Error> {
        let records = sqlx::query_as::<_, StopOrderRecord>(
            r#"
            SELECT * FROM stop_orders
            WHERE exchange = $1 AND market_type = $2 AND status = 'active'
            ORDER BY created_at DESC
            "#,
        )
        .bind(exchange)
        .bind(market_type)
        .fetch_all(&self.pool)
        .await?;

        Ok(records)
    }

    /// 获取指定交易对的活跃止损单
    pub async fn get_active_by_symbol(
        &self,
        exchange: &str,
        symbol: &str,
    ) -> Result<Option<StopOrderRecord>, sqlx::Error> {
        let record = sqlx::query_as::<_, StopOrderRecord>(
            r#"
            SELECT * FROM stop_orders
            WHERE exchange = $1 AND symbol = $2 AND status = 'active'
            ORDER BY created_at DESC LIMIT 1
            "#,
        )
        .bind(exchange)
        .bind(symbol)
        .fetch_optional(&self.pool)
        .await?;

        Ok(record)
    }

    /// 删除止损单
    pub async fn delete(&self, id: Uuid) -> Result<(), sqlx::Error> {
        sqlx::query("DELETE FROM stop_orders WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    /// 标记所有活跃订单为过期（引擎重启时调用）
    pub async fn expire_all_active(&self) -> Result<u64, sqlx::Error> {
        let result = sqlx::query(
            r#"
            UPDATE stop_orders SET status = 'expired', updated_at = NOW()
            WHERE status = 'active'
            "#,
        )
        .execute(&self.pool)
        .await?;

        Ok(result.rows_affected())
    }
}
