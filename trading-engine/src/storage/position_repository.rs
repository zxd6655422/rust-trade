// storage/position_repository.rs
// 持仓仓储层

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use sqlx::PgPool;
use uuid::Uuid;

use crate::exchange::types::{PositionInfo, PositionSide};

/// 持仓记录
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct PositionRecord {
    pub id: Uuid,
    pub exchange: String,
    pub symbol: String,
    pub side: String,
    pub quantity: Decimal,
    pub avg_entry_price: Decimal,
    pub unrealized_pnl: Decimal,
    pub stop_loss_price: Option<Decimal>,
    pub take_profit_price: Option<Decimal>,
    pub leverage: i32,
    pub margin: Decimal,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// 持仓仓储
pub struct PositionRepository {
    pool: PgPool,
}

impl PositionRepository {
    /// 创建新的持仓仓储
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// 创建或更新持仓
    pub async fn upsert_position(
        &self,
        exchange: &str,
        symbol: &str,
        side: &str,
        quantity: Decimal,
        avg_entry_price: Decimal,
    ) -> Result<PositionRecord, sqlx::Error> {
        let record = sqlx::query_as::<_, PositionRecord>(
            r#"
            INSERT INTO trading_positions (exchange, symbol, side, quantity, avg_entry_price)
            VALUES ($1, $2, $3, $4, $5)
            ON CONFLICT (exchange, symbol) DO UPDATE
            SET side = $3,
                quantity = $4,
                avg_entry_price = $5,
                updated_at = NOW()
            RETURNING *
            "#,
        )
        .bind(exchange)
        .bind(symbol)
        .bind(side)
        .bind(quantity)
        .bind(avg_entry_price)
        .fetch_one(&self.pool)
        .await?;

        Ok(record)
    }

    /// 更新持仓盈亏
    pub async fn update_unrealized_pnl(
        &self,
        exchange: &str,
        symbol: &str,
        unrealized_pnl: Decimal,
    ) -> Result<PositionRecord, sqlx::Error> {
        let record = sqlx::query_as::<_, PositionRecord>(
            r#"
            UPDATE trading_positions
            SET unrealized_pnl = $3,
                updated_at = NOW()
            WHERE exchange = $1 AND symbol = $2
            RETURNING *
            "#,
        )
        .bind(exchange)
        .bind(symbol)
        .bind(unrealized_pnl)
        .fetch_one(&self.pool)
        .await?;

        Ok(record)
    }

    /// 更新止损止盈价格
    pub async fn update_stop_loss_take_profit(
        &self,
        exchange: &str,
        symbol: &str,
        stop_loss_price: Option<Decimal>,
        take_profit_price: Option<Decimal>,
    ) -> Result<PositionRecord, sqlx::Error> {
        let record = sqlx::query_as::<_, PositionRecord>(
            r#"
            UPDATE trading_positions
            SET stop_loss_price = $3,
                take_profit_price = $4,
                updated_at = NOW()
            WHERE exchange = $1 AND symbol = $2
            RETURNING *
            "#,
        )
        .bind(exchange)
        .bind(symbol)
        .bind(stop_loss_price)
        .bind(take_profit_price)
        .fetch_one(&self.pool)
        .await?;

        Ok(record)
    }

    /// 获取持仓
    pub async fn get_position(
        &self,
        exchange: &str,
        symbol: &str,
    ) -> Result<Option<PositionRecord>, sqlx::Error> {
        let record = sqlx::query_as::<_, PositionRecord>(
            r#"
            SELECT * FROM trading_positions
            WHERE exchange = $1 AND symbol = $2
            "#,
        )
        .bind(exchange)
        .bind(symbol)
        .fetch_optional(&self.pool)
        .await?;

        Ok(record)
    }

    /// 获取所有持仓
    pub async fn get_all_positions(
        &self,
        exchange: &str,
    ) -> Result<Vec<PositionRecord>, sqlx::Error> {
        let records = sqlx::query_as::<_, PositionRecord>(
            r#"
            SELECT * FROM trading_positions
            WHERE exchange = $1 AND quantity > 0
            ORDER BY updated_at DESC
            "#,
        )
        .bind(exchange)
        .fetch_all(&self.pool)
        .await?;

        Ok(records)
    }

    /// 删除持仓
    pub async fn delete_position(&self, exchange: &str, symbol: &str) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"
            DELETE FROM trading_positions
            WHERE exchange = $1 AND symbol = $2
            "#,
        )
        .bind(exchange)
        .bind(symbol)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    /// 转换为 PositionInfo
    pub fn to_position_info(record: &PositionRecord) -> PositionInfo {
        PositionInfo {
            symbol: record.symbol.clone(),
            side: match record.side.as_str() {
                "LONG" => PositionSide::Long,
                "SHORT" => PositionSide::Short,
                _ => PositionSide::None,
            },
            quantity: record.quantity,
            avg_entry_price: record.avg_entry_price,
            mark_price: None,
            unrealized_pnl: record.unrealized_pnl,
            leverage: record.leverage as u32,
            margin: record.margin,
            liquidation_price: None,
        }
    }
}
