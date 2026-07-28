use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use sqlx::PgPool;
use uuid::Uuid;

/// 持仓信息（从 trading_positions 表查询）
#[derive(Debug, Clone)]
pub struct PositionInfo {
    pub id: Uuid,
    pub exchange: String,
    pub market_type: String,
    pub symbol: String,
    pub side: String,           // "LONG" / "SHORT"
    pub quantity: Decimal,
    pub avg_entry_price: Decimal,
    pub unrealized_pnl: Decimal,
    pub stop_loss_price: Option<Decimal>,
    pub take_profit_price: Option<Decimal>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

/// 查询指定交易对的活跃持仓
///
/// 返回 quantity > 0 的持仓，如果不存在返回 None
pub async fn get_active_position(
    pool: &PgPool,
    exchange: &str,
    market_type: &str,
    symbol: &str,
) -> Result<Option<PositionInfo>, sqlx::Error> {
    let row = sqlx::query_as::<_, PositionRow>(
        r#"
        SELECT id, exchange, market_type, symbol, side, quantity,
               avg_entry_price, unrealized_pnl, stop_loss_price,
               take_profit_price, created_at, updated_at
        FROM trading_positions
        WHERE exchange = $1 AND market_type = $2 AND symbol = $3 AND quantity > 0
        LIMIT 1
        "#
    )
    .bind(exchange)
    .bind(market_type)
    .bind(symbol)
    .fetch_optional(pool)
    .await?;

    Ok(row.map(|r| PositionInfo {
        id: r.id,
        exchange: r.exchange,
        market_type: r.market_type,
        symbol: r.symbol,
        side: r.side,
        quantity: r.quantity,
        avg_entry_price: r.avg_entry_price,
        unrealized_pnl: r.unrealized_pnl,
        stop_loss_price: r.stop_loss_price,
        take_profit_price: r.take_profit_price,
        created_at: r.created_at,
        updated_at: r.updated_at,
    }))
}

/// 查询所有活跃持仓
pub async fn get_all_active_positions(
    pool: &PgPool,
    exchange: &str,
    market_type: &str,
) -> Result<Vec<PositionInfo>, sqlx::Error> {
    let rows = sqlx::query_as::<_, PositionRow>(
        r#"
        SELECT id, exchange, market_type, symbol, side, quantity,
               avg_entry_price, unrealized_pnl, stop_loss_price,
               take_profit_price, created_at, updated_at
        FROM trading_positions
        WHERE exchange = $1 AND market_type = $2 AND quantity > 0
        ORDER BY updated_at DESC
        "#
    )
    .bind(exchange)
    .bind(market_type)
    .fetch_all(pool)
    .await?;

    Ok(rows.into_iter().map(|r| PositionInfo {
        id: r.id,
        exchange: r.exchange,
        market_type: r.market_type,
        symbol: r.symbol,
        side: r.side,
        quantity: r.quantity,
        avg_entry_price: r.avg_entry_price,
        unrealized_pnl: r.unrealized_pnl,
        stop_loss_price: r.stop_loss_price,
        take_profit_price: r.take_profit_price,
        created_at: r.created_at,
        updated_at: r.updated_at,
    }).collect())
}

/// DB 行映射结构
#[derive(sqlx::FromRow)]
struct PositionRow {
    pub id: Uuid,
    pub exchange: String,
    pub market_type: String,
    pub symbol: String,
    pub side: String,
    pub quantity: Decimal,
    pub avg_entry_price: Decimal,
    pub unrealized_pnl: Decimal,
    pub stop_loss_price: Option<Decimal>,
    pub take_profit_price: Option<Decimal>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}
