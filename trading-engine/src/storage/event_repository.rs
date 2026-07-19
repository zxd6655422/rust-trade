// storage/event_repository.rs
// 事件仓储层 - 将交易事件持久化到 trade_logs / risk_logs 表
//
// 职责：
// - OrderFilled / StopTriggered → trade_logs
// - RiskCheck / RiskAction → risk_logs
// - 提供查询接口供 REST API 使用

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use sqlx::PgPool;
use uuid::Uuid;

use trading_common::data::event_types::TradingEvent;

/// 成交日志记录
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct TradeLogRecord {
    pub id: Uuid,
    pub timestamp: DateTime<Utc>,
    pub strategy_id: Option<String>,
    pub symbol: String,
    pub side: String,
    pub quantity: Decimal,
    pub price: Decimal,
    pub order_id: Option<String>,
    pub pnl: Option<Decimal>,
    pub notes: Option<String>,
    pub signal_id: Option<Uuid>,
    pub exchange: Option<String>,
    pub market_type: Option<String>,
    pub event_type: Option<String>,
    pub commission: Option<Decimal>,
    pub slippage: Option<Decimal>,
    pub details: Option<serde_json::Value>,
}

/// 风控日志记录
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct RiskLogRecord {
    pub id: Uuid,
    pub timestamp: DateTime<Utc>,
    pub event_type: String,
    pub symbol: Option<String>,
    pub details: Option<serde_json::Value>,
    pub decision: String,
    pub signal_id: Option<Uuid>,
    pub exchange: Option<String>,
    pub market_type: Option<String>,
    pub check_result: Option<String>,
    pub current_equity: Option<Decimal>,
    pub peak_equity: Option<Decimal>,
    pub daily_pnl: Option<Decimal>,
}

/// 事件仓储
pub struct EventRepository {
    pool: PgPool,
}

impl EventRepository {
    /// 创建新的事件仓储
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// 统一写入入口（根据事件类型分发）
    pub async fn log_event(&self, event: &TradingEvent) -> Result<(), sqlx::Error> {
        match event {
            TradingEvent::OrderFilled { .. } | TradingEvent::StopTriggered { .. } => {
                self.log_trade(event).await
            }
            TradingEvent::RiskCheck { .. } | TradingEvent::RiskAction { .. } => {
                self.log_risk_event(event).await
            }
            _ => Ok(()), // StrategyAnalyzed / OrderPlaced 不写 trade_logs/risk_logs
        }
    }

    /// 写入成交日志
    async fn log_trade(&self, event: &TradingEvent) -> Result<(), sqlx::Error> {
        match event {
            TradingEvent::OrderFilled {
                signal_id,
                order_id,
                exchange,
                market_type,
                symbol,
                side,
                quantity,
                avg_price,
                commission,
                slippage,
                pnl,
                event_type,
                timestamp,
            } => {
                let source = if exchange == "paper" { "paper" } else { "live" };
                sqlx::query(
                    r#"
                    INSERT INTO trade_logs (
                        timestamp, symbol, side, quantity, price, order_id, pnl,
                        signal_id, exchange, market_type, event_type, commission, slippage, details, source
                    ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15)
                    "#,
                )
                .bind(timestamp)
                .bind(symbol)
                .bind(side)
                .bind(quantity)
                .bind(avg_price)
                .bind(order_id)
                .bind(pnl)
                .bind(signal_id)
                .bind(exchange)
                .bind(market_type)
                .bind(event_type)
                .bind(commission)
                .bind(slippage)
                .bind(serde_json::json!({
                    "exchange": exchange,
                    "market_type": market_type,
                }))
                .bind(source)
                .execute(&self.pool)
                .await?;
            }
            TradingEvent::StopTriggered {
                signal_id,
                order_id,
                exchange,
                market_type,
                symbol,
                trigger_type,
                trigger_price,
                close_price,
                quantity,
                pnl,
                timestamp,
            } => {
                let side = if *pnl >= Decimal::ZERO { "SELL" } else { "BUY" };
                let event_type = match trigger_type.as_str() {
                    "stop_loss" => "stop_loss",
                    "take_profit" => "take_profit",
                    _ => "stop_triggered",
                };
                let source = if exchange == "paper" { "paper" } else { "live" };
                sqlx::query(
                    r#"
                    INSERT INTO trade_logs (
                        timestamp, symbol, side, quantity, price, order_id, pnl,
                        signal_id, exchange, market_type, event_type, details, source
                    ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)
                    "#,
                )
                .bind(timestamp)
                .bind(symbol)
                .bind(side)
                .bind(quantity)
                .bind(close_price)
                .bind(order_id)
                .bind(pnl)
                .bind(signal_id)
                .bind(exchange)
                .bind(market_type)
                .bind(event_type)
                .bind(serde_json::json!({
                    "trigger_type": trigger_type,
                    "trigger_price": trigger_price,
                    "close_price": close_price,
                }))
                .bind(source)
                .execute(&self.pool)
                .await?;
            }
            _ => {}
        }
        Ok(())
    }

