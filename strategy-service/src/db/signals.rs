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
    // V7 新增字段：策略分析详情
    pub market_structure: Option<serde_json::Value>,
    pub key_levels: Option<serde_json::Value>,
    pub trade_setup: Option<serde_json::Value>,
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
    // V7 新增字段：策略分析详情
    pub market_structure: Option<serde_json::Value>,
    pub key_levels: Option<serde_json::Value>,
    pub trade_setup: Option<serde_json::Value>,
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

/// 校验策略实例是否存在
async fn validate_instance_exists(pool: &PgPool, instance_id: Uuid) -> Result<bool, sqlx::Error> {
    let exists = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS(SELECT 1 FROM strategy_instances WHERE id = $1)"
    )
    .bind(instance_id)
    .fetch_one(pool)
    .await?;
    Ok(exists)
}

/// 创建信号
pub async fn create_signal(
    pool: &PgPool,
    req: CreateSignalRequest,
) -> Result<StrategySignal, sqlx::Error> {
    // 校验 instance_id 是否存在（如果提供了的话）
    if let Some(instance_id) = req.instance_id {
        if !validate_instance_exists(pool, instance_id).await? {
            return Err(sqlx::Error::RowNotFound);
        }
    }

    let signal = sqlx::query_as::<_, StrategySignal>(
        r#"
        INSERT INTO strategy_signals (
            strategy_id, symbol, direction, entry_price,
            overall_confidence, entry_allowed, entry_direction,
            timeframe_details, instance_id, signal_strength,
            market_context, stop_loss, take_profit,
            market_structure, key_levels, trade_setup
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16)
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
    .bind(&req.market_structure)
    .bind(&req.key_levels)
    .bind(&req.trade_setup)
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

/// 获取指定实例和交易对的活跃信号（status='pending' 或 'executed'）
///
/// 用于方向反转检测，查询当前仍然活跃的信号
pub async fn get_active_signals(
    pool: &PgPool,
    instance_id: Uuid,
    symbol: &str,
) -> Result<Vec<StrategySignal>, sqlx::Error> {
    let signals = sqlx::query_as::<_, StrategySignal>(
        r#"
        SELECT * FROM strategy_signals
        WHERE instance_id = $1
          AND symbol = $2
          AND status IN ('pending', 'executed')
        ORDER BY created_at DESC
        LIMIT 10
        "#
    )
    .bind(instance_id)
    .bind(symbol)
    .fetch_all(pool)
    .await?;

    Ok(signals)
}

/// 获取指定实例和交易对的最近一个信号
///
/// 用于去重和方向判断
pub async fn get_last_signal(
    pool: &PgPool,
    instance_id: Uuid,
    symbol: &str,
) -> Result<Option<StrategySignal>, sqlx::Error> {
    let signal = sqlx::query_as::<_, StrategySignal>(
        r#"
        SELECT * FROM strategy_signals
        WHERE instance_id = $1 AND symbol = $2
        ORDER BY created_at DESC
        LIMIT 1
        "#
    )
    .bind(instance_id)
    .bind(symbol)
    .fetch_optional(pool)
    .await?;

    Ok(signal)
}

/// 将信号标记为 superseded（被取代）
///
/// 当新信号与旧信号方向相反时，关闭旧信号
pub async fn supersede_signal(
    pool: &PgPool,
    signal_id: Uuid,
    close_price: Decimal,
    actual_return_pct: Decimal,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"
        UPDATE strategy_signals
        SET status = 'superseded',
            closed_reason = 'direction_changed',
            closed_at = NOW(),
            close_price = $2,
            actual_return_pct = $3
        WHERE id = $1 AND status IN ('pending', 'executed')
        "#
    )
    .bind(signal_id)
    .bind(close_price)
    .bind(actual_return_pct)
    .execute(pool)
    .await?;

    Ok(())
}
