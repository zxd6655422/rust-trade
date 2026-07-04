//! K线数据归档工具
//! 将 PostgreSQL 中的历史 K线数据导出到 Parquet 文件
//!
//! 用法:
//!   archive_klines --days 7
//!   archive_klines --symbol BTCUSDT --days 30
//!   archive_klines --days 7 --output /path/to/parquet

use chrono::{DateTime, Duration, Utc};
use clap::Parser;
use sqlx::postgres::PgPoolOptions;
use std::path::PathBuf;
use trading_common::data::{
    polars_repository::{PolarsRepository, PolarsRepositoryConfig},
    types::{OHLCData, Timeframe},
};

#[derive(Parser, Debug)]
#[command(author, version, about = "K线数据归档工具 - PostgreSQL → Parquet")]
struct Args {
    /// 归档多少天前的数据
    #[arg(short, long, default_value = "7")]
    days: i64,

    /// 指定交易对 (可选，默认全部)
    #[arg(short, long)]
    symbol: Option<String>,

    /// Parquet 输出路径
    #[arg(short, long, default_value = "data/parquet")]
    output: PathBuf,

    /// 数据库连接 URL (也可通过 DATABASE_URL 环境变量设置)
    #[arg(long)]
    database_url: Option<String>,
}

/// 从数据库获取可用的 symbol 列表
async fn get_symbols(pool: &sqlx::PgPool) -> Result<Vec<String>, sqlx::Error> {
    let rows = sqlx::query_scalar!("SELECT DISTINCT symbol FROM kline_1m ORDER BY symbol")
        .fetch_all(pool)
        .await?;
    Ok(rows)
}

/// 从数据库获取指定时间之前的 K线数据
async fn get_klines_before(
    pool: &sqlx::PgPool,
    symbol: &str,
    before: DateTime<Utc>,
    limit: i64,
) -> Result<Vec<OHLCData>, sqlx::Error> {
    let rows = sqlx::query!(
        "SELECT timestamp, symbol, open, high, low, close, volume, trade_count \
         FROM kline_1m WHERE symbol = $1 AND timestamp < $2 ORDER BY timestamp ASC LIMIT $3",
        symbol,
        before,
        limit
    )
    .fetch_all(pool)
    .await?;

    let klines = rows
        .into_iter()
        .map(|row| OHLCData {
            timestamp: row.timestamp,
            symbol: row.symbol,
            timeframe: Timeframe::OneMinute,
            open: row.open,
            high: row.high,
            low: row.low,
            close: row.close,
            volume: row.volume,
            trade_count: row.trade_count as u64,
        })
        .collect();

    Ok(klines)
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    tracing_subscriber::fmt::init();

    let args = Args::parse();

    let database_url = args
        .database_url
        .or_else(|| std::env::var("DATABASE_URL").ok())
        .expect("DATABASE_URL must be set (via --database-url or environment variable)");

    println!("Connecting to database...");
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&database_url)
        .await?;

    let config = PolarsRepositoryConfig {
        parquet_path: args.output.clone(),
        hot_data_days: args.days,
    };
    let polars_repo = PolarsRepository::new(config);

    let cutoff = Utc::now() - Duration::days(args.days);
    println!("Archiving data before: {}", cutoff);

    let symbols = if let Some(symbol) = args.symbol {
        vec![symbol]
    } else {
        get_symbols(&pool).await?
    };

    println!("Found {} symbols to archive", symbols.len());

    for symbol in &symbols {
        println!("\nProcessing {}...", symbol);

        let klines = get_klines_before(&pool, symbol, cutoff, 1_000_000).await?;

        if klines.is_empty() {
            println!("  No data to archive for {}", symbol);
            continue;
        }

        println!("  Found {} klines to archive", klines.len());

        let exported = polars_repo.export_klines(symbol, &klines)?;
        println!("  Exported {} klines to Parquet", exported);

        let stats = polars_repo.get_stats(symbol)?;
        println!(
            "  Parquet stats: {} records, {} files, {:.2} MB",
            stats.total_records,
            stats.files,
            stats.total_size_bytes as f64 / 1024.0 / 1024.0
        );
    }

    println!("\nArchive completed!");

    let all_symbols = polars_repo.list_symbols()?;
    let mut total_records = 0;
    let mut total_size = 0;

    for symbol in &all_symbols {
        let stats = polars_repo.get_stats(symbol)?;
        total_records += stats.total_records;
        total_size += stats.total_size_bytes;
    }

    println!("\nTotal Parquet statistics:");
    println!("  Symbols: {}", all_symbols.len());
    println!("  Records: {}", total_records);
    println!("  Size: {:.2} MB", total_size as f64 / 1024.0 / 1024.0);

    Ok(())
}
