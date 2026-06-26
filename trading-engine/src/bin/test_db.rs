// 测试数据库连接
use sqlx::postgres::PgPoolOptions;
use std::time::Duration;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 加载环境变量
    let _ = dotenvy::from_filename(".env.development");

    let database_url = std::env::var("DATABASE_URL")
        .expect("DATABASE_URL must be set");

    println!("Connecting to database...");
    println!("URL: {}", database_url.replace("zxd6655422", "****"));

    // 创建连接池
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .acquire_timeout(Duration::from_secs(10))
        .connect(&database_url)
        .await?;

    println!("✅ Database connection successful!");

    // 测试查询
    let row: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM tick_data")
        .fetch_one(&pool)
        .await?;

    println!("📊 tick_data table has {} records", row.0);

    // 列出所有表
    let tables: Vec<(String,)> = sqlx::query_as(
        r#"
        SELECT tablename
        FROM pg_tables
        WHERE schemaname = 'public'
        ORDER BY tablename
        "#,
    )
    .fetch_all(&pool)
    .await?;

    println!("\n📋 Tables in database:");
    for (table,) in tables {
        println!("  - {}", table);
    }

    pool.close().await;
    println!("\n✅ Test completed successfully!");

    Ok(())
}
