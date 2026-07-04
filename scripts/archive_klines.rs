#!/usr/bin/env cargo +nightly script
//! K线数据归档脚本
//! 将 PostgreSQL 中的历史 K线数据导出到 Parquet 文件
//!
//! 用法:
//!   cargo run --bin archive_klines -- --days 7
//!   cargo run --bin archive_klines -- --symbol BTCUSDT --days 30

use chrono::{Duration, Utc};
use clap::Parser;
use sqlx::postgres::PgPoolOptions;
use std::path::PathBuf;
use trading_common::data::{
    parquet_store::ParquetStoreConfig,
    polars_repository::{PolarsRepository, PolarsRepositoryConfig},
    repository::TickDataRepository,
    types::OHLCData,
};

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
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

    /// 数据库连接 URL
    #[arg(long)]
    database_url: Option<String>,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 初始化日志
    tracing_subscriber::fmt::init();

    let args = Args::parse();

    // 获取数据库连接
    let database_url = args
        .database_url
        .or_else(|| std::env::var("DATABASE_URL").ok())
        .expect("DATABASE_URL must be set");

    println!("Connecting to database...");
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&database_url)
        .await?;

    let repository = TickDataRepository::new(pool.clone());

    // 配置 Polars 仓库
    let config = PolarsRepositoryConfig {
        parquet_path: args.output.clone(),
        hot_data_days: args.days,
    };
    let polars_repo = PolarsRepository::new(config);

    // 计算截止时间
    let cutoff = Utc::now() - Duration::days(args.days);
    println!("Archiving data before: {}", cutoff);

    // 获取可用的 symbol
    let symbols = if let Some(symbol) = args.symbol {
        vec![symbol]
    } else {
        let data_info = repository.get_backtest_data_info().await?;
        data_info.get_available_symbols()
    };

    println!("Found {} symbols to archive", symbols.len());

    // 逐个 symbol 归档
    for symbol in &symbols {
        println!("\nProcessing {}...", symbol);

        // 查询截止时间前的 K线数据
        let klines = repository
            .get_klines_before(symbol, cutoff, 1_000_000)
            .await?;

        if klines.is_empty() {
            println!("  No data to archive for {}", symbol);
            continue;
        }

        println!("  Found {} klines to archive", klines.len());

        // 导出到 Parquet
        let exported = polars_repo.export_klines(symbol, &klines)?;
        println!("  Exported {} klines to Parquet", exported);

        // 验证导出
        let stats = polars_repo.get_stats(symbol)?;
        println!(
            "  Parquet stats: {} records, {} files, {:.2} MB",
            stats.total_records,
            stats.files,
            stats.total_size_bytes as f64 / 1024.0 / 1024.0
        );
    }

    println!("\nArchive completed!");

    // 显示总统计
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
