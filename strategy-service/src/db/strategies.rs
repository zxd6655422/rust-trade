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
    /// 是否为默认策略
    pub is_default: bool,
    /// 作为哪个场景的默认策略: dashboard/paper_trading/backtest
    pub default_for: Option<String>,
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
    /// 是否为默认策略
    pub is_default: Option<bool>,
    /// 作为哪个场景的默认策略: dashboard/paper_trading/backtest
    pub default_for: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct UpdateStrategyRequest {
    pub display_name: Option<String>,
    pub params: Option<serde_json::Value>,
    pub symbols: Option<Vec<String>>,
    pub auto_trade: Option<bool>,
    pub position_size_pct: Option<Decimal>,
    pub note: Option<String>,
    /// 是否为默认策略
    pub is_default: Option<bool>,
    /// 作为哪个场景的默认策略: dashboard/paper_trading/backtest
    pub default_for: Option<String>,
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
            auto_trade, position_size_pct, exchange, market_type, note,
            is_default, default_for
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
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
    .bind(req.is_default.unwrap_or(false))
    .bind(&req.default_for)
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
            is_default = COALESCE($8, is_default),
            default_for = COALESCE($9, default_for),
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
    .bind(req.is_default)
    .bind(&req.default_for)
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

/// 获取指定场景的默认策略
///
/// # Arguments
/// * `pool` - 数据库连接池
/// * `default_for` - 场景名称: dashboard/paper_trading/backtest
///
/// # Returns
/// 返回该场景的默认策略实例，如果没有配置则返回 None
pub async fn get_default_strategy(
    pool: &PgPool,
    default_for: &str,
) -> Result<Option<StrategyInstance>, sqlx::Error> {
    let strategy = sqlx::query_as::<_, StrategyInstance>(
        r#"
        SELECT * FROM strategy_instances
        WHERE is_default = true
          AND default_for = $1
          AND status = 'active'
        ORDER BY updated_at DESC
        LIMIT 1
        "#
    )
    .bind(default_for)
    .fetch_optional(pool)
    .await?;

    Ok(strategy)
}

/// 获取所有默认策略配置
///
/// 返回每个场景的默认策略，如果没有配置则不包含该场景
pub async fn get_all_default_strategies(
    pool: &PgPool,
) -> Result<Vec<StrategyInstance>, sqlx::Error> {
    let strategies = sqlx::query_as::<_, StrategyInstance>(
        r#"
        SELECT * FROM strategy_instances
        WHERE is_default = true
          AND default_for IS NOT NULL
          AND status = 'active'
        ORDER BY default_for, updated_at DESC
        "#
    )
    .fetch_all(pool)
    .await?;

    Ok(strategies)
}

/// 获取可用于选择的策略列表（活跃状态）
///
/// # Arguments
/// * `pool` - 数据库连接池
/// * `market_type` - 可选的市场类型过滤: spot/futures
///
/// # Returns
/// 返回所有活跃的策略实例
pub async fn list_selectable_strategies(
    pool: &PgPool,
    market_type: Option<&str>,
) -> Result<Vec<StrategyInstance>, sqlx::Error> {
    let strategies = if let Some(market_type) = market_type {
        sqlx::query_as::<_, StrategyInstance>(
            r#"
            SELECT * FROM strategy_instances
            WHERE status = 'active'
              AND market_type = $1
            ORDER BY display_name
            "#
        )
        .bind(market_type)
        .fetch_all(pool)
        .await?
    } else {
        sqlx::query_as::<_, StrategyInstance>(
            r#"
            SELECT * FROM strategy_instances
            WHERE status = 'active'
            ORDER BY display_name
            "#
        )
        .fetch_all(pool)
        .await?
    };

    Ok(strategies)
}

/// 设置默认策略
///
/// 将指定策略设置为某个场景的默认策略，同时取消该场景其他策略的默认状态
///
/// # Arguments
/// * `pool` - 数据库连接池
/// * `id` - 策略实例 ID
/// * `default_for` - 场景名称: dashboard/paper_trading/backtest
pub async fn set_default_strategy(
    pool: &PgPool,
    id: Uuid,
    default_for: &str,
) -> Result<Option<StrategyInstance>, sqlx::Error> {
    // 首先取消该场景的所有默认策略
    sqlx::query(
        r#"
        UPDATE strategy_instances
        SET is_default = false, updated_at = NOW()
        WHERE default_for = $1 AND is_default = true
        "#
    )
    .bind(default_for)
    .execute(pool)
    .await?;

    // 然后将指定策略设置为默认
    let strategy = sqlx::query_as::<_, StrategyInstance>(
        r#"
        UPDATE strategy_instances
        SET is_default = true, default_for = $2, updated_at = NOW()
        WHERE id = $1 AND status = 'active'
        RETURNING *
        "#
    )
    .bind(id)
    .bind(default_for)
    .fetch_optional(pool)
    .await?;

    Ok(strategy)
}

/// 取消默认策略设置
///
/// # Arguments
/// * `pool` - 数据库连接池
/// * `id` - 策略实例 ID
pub async fn unset_default_strategy(
    pool: &PgPool,
    id: Uuid,
) -> Result<Option<StrategyInstance>, sqlx::Error> {
    let strategy = sqlx::query_as::<_, StrategyInstance>(
        r#"
        UPDATE strategy_instances
        SET is_default = false, default_for = NULL, updated_at = NOW()
        WHERE id = $1
        RETURNING *
        "#
    )
    .bind(id)
    .fetch_optional(pool)
    .await?;

    Ok(strategy)
}
