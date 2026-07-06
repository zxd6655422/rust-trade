use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

/// 策略信号（匹配实际数据库结构）
#[derive(Debug, Serialize, Deserialize, sqlx::FromRow)]
pub struct StrategySignal {
    pub id: Uuid,
    pub symbol: String,
    pub strategy_id: String,
    pub direction: String,           // bullish/bearish/neutral
    pub entry_price: Decimal,
    pub overall_confidence: Decimal,
    pub entry_allowed: bool,
    pub entry_direction: Option<String>,  // long/short
    pub timeframe_details: serde_json::Value,
    pub order_id: Option<String>,
    pub executed: bool,
    pub status: String,              // pending/confirmed/invalidated/expired/superseded
    pub closed_reason: Option<String>,
    pub evaluated_at: Option<DateTime<Utc>>,
    pub best_price: Option<Decimal>,
    pub worst_price: Option<Decimal>,
    pub eval_count: i32,
    pub closed_at: Option<DateTime<Utc>>,
    pub close_price: Option<Decimal>,
    pub actual_return_pct: Option<Decimal>,
    pub created_at: DateTime<Utc>,
    // V6 新增字段
    pub instance_id: Option<Uuid>,
    pub signal_strength: Option<Decimal>,
    pub market_context: Option<serde_json::Value>,
    pub stop_loss: Option<Decimal>,
    pub take_profit: Option<Decimal>,
}

#[derive(Debug, Deserialize)]
pub struct CreateSignalRequest {
    pub strategy_id: String,
    pub symbol: String,
    pub direction: String,           // bullish/bearish/neutral
    pub entry_price: Decimal,
    pub overall_confidence: Decimal,
    pub entry_allowed: bool,
    pub entry_direction: Option<String>,
    pub timeframe_details: Option<serde_json::Value>,
    // V6 新增字段
    pub instance_id: Option<Uuid>,
    pub signal_strength: Option<Decimal>,
    pub market_context: Option<serde_json::Value>,
    pub stop_loss: Option<Decimal>,
    pub take_profit: Option<Decimal>,
}

#[derive(Debug, Deserialize)]
pub struct SignalQuery {
    pub strategy_id: Option<String>,
    pub instance_id: Option<Uuid>,
    pub symbol: Option<String>,
    pub start: Option<DateTime<Utc>>,
    pub end: Option<DateTime<Utc>>,
    pub status: Option<String>,
    pub limit: Option<i64>,
}

/// 创建信号
pub async fn create_signal(
    pool: &PgPool,
    req: CreateSignalRequest,
) -> Result<StrategySignal, sqlx::Error> {
    let signal = sqlx::query_as::<_, StrategySignal>(
        r#"
        INSERT INTO strategy_signals (
            strategy_id, symbol, direction, entry_price,
            overall_confidence, entry_allowed, entry_direction,
            timeframe_details, instance_id, signal_strength,
            market_context, stop_loss, take_profit
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)
        RETURNING *
        "#
    )
    .bind(&req.strategy_id)
    .bind(&req.symbol)
    .bind(&req.direction)
    .bind(req.entry_price)
    .bind(req.overall_confidence)
    .bind(req.entry_allowed)
    .bind(&req.entry_direction)
    .bind(req.timeframe_details.unwrap_or(serde_json::json!({})))
    .bind(req.instance_id)
    .bind(req.signal_strength)
    .bind(&req.market_context)
    .bind(req.stop_loss)
    .bind(req.take_profit)
    .fetch_one(pool)
    .await?;

    Ok(signal)
}

/// 查询信号
pub async fn query_signals(
    pool: &PgPool,
    query: SignalQuery,
) -> Result<Vec<StrategySignal>, sqlx::Error> {
    let limit = query.limit.unwrap_or(100);

    let signals = sqlx::query_as::<_, StrategySignal>(
        r#"
        SELECT *
        FROM strategy_signals
        WHERE ($1::text IS NULL OR strategy_id = $1)
          AND ($2::uuid IS NULL OR instance_id = $2)
          AND ($3::text IS NULL OR symbol = $3)
          AND ($4::timestamptz IS NULL OR created_at >= $4)
          AND ($5::timestamptz IS NULL OR created_at <= $5)
          AND ($6::text IS NULL OR status = $6)
        ORDER BY created_at DESC
        LIMIT $7
        "#
    )
    .bind(query.strategy_id)
    .bind(query.instance_id)
    .bind(query.symbol)
    .bind(query.start)
    .bind(query.end)
    .bind(query.status)
    .bind(limit)
    .fetch_all(pool)
    .await?;

    Ok(signals)
}

/// 获取策略实例的信号
pub async fn get_signals_by_instance(
    pool: &PgPool,
    instance_id: Uuid,
    limit: Option<i64>,
) -> Result<Vec<StrategySignal>, sqlx::Error> {
    let limit = limit.unwrap_or(100);

    let signals = sqlx::query_as::<_, StrategySignal>(
        "SELECT * FROM strategy_signals WHERE instance_id = $1 ORDER BY created_at DESC LIMIT $2"
    )
    .bind(instance_id)
    .bind(limit)
    .fetch_all(pool)
    .await?;

    Ok(signals)
}

/// 更新信号状态
pub async fn update_signal_status(
    pool: &PgPool,
    id: Uuid,
    status: &str,
) -> Result<Option<StrategySignal>, sqlx::Error> {
    let signal = sqlx::query_as::<_, StrategySignal>(
        "UPDATE strategy_signals SET status = $2 WHERE id = $1 RETURNING *"
    )
    .bind(id)
    .bind(status)
    .fetch_optional(pool)
    .await?;

    Ok(signal)
}

/// 标记信号已执行
pub async fn mark_signal_executed(
    pool: &PgPool,
    id: Uuid,
    order_id: &str,
) -> Result<Option<StrategySignal>, sqlx::Error> {
    let signal = sqlx::query_as::<_, StrategySignal>(
        "UPDATE strategy_signals SET executed = true, order_id = $2, status = 'confirmed' WHERE id = $1 RETURNING *"
    )
    .bind(id)
    .bind(order_id)
    .fetch_optional(pool)
    .await?;

    Ok(signal)
}
