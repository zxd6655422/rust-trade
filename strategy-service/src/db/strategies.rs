use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Debug, Serialize, Deserialize, sqlx::FromRow)]
pub struct StrategyInstance {
    pub id: Uuid,
    pub strategy_type: String,
    pub display_name: String,
    pub params: serde_json::Value,
    pub status: String,
    pub symbols: Vec<String>,
    pub auto_trade: bool,
    pub position_size_pct: Decimal,
    pub exchange: String,
    pub market_type: String,
    pub note: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Deserialize)]
pub struct CreateStrategyRequest {
    pub strategy_type: String,
    pub display_name: String,
    pub params: serde_json::Value,
    pub symbols: Vec<String>,
    pub auto_trade: Option<bool>,
    pub position_size_pct: Option<Decimal>,
    pub exchange: Option<String>,
    pub market_type: Option<String>,
    pub note: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateStrategyRequest {
    pub display_name: Option<String>,
    pub params: Option<serde_json::Value>,
    pub symbols: Option<Vec<String>>,
    pub auto_trade: Option<bool>,
    pub position_size_pct: Option<Decimal>,
    pub note: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateStatusRequest {
    pub status: String,
}

/// 获取所有策略实例
pub async fn list_strategies(pool: &PgPool) -> Result<Vec<StrategyInstance>, sqlx::Error> {
    let strategies = sqlx::query_as::<_, StrategyInstance>(
        "SELECT * FROM strategy_instances ORDER BY created_at DESC"
    )
    .fetch_all(pool)
    .await?;

    Ok(strategies)
}

/// 获取活跃策略实例
pub async fn list_active_strategies(pool: &PgPool) -> Result<Vec<StrategyInstance>, sqlx::Error> {
    let strategies = sqlx::query_as::<_, StrategyInstance>(
        "SELECT * FROM strategy_instances WHERE status = 'active' ORDER BY created_at DESC"
    )
    .fetch_all(pool)
    .await?;

    Ok(strategies)
}

/// 根据 ID 获取策略实例
pub async fn get_strategy(pool: &PgPool, id: Uuid) -> Result<Option<StrategyInstance>, sqlx::Error> {
    let strategy = sqlx::query_as::<_, StrategyInstance>(
        "SELECT * FROM strategy_instances WHERE id = $1"
    )
    .bind(id)
    .fetch_optional(pool)
    .await?;

    Ok(strategy)
}

/// 创建策略实例
pub async fn create_strategy(
    pool: &PgPool,
    req: CreateStrategyRequest,
) -> Result<StrategyInstance, sqlx::Error> {
    let strategy = sqlx::query_as::<_, StrategyInstance>(
        r#"
        INSERT INTO strategy_instances (
            strategy_type, display_name, params, symbols,
            auto_trade, position_size_pct, exchange, market_type, note
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
        RETURNING *
        "#
    )
    .bind(&req.strategy_type)
    .bind(&req.display_name)
    .bind(&req.params)
    .bind(&req.symbols)
    .bind(req.auto_trade.unwrap_or(false))
    .bind(req.position_size_pct.unwrap_or(Decimal::from(10)))
    .bind(req.exchange.as_deref().unwrap_or("binance"))
    .bind(req.market_type.as_deref().unwrap_or("futures"))
    .bind(&req.note)
    .fetch_one(pool)
    .await?;

    Ok(strategy)
}

/// 更新策略实例
pub async fn update_strategy(
    pool: &PgPool,
    id: Uuid,
    req: UpdateStrategyRequest,
) -> Result<Option<StrategyInstance>, sqlx::Error> {
    let strategy = sqlx::query_as::<_, StrategyInstance>(
        r#"
        UPDATE strategy_instances
        SET
            display_name = COALESCE($2, display_name),
            params = COALESCE($3, params),
            symbols = COALESCE($4, symbols),
            auto_trade = COALESCE($5, auto_trade),
            position_size_pct = COALESCE($6, position_size_pct),
            note = COALESCE($7, note),
            updated_at = NOW()
        WHERE id = $1
        RETURNING *
        "#
    )
    .bind(id)
    .bind(&req.display_name)
    .bind(&req.params)
    .bind(&req.symbols)
    .bind(req.auto_trade)
    .bind(req.position_size_pct)
    .bind(&req.note)
    .fetch_optional(pool)
    .await?;

    Ok(strategy)
}

/// 更新策略状态
pub async fn update_strategy_status(
    pool: &PgPool,
    id: Uuid,
    status: &str,
) -> Result<Option<StrategyInstance>, sqlx::Error> {
    let strategy = sqlx::query_as::<_, StrategyInstance>(
        r#"
        UPDATE strategy_instances
        SET status = $2, updated_at = NOW()
        WHERE id = $1
        RETURNING *
        "#
    )
    .bind(id)
    .bind(status)
    .fetch_optional(pool)
    .await?;

    Ok(strategy)
}

/// 删除策略实例
pub async fn delete_strategy(pool: &PgPool, id: Uuid) -> Result<bool, sqlx::Error> {
    let result = sqlx::query("DELETE FROM strategy_instances WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await?;

    Ok(result.rows_affected() > 0)
}
