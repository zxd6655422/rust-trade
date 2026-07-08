use chrono::{DateTime, Duration, Utc};
use rust_decimal::Decimal;
use sqlx::{PgPool, QueryBuilder, Row};
use std::collections::HashMap;
use std::str::FromStr;
use tracing::{debug, error, info, warn};

use crate::data::types::{LiveStrategyLog, OHLCData, Timeframe};

use super::cache::{TickDataCache, TieredCache};
use super::types::{
    BacktestDataInfo, DataError, DataResult, DbStats, SymbolDataInfo, TickData, TickQuery,
    TradeSide,
};

// =================================================================
// Constants and Configuration
// =================================================================

const DEFAULT_QUERY_LIMIT: u32 = 1000;
const MAX_QUERY_LIMIT: u32 = 10000;
const MAX_BATCH_SIZE: usize = 1000;

// =================================================================
// Repository Implementation
// =================================================================

/// TickData repository for database operations
pub struct TickDataRepository {
    pool: PgPool,
    cache: TieredCache,
}

/// 24h statistics for a symbol
pub struct Kline24hStats {
    pub change_pct: Option<Decimal>,
    pub volume_24h: Option<Decimal>,
    pub high_24h: Option<Decimal>,
    pub low_24h: Option<Decimal>,
}

impl TickDataRepository {
    /// Create new repository instance
    pub fn new(pool: PgPool, cache: TieredCache) -> Self {
        Self { pool, cache }
    }

    /// Get database pool reference
    pub fn get_pool(&self) -> &PgPool {
        &self.pool
    }

    /// Get cache reference
    pub fn get_cache(&self) -> &TieredCache {
        &self.cache
    }

    // =================================================================
    // Insert Operations
    // =================================================================

    /// Insert single tick data
    pub async fn insert_tick(&self, tick: &TickData) -> DataResult<()> {
        self.validate_tick_data(tick)?;

        debug!(
            "Inserting tick: symbol={}, price={}, trade_id={}",
            tick.symbol, tick.price, tick.trade_id
        );

        // Insert to database first
        sqlx::query(
            "INSERT INTO tick_data (timestamp, symbol, price, quantity, side, trade_id, is_buyer_maker) \
             VALUES ($1, $2, $3, $4, $5, $6, $7) ON CONFLICT (symbol, trade_id, timestamp) DO NOTHING"
        )
        .bind(tick.timestamp)
        .bind(&tick.symbol)
        .bind(tick.price)
        .bind(tick.quantity)
        .bind(tick.side.as_db_str())
        .bind(&tick.trade_id)
        .bind(tick.is_buyer_maker)
        .execute(&self.pool)
        .await
        .map_err(|e| {
            error!("Failed to insert tick data: {}", e);
            DataError::Database(e)
        })?;

        // Update cache
        if let Err(e) = self.cache.push_tick(tick).await {
            warn!("Failed to update cache after insert: {}", e);
            // Don't fail the operation if cache update fails
        }

        Ok(())
    }

    /// Batch insert tick data with optimized performance
    pub async fn batch_insert(&self, ticks: Vec<TickData>) -> DataResult<usize> {
        if ticks.is_empty() {
            return Ok(0);
        }

        // Validate all ticks
        for tick in &ticks {
            self.validate_tick_data(tick)?;
        }

        let total_count = ticks.len();

        // Process in chunks to avoid memory issues
        let mut total_inserted = 0;
        for chunk in ticks.chunks(MAX_BATCH_SIZE) {
            let inserted = self.batch_insert_chunk(chunk).await?;
            total_inserted += inserted;

            // Update cache for each chunk
            for tick in chunk {
                if let Err(e) = self.cache.push_tick(tick).await {
                    debug!("Failed to update cache for tick {}: {}", tick.trade_id, e);
                }
            }
        }

        // 静默，由调用方决定是否打日志
        debug!("[tick_data] Batch inserted {} records", total_inserted);
        Ok(total_inserted)
    }

    /// Insert a chunk of ticks using bulk insert
    async fn batch_insert_chunk(&self, ticks: &[TickData]) -> DataResult<usize> {
        if ticks.is_empty() {
            return Ok(0);
        }

        let mut query_builder = QueryBuilder::new(
            "INSERT INTO tick_data (timestamp, symbol, price, quantity, side, trade_id, is_buyer_maker) "
        );

        query_builder.push_values(ticks, |mut b, tick| {
            b.push_bind(tick.timestamp)
                .push_bind(&tick.symbol)
                .push_bind(tick.price)
                .push_bind(tick.quantity)
                .push_bind(tick.side.as_db_str())
                .push_bind(&tick.trade_id)
                .push_bind(tick.is_buyer_maker);
        });

        // Handle duplicates by ignoring them
        query_builder.push(" ON CONFLICT (symbol, trade_id, timestamp) DO NOTHING");

        let query = query_builder.build();
        let result = query.execute(&self.pool).await?;

        Ok(result.rows_affected() as usize)
    }

    // =================================================================
    // Query Operations
    // =================================================================

    /// Get tick data based on query parameters
    pub async fn get_ticks(&self, query: &TickQuery) -> DataResult<Vec<TickData>> {
        let limit = query
            .limit
            .unwrap_or(DEFAULT_QUERY_LIMIT)
            .min(MAX_QUERY_LIMIT);

        debug!("Querying ticks: symbol={}, limit={}", query.symbol, limit);

        // Try cache first for recent data
        if self.is_recent_query(query) {
            let cached_ticks = self
                .cache
                .get_recent_ticks(&query.symbol, limit as usize)
                .await?;
            if cached_ticks.len() == limit as usize {
                debug!(
                    "Cache hit: retrieved {} ticks from cache",
                    cached_ticks.len()
                );
                return Ok(cached_ticks);
            }
        }

        // Cache miss or not recent, query database
        let ticks = self.query_ticks_from_db(query).await?;

        // Update cache with fetched data
        for tick in &ticks {
            if let Err(e) = self.cache.push_tick(tick).await {
                warn!("Failed to cache tick {}: {}", tick.trade_id, e);
            }
        }

        debug!("Retrieved {} tick records from database", ticks.len());
        Ok(ticks)
    }

    /// Query ticks directly from database
    async fn query_ticks_from_db(&self, query: &TickQuery) -> DataResult<Vec<TickData>> {
        let limit = query
            .limit
            .unwrap_or(DEFAULT_QUERY_LIMIT)
            .min(MAX_QUERY_LIMIT);

        let mut sql_query = QueryBuilder::new(
            "SELECT timestamp, symbol, price, quantity, side, trade_id, is_buyer_maker FROM tick_data WHERE symbol = "
        );
        sql_query.push_bind(&query.symbol);

        // Add time range filter
        if let Some(start_time) = query.start_time {
            sql_query.push(" AND timestamp >= ").push_bind(start_time);
        }
        if let Some(end_time) = query.end_time {
            sql_query.push(" AND timestamp <= ").push_bind(end_time);
        }

        // Add side filter
        if let Some(side) = query.trade_side {
            sql_query.push(" AND side = ").push_bind(side.as_db_str());
        }

        sql_query
            .push(" ORDER BY timestamp DESC LIMIT ")
            .push_bind(limit as i64);

        let rows = sql_query.build().fetch_all(&self.pool).await?;

        let ticks: DataResult<Vec<TickData>> = rows
            .iter()
            .map(|row| {
                Ok(TickData {
                    timestamp: row.get("timestamp"),
                    symbol: row.get("symbol"),
                    price: row.get("price"),
                    quantity: row.get("quantity"),
                    side: self.parse_trade_side(row.get("side"))?,
                    trade_id: row.get("trade_id"),
                    is_buyer_maker: row.get("is_buyer_maker"),
                })
            })
            .collect();

        ticks
    }

    /// Get latest price for a symbol
    pub async fn get_latest_price(&self, symbol: &str) -> DataResult<Option<Decimal>> {
        debug!("Fetching latest price for symbol: {}", symbol);

        // Try cache first
        let cached_ticks = self.cache.get_recent_ticks(symbol, 1).await?;
        if let Some(latest_tick) = cached_ticks.first() {
            debug!("Latest price from cache: {}", latest_tick.price);
            return Ok(Some(latest_tick.price));
        }

        // Cache miss, query database
        let row = sqlx::query("SELECT price FROM tick_data WHERE symbol = $1 ORDER BY timestamp DESC LIMIT 1")
            .bind(symbol)
            .fetch_optional(&self.pool)
            .await?;

        let price = row.map(|r| r.get::<Decimal, _>("price"));
        debug!("Latest price from database: {:?}", price);
        Ok(price)
    }

    /// Get latest tick data for a symbol (with timestamp)
    pub async fn get_latest_tick(&self, symbol: &str) -> DataResult<Option<TickData>> {
        debug!("Fetching latest tick for symbol: {}", symbol);

        // Try cache first
        let cached_ticks = self.cache.get_recent_ticks(symbol, 1).await?;
        if let Some(latest_tick) = cached_ticks.first() {
            debug!("Latest tick from cache");
            return Ok(Some(latest_tick.clone()));
        }

        // Cache miss, query database
        let row = sqlx::query(
            "SELECT timestamp, symbol, price, quantity, side, trade_id, is_buyer_maker \
             FROM tick_data WHERE symbol = $1 ORDER BY timestamp DESC LIMIT 1"
        )
        .bind(symbol)
        .fetch_optional(&self.pool)
        .await?;

        match row {
            Some(r) => {
                let side_str: String = r.get("side");
                let tick = TickData {
                    timestamp: r.get("timestamp"),
                    symbol: r.get("symbol"),
                    price: r.get("price"),
                    quantity: r.get("quantity"),
                    side: if side_str == "buy" || side_str == "BUY" { TradeSide::Buy } else { TradeSide::Sell },
                    trade_id: r.get("trade_id"),
                    is_buyer_maker: r.get("is_buyer_maker"),
                };
                Ok(Some(tick))
            }
            None => Ok(None),
        }
    }

    /// Get 24h statistics for a symbol
    pub async fn get_symbol_stats(&self, symbol: &str, hours: i32) -> DataResult<serde_json::Value> {
        debug!("Getting {}h stats for: {}", hours, symbol);

        let hours_f64 = hours as f64;
        let row = sqlx::query(
            "SELECT COUNT(*) as total_ticks, MIN(price) as min_price, MAX(price) as max_price, \
             SUM(quantity) as total_volume FROM tick_data \
             WHERE symbol = $1 AND timestamp > NOW() - INTERVAL '1 hour' * $2"
        )
        .bind(symbol)
        .bind(hours_f64)
        .fetch_one(&self.pool)
        .await?;

        let stats = serde_json::json!({
            "symbol": symbol,
            "hours": hours,
            "total_ticks": row.get::<Option<i64>, _>("total_ticks").unwrap_or(0),
            "min_price": row.get::<Option<Decimal>, _>("min_price").map(|p| p.to_string()),
            "max_price": row.get::<Option<Decimal>, _>("max_price").map(|p| p.to_string()),
            "total_volume": row.get::<Option<Decimal>, _>("total_volume").map(|v| v.to_string()),
        });

        Ok(stats)
    }

    // ============ P9: 持仓和交易记录 ============

    /// Get all positions
    pub async fn get_positions(&self) -> DataResult<Vec<serde_json::Value>> {
        debug!("Fetching all positions");

        let rows = sqlx::query(
            "SELECT id, symbol, side, quantity, avg_entry_price, current_price, \
             unrealized_pnl, realized_pnl, opened_at, updated_at FROM positions ORDER BY updated_at DESC"
        )
        .fetch_all(&self.pool)
        .await?;

        let positions: Vec<serde_json::Value> = rows.iter().map(|r| {
            let id: uuid::Uuid = r.get("id");
            let opened_at: DateTime<Utc> = r.get("opened_at");
            let updated_at: DateTime<Utc> = r.get("updated_at");
            serde_json::json!({
                "id": id.to_string(),
                "symbol": r.get::<String, _>("symbol"),
                "side": r.get::<String, _>("side"),
                "quantity": r.get::<Decimal, _>("quantity").to_string(),
                "avg_entry_price": r.get::<Decimal, _>("avg_entry_price").to_string(),
                "current_price": r.get::<Option<Decimal>, _>("current_price").map(|p| p.to_string()),
                "unrealized_pnl": r.get::<Option<Decimal>, _>("unrealized_pnl").map(|p| p.to_string()),
                "realized_pnl": r.get::<Decimal, _>("realized_pnl").to_string(),
                "opened_at": opened_at.to_rfc3339(),
                "updated_at": updated_at.to_rfc3339(),
            })
        }).collect();

        debug!("Found {} positions", positions.len());
        Ok(positions)
    }

    /// Get trade history with pagination
    pub async fn get_trade_history(
        &self,
        symbol: Option<&str>,
        limit: i64,
        offset: i64,
    ) -> DataResult<Vec<serde_json::Value>> {
        debug!("Fetching trade history: symbol={:?}, limit={}, offset={}", symbol, limit, offset);

        // Use raw SQL with sqlx::query (not macro) for dynamic queries
        let sql = match symbol {
            Some(_) => {
                "SELECT id, order_id, symbol, side, price, quantity, commission, realized_pnl, strategy_id, trade_time, created_at FROM trades WHERE symbol = $1 ORDER BY trade_time DESC LIMIT $2 OFFSET $3"
            }
            None => {
                "SELECT id, order_id, symbol, side, price, quantity, commission, realized_pnl, strategy_id, trade_time, created_at FROM trades ORDER BY trade_time DESC LIMIT $1 OFFSET $2"
            }
        };

        let mut query = sqlx::query(sql);

        if let Some(sym) = symbol {
            query = query.bind(sym);
        }
        query = query.bind(limit).bind(offset);

        let rows = query.fetch_all(&self.pool).await?;

        let trades: Vec<serde_json::Value> = rows.iter().map(|r| {
            let id: uuid::Uuid = r.get("id");
            let trade_time: DateTime<Utc> = r.get("trade_time");
            let created_at: DateTime<Utc> = r.get("created_at");

            serde_json::json!({
                "id": id.to_string(),
                "order_id": r.get::<Option<String>, _>("order_id"),
                "symbol": r.get::<String, _>("symbol"),
                "side": r.get::<String, _>("side"),
                "price": r.get::<Decimal, _>("price").to_string(),
                "quantity": r.get::<Decimal, _>("quantity").to_string(),
                "commission": r.get::<Decimal, _>("commission").to_string(),
                "realized_pnl": r.get::<Option<Decimal>, _>("realized_pnl").map(|p| p.to_string()),
                "strategy_id": r.get::<Option<String>, _>("strategy_id"),
                "trade_time": trade_time.to_rfc3339(),
                "created_at": created_at.to_rfc3339(),
            })
        }).collect();

        debug!("Found {} trades", trades.len());
        Ok(trades)
    }

