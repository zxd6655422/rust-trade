// main.rs
// 交易引擎入口

use std::sync::Arc;
use tracing::{error, info, warn};

mod config;
mod engine;
mod exchange;
mod order;
mod risk;
mod storage;
mod utils;

use config::Settings;
use engine::trading_loop::TradingLoop;
use exchange::ExchangeFactory;
use order::OrderManager;
use risk::RiskEngine;
use storage::{Database, OrderRepository, PositionRepository, RedisCache};
use trading_common::backtest::strategy;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // 加载环境变量
    config::load_env();

    // 初始化日志
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .with_file(true)
        .with_line_number(true)
        .init();

    info!("🚀 Trading Engine starting...");

    // 解析命令行参数
    let args: Vec<String> = std::env::args().collect();

    match args.get(1).map(|s| s.as_str()) {
        Some("live") => run_live_mode(&args).await,
        Some("--help") | Some("-h") => {
            print_usage();
            Ok(())
        }
        None => run_live_mode(&args).await,
        _ => {
            error!("❌ Unknown command: {}", args[1]);
            print_usage();
            std::process::exit(1);
        }
    }
}

fn print_usage() {
    println!("Trading Engine - Automated Trading System");
    println!();
    println!("Usage:");
    println!("  cargo run                 # Run in live mode");
    println!("  cargo run live            # Run in live mode");
    println!("  cargo run --help          # Show this help message");
    println!();
    println!("Environment Variables:");
    println!("  BINANCE_API_KEY           # Binance API key");
    println!("  BINANCE_API_SECRET        # Binance API secret");
    println!("  BINANCE_TESTNET           # Use testnet (true/false)");
    println!("  DATABASE_URL              # PostgreSQL connection string");
    println!("  REDIS_URL                 # Redis connection string");
    println!();
}

async fn run_live_mode(_args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    info!("Starting Trading Engine in live mode");

    // 加载配置
    let settings = Settings::new()?;
    info!("✅ Configuration loaded successfully");

    // 检查是否为测试网模式
    if settings.is_testnet() {
        info!("⚠️  Running in TESTNET mode");
    } else {
        warn!("🔥 Running in LIVE mode - Real money at risk!");
    }

    // 获取 API Key
    let api_key = std::env::var("BINANCE_API_KEY")
        .map_err(|_| "BINANCE_API_KEY not set")?;
    let api_secret = std::env::var("BINANCE_API_SECRET")
        .map_err(|_| "BINANCE_API_SECRET not set")?;

    // 创建交易所适配器
    let exchange = ExchangeFactory::create(
        settings.exchange_id(),
        settings.is_testnet(),
        &api_key,
        &api_secret,
        None,
    )?;
    let exchange: Arc<dyn exchange::Exchange> = Arc::from(exchange);

    info!("✅ Exchange adapter created: {}", settings.exchange_id());

    // 创建风控引擎
    let risk_engine = Arc::new(RiskEngine::new(settings.risk_control.clone()));
    info!("✅ Risk engine created");

    // 创建数据库连接
    let database = Database::new(&settings.database).await?;
    let pool = database.pool().clone();
    info!("✅ Database connected");

    // 创建 Redis 缓存
    let cache = RedisCache::new(&settings.cache).await?;
    info!("✅ Redis connected");

    // 创建仓储
    let order_repo = Arc::new(OrderRepository::new(pool.clone()));
    let position_repo = Arc::new(PositionRepository::new(pool.clone()));
    info!("✅ Repositories created");

    // 创建策略
    let strategy = strategy::create_strategy(&settings.trading.strategy)?;
    info!("✅ Strategy created: {}", strategy.name());

    // 创建订单管理器
    let order_manager = Arc::new(OrderManager::new(
        exchange.clone(),
        risk_engine.clone(),
    ));
    info!("✅ Order manager created");

    // 创建交易循环
    let trading_loop = TradingLoop::new(
        exchange.clone(),
        order_manager.clone(),
        risk_engine.clone(),
        strategy,
        &settings,
    );
    info!("✅ Trading loop created");

    info!("🎯 Trading Engine initialization complete");
    info!("Starting main loop...");

    // 启动交易循环
    trading_loop.start().await?;

    // 清理资源
    database.close().await;
    info!("👋 Trading Engine stopped");

    Ok(())
}
