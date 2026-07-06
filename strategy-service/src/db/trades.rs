use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Debug, Serialize, Deserialize, sqlx::FromRow)]
pub struct Trade {
    pub id: Uuid,
    pub order_id: Option<String>,
    pub symbol: String,
    pub side: String,
    pub price: Decimal,
    pub quantity: Decimal,
    pub commission: Decimal,
    pub realized_pnl: Option<Decimal>,
    pub strategy_id: Option<String>,
    pub trade_time: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
    // V6 新增字段
    pub signal_id: Option<Uuid>,
    pub exchange: Option<String>,
    pub market_type: Option<String>,
    pub order_status: Option<String>,
    pub order_type: Option<String>,
    pub leverage: Option<i32>,
    pub slippage: Option<Decimal>,
    pub metadata: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
pub struct TradeQuery {
    pub strategy_id: Option<String>,
    pub signal_id: Option<Uuid>,
    pub symbol: Option<String>,
    pub start: Option<DateTime<Utc>>,
    pub end: Option<DateTime<Utc>>,
    pub limit: Option<i64>,
}

/// 查询交易记录
pub async fn query_trades(
    pool: &PgPool,
    query: TradeQuery,
) -> Result<Vec<Trade>, sqlx::Error> {
    let limit = query.limit.unwrap_or(100);

    let trades = sqlx::query_as::<_, Trade>(
        r#"
        SELECT *
        FROM trades
        WHERE ($1::text IS NULL OR strategy_id = $1)
          AND ($2::uuid IS NULL OR signal_id = $2)
          AND ($3::text IS NULL OR symbol = $3)
          AND ($4::timestamptz IS NULL OR trade_time >= $4)
          AND ($5::timestamptz IS NULL OR trade_time <= $5)
        ORDER BY trade_time DESC
        LIMIT $6
        "#
    )
    .bind(query.strategy_id)
    .bind(query.signal_id)
    .bind(query.symbol)
    .bind(query.start)
    .bind(query.end)
    .bind(limit)
    .fetch_all(pool)
    .await?;

    Ok(trades)
}

/// 根据 ID 获取交易详情
pub async fn get_trade(
    pool: &PgPool,
    id: Uuid,
) -> Result<Option<Trade>, sqlx::Error> {
    let trade = sqlx::query_as::<_, Trade>(
        "SELECT * FROM trades WHERE id = $1"
    )
    .bind(id)
    .fetch_optional(pool)
    .await?;

    Ok(trade)
}

/// 获取策略实例的交易记录（通过信号关联）
pub async fn get_trades_by_instance(
    pool: &PgPool,
    instance_id: Uuid,
    limit: Option<i64>,
) -> Result<Vec<Trade>, sqlx::Error> {
    let limit = limit.unwrap_or(100);

    let trades = sqlx::query_as::<_, Trade>(
        r#"
        SELECT t.*
        FROM trades t
        JOIN strategy_signals s ON t.signal_id = s.id
        WHERE s.instance_id = $1
        ORDER BY t.trade_time DESC
        LIMIT $2
        "#
    )
    .bind(instance_id)
    .bind(limit)
    .fetch_all(pool)
    .await?;

    Ok(trades)
}
