// service/backtest_service.rs
// 回测结果存储服务
// 负责将回测结果保存到数据库，并支持按策略实例查询回测历史

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

/// 回测结果数据库模型
#[derive(Debug, Serialize, Deserialize, sqlx::FromRow)]
pub struct BacktestResultRecord {
    pub id: Uuid,
    /// 关联的策略实例 ID（可选）
    pub instance_id: Option<Uuid>,
    /// 策略类型名称
    pub strategy_id: String,
    pub symbol: String,
    pub initial_capital: Decimal,
    pub final_capital: Decimal,
    pub return_pct: Decimal,
    pub total_trades: i32,
    pub winning_trades: i32,
    pub losing_trades: i32,
    pub win_rate: Decimal,
    pub max_drawdown: Decimal,
    pub sharpe_ratio: Decimal,
    pub profit_factor: Decimal,
    pub data_points: i32,
    pub data_start_time: Option<DateTime<Utc>>,
    pub data_end_time: Option<DateTime<Utc>>,
    /// 策略参数 JSON
    pub strategy_params: Option<serde_json::Value>,
    pub created_at: DateTime<Utc>,
}

/// 保存回测结果请求
#[derive(Debug, Deserialize)]
pub struct SaveBacktestResultRequest {
    /// 关联的策略实例 ID（可选）
    pub instance_id: Option<Uuid>,
    /// 策略类型名称
    pub strategy_id: String,
    pub symbol: String,
    pub initial_capital: Decimal,
    pub final_capital: Decimal,
    pub return_pct: Decimal,
    pub total_trades: i32,
    pub winning_trades: i32,
    pub losing_trades: i32,
    pub win_rate: Decimal,
    pub max_drawdown: Decimal,
    pub sharpe_ratio: Decimal,
    pub profit_factor: Decimal,
    pub data_points: i32,
    pub data_start_time: Option<DateTime<Utc>>,
    pub data_end_time: Option<DateTime<Utc>>,
    /// 策略参数 JSON
    pub strategy_params: Option<serde_json::Value>,
}

/// 回测服务
pub struct BacktestService {
    pool: PgPool,
}

impl BacktestService {
    /// 创建新的回测服务实例
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// 保存回测结果
    ///
    /// # Arguments
    /// * `request` - 回测结果请求
    ///
    /// # Returns
    /// 返回保存的回测结果记录
    pub async fn save_result(
        &self,
        request: SaveBacktestResultRequest,
    ) -> Result<BacktestResultRecord, sqlx::Error> {
        let record = sqlx::query_as::<_, BacktestResultRecord>(
            r#"
            INSERT INTO backtest_results (
                instance_id, strategy_id, symbol,
                initial_capital, final_capital, return_pct,
                total_trades, winning_trades, losing_trades,
                win_rate, max_drawdown, sharpe_ratio, profit_factor,
                data_points, data_start_time, data_end_time,
                strategy_params
            ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17)
            RETURNING *
            "#
        )
        .bind(request.instance_id)
        .bind(&request.strategy_id)
        .bind(&request.symbol)
        .bind(request.initial_capital)
        .bind(request.final_capital)
        .bind(request.return_pct)
        .bind(request.total_trades)
        .bind(request.winning_trades)
        .bind(request.losing_trades)
        .bind(request.win_rate)
        .bind(request.max_drawdown)
        .bind(request.sharpe_ratio)
        .bind(request.profit_factor)
        .bind(request.data_points)
        .bind(request.data_start_time)
        .bind(request.data_end_time)
        .bind(&request.strategy_params)
        .fetch_one(&self.pool)
        .await?;

        Ok(record)
    }

    /// 根据策略实例 ID 获取回测历史
    ///
    /// # Arguments
    /// * `instance_id` - 策略实例 ID
    /// * `limit` - 返回记录数量限制
    ///
    /// # Returns
    /// 返回该策略实例的回测历史列表
    pub async fn get_by_instance(
        &self,
        instance_id: Uuid,
        limit: Option<i64>,
    ) -> Result<Vec<BacktestResultRecord>, sqlx::Error> {
        let limit = limit.unwrap_or(50);

        let records = sqlx::query_as::<_, BacktestResultRecord>(
            r#"
            SELECT * FROM backtest_results
            WHERE instance_id = $1
            ORDER BY created_at DESC
            LIMIT $2
            "#
        )
        .bind(instance_id)
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;

        Ok(records)
    }

