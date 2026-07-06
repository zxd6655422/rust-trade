use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Debug, Serialize, Deserialize, sqlx::FromRow)]
pub struct StrategySignal {
    pub id: Uuid,
    pub strategy_id: String,
    pub symbol: String,
    pub signal_time: DateTime<Utc>,
    pub signal_type: String,
    pub signal_price: Decimal,
    pub signal_quantity: Option<Decimal>,
    pub confidence: Option<Decimal>,
    pub trend_direction: Option<String>,
    pub timeframe_analysis: Option<serde_json::Value>,
    pub created_at: DateTime<Utc>,
    // V6 新增字段
    pub instance_id: Option<Uuid>,
    pub signal_strength: Option<Decimal>,
    pub market_context: Option<serde_json::Value>,
    pub entry_price: Option<Decimal>,
    pub stop_loss: Option<Decimal>,
    pub take_profit: Option<Decimal>,
    pub exchange: Option<String>,
    pub market_type: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct CreateSignalRequest {
    pub strategy_id: String,
    pub symbol: String,
    pub signal_time: DateTime<Utc>,
    pub signal_type: String,
    pub signal_price: Decimal,
    pub signal_quantity: Option<Decimal>,
    pub confidence: Option<Decimal>,
    pub trend_direction: Option<String>,
    pub timeframe_analysis: Option<serde_json::Value>,
    pub instance_id: Option<Uuid>,
    pub signal_strength: Option<Decimal>,
    pub market_context: Option<serde_json::Value>,
    pub entry_price: Option<Decimal>,
    pub stop_loss: Option<Decimal>,
    pub take_profit: Option<Decimal>,
    pub exchange: Option<String>,
    pub market_type: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct SignalQuery {
    pub strategy_id: Option<String>,
    pub instance_id: Option<Uuid>,
    pub symbol: Option<String>,
    pub start: Option<DateTime<Utc>>,
    pub end: Option<DateTime<Utc>>,
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
            strategy_id, symbol, signal_time, signal_type, signal_price,
            signal_quantity, confidence, trend_direction, timeframe_analysis,
            instance_id, signal_strength, market_context, entry_price,
            stop_loss, take_profit, exchange, market_type
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17)
        RETURNING *
        "#
    )
    .bind(&req.strategy_id)
    .bind(&req.symbol)
    .bind(req.signal_time)
    .bind(&req.signal_type)
    .bind(req.signal_price)
    .bind(req.signal_quantity)
    .bind(req.confidence)
    .bind(&req.trend_direction)
    .bind(&req.timeframe_analysis)
    .bind(req.instance_id)
    .bind(req.signal_strength)
    .bind(&req.market_context)
    .bind(req.entry_price)
    .bind(req.stop_loss)
    .bind(req.take_profit)
    .bind(req.exchange.as_deref().unwrap_or("binance"))
    .bind(req.market_type.as_deref().unwrap_or("futures"))
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
          AND ($4::timestamptz IS NULL OR signal_time >= $4)
          AND ($5::timestamptz IS NULL OR signal_time <= $5)
        ORDER BY signal_time DESC
        LIMIT $6
        "#
    )
    .bind(query.strategy_id)
    .bind(query.instance_id)
    .bind(query.symbol)
    .bind(query.start)
    .bind(query.end)
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
        "SELECT * FROM strategy_signals WHERE instance_id = $1 ORDER BY signal_time DESC LIMIT $2"
    )
    .bind(instance_id)
    .bind(limit)
    .fetch_all(pool)
    .await?;

    Ok(signals)
}
