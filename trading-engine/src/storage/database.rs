// storage/database.rs
// 数据库连接管理

use sqlx::postgres::{PgPool, PgPoolOptions};
use std::time::Duration;
use tracing::info;

use crate::config::DatabaseConfig;

/// 数据库连接池
pub struct Database {
    pool: PgPool,
}

impl Database {
    /// 创建新的数据库连接
    pub async fn new(config: &DatabaseConfig) -> Result<Self, sqlx::Error> {
        info!("Connecting to database...");

        let pool = PgPoolOptions::new()
            .max_connections(config.max_connections)
            .min_connections(config.min_connections)
            .max_lifetime(Duration::from_secs(config.max_lifetime))
            .acquire_timeout(Duration::from_secs(10))
            .connect(&config.url)
            .await?;

        info!("Database connection established");

        // 初始化数据库表
        Self::init_tables(&pool).await?;

        Ok(Self { pool })
    }

    /// 获取连接池
    pub fn pool(&self) -> &PgPool {
        &self.pool
    }

    /// 初始化数据库表
    async fn init_tables(pool: &PgPool) -> Result<(), sqlx::Error> {
        info!("Initializing database tables...");

        // 创建订单表
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS trading_orders (
                id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
                order_id VARCHAR(50) NOT NULL,
                exchange VARCHAR(20) NOT NULL,
                symbol VARCHAR(20) NOT NULL,
                side VARCHAR(4) NOT NULL,
                order_type VARCHAR(20) NOT NULL,
                quantity DECIMAL(20,8) NOT NULL,
                price DECIMAL(20,8),
                status VARCHAR(20) NOT NULL,
                filled_quantity DECIMAL(20,8) DEFAULT 0,
                avg_price DECIMAL(20,8),
                commission DECIMAL(20,8),
                commission_asset VARCHAR(10),
                client_order_id VARCHAR(50),
                created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
                updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
                UNIQUE(order_id, exchange)
            )
            "#,
        )
        .execute(pool)
        .await?;

        // 创建持仓表
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS trading_positions (
                id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
                exchange VARCHAR(20) NOT NULL,
                symbol VARCHAR(20) NOT NULL,
                side VARCHAR(10) NOT NULL,
                quantity DECIMAL(20,8) NOT NULL,
                avg_entry_price DECIMAL(20,8) NOT NULL,
                unrealized_pnl DECIMAL(20,8) DEFAULT 0,
                stop_loss_price DECIMAL(20,8),
                take_profit_price DECIMAL(20,8),
                leverage INTEGER DEFAULT 1,
                margin DECIMAL(20,8) DEFAULT 0,
                created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
                updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
                UNIQUE(exchange, symbol)
            )
            "#,
        )
        .execute(pool)
        .await?;

        // 创建风控日志表
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS risk_logs (
                id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
                timestamp TIMESTAMPTZ NOT NULL DEFAULT NOW(),
                event_type VARCHAR(50) NOT NULL,
                symbol VARCHAR(20),
                details JSONB,
                decision VARCHAR(20) NOT NULL
            )
            "#,
        )
        .execute(pool)
        .await?;

        // 创建交易日志表
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS trade_logs (
                id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
                timestamp TIMESTAMPTZ NOT NULL DEFAULT NOW(),
                strategy_id VARCHAR(50),
                symbol VARCHAR(20) NOT NULL,
                side VARCHAR(4) NOT NULL,
                quantity DECIMAL(20,8) NOT NULL,
                price DECIMAL(20,8) NOT NULL,
                order_id VARCHAR(50),
                pnl DECIMAL(20,8),
                notes TEXT
            )
            "#,
        )
        .execute(pool)
        .await?;

        // 创建索引
        sqlx::query("CREATE INDEX IF NOT EXISTS idx_orders_symbol ON trading_orders(symbol)")
            .execute(pool)
            .await?;
        sqlx::query("CREATE INDEX IF NOT EXISTS idx_orders_status ON trading_orders(status)")
            .execute(pool)
            .await?;
        sqlx::query("CREATE INDEX IF NOT EXISTS idx_positions_symbol ON trading_positions(symbol)")
            .execute(pool)
            .await?;
        sqlx::query("CREATE INDEX IF NOT EXISTS idx_risk_logs_timestamp ON risk_logs(timestamp)")
            .execute(pool)
            .await?;
        sqlx::query("CREATE INDEX IF NOT EXISTS idx_trade_logs_timestamp ON trade_logs(timestamp)")
            .execute(pool)
            .await?;

        info!("Database tables initialized successfully");
        Ok(())
    }

    /// 关闭数据库连接
    pub async fn close(&self) {
        info!("Closing database connection...");
        self.pool.close().await;
        info!("Database connection closed");
    }
}