    /// Get PnL summary by period (daily/weekly/monthly)
    pub async fn get_pnl_summary(
        &self,
        symbol: Option<&str>,
        days: i32,
        exchange: Option<&str>,
        market_type: Option<&str>,
    ) -> DataResult<serde_json::Value> {
        debug!("Getting PnL summary for {} days, exchange={:?}, market_type={:?}", days, exchange, market_type);

        let days_f64 = days as f64;

        // Build dynamic WHERE clause
        let mut conditions: Vec<String> = vec![
            "trade_time > NOW() - INTERVAL '1 day' * $1".to_string(),
            "realized_pnl IS NOT NULL".to_string(),
        ];
        let mut param_index = 2;

        if symbol.is_some() {
            conditions.push(format!("symbol = ${}", param_index));
            param_index += 1;
        }
        if exchange.is_some() {
            conditions.push(format!("exchange = ${}", param_index));
            param_index += 1;
        }
        if market_type.is_some() {
            conditions.push(format!("market_type = ${}", param_index));
        }

        let where_clause = conditions.join(" AND ");
        let sql = format!(
            r#"
            SELECT
                COUNT(*) as total_trades,
                SUM(CASE WHEN realized_pnl > 0 THEN 1 ELSE 0 END) as winning_trades,
                SUM(CASE WHEN realized_pnl < 0 THEN 1 ELSE 0 END) as losing_trades,
                SUM(realized_pnl) as total_pnl,
                SUM(commission) as total_commission,
                MAX(realized_pnl) as best_trade,
                MIN(realized_pnl) as worst_trade,
                AVG(realized_pnl) as avg_pnl
            FROM trades
            WHERE {}
            "#,
            where_clause
        );

        let mut query = sqlx::query(&sql);
        query = query.bind(days_f64);

        if let Some(sym) = symbol {
            query = query.bind(sym);
        }
        if let Some(ex) = exchange {
            query = query.bind(ex);
        }
        if let Some(mt) = market_type {
            query = query.bind(mt);
        }

        let row = query.fetch_one(&self.pool).await?;

        let total_trades: i64 = row.get("total_trades");
        let winning_trades: i64 = row.get::<Option<i64>, _>("winning_trades").unwrap_or(0);
        let losing_trades: i64 = row.get::<Option<i64>, _>("losing_trades").unwrap_or(0);
        let total_pnl: Option<Decimal> = row.get("total_pnl");
        let total_commission: Option<Decimal> = row.get("total_commission");
        let best_trade: Option<Decimal> = row.get("best_trade");
        let worst_trade: Option<Decimal> = row.get("worst_trade");
        let avg_pnl: Option<Decimal> = row.get("avg_pnl");

        let win_rate = if total_trades > 0 {
            (winning_trades as f64 / total_trades as f64) * 100.0
        } else {
            0.0
        };

        let summary = serde_json::json!({
            "period_days": days,
            "symbol": symbol,
            "total_trades": total_trades,
            "winning_trades": winning_trades,
            "losing_trades": losing_trades,
            "win_rate": format!("{:.2}", win_rate),
            "total_pnl": total_pnl.map(|p| p.to_string()),
            "total_commission": total_commission.map(|c| c.to_string()),
            "best_trade": best_trade.map(|p| p.to_string()),
            "worst_trade": worst_trade.map(|p| p.to_string()),
            "avg_pnl": avg_pnl.map(|p| p.to_string()),
        });

        Ok(summary)
    }

    /// Get latest prices for multiple symbols
    pub async fn get_latest_prices(
        &self,
        symbols: &[String],
    ) -> DataResult<HashMap<String, Decimal>> {
        if symbols.is_empty() {
            return Ok(HashMap::new());
        }

        debug!("Fetching latest prices for symbols: {:?}", symbols);

        let mut prices = HashMap::new();

        // Try to get from cache first
        for symbol in symbols {
            if let Ok(cached_ticks) = self.cache.get_recent_ticks(symbol, 1).await {
                if let Some(latest_tick) = cached_ticks.first() {
                    prices.insert(symbol.clone(), latest_tick.price);
                }
            }
        }

        // Get remaining symbols from database
        let missing_symbols: Vec<String> = symbols
            .iter()
            .filter(|symbol| !prices.contains_key(*symbol))
            .map(|s| s.clone())
            .collect();

        if !missing_symbols.is_empty() {
            let rows = sqlx::query(
                "SELECT DISTINCT ON (symbol) symbol, price FROM tick_data \
                 WHERE symbol = ANY($1) ORDER BY symbol, timestamp DESC"
            )
            .bind(&missing_symbols[..])
            .fetch_all(&self.pool)
            .await?;

            for row in rows {
                prices.insert(row.get("symbol"), row.get("price"));
            }
        }

        debug!("Retrieved latest prices for {} symbols", prices.len());
        Ok(prices)
    }

    // ============ P10: 统计分析 ============

