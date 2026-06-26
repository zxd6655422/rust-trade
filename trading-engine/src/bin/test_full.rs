// 完整功能测试
use sqlx::postgres::PgPoolOptions;
use std::time::Duration;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 加载环境变量
    let _ = dotenvy::from_filename(".env.development");

    let database_url = std::env::var("DATABASE_URL")
        .expect("DATABASE_URL must be set");

    println!("🔗 Connecting to database...");

    // 创建连接池
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .acquire_timeout(Duration::from_secs(10))
        .connect(&database_url)
        .await?;

    println!("✅ Database connected!\n");

    // 1. 创建交易引擎表
    println!("📝 Creating trading engine tables...");

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
    .execute(&pool)
    .await?;

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
    .execute(&pool)
    .await?;

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
    .execute(&pool)
    .await?;

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
    .execute(&pool)
    .await?;

    println!("✅ Tables created!\n");

    // 2. 插入测试订单
    println!("📝 Inserting test order...");

    sqlx::query(
        r#"
        INSERT INTO trading_orders (order_id, exchange, symbol, side, order_type, quantity, price, status)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
        ON CONFLICT (order_id, exchange) DO NOTHING
        "#,
    )
    .bind("TEST_ORDER_001")
    .bind("binance")
    .bind("BTCUSDT")
    .bind("BUY")
    .bind("MARKET")
    .bind(0.001)
    .bind(50000.0)
    .bind("FILLED")
    .execute(&pool)
    .await?;

    println!("✅ Test order inserted!\n");

    // 3. 查询订单
    println!("📝 Querying test order...");

    let order: (String, String, String, String) = sqlx::query_as(
        r#"
        SELECT order_id, symbol, side, status
        FROM trading_orders
        WHERE order_id = $1
        "#,
    )
    .bind("TEST_ORDER_001")
    .fetch_one(&pool)
    .await?;

    println!("📋 Order: {} {} {} {}", order.0, order.1, order.2, order.3);
    println!("✅ Order query successful!\n");

    // 4. 插入测试持仓
    println!("📝 Inserting test position...");

    sqlx::query(
        r#"
        INSERT INTO trading_positions (exchange, symbol, side, quantity, avg_entry_price)
        VALUES ($1, $2, $3, $4, $5)
        ON CONFLICT (exchange, symbol) DO UPDATE
        SET quantity = $4, avg_entry_price = $5, updated_at = NOW()
        "#,
    )
    .bind("binance")
    .bind("BTCUSDT")
    .bind("LONG")
    .bind(0.001)
    .bind(50000.0)
    .execute(&pool)
    .await?;

    println!("✅ Test position inserted!\n");

    // 5. 查询持仓
    println!("📝 Querying test position...");

    let position: (String, String, rust_decimal::Decimal, rust_decimal::Decimal) = sqlx::query_as(
        r#"
        SELECT symbol, side, quantity, avg_entry_price
        FROM trading_positions
        WHERE exchange = $1 AND symbol = $2
        "#,
    )
    .bind("binance")
    .bind("BTCUSDT")
    .fetch_one(&pool)
    .await?;

    println!("📋 Position: {} {} {} @ {}", position.0, position.1, position.2, position.3);
    println!("✅ Position query successful!\n");

    // 6. 统计信息
    println!("📊 Statistics:");

    let order_count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM trading_orders")
        .fetch_one(&pool)
        .await?;
    println!("  - Orders: {}", order_count.0);

    let position_count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM trading_positions")
        .fetch_one(&pool)
        .await?;
    println!("  - Positions: {}", position_count.0);

    let tick_count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM tick_data")
        .fetch_one(&pool)
        .await?;
    println!("  - Ticks: {}", tick_count.0);

    // 清理测试数据
    println!("\n🧹 Cleaning up test data...");
    sqlx::query("DELETE FROM trading_orders WHERE order_id = 'TEST_ORDER_001'")
        .execute(&pool)
        .await?;
    sqlx::query("DELETE FROM trading_positions WHERE exchange = 'binance' AND symbol = 'BTCUSDT'")
        .execute(&pool)
        .await?;
    println!("✅ Test data cleaned up!\n");

    pool.close().await;
    println!("🎉 All tests passed!");

    Ok(())
}