    /// 根据策略类型获取回测历史
    ///
    /// # Arguments
    /// * `strategy_id` - 策略类型名称
    /// * `symbol` - 交易对（可选）
    /// * `limit` - 返回记录数量限制
    ///
    /// # Returns
    /// 返回该策略类型的回测历史列表
    pub async fn get_by_strategy(
        &self,
        strategy_id: &str,
        symbol: Option<&str>,
        limit: Option<i64>,
    ) -> Result<Vec<BacktestResultRecord>, sqlx::Error> {
        let limit = limit.unwrap_or(50);

        let records = if let Some(symbol) = symbol {
            sqlx::query_as::<_, BacktestResultRecord>(
                r#"
                SELECT * FROM backtest_results
                WHERE strategy_id = $1 AND symbol = $2
                ORDER BY created_at DESC
                LIMIT $3
                "#
            )
            .bind(strategy_id)
            .bind(symbol)
            .bind(limit)
            .fetch_all(&self.pool)
            .await?
        } else {
            sqlx::query_as::<_, BacktestResultRecord>(
                r#"
                SELECT * FROM backtest_results
                WHERE strategy_id = $1
                ORDER BY created_at DESC
                LIMIT $2
                "#
            )
            .bind(strategy_id)
            .bind(limit)
            .fetch_all(&self.pool)
            .await?
        };

        Ok(records)
    }

    /// 获取回测结果详情
    ///
    /// # Arguments
    /// * `id` - 回测结果 ID
    ///
    /// # Returns
    /// 返回回测结果记录
    pub async fn get_by_id(
        &self,
        id: Uuid,
    ) -> Result<Option<BacktestResultRecord>, sqlx::Error> {
        let record = sqlx::query_as::<_, BacktestResultRecord>(
            "SELECT * FROM backtest_results WHERE id = $1"
        )
        .bind(id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(record)
    }

    /// 删除回测结果
    ///
    /// # Arguments
    /// * `id` - 回测结果 ID
    ///
    /// # Returns
    /// 返回是否删除成功
    pub async fn delete(&self, id: Uuid) -> Result<bool, sqlx::Error> {
        let result = sqlx::query("DELETE FROM backtest_results WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await?;

        Ok(result.rows_affected() > 0)
    }

    /// 获取策略实例的回测统计
    ///
    /// # Arguments
    /// * `instance_id` - 策略实例 ID
    ///
    /// # Returns
    /// 返回回测统计数据
    pub async fn get_instance_stats(
        &self,
        instance_id: Uuid,
    ) -> Result<Option<BacktestStats>, sqlx::Error> {
        let stats = sqlx::query_as::<_, BacktestStats>(
            r#"
            SELECT
                COUNT(*) as total_backtests,
                AVG(return_pct) as avg_return_pct,
                MAX(return_pct) as best_return_pct,
                MIN(return_pct) as worst_return_pct,
                AVG(win_rate) as avg_win_rate,
                AVG(sharpe_ratio) as avg_sharpe_ratio,
                AVG(max_drawdown) as avg_max_drawdown,
                AVG(profit_factor) as avg_profit_factor
            FROM backtest_results
            WHERE instance_id = $1
            "#
        )
        .bind(instance_id)
        .fetch_optional(&self.pool)
        .await?;

        Ok(stats)
    }
}

/// 回测统计数据
#[derive(Debug, Serialize, Deserialize, sqlx::FromRow)]
pub struct BacktestStats {
    pub total_backtests: i64,
    pub avg_return_pct: Option<Decimal>,
    pub best_return_pct: Option<Decimal>,
    pub worst_return_pct: Option<Decimal>,
    pub avg_win_rate: Option<Decimal>,
    pub avg_sharpe_ratio: Option<Decimal>,
    pub avg_max_drawdown: Option<Decimal>,
    pub avg_profit_factor: Option<Decimal>,
}
