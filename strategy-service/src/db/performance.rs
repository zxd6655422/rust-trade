use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Debug, Serialize, Deserialize, sqlx::FromRow)]
pub struct StrategyPerformance {
    pub id: Uuid,
    pub instance_id: Uuid,
    pub period_start: DateTime<Utc>,
    pub period_end: DateTime<Utc>,
    pub total_signals: i32,
    pub buy_signals: i32,
    pub sell_signals: i32,
    pub total_trades: i32,
    pub winning_trades: i32,
    pub losing_trades: i32,
    pub total_pnl: Decimal,
    pub win_rate: Option<Decimal>,
    pub avg_win: Option<Decimal>,
    pub avg_loss: Option<Decimal>,
    pub profit_factor: Option<Decimal>,
    pub max_drawdown: Option<Decimal>,
    pub updated_at: DateTime<Utc>,
}

/// 获取策略实例的性能统计
pub async fn get_performance(
    pool: &PgPool,
    instance_id: Uuid,
    start: Option<DateTime<Utc>>,
    end: Option<DateTime<Utc>>,
) -> Result<Vec<StrategyPerformance>, sqlx::Error> {
    let performances = sqlx::query_as::<_, StrategyPerformance>(
        r#"
        SELECT *
        FROM strategy_performance
        WHERE instance_id = $1
          AND ($2::timestamptz IS NULL OR period_start >= $2)
          AND ($3::timestamptz IS NULL OR period_end <= $3)
        ORDER BY period_start DESC
        "#
    )
    .bind(instance_id)
    .bind(start)
    .bind(end)
    .fetch_all(pool)
    .await?;

    Ok(performances)
}

/// 获取策略实例的汇总统计
pub async fn get_summary(
    pool: &PgPool,
    instance_id: Uuid,
) -> Result<Option<serde_json::Value>, sqlx::Error> {
    let summary = sqlx::query_as::<_, (i64, i64, i64, Decimal, Option<Decimal>)>(
        r#"
        SELECT
            COUNT(*) as total_trades,
            COUNT(*) FILTER (WHERE realized_pnl > 0) as winning_trades,
            COUNT(*) FILTER (WHERE realized_pnl <= 0) as losing_trades,
            COALESCE(SUM(realized_pnl), 0) as total_pnl,
            CASE
                WHEN COUNT(*) > 0
                THEN ROUND(COUNT(*) FILTER (WHERE realized_pnl > 0)::decimal / COUNT(*)::decimal, 4)
                ELSE NULL
            END as win_rate
        FROM trades t
        JOIN strategy_signals s ON t.signal_id = s.id
        WHERE s.instance_id = $1
        "#
    )
    .bind(instance_id)
    .fetch_optional(pool)
    .await?;

    match summary {
        Some((total_trades, winning_trades, losing_trades, total_pnl, win_rate)) => {
            let result = serde_json::json!({
                "instance_id": instance_id,
                "total_trades": total_trades,
                "winning_trades": winning_trades,
                "losing_trades": losing_trades,
                "total_pnl": total_pnl,
                "win_rate": win_rate
            });
            Ok(Some(result))
        }
        None => Ok(None),
    }
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

/// 计算并更新策略性能统计
pub async fn update_performance(
    pool: &PgPool,
    instance_id: Uuid,
    period_start: DateTime<Utc>,
    period_end: DateTime<Utc>,
) -> Result<StrategyPerformance, sqlx::Error> {
    // 校验 instance_id 是否存在
    if !validate_instance_exists(pool, instance_id).await? {
        return Err(sqlx::Error::RowNotFound);
    }

    // 计算信号统计
    let signal_stats = sqlx::query_as::<_, (i64, i64, i64)>(
        r#"
        SELECT
            COUNT(*) as total,
            COUNT(*) FILTER (WHERE signal_type = 'BUY') as buy_count,
            COUNT(*) FILTER (WHERE signal_type = 'SELL') as sell_count
        FROM strategy_signals
        WHERE instance_id = $1
          AND signal_time BETWEEN $2 AND $3
        "#
    )
    .bind(instance_id)
    .bind(period_start)
    .bind(period_end)
    .fetch_one(pool)
    .await?;

    // 计算交易统计
    let trade_stats = sqlx::query_as::<_, (i64, i64, i64, Decimal)>(
        r#"
        SELECT
            COUNT(*) as total,
            COUNT(*) FILTER (WHERE realized_pnl > 0) as winning,
            COUNT(*) FILTER (WHERE realized_pnl <= 0) as losing,
            COALESCE(SUM(realized_pnl), 0) as total_pnl
        FROM trades t
        JOIN strategy_signals s ON t.signal_id = s.id
        WHERE s.instance_id = $1
          AND t.trade_time BETWEEN $2 AND $3
        "#
    )
    .bind(instance_id)
    .bind(period_start)
    .bind(period_end)
    .fetch_one(pool)
    .await?;

    let total_signals = signal_stats.0 as i32;
    let buy_signals = signal_stats.1 as i32;
    let sell_signals = signal_stats.2 as i32;
    let total_trades = trade_stats.0 as i32;
    let winning_trades = trade_stats.1 as i32;
    let losing_trades = trade_stats.2 as i32;
    let total_pnl = trade_stats.3;

    let win_rate = if total_trades > 0 {
        Some(Decimal::from(winning_trades) / Decimal::from(total_trades))
    } else {
        None
    };

    // 插入或更新性能统计
    let performance = sqlx::query_as::<_, StrategyPerformance>(
        r#"
        INSERT INTO strategy_performance (
            instance_id, period_start, period_end,
            total_signals, buy_signals, sell_signals,
            total_trades, winning_trades, losing_trades,
            total_pnl, win_rate
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
        ON CONFLICT (instance_id, period_start, period_end)
        DO UPDATE SET
            total_signals = EXCLUDED.total_signals,
            buy_signals = EXCLUDED.buy_signals,
            sell_signals = EXCLUDED.sell_signals,
            total_trades = EXCLUDED.total_trades,
            winning_trades = EXCLUDED.winning_trades,
            losing_trades = EXCLUDED.losing_trades,
            total_pnl = EXCLUDED.total_pnl,
            win_rate = EXCLUDED.win_rate,
            updated_at = NOW()
        RETURNING *
        "#
    )
    .bind(instance_id)
    .bind(period_start)
    .bind(period_end)
    .bind(total_signals)
    .bind(buy_signals)
    .bind(sell_signals)
    .bind(total_trades)
    .bind(winning_trades)
    .bind(losing_trades)
    .bind(total_pnl)
    .bind(win_rate)
    .fetch_one(pool)
    .await?;

    Ok(performance)
}