    /// Get equity curve data grouped by period
    pub async fn get_equity_curve(
        &self,
        symbol: Option<&str>,
        period: &str,
        days: i32,
    ) -> DataResult<Vec<serde_json::Value>> {
        debug!("Getting equity curve: period={}, days={}", period, days);

        let days_f64 = days as f64;
        let date_trunc = match period {
            "weekly" => "week",
            "monthly" => "month",
            _ => "day",
        };

        let sql = match symbol {
            Some(_) => {
                format!(r#"
                SELECT
                    DATE_TRUNC('{date_trunc}', trade_time) as period_date,
                    SUM(COALESCE(realized_pnl, 0)) as period_pnl,
                    SUM(commission) as period_commission
                FROM trades
                WHERE symbol = $1
                  AND trade_time > NOW() - INTERVAL '1 day' * $2
                GROUP BY period_date
                ORDER BY period_date
                "#)
            }
            None => {
                format!(r#"
                SELECT
                    DATE_TRUNC('{date_trunc}', trade_time) as period_date,
                    SUM(COALESCE(realized_pnl, 0)) as period_pnl,
                    SUM(commission) as period_commission
                FROM trades
                WHERE trade_time > NOW() - INTERVAL '1 day' * $1
                GROUP BY period_date
                ORDER BY period_date
                "#)
            }
        };

        let mut query = sqlx::query(&sql);

        if let Some(sym) = symbol {
            query = query.bind(sym);
        }
        query = query.bind(days_f64);

        let rows = query.fetch_all(&self.pool).await?;

        let mut cumulative_pnl = Decimal::ZERO;
        let mut equity_curve: Vec<serde_json::Value> = Vec::new();

        for row in rows {
            let period_date: chrono::NaiveDateTime = row.get("period_date");
            let period_pnl: Decimal = row.get("period_pnl");
            let period_commission: Option<Decimal> = row.get("period_commission");

            cumulative_pnl += period_pnl - period_commission.unwrap_or(Decimal::ZERO);

            equity_curve.push(serde_json::json!({
                "date": period_date.format("%Y-%m-%d").to_string(),
                "pnl": period_pnl.to_string(),
                "commission": period_commission.map(|c| c.to_string()).unwrap_or_else(|| "0".to_string()),
                "cumulative_pnl": cumulative_pnl.to_string(),
            }));
        }

        debug!("Generated {} equity curve points", equity_curve.len());
        Ok(equity_curve)
    }

    /// Get detailed performance metrics
    pub async fn get_performance_metrics(
        &self,
        symbol: Option<&str>,
        days: i32,
        exchange: Option<&str>,
        market_type: Option<&str>,
    ) -> DataResult<serde_json::Value> {
        debug!("Getting performance metrics for {} days, exchange={:?}, market_type={:?}", days, exchange, market_type);

        let days_f64 = days as f64;

        // Build dynamic WHERE clause
        let mut conditions: Vec<String> = vec![
            "trade_time > NOW() - INTERVAL '1 day' * $1".to_string(),
            "realized_pnl IS NOT NULL".to_string(),
        ];
        let mut param_index = 2;

        if symbol.is_some() {
            conditions.push(format!("symbol = ${}", param_index));
            param_index += 1;
        }
        if exchange.is_some() {
            conditions.push(format!("exchange = ${}", param_index));
            param_index += 1;
        }
        if market_type.is_some() {
            conditions.push(format!("market_type = ${}", param_index));
        }

        let where_clause = conditions.join(" AND ");
        let sql = format!(
            r#"
            SELECT
                realized_pnl,
                commission,
                trade_time
            FROM trades
            WHERE {}
            ORDER BY trade_time
            "#,
            where_clause
        );

        let mut query = sqlx::query(&sql);
        query = query.bind(days_f64);

        if let Some(sym) = symbol {
            query = query.bind(sym);
        }
        if let Some(ex) = exchange {
            query = query.bind(ex);
        }
        if let Some(mt) = market_type {
            query = query.bind(mt);
        }

        let rows = query.fetch_all(&self.pool).await?;

        let mut pnls: Vec<Decimal> = Vec::new();
        let mut winning_trades = 0i64;
        let mut losing_trades = 0i64;
        let mut total_win = Decimal::ZERO;
        let mut total_loss = Decimal::ZERO;
        let mut largest_win = Decimal::ZERO;
        let mut largest_loss = Decimal::ZERO;
        let mut consecutive_wins = 0i32;
        let mut consecutive_losses = 0i32;
        let mut max_consecutive_wins = 0i32;
        let mut max_consecutive_losses = 0i32;

        for row in &rows {
            let pnl: Decimal = row.get("realized_pnl");
            pnls.push(pnl);

            if pnl > Decimal::ZERO {
                winning_trades += 1;
                total_win += pnl;
                consecutive_wins += 1;
                consecutive_losses = 0;
                if pnl > largest_win {
                    largest_win = pnl;
                }
                if consecutive_wins > max_consecutive_wins {
                    max_consecutive_wins = consecutive_wins;
                }
            } else {
                losing_trades += 1;
                total_loss += pnl.abs();
                consecutive_losses += 1;
                consecutive_wins = 0;
                if pnl.abs() > largest_loss {
                    largest_loss = pnl.abs();
                }
                if consecutive_losses > max_consecutive_losses {
                    max_consecutive_losses = consecutive_losses;
                }
            }
        }

        let total_trades = winning_trades + losing_trades;
        let win_rate = if total_trades > 0 {
            Decimal::from(winning_trades) / Decimal::from(total_trades) * Decimal::from(100)
        } else {
            Decimal::ZERO
        };

        let avg_win = if winning_trades > 0 {
            total_win / Decimal::from(winning_trades)
        } else {
            Decimal::ZERO
        };

        let avg_loss = if losing_trades > 0 {
            total_loss / Decimal::from(losing_trades)
        } else {
            Decimal::ZERO
        };

        let profit_factor = if total_loss > Decimal::ZERO {
            total_win / total_loss
        } else if total_win > Decimal::ZERO {
            Decimal::MAX
        } else {
            Decimal::ZERO
        };

        // Calculate Sharpe ratio (assuming daily returns)
        let risk_free_rate = Decimal::from_str("0.02").unwrap_or(Decimal::ZERO) / Decimal::from(365);
        let sharpe = crate::backtest::metrics::BacktestMetrics::calculate_sharpe_ratio(&pnls, risk_free_rate);

        // Calculate Sortino ratio
        let sortino = crate::backtest::metrics::BacktestMetrics::calculate_sortino_ratio(
            &pnls, risk_free_rate, Decimal::ZERO
        );

        // Calculate max drawdown from cumulative PnL (with dates for duration)
        let mut equity_curve: Vec<Decimal> = Vec::new();
        let mut equity_with_dates: Vec<(chrono::NaiveDateTime, Decimal)> = Vec::new();
        let mut cum_pnl = Decimal::ZERO;
        for row in &rows {
            let pnl: Decimal = row.get("realized_pnl");
            let trade_time: chrono::NaiveDateTime = row.get("trade_time");
            cum_pnl += pnl;
            equity_curve.push(cum_pnl);
            equity_with_dates.push((trade_time, cum_pnl));
        }
        let max_drawdown = crate::backtest::metrics::BacktestMetrics::calculate_max_drawdown(&equity_curve);

        // Calculate max drawdown duration (longest peak-to-recovery period)
        let max_drawdown_duration_days = if equity_with_dates.len() >= 2 {
            let mut max_duration = chrono::Duration::zero();
            let mut peak_equity = Decimal::MIN;
            let mut peak_time = equity_with_dates[0].0;
            let mut in_drawdown = false;
            let mut drawdown_start = equity_with_dates[0].0;

            for (time, equity) in &equity_with_dates {
                if *equity >= peak_equity {
                    // New peak or recovery
                    if in_drawdown {
                        let duration = *time - drawdown_start;
                        if duration > max_duration {
                            max_duration = duration;
                        }
                        in_drawdown = false;
                    }
                    peak_equity = *equity;
                    peak_time = *time;
                } else {
                    // In drawdown
                    if !in_drawdown {
                        drawdown_start = peak_time;
                        in_drawdown = true;
                    }
                }
            }

            // Check if currently in drawdown (extends to now)
            if in_drawdown {
                let now = chrono::Utc::now().naive_utc();
                let duration = now - drawdown_start;
                if duration > max_duration {
                    max_duration = duration;
                }
            }

            max_duration.num_days()
        } else {
            0
        };

        // Estimate average trade duration by pairing buy/sell trades for same symbol
        let avg_trade_duration_hours = {
            // Query to get consecutive trade pairs (buy then sell) for duration estimation
            let duration_sql = match symbol {
                Some(_) => {
                    r#"
                    WITH ordered_trades AS (
                        SELECT symbol, side, trade_time,
                               LEAD(trade_time) OVER (PARTITION BY symbol ORDER BY trade_time) as next_trade_time,
                               LEAD(side) OVER (PARTITION BY symbol ORDER BY trade_time) as next_side
                        FROM trades
                        WHERE symbol = $1
                          AND trade_time > NOW() - INTERVAL '1 day' * $2
                        ORDER BY symbol, trade_time
                    )
                    SELECT AVG(EXTRACT(EPOCH FROM (next_trade_time - trade_time)) / 3600.0) as avg_hours
                    FROM ordered_trades
                    WHERE next_side IS NOT NULL
                      AND side != next_side
                      AND next_trade_time > trade_time
                    "#
                }
                None => {
                    r#"
                    WITH ordered_trades AS (
                        SELECT symbol, side, trade_time,
                               LEAD(trade_time) OVER (PARTITION BY symbol ORDER BY trade_time) as next_trade_time,
                               LEAD(side) OVER (PARTITION BY symbol ORDER BY trade_time) as next_side
                        FROM trades
                        WHERE trade_time > NOW() - INTERVAL '1 day' * $1
                        ORDER BY symbol, trade_time
                    )
                    SELECT AVG(EXTRACT(EPOCH FROM (next_trade_time - trade_time)) / 3600.0) as avg_hours
                    FROM ordered_trades
                    WHERE next_side IS NOT NULL
                      AND side != next_side
                      AND next_trade_time > trade_time
                    "#
                }
            };

            let mut duration_query = sqlx::query(duration_sql);
            if let Some(sym) = symbol {
                duration_query = duration_query.bind(sym);
            }
            duration_query = duration_query.bind(days_f64);

            match duration_query.fetch_optional(&self.pool).await {
                Ok(Some(row)) => {
                    row.get::<Option<f64>, _>("avg_hours").unwrap_or(0.0)
                }
                _ => 0.0,
            }
        };

        // Calculate volatility
        let volatility = crate::backtest::metrics::BacktestMetrics::calculate_volatility(&pnls);

        // Calculate Calmar ratio
        let annual_return = if days > 0 {
            cum_pnl * Decimal::from(365) / Decimal::from(days)
        } else {
            Decimal::ZERO
        };
        let calmar = crate::backtest::metrics::BacktestMetrics::calculate_calmar_ratio(annual_return, max_drawdown);

        let metrics = serde_json::json!({
            "sharpe_ratio": sharpe.to_string(),
            "sortino_ratio": sortino.to_string(),
            "max_drawdown": max_drawdown.to_string(),
            "max_drawdown_duration_days": max_drawdown_duration_days,
            "calmar_ratio": calmar.to_string(),
            "volatility": volatility.to_string(),
            "win_rate": win_rate.to_string(),
            "profit_factor": profit_factor.to_string(),
            "avg_trade_duration_hours": avg_trade_duration_hours,
            "total_trades": total_trades,
            "winning_trades": winning_trades,
            "losing_trades": losing_trades,
            "avg_win": avg_win.to_string(),
            "avg_loss": avg_loss.to_string(),
            "largest_win": largest_win.to_string(),
            "largest_loss": largest_loss.to_string(),
            "consecutive_wins": max_consecutive_wins,
            "consecutive_losses": max_consecutive_losses,
            "total_pnl": cum_pnl.to_string(),
        });

        Ok(metrics)
    }

    /// Get commission statistics
    pub async fn get_commission_stats(
        &self,
        symbol: Option<&str>,
        days: i32,
    ) -> DataResult<serde_json::Value> {
        debug!("Getting commission stats for {} days", days);

        let days_f64 = days as f64;

        // Get total commission
        let total_sql = match symbol {
            Some(_) => {
                r#"
                SELECT
                    SUM(commission) as total_commission,
                    COUNT(*) as trade_count
                FROM trades
                WHERE symbol = $1
                  AND trade_time > NOW() - INTERVAL '1 day' * $2
                "#
            }
            None => {
                r#"
                SELECT
                    SUM(commission) as total_commission,
                    COUNT(*) as trade_count
                FROM trades
                WHERE trade_time > NOW() - INTERVAL '1 day' * $1
                "#
            }
        };

        let mut total_query = sqlx::query(total_sql);
        if let Some(sym) = symbol {
            total_query = total_query.bind(sym);
        }
        total_query = total_query.bind(days_f64);

        let total_row = total_query.fetch_one(&self.pool).await?;
        let total_commission: Decimal = total_row.get::<Option<Decimal>, _>("total_commission").unwrap_or(Decimal::ZERO);
        let trade_count: i64 = total_row.get("trade_count");

        let avg_commission = if trade_count > 0 {
            total_commission / Decimal::from(trade_count)
        } else {
            Decimal::ZERO
        };

        // Get commission by symbol
        let symbol_sql = r#"
        SELECT
            symbol,
            SUM(commission) as total_commission,
            COUNT(*) as trade_count
        FROM trades
        WHERE trade_time > NOW() - INTERVAL '1 day' * $1
        GROUP BY symbol
        ORDER BY total_commission DESC
        "#;

        let symbol_rows = sqlx::query(symbol_sql)
            .bind(days_f64)
            .fetch_all(&self.pool)
            .await?;

        let by_symbol: Vec<serde_json::Value> = symbol_rows.iter().map(|r| {
            serde_json::json!({
                "symbol": r.get::<String, _>("symbol"),
                "total_commission": r.get::<Decimal, _>("total_commission").to_string(),
                "trade_count": r.get::<i64, _>("trade_count"),
            })
        }).collect();

        // Get commission by month
        let month_sql = r#"
        SELECT
            DATE_TRUNC('month', trade_time) as month,
            SUM(commission) as total_commission,
            COUNT(*) as trade_count
        FROM trades
        WHERE trade_time > NOW() - INTERVAL '1 day' * $1
        GROUP BY month
        ORDER BY month DESC
        "#;

        let month_rows = sqlx::query(month_sql)
            .bind(days_f64)
            .fetch_all(&self.pool)
            .await?;

        let by_month: Vec<serde_json::Value> = month_rows.iter().map(|r| {
            let month: chrono::NaiveDateTime = r.get("month");
            serde_json::json!({
                "month": month.format("%Y-%m").to_string(),
                "total_commission": r.get::<Decimal, _>("total_commission").to_string(),
                "trade_count": r.get::<i64, _>("trade_count"),
            })
        }).collect();

        let stats = serde_json::json!({
            "total_commission": total_commission.to_string(),
            "avg_commission_per_trade": avg_commission.to_string(),
            "trade_count": trade_count,
            "commission_by_symbol": by_symbol,
            "commission_by_month": by_month,
        });

        Ok(stats)
    }

    // =================================================================
    // Backtest Specific Query Operations
    // =================================================================

    /// Get recent N ticks for backtesting (ordered by time ASC)
    pub async fn get_recent_ticks_for_backtest(
        &self,
        symbol: &str,
        count: i64,
    ) -> DataResult<Vec<TickData>> {
        debug!("Fetching {} recent ticks for backtest: {}", count, symbol);

        let limit = count.min(MAX_QUERY_LIMIT as i64);

        let rows = sqlx::query(
            "SELECT timestamp, symbol, price, quantity, side, trade_id, is_buyer_maker \
             FROM tick_data WHERE symbol = $1 ORDER BY timestamp DESC LIMIT $2"
        )
        .bind(symbol)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;

        // Convert rows to TickData and reverse to get ASC order (oldest first)
        let ticks: DataResult<Vec<TickData>> = rows
            .iter()
            .map(|row| {
                let side_str: String = row.get("side");
                Ok(TickData {
                    timestamp: row.get("timestamp"),
                    symbol: row.get("symbol"),
                    price: row.get("price"),
                    quantity: row.get("quantity"),
                    side: self.parse_trade_side(&side_str)?,
                    trade_id: row.get("trade_id"),
                    is_buyer_maker: row.get("is_buyer_maker"),
                })
            })
            .collect();

        let mut ticks = ticks?;
        ticks.reverse(); // Reverse to get chronological order (ASC)

        debug!("Retrieved {} ticks for backtest", ticks.len());
        Ok(ticks)
    }

    /// Get historical data for backtesting within time range (ordered by time ASC)
    pub async fn get_historical_data_for_backtest(
        &self,
        symbol: &str,
        start_time: DateTime<Utc>,
        end_time: DateTime<Utc>,
        limit: Option<i64>,
    ) -> DataResult<Vec<TickData>> {
        debug!(
            "Fetching historical data for backtest: {} from {} to {}",
            symbol, start_time, end_time
        );

        let query_limit = limit
            .unwrap_or(MAX_QUERY_LIMIT as i64)
            .min(MAX_QUERY_LIMIT as i64);

        let rows = sqlx::query(
            "SELECT timestamp, symbol, price, quantity, side, trade_id, is_buyer_maker \
             FROM tick_data WHERE symbol = $1 AND timestamp >= $2 AND timestamp <= $3 \
             ORDER BY timestamp ASC LIMIT $4"
        )
        .bind(symbol)
        .bind(start_time)
        .bind(end_time)
        .bind(query_limit)
        .fetch_all(&self.pool)
        .await?;

        let ticks: DataResult<Vec<TickData>> = rows
            .iter()
            .map(|row| {
                let side_str: String = row.get("side");
                Ok(TickData {
                    timestamp: row.get("timestamp"),
                    symbol: row.get("symbol"),
                    price: row.get("price"),
                    quantity: row.get("quantity"),
                    side: self.parse_trade_side(&side_str)?,
                    trade_id: row.get("trade_id"),
                    is_buyer_maker: row.get("is_buyer_maker"),
                })
            })
            .collect();

        let ticks = ticks?;
        debug!("Retrieved {} historical ticks for backtest", ticks.len());
        Ok(ticks)
    }

    /// Get backtest data information for user selection
    pub async fn get_backtest_data_info(&self) -> DataResult<BacktestDataInfo> {
        debug!("Fetching backtest data information");

        // Get overall statistics from kline_1m
        let overall_stats = sqlx::query(
            "SELECT COUNT(*) as total_records, COUNT(DISTINCT symbol) as symbols_count, \
             MIN(timestamp) as earliest_time, MAX(timestamp) as latest_time FROM kline_1m"
        )
        .fetch_one(&self.pool)
        .await?;

        // Get per-symbol statistics from kline_1m
        let symbol_stats = sqlx::query(
            "SELECT symbol, COUNT(*) as records_count, MIN(timestamp) as earliest_time, \
             MAX(timestamp) as latest_time, MIN(low) as min_price, MAX(high) as max_price, \
             SUM(volume * close) as total_volume_usd \
             FROM kline_1m GROUP BY symbol ORDER BY total_volume_usd DESC"
        )
        .fetch_all(&self.pool)
        .await?;

        let symbol_info: Vec<SymbolDataInfo> = symbol_stats
            .into_iter()
            .map(|row| SymbolDataInfo {
                symbol: row.get("symbol"),
                records_count: row.get::<Option<i64>, _>("records_count").unwrap_or(0) as u64,
                earliest_time: row.get("earliest_time"),
                latest_time: row.get("latest_time"),
                min_price: row.get("min_price"),
                max_price: row.get("max_price"),
                total_volume_usd: row.get::<Option<Decimal>, _>("total_volume_usd").unwrap_or(Decimal::ZERO),
            })
            .collect();

        let info = BacktestDataInfo {
            total_records: overall_stats.get::<Option<i64>, _>("total_records").unwrap_or(0) as u64,
            symbols_count: overall_stats.get::<Option<i64>, _>("symbols_count").unwrap_or(0) as u64,
            earliest_time: overall_stats.get("earliest_time"),
            latest_time: overall_stats.get("latest_time"),
            symbol_info,
        };

        debug!(
            "Backtest data info: {} total records, {} symbols",
            info.total_records, info.symbols_count
        );
        Ok(info)
    }

    // =================================================================
    // Maintenance Operations
    // =================================================================

    /// Clean up old tick data
    pub async fn cleanup_old_data(&self, days_to_keep: f64) -> DataResult<u64> {
        info!("Cleaning up tick data older than {} days", days_to_keep);

        let result = sqlx::query(
            "WITH deleted AS (DELETE FROM tick_data WHERE timestamp < NOW() - INTERVAL '1 day' * $1 RETURNING *) \
             SELECT COUNT(*) as count FROM deleted"
        )
        .bind(days_to_keep)
        .fetch_one(&self.pool)
        .await?;

        let deleted_count = result.get::<Option<i64>, _>("count").unwrap_or(0) as u64;
        info!("Cleaned up {} old tick data records", deleted_count);
        Ok(deleted_count)
    }

    /// Get database statistics
    pub async fn get_db_stats(&self, symbol: Option<&str>) -> DataResult<DbStats> {
        let (total_records, earliest_timestamp, latest_timestamp) = if let Some(sym) = symbol {
            let row = sqlx::query(
                "SELECT COUNT(*) as total_records, MIN(timestamp) as earliest_timestamp, \
                 MAX(timestamp) as latest_timestamp FROM tick_data WHERE symbol = $1"
            )
            .bind(sym)
            .fetch_one(&self.pool)
            .await?;

            (
                row.get::<Option<i64>, _>("total_records"),
                row.get("earliest_timestamp"),
                row.get("latest_timestamp"),
            )
        } else {
            let row = sqlx::query(
                "SELECT COUNT(*) as total_records, MIN(timestamp) as earliest_timestamp, \
                 MAX(timestamp) as latest_timestamp FROM tick_data"
            )
            .fetch_one(&self.pool)
            .await?;

            (
                row.get::<Option<i64>, _>("total_records"),
                row.get("earliest_timestamp"),
                row.get("latest_timestamp"),
            )
        };

        Ok(DbStats {
            symbol: symbol.map(|s| s.to_string()),
            total_records: total_records.unwrap_or(0) as u64,
            earliest_timestamp,
            latest_timestamp,
        })
    }

    // =================================================================
    // Helper Methods
    // =================================================================

    /// Validate tick data
    fn validate_tick_data(&self, tick: &TickData) -> DataResult<()> {
        if tick.symbol.is_empty() {
            return Err(DataError::Validation("Symbol cannot be empty".into()));
        }

        if tick.price <= Decimal::ZERO {
            return Err(DataError::Validation("Price must be positive".into()));
        }

        if tick.quantity <= Decimal::ZERO {
            return Err(DataError::Validation("Quantity must be positive".into()));
        }

        if tick.trade_id.is_empty() {
            return Err(DataError::Validation("Trade ID cannot be empty".into()));
        }

        Ok(())
    }

    /// Parse trade side from database string
    fn parse_trade_side(&self, side_str: &str) -> DataResult<TradeSide> {
        match side_str.to_uppercase().as_str() {
            "BUY" => Ok(TradeSide::Buy),
            "SELL" => Ok(TradeSide::Sell),
            _ => Err(DataError::InvalidFormat(format!(
                "Invalid trade side: {}",
                side_str
            ))),
        }
    }

    /// Check if query is for recent data (suitable for cache)
    fn is_recent_query(&self, query: &TickQuery) -> bool {
        if let Some(start_time) = query.start_time {
            let now = Utc::now();
            let duration = now - start_time;
            // Consider "recent" if within last hour
            duration <= Duration::hours(1)
        } else {
            // If no start time specified, assume it's a recent query
            true
        }
    }

    pub async fn insert_live_strategy_log(&self, log: &LiveStrategyLog) -> DataResult<()> {
        sqlx::query(
            "INSERT INTO live_strategy_log \
             (timestamp, strategy_id, symbol, current_price, signal_type, \
              portfolio_value, total_pnl, cache_hit, processing_time_us) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)"
        )
        .bind(log.timestamp)
        .bind(&log.strategy_id)
        .bind(&log.symbol)
        .bind(log.current_price)
        .bind(&log.signal_type)
        .bind(log.portfolio_value)
        .bind(log.total_pnl)
        .bind(log.cache_hit)
        .bind(log.processing_time_us as i32)
        .execute(&self.pool)
        .await?;

        Ok(())
    }

    // =================================================================
    // K-line (kline_1m) Operations
    // =================================================================

    /// Insert a single kline into kline_1m table (upsert)
    pub async fn insert_kline(&self, kline: &OHLCData) -> DataResult<()> {
        let mut query_builder = QueryBuilder::new(
            "INSERT INTO kline_1m (timestamp, symbol, open, high, low, close, volume, trade_count) "
        );

        query_builder.push_values(std::slice::from_ref(kline), |mut b, k| {
            b.push_bind(k.timestamp)
                .push_bind(&k.symbol)
                .push_bind(k.open)
                .push_bind(k.high)
                .push_bind(k.low)
                .push_bind(k.close)
                .push_bind(k.volume)
                .push_bind(k.trade_count.min(i32::MAX as u64) as i32);
        });

        query_builder.push(
            " ON CONFLICT (symbol, timestamp) DO UPDATE SET \
              open = EXCLUDED.open, high = EXCLUDED.high, low = EXCLUDED.low, \
              close = EXCLUDED.close, volume = EXCLUDED.volume, trade_count = EXCLUDED.trade_count"
        );

        query_builder.build().execute(&self.pool).await.map_err(|e| {
            error!("[kline_1m] Insert failed for {}: {}", kline.symbol, e);
            DataError::Database(e)
        })?;

        Ok(())
    }

    /// Batch insert klines into kline_1m table (upsert)
    pub async fn batch_insert_klines(&self, klines: Vec<OHLCData>) -> DataResult<usize> {
        if klines.is_empty() {
            return Ok(0);
        }

        let total_count = klines.len();
        let mut total_inserted = 0;

        for chunk in klines.chunks(MAX_BATCH_SIZE) {
            let mut query_builder = QueryBuilder::new(
                "INSERT INTO kline_1m (timestamp, symbol, open, high, low, close, volume, trade_count) "
            );

            query_builder.push_values(chunk, |mut b, kline| {
                b.push_bind(kline.timestamp)
                    .push_bind(&kline.symbol)
                    .push_bind(kline.open)
                    .push_bind(kline.high)
                    .push_bind(kline.low)
                    .push_bind(kline.close)
                    .push_bind(kline.volume)
                    .push_bind(kline.trade_count.min(i32::MAX as u64) as i32);
            });

            query_builder.push(
                " ON CONFLICT (symbol, timestamp) DO UPDATE SET \
                  open = EXCLUDED.open, high = EXCLUDED.high, low = EXCLUDED.low, \
                  close = EXCLUDED.close, volume = EXCLUDED.volume, trade_count = EXCLUDED.trade_count"
            );

            let result = query_builder.build().execute(&self.pool).await?;
            total_inserted += result.rows_affected() as usize;
        }

        // 静默，由调用方决定是否打日志
        debug!("[kline_1m] Batch upserted {} records", total_inserted);
        Ok(total_inserted)
    }

    /// 批量写入高时间框架 K 线（接受 Timeframe 枚举）
    pub async fn batch_insert_high_tf_klines(
        &self,
        klines: &[OHLCData],
        timeframe: &Timeframe,
    ) -> DataResult<usize> {
        let tf_str = timeframe.as_str();
        self.batch_insert_high_tf_klines_by_str(klines, tf_str).await
    }

    /// 批量写入高时间框架 K 线（接受字符串）
    pub async fn batch_insert_high_tf_klines_by_str(
        &self,
        klines: &[OHLCData],
        timeframe: &str,
    ) -> DataResult<usize> {
        if klines.is_empty() {
            return Ok(0);
        }

        let table_name = get_high_tf_table_name(timeframe)?;

        let total_count = klines.len();
        let mut total_inserted = 0;

        for chunk in klines.chunks(MAX_BATCH_SIZE) {
            let mut query_builder = QueryBuilder::new(format!(
                "INSERT INTO {} (symbol, open_time, open, high, low, close, volume, trade_count) ", table_name
            ));

            query_builder.push_values(chunk, |mut b, kline| {
                b.push_bind(&kline.symbol)
                    .push_bind(kline.timestamp)
                    .push_bind(kline.open)
                    .push_bind(kline.high)
                    .push_bind(kline.low)
                    .push_bind(kline.close)
                    .push_bind(kline.volume)
                    .push_bind(kline.trade_count.min(i32::MAX as u64) as i32);
            });

            query_builder.push(format!(
                // 注意：open 字段不在 UPDATE 中，保留首次写入的开盘价
                // 这是设计意图：高TF K线可能被多次部分写入（如4h K线在2小时时写入一次，4小时时更新）
                // open 应保持时间窗口开始时的价格，high/low 应保留极值，close 应为最新值
                " ON CONFLICT (symbol, open_time) DO UPDATE SET \
                  high = GREATEST({0}.high, EXCLUDED.high), \
                  low = LEAST({0}.low, EXCLUDED.low), \
                  close = EXCLUDED.close, \
                  volume = EXCLUDED.volume, \
                  trade_count = EXCLUDED.trade_count", table_name
            ));

            let result = query_builder.build().execute(&self.pool).await?;
            total_inserted += result.rows_affected() as usize;
        }

        debug!("[{}] Batch upserted {} records", table_name, total_inserted);
        Ok(total_inserted)
    }

    // =================================================================
    // High Timeframe Query Operations
    // =================================================================

    /// 获取高时间框架 K 线数据
    ///
    /// 返回最新的 `limit` 条记录，按时间升序排列
    pub async fn get_high_tf_klines(
        &self,
        symbol: &str,
        timeframe: &str,
        limit: u32,
    ) -> DataResult<Vec<OHLCData>> {
        let table_name = get_high_tf_table_name(timeframe)?;
        let limit = limit.min(MAX_QUERY_LIMIT);
        let tf = Timeframe::from_str(timeframe).unwrap_or(Timeframe::FourHour);

        let sql = format!(
            "SELECT * FROM (\
             SELECT symbol, open_time as timestamp, open, high, low, close, volume, trade_count \
             FROM {} WHERE symbol = $1 \
             ORDER BY open_time DESC LIMIT $2\
             ) sub ORDER BY timestamp ASC",
            table_name
        );

        let rows = sqlx::query(&sql)
            .bind(symbol)
            .bind(limit as i64)
            .fetch_all(&self.pool)
            .await?;

        let klines: Vec<OHLCData> = rows
            .iter()
            .map(|row| OHLCData {
                timestamp: row.get("timestamp"),
                symbol: row.get("symbol"),
                timeframe: tf,
                open: row.get("open"),
                high: row.get("high"),
                low: row.get("low"),
                close: row.get("close"),
                volume: row.get("volume"),
                trade_count: row.get::<i32, _>("trade_count") as u64,
            })
            .collect();

        debug!("[{}] Retrieved {} {} klines", symbol, klines.len(), timeframe);
        Ok(klines)
    }

    /// 获取高时间框架的最新数据时间
    pub async fn get_high_tf_latest(
        &self,
        symbol: &str,
        timeframe: &str,
    ) -> DataResult<Option<DateTime<Utc>>> {
        let table_name = get_high_tf_table_name(timeframe)?;

        let sql = format!(
            "SELECT MAX(open_time) AS latest FROM {} WHERE symbol = $1",
            table_name
        );

        let row = sqlx::query(&sql)
            .bind(symbol)
            .fetch_optional(&self.pool)
            .await?;

        let latest = row.and_then(|r| r.get::<Option<DateTime<Utc>>, _>("latest"));
        debug!("[{}] Latest {} kline: {:?}", symbol, timeframe, latest);
        Ok(latest)
    }

    /// 获取高时间框架的最早数据时间
    pub async fn get_high_tf_earliest(
        &self,
        symbol: &str,
        timeframe: &str,
    ) -> DataResult<Option<DateTime<Utc>>> {
        let table_name = get_high_tf_table_name(timeframe)?;

        let sql = format!(
            "SELECT MIN(open_time) AS earliest FROM {} WHERE symbol = $1",
            table_name
        );

        let row = sqlx::query(&sql)
            .bind(symbol)
            .fetch_optional(&self.pool)
            .await?;

        let earliest = row.and_then(|r| r.get::<Option<DateTime<Utc>>, _>("earliest"));
        debug!("[{}] Earliest {} kline: {:?}", symbol, timeframe, earliest);
        Ok(earliest)
    }

    /// 获取高时间框架数据统计
    pub async fn get_high_tf_stats(
        &self,
        symbol: &str,
        timeframe: &str,
    ) -> DataResult<(u64, Option<DateTime<Utc>>, Option<DateTime<Utc>>)> {
        let table_name = get_high_tf_table_name(timeframe)?;

        let sql = format!(
            "SELECT COUNT(*) as total, MIN(open_time) as earliest, MAX(open_time) as latest \
             FROM {} WHERE symbol = $1",
            table_name
        );

        let row = sqlx::query(&sql)
            .bind(symbol)
            .fetch_one(&self.pool)
            .await?;

        Ok((
            row.get::<Option<i64>, _>("total").unwrap_or(0) as u64,
            row.get("earliest"),
            row.get("latest"),
        ))
    }

    /// Get klines from kline_1m table
    /// Returns the latest `limit` records in ascending time order (oldest first, newest last)
    pub async fn get_klines(
        &self,
        symbol: &str,
        limit: u32,
    ) -> DataResult<Vec<OHLCData>> {
        let limit = limit.min(MAX_QUERY_LIMIT);

        let mut query_builder = QueryBuilder::new(
            "SELECT * FROM (\
             SELECT timestamp, symbol, open, high, low, close, volume, trade_count \
             FROM kline_1m WHERE symbol = "
        );
        query_builder.push_bind(symbol);
        query_builder.push(" ORDER BY timestamp DESC LIMIT ");
        query_builder.push_bind(limit as i64);
        query_builder.push(") sub ORDER BY timestamp ASC");

        let rows = query_builder.build().fetch_all(&self.pool).await?;

        let klines: Vec<OHLCData> = rows
            .iter()
            .map(|row| OHLCData {
                timestamp: row.get("timestamp"),
                symbol: row.get("symbol"),
                timeframe: Timeframe::OneMinute,
                open: row.get("open"),
                high: row.get("high"),
                low: row.get("low"),
                close: row.get("close"),
                volume: row.get("volume"),
                trade_count: row.get::<i32, _>("trade_count") as u64,
            })
            .collect();

        debug!("Retrieved {} klines for {}", klines.len(), symbol);
        Ok(klines)
    }

    /// Get all available symbols from kline_1m table
    pub async fn get_available_symbols(&self) -> DataResult<Vec<String>> {
        let rows = sqlx::query("SELECT DISTINCT symbol FROM kline_1m ORDER BY symbol")
            .fetch_all(&self.pool)
            .await?;

        Ok(rows.into_iter().map(|r| r.get::<String, _>("symbol")).collect())
    }

    /// Get latest kline + 24h statistics for a symbol
    pub async fn get_kline_with_24h_stats(
        &self,
        symbol: &str,
    ) -> DataResult<Option<(OHLCData, Kline24hStats)>> {
        // Get latest kline
        let latest = sqlx::query(
            "SELECT timestamp, symbol, open, high, low, close, volume, trade_count \
             FROM kline_1m WHERE symbol = $1 ORDER BY timestamp DESC LIMIT 1"
        )
        .bind(symbol)
        .fetch_optional(&self.pool)
        .await?;

        let latest = match latest {
            Some(row) => OHLCData {
                timestamp: row.get("timestamp"),
                symbol: row.get("symbol"),
                timeframe: Timeframe::OneMinute,
                open: row.get("open"),
                high: row.get("high"),
                low: row.get("low"),
                close: row.get("close"),
                volume: row.get("volume"),
                trade_count: row.get::<i32, _>("trade_count") as u64,
            },
            None => return Ok(None),
        };

        // Get 24h ago kline for price change calculation
        let day_ago = latest.timestamp - chrono::Duration::hours(24);
        let old_kline = sqlx::query(
            "SELECT close FROM kline_1m WHERE symbol = $1 AND timestamp <= $2 \
             ORDER BY timestamp DESC LIMIT 1"
        )
        .bind(symbol)
        .bind(day_ago)
        .fetch_optional(&self.pool)
        .await?;

        let change_pct = old_kline.and_then(|row| {
            let old_close: Decimal = row.get("close");
            if old_close != Decimal::ZERO {
                Some((latest.close - old_close) / old_close * Decimal::from(100))
            } else {
                None
            }
        });

        // Get 24h volume, high, low
        let stats = sqlx::query(
            "SELECT SUM(volume) as vol, MAX(high) as high, MIN(low) as low \
             FROM kline_1m WHERE symbol = $1 AND timestamp >= $2"
        )
        .bind(symbol)
        .bind(day_ago)
        .fetch_one(&self.pool)
        .await?;

        Ok(Some((latest, Kline24hStats {
            change_pct,
            volume_24h: stats.get::<Option<Decimal>, _>("vol"),
            high_24h: stats.get::<Option<Decimal>, _>("high"),
            low_24h: stats.get::<Option<Decimal>, _>("low"),
        })))
    }

    /// Get the earliest kline timestamp for a symbol
    pub async fn get_kline_earliest(
        &self,
        symbol: &str,
    ) -> DataResult<Option<DateTime<Utc>>> {
        let mut query_builder = QueryBuilder::new(
            "SELECT MIN(timestamp) AS earliest FROM kline_1m WHERE symbol = "
        );
        query_builder.push_bind(symbol);

        let row = query_builder.build().fetch_optional(&self.pool).await?;

        let earliest = row.and_then(|r| r.get::<Option<DateTime<Utc>>, _>("earliest"));
        debug!("Earliest kline for {}: {:?}", symbol, earliest);
        Ok(earliest)
    }

    /// Get the latest kline timestamp for a symbol
    pub async fn get_kline_latest(
        &self,
        symbol: &str,
    ) -> DataResult<Option<DateTime<Utc>>> {
        let mut query_builder = QueryBuilder::new(
            "SELECT MAX(timestamp) AS latest FROM kline_1m WHERE symbol = "
        );
        query_builder.push_bind(symbol);

        let row = query_builder.build().fetch_optional(&self.pool).await?;

        let latest = row.and_then(|r| r.get::<Option<DateTime<Utc>>, _>("latest"));
        debug!("Latest kline for {}: {:?}", symbol, latest);
        Ok(latest)
    }

    /// Find gaps in kline data within a time range.
    /// Returns Vec<(gap_start, gap_end)> where each gap is > 2 minutes.
    pub async fn find_kline_gaps(
        &self,
        symbol: &str,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> DataResult<Vec<(DateTime<Utc>, DateTime<Utc>)>> {
        let rows = sqlx::query(
            r#"
            WITH ordered AS (
                SELECT timestamp,
                       LAG(timestamp) OVER (ORDER BY timestamp) AS prev_ts
                FROM kline_1m
                WHERE symbol = $1 AND timestamp BETWEEN $2 AND $3
            )
            SELECT prev_ts AS gap_start, timestamp AS gap_end
            FROM ordered
            WHERE prev_ts IS NOT NULL
              AND timestamp - prev_ts > INTERVAL '2 minutes'
            ORDER BY gap_start
            "#
        )
        .bind(symbol)
        .bind(start)
        .bind(end)
        .fetch_all(&self.pool)
        .await?;

        let gaps: Vec<(DateTime<Utc>, DateTime<Utc>)> = rows
            .iter()
            .filter_map(|row| {
                let gap_start: DateTime<Utc> = row.get("gap_start");
                let gap_end: DateTime<Utc> = row.get("gap_end");
                Some((gap_start, gap_end))
            })
            .collect();

        info!("Found {} gaps for {} between {} and {}", gaps.len(), symbol, start, end);
        Ok(gaps)
    }

    /// Generate OHLC data from tick data for a specific time range
    pub async fn generate_ohlc_from_ticks(
        &self,
        symbol: &str,
        timeframe: Timeframe,
        start_time: DateTime<Utc>,
        end_time: DateTime<Utc>,
        limit: Option<i64>,
    ) -> DataResult<Vec<OHLCData>> {
        debug!(
            "Generating OHLC data: {} {} from {} to {}",
            symbol,
            timeframe.as_str(),
            start_time,
            end_time
        );

        // Align start and end times to timeframe boundaries
        let aligned_start = timeframe.align_timestamp(start_time);
        let aligned_end = timeframe.align_timestamp(end_time);

        // Query all ticks in the time range
        let ticks = self
            .get_historical_data_for_backtest(
                symbol,
                aligned_start,
                aligned_end + timeframe.as_duration(), // Extend to include the last window
                limit,
            )
            .await?;

        if ticks.is_empty() {
            debug!("No ticks found for OHLC generation");
            return Ok(Vec::new());
        }

        // Group ticks by time windows
        let mut windows: HashMap<DateTime<Utc>, Vec<TickData>> = HashMap::new();

        for tick in ticks {
            let window_start = timeframe.align_timestamp(tick.timestamp);
            windows
                .entry(window_start)
                .or_insert_with(Vec::new)
                .push(tick);
        }

        // Convert each window to OHLC
        let mut ohlc_data: Vec<OHLCData> = windows
            .into_iter()
            .filter_map(|(window_start, mut window_ticks)| {
                if window_start >= aligned_start && window_start <= aligned_end {
                    // Sort ticks by timestamp within each window
                    window_ticks.sort_by(|a, b| a.timestamp.cmp(&b.timestamp));
                    OHLCData::from_ticks(&window_ticks, timeframe, window_start)
                } else {
                    None
                }
            })
            .collect();

        // Sort OHLC data by timestamp
        ohlc_data.sort_by(|a, b| a.timestamp.cmp(&b.timestamp));

        debug!(
            "Generated {} OHLC candles for {} {}",
            ohlc_data.len(),
            symbol,
            timeframe.as_str()
        );

        Ok(ohlc_data)
    }

    // Time-based query operations for OHLC generation

    /// Get ticks for a specific time duration (ordered by time ASC)
    pub async fn get_ticks_for_timespan(
        &self,
        symbol: &str,
        duration_hours: i64,
    ) -> DataResult<Vec<TickData>> {
        let end_time = Utc::now();
        let start_time = end_time - Duration::hours(duration_hours);

        let rows = sqlx::query(
            "SELECT timestamp, symbol, price, quantity, side, trade_id, is_buyer_maker \
             FROM tick_data WHERE symbol = $1 AND timestamp >= $2 AND timestamp <= $3 \
             ORDER BY timestamp ASC"
        )
        .bind(symbol)
        .bind(start_time)
        .bind(end_time)
        .fetch_all(&self.pool)
        .await?;

        let ticks: DataResult<Vec<TickData>> = rows
            .iter()
            .map(|row| {
                let side_str: String = row.get("side");
                Ok(TickData {
                    timestamp: row.get("timestamp"),
                    symbol: row.get("symbol"),
                    price: row.get("price"),
                    quantity: row.get("quantity"),
                    side: self.parse_trade_side(&side_str)?,
                    trade_id: row.get("trade_id"),
                    is_buyer_maker: row.get("is_buyer_maker"),
                })
            })
            .collect();

        ticks
    }

    /// Get ticks for a specific time duration with record limit
    pub async fn get_ticks_for_timespan_limited(
        &self,
        symbol: &str,
        duration_hours: i64,
        max_records: i64,
    ) -> DataResult<Vec<TickData>> {
        let end_time = Utc::now();
        let start_time = end_time - Duration::hours(duration_hours);
        let limit = max_records.min(MAX_QUERY_LIMIT as i64);

        let rows = sqlx::query(
            "SELECT timestamp, symbol, price, quantity, side, trade_id, is_buyer_maker \
             FROM tick_data WHERE symbol = $1 AND timestamp >= $2 AND timestamp <= $3 \
             ORDER BY timestamp ASC LIMIT $4"
        )
        .bind(symbol)
        .bind(start_time)
        .bind(end_time)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;

        let ticks: DataResult<Vec<TickData>> = rows
            .iter()
            .map(|row| {
                let side_str: String = row.get("side");
                Ok(TickData {
                    timestamp: row.get("timestamp"),
                    symbol: row.get("symbol"),
                    price: row.get("price"),
                    quantity: row.get("quantity"),
                    side: self.parse_trade_side(&side_str)?,
                    trade_id: row.get("trade_id"),
                    is_buyer_maker: row.get("is_buyer_maker"),
                })
            })
            .collect();

        ticks
    }

    /// Generate recent OHLC data for backtesting with time-based approach
    pub async fn generate_recent_ohlc_for_backtest(
        &self,
        symbol: &str,
        timeframe: Timeframe,
        candle_count: u32,
    ) -> DataResult<Vec<OHLCData>> {
        // Calculate required time duration
        let duration_hours = calculate_required_duration_hours(timeframe, candle_count);

        // Set reasonable limits for different timeframes
        let max_ticks = match timeframe {
            Timeframe::OneMinute | Timeframe::FiveMinutes => 50000,
            Timeframe::FifteenMinutes | Timeframe::ThirtyMinutes => 100000,
            Timeframe::OneHour | Timeframe::TwoHour => 200000,
            Timeframe::FourHour => 500000,
            Timeframe::OneDay | Timeframe::ThreeDay => 1000000,
            Timeframe::OneWeek => 2000000,
        };

        // Get ticks for the calculated time duration
        let recent_ticks = self
            .get_ticks_for_timespan_limited(symbol, duration_hours, max_ticks)
            .await?;

        if recent_ticks.is_empty() {
            return Ok(Vec::new());
        }

        // Use actual data time range for OHLC generation
        let start_time = recent_ticks[0].timestamp;
        let end_time = recent_ticks[recent_ticks.len() - 1].timestamp;

        // Generate OHLC data from tick data
        let mut ohlc_data = self
            .generate_ohlc_from_ticks(symbol, timeframe, start_time, end_time, None)
            .await?;

        // Sort by timestamp descending and take requested count
        ohlc_data.sort_by(|a, b| b.timestamp.cmp(&a.timestamp));
        ohlc_data.truncate(candle_count as usize);
        ohlc_data.reverse(); // Return in chronological order

        Ok(ohlc_data)
    }

    /// Get OHLC data statistics for a symbol
    pub async fn get_ohlc_data_info(
        &self,
        symbol: &str,
        timeframe: Timeframe,
    ) -> DataResult<(u64, Option<DateTime<Utc>>, Option<DateTime<Utc>>)> {
        // Get basic tick data info first
        let stats = self.get_db_stats(Some(symbol)).await?;

        if let (Some(earliest), Some(latest)) = (stats.earliest_timestamp, stats.latest_timestamp) {
            // Align to timeframe boundaries
            let aligned_earliest = timeframe.align_timestamp(earliest);
            let aligned_latest = timeframe.align_timestamp(latest);

            // Calculate approximate number of candles
            let duration_diff = aligned_latest - aligned_earliest;
            let timeframe_duration = timeframe.as_duration();

            let estimated_candles = if timeframe_duration.num_seconds() > 0 {
                (duration_diff.num_seconds() / timeframe_duration.num_seconds()) as u64
            } else {
                0
            };

            Ok((
                estimated_candles,
                Some(aligned_earliest),
                Some(aligned_latest),
            ))
        } else {
            Ok((0, None, None))
        }
    }
}

// =================================================================
// 策略信号生命周期管理
// =================================================================

/// 通用信号数据结构（两表共用）
#[derive(Debug, Clone)]
pub struct SignalRecord {
    pub id: uuid::Uuid,
    pub symbol: String,
    pub strategy_id: String,
    pub direction: String,
    pub entry_price: Decimal,
    pub overall_confidence: Decimal,
    pub entry_allowed: bool,
    pub entry_direction: Option<String>,
    pub timeframe_details: serde_json::Value,
    pub status: String,
    pub closed_reason: Option<String>,
    pub evaluated_at: Option<DateTime<Utc>>,
    pub best_price: Option<Decimal>,
    pub worst_price: Option<Decimal>,
    pub eval_count: i32,
    pub closed_at: Option<DateTime<Utc>>,
    pub close_price: Option<Decimal>,
    pub actual_return_pct: Option<Decimal>,
    pub created_at: DateTime<Utc>,
}

/// 信号统计数据
#[derive(Debug, Clone)]
pub struct SignalStatsData {
    pub total_signals: i64,
    pub confirmed: i64,
    pub invalidated: i64,
    pub expired: i64,
    pub superseded: i64,
    pub pending: i64,
    pub confirmation_rate_pct: Option<Decimal>,
    pub avg_return_pct: Option<Decimal>,
    pub avg_duration_hours: Option<f64>,
}

// -----------------------------------------------------------------
// strategy_signals（引擎表）公共方法
// -----------------------------------------------------------------

impl TickDataRepository {
    /// 保存引擎信号
    pub async fn save_engine_signal(
        &self,
        symbol: &str,
        strategy_id: &str,
        direction: &str,
        entry_price: Decimal,
        overall_confidence: Decimal,
        entry_allowed: bool,
        entry_direction: Option<&str>,
        timeframe_details: serde_json::Value,
    ) -> DataResult<uuid::Uuid> {
        let id = uuid::Uuid::new_v4();
        sqlx::query(
            "INSERT INTO strategy_signals \
             (id, symbol, strategy_id, direction, entry_price, \
              overall_confidence, entry_allowed, entry_direction, \
              timeframe_details, status, evaluated_at, eval_count, best_price, worst_price) \
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,'pending',NOW(),1,$10,$10)"
        )
        .bind(id).bind(symbol).bind(strategy_id).bind(direction)
        .bind(entry_price).bind(overall_confidence).bind(entry_allowed)
        .bind(entry_direction).bind(timeframe_details).bind(entry_price)
        .execute(&self.pool).await?;
        debug!("Saved engine signal: id={}, {} {}", id, symbol, direction);
        Ok(id)
    }

    /// 获取引擎待验证信号
    pub async fn get_pending_engine_signal(
        &self, symbol: &str, strategy_id: &str,
    ) -> DataResult<Option<SignalRecord>> {
        let row = sqlx::query(
            "SELECT id,symbol,strategy_id,direction,entry_price,overall_confidence, \
                    entry_allowed,entry_direction,timeframe_details,status,closed_reason, \
                    evaluated_at,best_price,worst_price,eval_count, \
                    closed_at,close_price,actual_return_pct,created_at \
             FROM strategy_signals \
             WHERE symbol=$1 AND strategy_id=$2 AND status='pending' \
             ORDER BY created_at DESC LIMIT 1"
        ).bind(symbol).bind(strategy_id)
         .fetch_optional(&self.pool).await?;
        Ok(row.map(|r| parse_signal_row(&r)))
    }

    /// 更新引擎信号验证
    pub async fn update_engine_signal_eval(
        &self, signal_id: uuid::Uuid, current_price: Decimal,
    ) -> DataResult<()> {
        sqlx::query(
            "UPDATE strategy_signals SET \
             evaluated_at=NOW(), eval_count=eval_count+1, \
             best_price=CASE WHEN direction='bullish' THEN GREATEST(COALESCE(best_price,$2),$2) \
                              WHEN direction='bearish' THEN LEAST(COALESCE(best_price,$2),$2) \
                              ELSE best_price END, \
             worst_price=CASE WHEN direction='bullish' THEN LEAST(COALESCE(worst_price,$2),$2) \
                               WHEN direction='bearish' THEN GREATEST(COALESCE(worst_price,$2),$2) \
                               ELSE worst_price END \
             WHERE id=$1"
        ).bind(signal_id).bind(current_price)
         .execute(&self.pool).await?;
        Ok(())
    }

    /// 关闭引擎信号
    pub async fn close_engine_signal(
        &self, signal_id: uuid::Uuid, new_status: &str, reason: &str,
        close_price: Decimal, return_pct: Decimal,
    ) -> DataResult<()> {
        sqlx::query(
            "UPDATE strategy_signals SET status=$2,closed_reason=$3, \
             closed_at=NOW(),close_price=$4,actual_return_pct=$5 \
             WHERE id=$1 AND status='pending'"
        ).bind(signal_id).bind(new_status).bind(reason)
         .bind(close_price).bind(return_pct)
         .execute(&self.pool).await?;
        debug!("Closed engine signal: {} -> {} ({}%)", signal_id, new_status, return_pct);
        Ok(())
    }

    /// 关闭引擎过期信号
    pub async fn close_expired_engine_signals(&self, max_age_hours: i64) -> DataResult<u64> {
        let r = sqlx::query(
            "UPDATE strategy_signals SET status='expired',closed_reason='expired', \
             closed_at=NOW(),close_price=entry_price,actual_return_pct=0 \
             WHERE status='pending' AND created_at < NOW() - INTERVAL '1 hour' * $1"
        ).bind(max_age_hours).execute(&self.pool).await?;
        let n = r.rows_affected();
        if n > 0 { info!("Closed {} expired engine signals ({}h)", n, max_age_hours); }
        Ok(n)
    }

    /// 查询引擎信号历史
    pub async fn get_engine_signal_history(
        &self, symbol: Option<&str>, strategy_id: Option<&str>, limit: i32,
    ) -> DataResult<Vec<SignalRecord>> {
        const SIGNAL_COLUMNS: &str = "id,symbol,strategy_id,direction,entry_price,overall_confidence,entry_allowed,entry_direction,timeframe_details,status,closed_reason,evaluated_at,best_price,worst_price,eval_count,closed_at,close_price,actual_return_pct,created_at";

        let rows = match (symbol, strategy_id) {
            (Some(sym), Some(sid)) => sqlx::query(
                SIGNAL_SELECT_QUERY
            ).bind(sym).bind(sid).bind(limit).fetch_all(&self.pool).await?,
            (Some(sym), None) => sqlx::query(
                &format!("SELECT {} FROM strategy_signals WHERE symbol=$1 ORDER BY created_at DESC LIMIT $2", SIGNAL_COLUMNS)
            ).bind(sym).bind(limit).fetch_all(&self.pool).await?,
            _ => sqlx::query(
                &format!("SELECT {} FROM strategy_signals ORDER BY created_at DESC LIMIT $1", SIGNAL_COLUMNS)
            ).bind(limit).fetch_all(&self.pool).await?,
        };
        Ok(rows.iter().map(|r| parse_signal_row(r)).collect())
    }
}

const SIGNAL_SELECT_QUERY: &str = "SELECT id,symbol,strategy_id,direction,entry_price,overall_confidence,entry_allowed,entry_direction,timeframe_details,status,closed_reason,evaluated_at,best_price,worst_price,eval_count,closed_at,close_price,actual_return_pct,created_at FROM strategy_signals WHERE symbol=$1 AND strategy_id=$2 ORDER BY created_at DESC LIMIT $3";

// -----------------------------------------------------------------
// strategy_analysis_log（前端表）方法
// -----------------------------------------------------------------

impl TickDataRepository {
    /// 保存前端分析记录
    pub async fn save_analysis_log(
        &self,
        symbol: &str,
        strategy_id: &str,
        direction: &str,
        entry_price: Decimal,
        overall_confidence: Decimal,
        entry_allowed: bool,
        entry_direction: Option<&str>,
        timeframe_details: serde_json::Value,
    ) -> DataResult<uuid::Uuid> {
        let id = uuid::Uuid::new_v4();
        sqlx::query(
            "INSERT INTO strategy_analysis_log \
             (id,symbol,strategy_id,direction,entry_price, \
              overall_confidence,entry_allowed,entry_direction, \
              timeframe_details,status,evaluated_at,eval_count,best_price,worst_price) \
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,'pending',NOW(),1,$10,$10)"
        )
        .bind(id).bind(symbol).bind(strategy_id).bind(direction)
        .bind(entry_price).bind(overall_confidence).bind(entry_allowed)
        .bind(entry_direction).bind(timeframe_details).bind(entry_price)
        .execute(&self.pool).await?;
        debug!("Saved analysis log: id={}, {} {}", id, symbol, direction);
        Ok(id)
    }

    /// 获取前端待验证分析
    pub async fn get_pending_analysis(
        &self, symbol: &str, strategy_id: &str,
    ) -> DataResult<Option<SignalRecord>> {
        let row = sqlx::query(
            "SELECT id,symbol,strategy_id,direction,entry_price,overall_confidence, \
                    entry_allowed,entry_direction,timeframe_details,status,closed_reason, \
                    evaluated_at,best_price,worst_price,eval_count, \
                    closed_at,close_price,actual_return_pct,created_at \
             FROM strategy_analysis_log \
             WHERE symbol=$1 AND strategy_id=$2 AND status='pending' \
             ORDER BY created_at DESC LIMIT 1"
        ).bind(symbol).bind(strategy_id)
         .fetch_optional(&self.pool).await?;
        Ok(row.map(|r| parse_signal_row(&r)))
    }

    /// 更新前端分析验证
    pub async fn update_analysis_eval(
        &self, log_id: uuid::Uuid, current_price: Decimal,
    ) -> DataResult<()> {
        sqlx::query(
            "UPDATE strategy_analysis_log SET \
             evaluated_at=NOW(), eval_count=eval_count+1, \
             best_price=CASE WHEN direction='bullish' THEN GREATEST(COALESCE(best_price,$2),$2) \
                              WHEN direction='bearish' THEN LEAST(COALESCE(best_price,$2),$2) \
                              ELSE best_price END, \
             worst_price=CASE WHEN direction='bullish' THEN LEAST(COALESCE(worst_price,$2),$2) \
                               WHEN direction='bearish' THEN GREATEST(COALESCE(worst_price,$2),$2) \
                               ELSE worst_price END \
             WHERE id=$1"
        ).bind(log_id).bind(current_price)
         .execute(&self.pool).await?;
        Ok(())
    }

    /// 关闭前端分析
    pub async fn close_analysis(
        &self, log_id: uuid::Uuid, new_status: &str, reason: &str,
        close_price: Decimal, return_pct: Decimal,
    ) -> DataResult<()> {
        sqlx::query(
            "UPDATE strategy_analysis_log SET status=$2,closed_reason=$3, \
             closed_at=NOW(),close_price=$4,actual_return_pct=$5 \
             WHERE id=$1 AND status='pending'"
        ).bind(log_id).bind(new_status).bind(reason)
         .bind(close_price).bind(return_pct)
         .execute(&self.pool).await?;
        debug!("Closed analysis log: {} -> {} ({}%)", log_id, new_status, return_pct);
        Ok(())
    }

    /// 关闭前端过期分析
    pub async fn close_expired_analysis(&self, max_age_hours: i64) -> DataResult<u64> {
        let r = sqlx::query(
            "UPDATE strategy_analysis_log SET status='expired',closed_reason='expired', \
             closed_at=NOW(),close_price=entry_price,actual_return_pct=0 \
             WHERE status='pending' AND created_at < NOW() - INTERVAL '1 hour' * $1"
        ).bind(max_age_hours).execute(&self.pool).await?;
        let n = r.rows_affected();
        if n > 0 { info!("Closed {} expired analysis logs ({}h)", n, max_age_hours); }
        Ok(n)
    }

    /// 查询前端分析历史
    pub async fn get_analysis_history(
        &self, symbol: Option<&str>, strategy_id: Option<&str>, limit: i32,
    ) -> DataResult<Vec<SignalRecord>> {
        let rows = match (symbol, strategy_id) {
            (Some(sym), Some(sid)) => sqlx::query(
                "SELECT id,symbol,strategy_id,direction,entry_price,overall_confidence, \
                        entry_allowed,entry_direction,timeframe_details,status,closed_reason, \
                        evaluated_at,best_price,worst_price,eval_count, \
                        closed_at,close_price,actual_return_pct,created_at \
                 FROM strategy_analysis_log WHERE symbol=$1 AND strategy_id=$2 \
                 ORDER BY created_at DESC LIMIT $3"
            ).bind(sym).bind(sid).bind(limit).fetch_all(&self.pool).await?,
            (Some(sym), None) => sqlx::query(
                "SELECT id,symbol,strategy_id,direction,entry_price,overall_confidence, \
                        entry_allowed,entry_direction,timeframe_details,status,closed_reason, \
                        evaluated_at,best_price,worst_price,eval_count, \
                        closed_at,close_price,actual_return_pct,created_at \
                 FROM strategy_analysis_log WHERE symbol=$1 \
                 ORDER BY created_at DESC LIMIT $2"
            ).bind(sym).bind(limit).fetch_all(&self.pool).await?,
            _ => sqlx::query(
                "SELECT id,symbol,strategy_id,direction,entry_price,overall_confidence, \
                        entry_allowed,entry_direction,timeframe_details,status,closed_reason, \
                        evaluated_at,best_price,worst_price,eval_count, \
                        closed_at,close_price,actual_return_pct,created_at \
                 FROM strategy_analysis_log ORDER BY created_at DESC LIMIT $1"
            ).bind(limit).fetch_all(&self.pool).await?,
        };
        Ok(rows.iter().map(|r| parse_signal_row(r)).collect())
    }

    /// 获取分析统计（通用，可用于引擎或前端表）
    pub async fn get_signal_stats(
        &self, table: &str, symbol: Option<&str>, strategy_id: Option<&str>,
    ) -> DataResult<SignalStatsData> {
        // table 白名单防注入
        let table = match table {
            "strategy_signals" | "strategy_analysis_log" => table,
            _ => return Err(DataError::InvalidFormat(format!("Invalid table: {}", table))),
        };

        let (where_clause, binds): (String, Vec<&str>) = match (symbol, strategy_id) {
            (Some(sym), Some(sid)) => (format!("WHERE symbol=$1 AND strategy_id=$2"), vec![sym, sid]),
            (Some(sym), None) => ("WHERE symbol=$1".into(), vec![sym]),
            _ => (String::new(), vec![]),
        };

        let sql = format!(
            "SELECT COUNT(*) as total, \
                    COUNT(*) FILTER (WHERE status='confirmed') as confirmed, \
                    COUNT(*) FILTER (WHERE status='invalidated') as invalidated, \
                    COUNT(*) FILTER (WHERE status='expired') as expired, \
                    COUNT(*) FILTER (WHERE status='superseded') as superseded, \
                    COUNT(*) FILTER (WHERE status='pending') as pending, \
                    ROUND(COUNT(*) FILTER (WHERE status='confirmed')::numeric / \
                          NULLIF(COUNT(*) FILTER (WHERE status IN ('confirmed','invalidated')),0)*100,2) as confirm_rate, \
                    AVG(actual_return_pct) FILTER (WHERE status IN ('confirmed','invalidated')) as avg_return, \
                    AVG(EXTRACT(EPOCH FROM (closed_at-created_at))/3600) FILTER (WHERE closed_at IS NOT NULL)::float8 as avg_hours \
             FROM {} {}", table, where_clause
        );

        let row = if binds.len() == 2 {
            sqlx::query(&sql).bind(binds[0]).bind(binds[1]).fetch_one(&self.pool).await?
        } else if binds.len() == 1 {
            sqlx::query(&sql).bind(binds[0]).fetch_one(&self.pool).await?
        } else {
            sqlx::query(&sql).fetch_one(&self.pool).await?
        };

        Ok(SignalStatsData {
            total_signals: row.get::<Option<i64>, _>("total").unwrap_or(0),
            confirmed: row.get::<Option<i64>, _>("confirmed").unwrap_or(0),
            invalidated: row.get::<Option<i64>, _>("invalidated").unwrap_or(0),
            expired: row.get::<Option<i64>, _>("expired").unwrap_or(0),
            superseded: row.get::<Option<i64>, _>("superseded").unwrap_or(0),
            pending: row.get::<Option<i64>, _>("pending").unwrap_or(0),
            confirmation_rate_pct: row.get::<Option<Decimal>, _>("confirm_rate"),
            avg_return_pct: row.get::<Option<Decimal>, _>("avg_return"),
            avg_duration_hours: row.get::<Option<f64>, _>("avg_hours"),
        })
    }
}

/// 解析信号行（两表结构相同，共用解析函数）
fn parse_signal_row(row: &sqlx::postgres::PgRow) -> SignalRecord {
    SignalRecord {
        id: row.get("id"),
        symbol: row.get("symbol"),
        strategy_id: row.get("strategy_id"),
        direction: row.get("direction"),
        entry_price: row.get("entry_price"),
        overall_confidence: row.get("overall_confidence"),
        entry_allowed: row.get("entry_allowed"),
        entry_direction: row.get("entry_direction"),
        timeframe_details: row.get("timeframe_details"),
        status: row.get("status"),
        closed_reason: row.get("closed_reason"),
        evaluated_at: row.get("evaluated_at"),
        best_price: row.get("best_price"),
        worst_price: row.get("worst_price"),
        eval_count: row.get("eval_count"),
        closed_at: row.get("closed_at"),
        close_price: row.get("close_price"),
        actual_return_pct: row.get("actual_return_pct"),
        created_at: row.get("created_at"),
    }
}

// =================================================================
// symbol_config 交易对管理
// =================================================================

impl TickDataRepository {
    /// 获取所有启用的交易对
    pub async fn get_enabled_symbols(&self) -> DataResult<Vec<String>> {
        let rows = sqlx::query("SELECT symbol FROM symbol_config WHERE enabled = true ORDER BY symbol")
            .fetch_all(&self.pool).await?;
        Ok(rows.iter().map(|r| r.get::<String, _>("symbol")).collect())
    }

    /// 获取所有交易对（含启用状态）
    pub async fn get_all_symbols(&self) -> DataResult<Vec<(String, bool)>> {
        let rows = sqlx::query("SELECT symbol, enabled FROM symbol_config ORDER BY symbol")
            .fetch_all(&self.pool).await?;
        Ok(rows.iter().map(|r| (r.get::<String, _>("symbol"), r.get::<bool, _>("enabled"))).collect())
    }

    /// 添加交易对
    pub async fn add_symbol(&self, symbol: &str) -> DataResult<()> {
        sqlx::query("INSERT INTO symbol_config (symbol) VALUES ($1) ON CONFLICT (symbol) DO UPDATE SET enabled = true")
            .bind(symbol).execute(&self.pool).await?;
        info!("Added symbol: {}", symbol);
        Ok(())
    }

    /// 删除交易对
    pub async fn remove_symbol(&self, symbol: &str) -> DataResult<()> {
        sqlx::query("DELETE FROM symbol_config WHERE symbol = $1")
            .bind(symbol).execute(&self.pool).await?;
        info!("Removed symbol: {}", symbol);
        Ok(())
    }

    /// 启用/禁用交易对
    pub async fn set_symbol_enabled(&self, symbol: &str, enabled: bool) -> DataResult<()> {
        sqlx::query("UPDATE symbol_config SET enabled = $2 WHERE symbol = $1")
            .bind(symbol).bind(enabled).execute(&self.pool).await?;
        info!("Symbol {} enabled={}", symbol, enabled);
        Ok(())
    }
}

// =================================================================
// 系统配置操作
// =================================================================

impl TickDataRepository {
    /// 获取系统配置值
    pub async fn get_system_config(&self, key: &str) -> DataResult<Option<String>> {
        let row = sqlx::query("SELECT value FROM system_config WHERE key = $1")
            .bind(key)
            .fetch_optional(&self.pool)
            .await?;
        Ok(row.map(|r| r.get::<String, _>("value")))
    }

    /// 设置系统配置值
    pub async fn set_system_config(&self, key: &str, value: &str) -> DataResult<()> {
        sqlx::query(
            "INSERT INTO system_config (key, value, updated_at) VALUES ($1, $2, NOW()) \
             ON CONFLICT (key) DO UPDATE SET value = $2, updated_at = NOW()"
        )
        .bind(key)
        .bind(value)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// 获取调度器是否暂停
    pub async fn is_scheduler_paused(&self) -> DataResult<bool> {
        let value = self.get_system_config("scheduler_paused").await?;
        Ok(value.map(|v| v == "true").unwrap_or(false))
    }

    /// 设置调度器暂停状态
    pub async fn set_scheduler_paused(&self, paused: bool) -> DataResult<()> {
        self.set_system_config("scheduler_paused", &paused.to_string()).await?;
        info!("Scheduler paused={}", paused);
        Ok(())
    }

    // =================================================================
    // Market Sentiment Data (资金费率/持仓量/多空比)
    // =================================================================

    /// 插入资金费率数据
    pub async fn insert_funding_rate(
        &self,
        symbol: &str,
        funding_rate: Decimal,
        funding_time: DateTime<Utc>,
        mark_price: Option<Decimal>,
    ) -> DataResult<()> {
        sqlx::query(
            "INSERT INTO funding_rate (symbol, funding_rate, funding_time, mark_price) \
             VALUES ($1, $2, $3, $4) \
             ON CONFLICT (symbol, funding_time) DO UPDATE SET \
             funding_rate = EXCLUDED.funding_rate, mark_price = EXCLUDED.mark_price"
        )
        .bind(symbol)
        .bind(funding_rate)
        .bind(funding_time)
        .bind(mark_price)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// 获取最新资金费率
    pub async fn get_latest_funding_rate(
        &self,
        symbol: &str,
    ) -> DataResult<Option<(Decimal, DateTime<Utc>)>> {
        let row = sqlx::query(
            "SELECT funding_rate, funding_time FROM funding_rate \
             WHERE symbol = $1 ORDER BY funding_time DESC LIMIT 1"
        )
        .bind(symbol)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(|r| (r.get("funding_rate"), r.get("funding_time"))))
    }

    /// 插入持仓量数据
    pub async fn insert_open_interest(
        &self,
        symbol: &str,
        open_interest: Decimal,
        open_value: Option<Decimal>,
        timestamp: DateTime<Utc>,
    ) -> DataResult<()> {
        sqlx::query(
            "INSERT INTO open_interest (symbol, open_interest, open_value, timestamp) \
             VALUES ($1, $2, $3, $4) \
             ON CONFLICT (symbol, timestamp) DO UPDATE SET \
             open_interest = EXCLUDED.open_interest, open_value = EXCLUDED.open_value"
        )
        .bind(symbol)
        .bind(open_interest)
        .bind(open_value)
        .bind(timestamp)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// 获取最新持仓量
    pub async fn get_latest_open_interest(
        &self,
        symbol: &str,
    ) -> DataResult<Option<(Decimal, Option<Decimal>, DateTime<Utc>)>> {
        let row = sqlx::query(
            "SELECT open_interest, open_value, timestamp FROM open_interest \
             WHERE symbol = $1 ORDER BY timestamp DESC LIMIT 1"
        )
        .bind(symbol)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(|r| (r.get("open_interest"), r.get("open_value"), r.get("timestamp"))))
    }

    /// 插入多空比数据
    pub async fn insert_long_short_ratio(
        &self,
        symbol: &str,
        long_ratio: Decimal,
        short_ratio: Decimal,
        ratio: Decimal,
        timestamp: DateTime<Utc>,
    ) -> DataResult<()> {
        sqlx::query(
            "INSERT INTO long_short_ratio (symbol, long_ratio, short_ratio, ratio, timestamp) \
             VALUES ($1, $2, $3, $4, $5) \
             ON CONFLICT (symbol, timestamp) DO UPDATE SET \
             long_ratio = EXCLUDED.long_ratio, short_ratio = EXCLUDED.short_ratio, ratio = EXCLUDED.ratio"
        )
        .bind(symbol)
        .bind(long_ratio)
        .bind(short_ratio)
        .bind(ratio)
        .bind(timestamp)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// 获取最新多空比
    pub async fn get_latest_long_short_ratio(
        &self,
        symbol: &str,
    ) -> DataResult<Option<(Decimal, Decimal, Decimal, DateTime<Utc>)>> {
        let row = sqlx::query(
            "SELECT long_ratio, short_ratio, ratio, timestamp FROM long_short_ratio \
             WHERE symbol = $1 ORDER BY timestamp DESC LIMIT 1"
        )
        .bind(symbol)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(|r| (r.get("long_ratio"), r.get("short_ratio"), r.get("ratio"), r.get("timestamp"))))
    }

    // =================================================================
    // Account Snapshot v2 (统一账户快照)
    // =================================================================

    /// 插入账户快照
    pub async fn insert_account_snapshot(
        &self,
        exchange: &str,
        market_type: &str,
        total_equity: Decimal,
        total_balance: Decimal,
        available_balance: Decimal,
        frozen_balance: Decimal,
        unrealized_pnl: Decimal,
        initial_margin: Option<Decimal>,
        maint_margin: Option<Decimal>,
        margin_ratio: Option<Decimal>,
        position_count: i32,
        raw_data: Option<serde_json::Value>,
    ) -> DataResult<()> {
        sqlx::query(
            "INSERT INTO account_snapshot \
             (exchange, market_type, snapshot_at, total_equity, total_balance, \
              available_balance, frozen_balance, unrealized_pnl, \
              initial_margin, maint_margin, margin_ratio, position_count, raw_data) \
             VALUES ($1, $2, NOW(), $3, $4, $5, $6, $7, $8, $9, $10, $11, $12) \
             ON CONFLICT (exchange, market_type, snapshot_at) DO UPDATE SET \
              total_equity = EXCLUDED.total_equity, \
              total_balance = EXCLUDED.total_balance, \
              available_balance = EXCLUDED.available_balance, \
              frozen_balance = EXCLUDED.frozen_balance, \
              unrealized_pnl = EXCLUDED.unrealized_pnl, \
              initial_margin = EXCLUDED.initial_margin, \
              maint_margin = EXCLUDED.maint_margin, \
              margin_ratio = EXCLUDED.margin_ratio, \
              position_count = EXCLUDED.position_count, \
              raw_data = EXCLUDED.raw_data"
        )
        .bind(exchange)
        .bind(market_type)
        .bind(total_equity)
        .bind(total_balance)
        .bind(available_balance)
        .bind(frozen_balance)
        .bind(unrealized_pnl)
        .bind(initial_margin)
        .bind(maint_margin)
        .bind(margin_ratio)
        .bind(position_count)
        .bind(raw_data)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// 插入资产余额
    pub async fn insert_asset_balance(
        &self,
        exchange: &str,
        market_type: &str,
        asset: &str,
        total: Decimal,
        available: Decimal,
        frozen: Decimal,
        unrealized_pnl: Decimal,
        usd_value: Option<Decimal>,
    ) -> DataResult<()> {
        sqlx::query(
            "INSERT INTO asset_balance \
             (exchange, market_type, asset, snapshot_at, total, available, frozen, unrealized_pnl, usd_value) \
             VALUES ($1, $2, $3, NOW(), $4, $5, $6, $7, $8) \
             ON CONFLICT (exchange, market_type, asset, snapshot_at) DO UPDATE SET \
              total = EXCLUDED.total, \
              available = EXCLUDED.available, \
              frozen = EXCLUDED.frozen, \
              unrealized_pnl = EXCLUDED.unrealized_pnl, \
              usd_value = EXCLUDED.usd_value"
        )
        .bind(exchange)
        .bind(market_type)
        .bind(asset)
        .bind(total)
        .bind(available)
        .bind(frozen)
        .bind(unrealized_pnl)
        .bind(usd_value)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// 插入持仓快照
    pub async fn insert_position_snapshot(
        &self,
        exchange: &str,
        symbol: &str,
        raw_symbol: &str,
        position_side: &str,
        position_amt: Decimal,
        entry_price: Decimal,
        mark_price: Decimal,
        unrealized_pnl: Decimal,
        leverage: i32,
        margin_type: &str,
        initial_margin: Decimal,
        maint_margin: Decimal,
        liquidation_price: Option<Decimal>,
        notional: Decimal,
        pnl_ratio: Option<Decimal>,
        raw_data: Option<serde_json::Value>,
    ) -> DataResult<()> {
        sqlx::query(
            "INSERT INTO position_snapshot \
             (exchange, symbol, raw_symbol, snapshot_at, position_side, position_amt, \
              entry_price, mark_price, unrealized_pnl, leverage, margin_type, \
              initial_margin, maint_margin, liquidation_price, notional, pnl_ratio, raw_data) \
             VALUES ($1, $2, $3, NOW(), $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16) \
             ON CONFLICT (exchange, symbol, position_side, snapshot_at) DO UPDATE SET \
              position_amt = EXCLUDED.position_amt, \
              entry_price = EXCLUDED.entry_price, \
              mark_price = EXCLUDED.mark_price, \
              unrealized_pnl = EXCLUDED.unrealized_pnl, \
              leverage = EXCLUDED.leverage, \
              margin_type = EXCLUDED.margin_type, \
              initial_margin = EXCLUDED.initial_margin, \
              maint_margin = EXCLUDED.maint_margin, \
              liquidation_price = EXCLUDED.liquidation_price, \
              notional = EXCLUDED.notional, \
              pnl_ratio = EXCLUDED.pnl_ratio, \
              raw_data = EXCLUDED.raw_data"
        )
        .bind(exchange)
        .bind(symbol)
        .bind(raw_symbol)
        .bind(position_side)
        .bind(position_amt)
        .bind(entry_price)
        .bind(mark_price)
        .bind(unrealized_pnl)
        .bind(leverage)
        .bind(margin_type)
        .bind(initial_margin)
        .bind(maint_margin)
        .bind(liquidation_price)
        .bind(notional)
        .bind(pnl_ratio)
        .bind(raw_data)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// 获取最新账户快照
    pub async fn get_latest_account_snapshot(
        &self,
        exchange: &str,
        market_type: &str,
    ) -> DataResult<Option<crate::data::account_types::AccountSnapshot>> {
        let row = sqlx::query(
            "SELECT * FROM account_snapshot \
             WHERE exchange = $1 AND market_type = $2 \
             ORDER BY snapshot_at DESC LIMIT 1"
        )
        .bind(exchange)
        .bind(market_type)
        .fetch_optional(&self.pool)
        .await?;

        Ok(row.map(|r| crate::data::account_types::AccountSnapshot {
            exchange: r.get("exchange"),
            market_type: r.get("market_type"),
            snapshot_at: r.get("snapshot_at"),
            total_equity: r.get("total_equity"),
            total_balance: r.get("total_balance"),
            available_balance: r.get("available_balance"),
            frozen_balance: r.get("frozen_balance"),
            unrealized_pnl: r.get("unrealized_pnl"),
            initial_margin: r.get("initial_margin"),
            maint_margin: r.get("maint_margin"),
            margin_ratio: r.get("margin_ratio"),
            position_count: r.get("position_count"),
            raw_data: r.get("raw_data"),
        }))
    }

    /// 获取最新持仓列表
    pub async fn get_latest_positions(
        &self,
        exchange: &str,
    ) -> DataResult<Vec<crate::data::account_types::PositionInfo>> {
        // 获取最新的快照时间
        let latest_time: Option<DateTime<Utc>> = sqlx::query_scalar(
            "SELECT MAX(snapshot_at) FROM position_snapshot WHERE exchange = $1"
        )
        .bind(exchange)
        .fetch_one(&self.pool)
        .await?;

        let snapshot_at = match latest_time {
            Some(t) => t,
            None => return Ok(Vec::new()),
        };

        let rows = sqlx::query(
            "SELECT * FROM position_snapshot \
             WHERE exchange = $1 AND snapshot_at = $2 \
             ORDER BY symbol"
        )
        .bind(exchange)
        .bind(snapshot_at)
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.iter().map(|r| crate::data::account_types::PositionInfo {
            exchange: r.get("exchange"),
            symbol: r.get("symbol"),
            raw_symbol: r.get("raw_symbol"),
            snapshot_at: r.get("snapshot_at"),
            position_side: crate::data::account_types::PositionSide::from_str(
                r.get::<&str, _>("position_side")
            ),
            position_amt: r.get("position_amt"),
            entry_price: r.get("entry_price"),
            mark_price: r.get("mark_price"),
            unrealized_pnl: r.get("unrealized_pnl"),
            leverage: r.get::<i32, _>("leverage") as u32,
            margin_type: crate::data::account_types::MarginType::from_str(
                r.get::<&str, _>("margin_type")
            ),
            initial_margin: r.get("initial_margin"),
            maint_margin: r.get("maint_margin"),
            liquidation_price: r.get("liquidation_price"),
            notional: r.get("notional"),
            raw_data: r.get("raw_data"),
        }).collect())
    }

    /// 清理旧的账户快照
    pub async fn cleanup_old_account_snapshots(&self) -> DataResult<u64> {
        let result = sqlx::query(
            "SELECT cleanup_old_account_snapshots()"
        )
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected())
    }
}

/// Calculate required time duration based on timeframe and candle count
fn calculate_required_duration_hours(timeframe: Timeframe, candle_count: u32) -> i64 {
    let base_hours = match timeframe {
        Timeframe::OneMinute => 1,
        Timeframe::FiveMinutes => 1,
        Timeframe::FifteenMinutes => 1,
        Timeframe::ThirtyMinutes => 1,
        Timeframe::OneHour => 1,
        Timeframe::TwoHour => 2,
        Timeframe::FourHour => 4,
        Timeframe::OneDay => 24,
        Timeframe::ThreeDay => 24 * 3,
        Timeframe::OneWeek => 24 * 7,
    };

    // Add 20% buffer for data gaps
    let total_hours = (base_hours * candle_count as i64) as f64 * 1.2;
    total_hours.ceil() as i64
}

/// 获取高时间框架对应的表名
///
/// 支持的时间框架：5m, 15m, 30m, 1h, 2h, 4h, 1d, 3d, 1w
fn get_high_tf_table_name(timeframe: &str) -> DataResult<String> {
    match timeframe {
        "5m" => Ok("kline_5m".to_string()),
        "15m" => Ok("kline_15m".to_string()),
        "30m" => Ok("kline_30m".to_string()),
        "1h" => Ok("kline_1h".to_string()),
        "2h" => Ok("kline_2h".to_string()),
        "4h" => Ok("kline_4h".to_string()),
        "1d" => Ok("kline_1d".to_string()),
        "3d" => Ok("kline_3d".to_string()),
        "1w" => Ok("kline_1w".to_string()),
        _ => Err(DataError::Validation(format!(
            "Unsupported high timeframe: {}. Supported: 5m, 15m, 30m, 1h, 2h, 4h, 1d, 3d, 1w",
            timeframe
        ))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{Duration, Utc};
    use dotenv::dotenv;
    use rust_decimal::Decimal;
    use std::env;
    use std::str::FromStr;

    fn create_test_tick(
        symbol: &str,
        price: &str,
        trade_id: &str,
        timestamp: Option<DateTime<Utc>>,
    ) -> TickData {
        TickData::new(
            timestamp.unwrap_or_else(Utc::now),
            symbol.to_string(),
            Decimal::from_str(price).unwrap(),
            Decimal::from_str("1.0").unwrap(),
            TradeSide::Buy,
            trade_id.to_string(),
            false,
        )
    }

    async fn create_repository() -> TickDataRepository {
        dotenv().ok();
        let database_url = env::var("DATABASE_URL").expect("DATABASE_URL must be set in .env file");
        let redis_url = env::var("REDIS_URL").expect("REDIS_URL must be set in .env file");
        let pool = PgPool::connect(&database_url)
            .await
            .expect("Failed to connect to database");
        let cache = TieredCache::new((100, 300), (&redis_url, 1000, 3600))
            .await
            .expect("Failed to create cache");
        TickDataRepository::new(pool, cache)
    }

    async fn cleanup_database(pool: &PgPool, symbol: &str) {
        sqlx::query("DELETE FROM tick_data WHERE symbol = $1")
            .bind(symbol)
            .execute(pool)
            .await
            .expect("Failed to clean up database");
    }

    #[tokio::test]
    async fn test_insert_and_read_single_tick() {
        let repo = create_repository().await;
        let pool = repo.get_pool();
        let symbol = "BTCUSDT_TEST_SINGLE";

        // Clean up before test
        cleanup_database(pool, symbol).await;

        // Insert a single tick
        let tick = create_test_tick(symbol, "50000.0", "test1", None);
        repo.insert_tick(&tick)
            .await
            .expect("Failed to insert tick");

        // Query the tick
        let query = TickQuery {
            symbol: symbol.to_string(),
            limit: Some(1),
            start_time: None,
            end_time: None,
            trade_side: None,
        };
        let ticks = repo.get_ticks(&query).await.expect("Failed to query ticks");

        // Verify
        assert_eq!(ticks.len(), 1);
        assert_eq!(ticks[0].symbol, symbol);
        assert_eq!(ticks[0].price, Decimal::from_str("50000.0").unwrap());
        assert_eq!(ticks[0].trade_id, "test1");

        // Clean up
        cleanup_database(pool, symbol).await;
    }

    #[tokio::test]
    async fn test_batch_insert_and_read() {
        let repo = create_repository().await;
        let pool = repo.get_pool();
        let symbol = "BTCUSDT_TEST_BATCH";

        // Clean up before test
        cleanup_database(pool, symbol).await;

        // Prepare batch ticks
        let ticks = vec![
            create_test_tick(symbol, "50000.0", "batch1", None),
            create_test_tick(symbol, "51000.0", "batch2", None),
            create_test_tick(symbol, "52000.0", "batch3", None),
        ];

        // Batch insert
        let inserted_count = repo
            .batch_insert(ticks.clone())
            .await
            .expect("Failed to batch insert");
        assert_eq!(inserted_count, 3);

        // Query ticks
        let query = TickQuery {
            symbol: symbol.to_string(),
            limit: Some(3),
            start_time: None,
            end_time: None,
            trade_side: None,
        };
        let queried_ticks = repo.get_ticks(&query).await.expect("Failed to query ticks");

        // Verify
        assert_eq!(queried_ticks.len(), 3);
        assert!(queried_ticks
            .iter()
            .any(|t| t.trade_id == "batch1" && t.price == Decimal::from_str("50000.0").unwrap()));
        assert!(queried_ticks
            .iter()
            .any(|t| t.trade_id == "batch2" && t.price == Decimal::from_str("51000.0").unwrap()));
        assert!(queried_ticks
            .iter()
            .any(|t| t.trade_id == "batch3" && t.price == Decimal::from_str("52000.0").unwrap()));

        // Clean up
        cleanup_database(pool, symbol).await;
    }

    #[tokio::test]
    async fn test_cache_read_write() {
        let repo = create_repository().await;
        let pool = repo.get_pool();
        let cache = repo.get_cache();
        let symbol = "BTCUSDT_TEST_CACHE";

        // Clean up before test
        cleanup_database(pool, symbol).await;
        cache
            .clear_symbol(symbol)
            .await
            .expect("Failed to clear cache");

        // Insert a tick
        let tick = create_test_tick(symbol, "50000.0", "cache1", None);
        repo.insert_tick(&tick)
            .await
            .expect("Failed to insert tick");

        // Query from cache
        let cached_ticks = cache
            .get_recent_ticks(symbol, 1)
            .await
            .expect("Failed to read from cache");
        assert_eq!(cached_ticks.len(), 1);
        assert_eq!(cached_ticks[0].symbol, symbol);
        assert_eq!(cached_ticks[0].price, Decimal::from_str("50000.0").unwrap());
        assert_eq!(cached_ticks[0].trade_id, "cache1");

        // Query via get_ticks (should hit cache for recent data)
        let query = TickQuery {
            symbol: symbol.to_string(),
            limit: Some(1),
            start_time: Some(Utc::now() - Duration::hours(1)),
            end_time: None,
            trade_side: None,
        };
        let ticks = repo.get_ticks(&query).await.expect("Failed to query ticks");
        assert_eq!(ticks.len(), 1);
        assert_eq!(ticks[0].symbol, symbol);
        assert_eq!(ticks[0].price, Decimal::from_str("50000.0").unwrap());

        // Clean up
        cleanup_database(pool, symbol).await;
        cache
            .clear_symbol(symbol)
            .await
            .expect("Failed to clear cache");
    }

    #[tokio::test]
    async fn test_latest_price() {
        let repo = create_repository().await;
        let pool = repo.get_pool();
        let symbol = "BTCUSDT_TEST_PRICE";

        // Clean up before test
        cleanup_database(pool, symbol).await;

        // Insert ticks with different timestamps
        let base_time = Utc::now();
        let tick1 = create_test_tick(symbol, "50000.0", "price1", Some(base_time));
        let tick2 = create_test_tick(
            symbol,
            "51000.0",
            "price2",
            Some(base_time + Duration::seconds(1)),
        );
        repo.insert_tick(&tick1)
            .await
            .expect("Failed to insert tick1");
        repo.insert_tick(&tick2)
            .await
            .expect("Failed to insert tick2");

        // Query latest price
        let price = repo
            .get_latest_price(symbol)
            .await
            .expect("Failed to get latest price");
        assert_eq!(price, Some(Decimal::from_str("51000.0").unwrap()));

        // Clean up
        cleanup_database(pool, symbol).await;
    }

    #[tokio::test]
    async fn test_tick_validation() {
        let repo = create_repository().await;

        let valid_tick = create_test_tick("BTCUSDT", "50000.0", "test1", None);
        assert!(repo.validate_tick_data(&valid_tick).is_ok());

        let invalid_tick = TickData::new(
            Utc::now(),
            "".to_string(),
            Decimal::from_str("50000.0").unwrap(),
            Decimal::from_str("1.0").unwrap(),
            TradeSide::Buy,
            "test".to_string(),
            false,
        );
        assert!(repo.validate_tick_data(&invalid_tick).is_err());
    }

    #[tokio::test]
    async fn test_get_recent_ticks_for_backtest() {
        let repo = create_repository().await;
        let pool = repo.get_pool();
        let symbol = "BTCUSDT_BACKTEST";

        // Clean up before test
        cleanup_database(pool, symbol).await;

        // Insert ticks with different timestamps
        let base_time = Utc::now();
        let ticks = vec![
            create_test_tick(symbol, "50000.0", "bt1", Some(base_time)),
            create_test_tick(
                symbol,
                "51000.0",
                "bt2",
                Some(base_time + Duration::seconds(1)),
            ),
            create_test_tick(
                symbol,
                "52000.0",
                "bt3",
                Some(base_time + Duration::seconds(2)),
            ),
        ];

        for tick in ticks {
            repo.insert_tick(&tick)
                .await
                .expect("Failed to insert tick");
        }

        // Get recent ticks for backtest
        let backtest_ticks = repo
            .get_recent_ticks_for_backtest(symbol, 3)
            .await
            .expect("Failed to get recent ticks for backtest");

        // Verify order is ASC (oldest first)
        assert_eq!(backtest_ticks.len(), 3);
        assert_eq!(backtest_ticks[0].trade_id, "bt1");
        assert_eq!(backtest_ticks[1].trade_id, "bt2");
        assert_eq!(backtest_ticks[2].trade_id, "bt3");
        assert!(backtest_ticks[0].timestamp <= backtest_ticks[1].timestamp);
        assert!(backtest_ticks[1].timestamp <= backtest_ticks[2].timestamp);

        // Clean up
        cleanup_database(pool, symbol).await;
    }

    #[tokio::test]
    async fn test_get_historical_data_for_backtest() {
        let repo = create_repository().await;
        let pool = repo.get_pool();
        let symbol = "BTCUSDT_BACKTEST";

        // Clean up before test
        cleanup_database(pool, symbol).await;

        // Insert ticks with different timestamps
        let base_time = Utc::now();
        let ticks = vec![
            create_test_tick(
                symbol,
                "50000.0",
                "hist1",
                Some(base_time - Duration::hours(2)),
            ),
            create_test_tick(
                symbol,
                "51000.0",
                "hist2",
                Some(base_time - Duration::hours(1)),
            ),
            create_test_tick(symbol, "52000.0", "hist3", Some(base_time)),
        ];

        for tick in ticks {
            repo.insert_tick(&tick)
                .await
                .expect("Failed to insert tick");
        }

        // Get historical data for backtest
        let start_time = base_time - Duration::hours(3);
        let end_time = base_time + Duration::hours(1);
        let historical_ticks = repo
            .get_historical_data_for_backtest(symbol, start_time, end_time, None)
            .await
            .expect("Failed to get historical data for backtest");

        // Verify order is ASC and within time range
        assert_eq!(historical_ticks.len(), 3);
        assert_eq!(historical_ticks[0].trade_id, "hist1");
        assert_eq!(historical_ticks[1].trade_id, "hist2");
        assert_eq!(historical_ticks[2].trade_id, "hist3");

        for tick in &historical_ticks {
            assert!(tick.timestamp >= start_time);
            assert!(tick.timestamp <= end_time);
        }

        // Clean up
        cleanup_database(pool, symbol).await;
    }

    #[tokio::test]
    async fn test_get_backtest_data_info() {
        let repo = create_repository().await;
        let pool = repo.get_pool();
        let symbol = "BTCUSDT_BACKTEST";

        // Clean up before test
        cleanup_database(pool, symbol).await;

        // Insert test data
        let tick = create_test_tick(symbol, "50000.0", "info1", None);
        repo.insert_tick(&tick)
            .await
            .expect("Failed to insert tick");

        // Get backtest data info
        let info = repo
            .get_backtest_data_info()
            .await
            .expect("Failed to get backtest data info");

        // Verify structure
        assert!(info.total_records > 0);
        assert!(info.symbols_count > 0);
        assert!(info.earliest_time.is_some());
        assert!(info.latest_time.is_some());
        assert!(!info.symbol_info.is_empty());

        // Test helper methods
        let symbols = info.get_available_symbols();
        assert!(symbols.contains(&symbol.to_string()));

        let symbol_info = info.get_symbol_info(symbol);
        assert!(symbol_info.is_some());
        assert!(symbol_info.unwrap().records_count > 0);

        // Clean up
        cleanup_database(pool, symbol).await;
    }
}