    /// 写入风控日志
    async fn log_risk_event(&self, event: &TradingEvent) -> Result<(), sqlx::Error> {
        match event {
            TradingEvent::RiskCheck {
                signal_id,
                exchange,
                market_type,
                symbol,
                check_type,
                result,
                reason,
                current_equity,
                peak_equity,
                daily_pnl,
                details,
                timestamp,
            } => {
                let source = if exchange == "paper" { "paper" } else { "live" };
                sqlx::query(
                    r#"
                    INSERT INTO risk_logs (
                        timestamp, event_type, symbol, decision, details,
                        signal_id, exchange, market_type, check_result,
                        current_equity, peak_equity, daily_pnl, source
                    ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)
                    "#,
                )
                .bind(timestamp)
                .bind(check_type)
                .bind(symbol)
                .bind(result)
                .bind(details)
                .bind(signal_id)
                .bind(exchange)
                .bind(market_type)
                .bind(result)
                .bind(current_equity)
                .bind(peak_equity)
                .bind(daily_pnl)
                .bind(source)
                .execute(&self.pool)
                .await?;
            }
            TradingEvent::RiskAction {
                signal_id,
                exchange,
                market_type,
                action_type,
                symbol,
                reason,
                details,
                timestamp,
            } => {
                let source = if exchange == "paper" { "paper" } else { "live" };
                sqlx::query(
                    r#"
                    INSERT INTO risk_logs (
                        timestamp, event_type, symbol, decision, details,
                        signal_id, exchange, market_type, check_result, source
                    ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
                    "#,
                )
                .bind(timestamp)
                .bind(action_type)
                .bind(symbol)
                .bind("action_triggered")
                .bind(details)
                .bind(signal_id)
                .bind(exchange)
                .bind(market_type)
                .bind("action_triggered")
                .bind(source)
                .execute(&self.pool)
                .await?;
            }
            _ => {}
        }
        Ok(())
    }

    /// 查询成交日志
    pub async fn get_trade_logs(
        &self,
        symbol: Option<&str>,
        signal_id: Option<Uuid>,
        event_type: Option<&str>,
        limit: i64,
    ) -> Result<Vec<TradeLogRecord>, sqlx::Error> {
        let mut query = String::from(
            "SELECT id, timestamp, strategy_id, symbol, side, quantity, price, order_id, pnl, notes,
                    signal_id, exchange, market_type, event_type, commission, slippage, details
             FROM trade_logs WHERE 1=1"
        );
        let mut bind_idx = 1;
        let mut binds: Vec<String> = Vec::new();

        if symbol.is_some() {
            query.push_str(&format!(" AND symbol = ${}", bind_idx));
            binds.push(symbol.unwrap().to_string());
            bind_idx += 1;
        }
        if signal_id.is_some() {
            query.push_str(&format!(" AND signal_id = ${}", bind_idx));
            bind_idx += 1;
        }
        if event_type.is_some() {
            query.push_str(&format!(" AND event_type = ${}", bind_idx));
            binds.push(event_type.unwrap().to_string());
            bind_idx += 1;
        }

        query.push_str(&format!(" ORDER BY timestamp DESC LIMIT ${}", bind_idx));

        // 使用简单查询方式（避免动态绑定的复杂性）
        let records = sqlx::query_as::<_, TradeLogRecord>(&query)
            .bind(symbol)
            .bind(signal_id)
            .bind(event_type)
            .bind(limit)
            .fetch_all(&self.pool)
            .await?;

        Ok(records)
    }

    /// 查询风控日志
    pub async fn get_risk_logs(
        &self,
        event_type: Option<&str>,
        symbol: Option<&str>,
        limit: i64,
    ) -> Result<Vec<RiskLogRecord>, sqlx::Error> {
        let records = sqlx::query_as::<_, RiskLogRecord>(
            r#"
            SELECT id, timestamp, event_type, symbol, details, decision,
                   signal_id, exchange, market_type, check_result,
                   current_equity, peak_equity, daily_pnl
            FROM risk_logs
            WHERE ($1::text IS NULL OR event_type = $1)
              AND ($2::text IS NULL OR symbol = $2)
            ORDER BY timestamp DESC
            LIMIT $3
            "#,
        )
        .bind(event_type)
        .bind(symbol)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;

        Ok(records)
    }

    /// 根据 signal_id 查询相关日志（用于时间线）
    pub async fn get_logs_by_signal_id(
        &self,
        signal_id: Uuid,
    ) -> Result<(Vec<TradeLogRecord>, Vec<RiskLogRecord>), sqlx::Error> {
        let trades = sqlx::query_as::<_, TradeLogRecord>(
            r#"
            SELECT id, timestamp, strategy_id, symbol, side, quantity, price, order_id, pnl, notes,
                    signal_id, exchange, market_type, event_type, commission, slippage, details
            FROM trade_logs
            WHERE signal_id = $1
            ORDER BY timestamp ASC
            "#,
        )
        .bind(signal_id)
        .fetch_all(&self.pool)
        .await?;

        let risks = sqlx::query_as::<_, RiskLogRecord>(
            r#"
            SELECT id, timestamp, event_type, symbol, details, decision,
                   signal_id, exchange, market_type, check_result,
                   current_equity, peak_equity, daily_pnl
            FROM risk_logs
            WHERE signal_id = $1
            ORDER BY timestamp ASC
            "#,
        )
        .bind(signal_id)
        .fetch_all(&self.pool)
        .await?;

        Ok((trades, risks))
    }
}
