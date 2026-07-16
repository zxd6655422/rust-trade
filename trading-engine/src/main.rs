// main.rs
// 交易引擎入口
//
// 统一启动模式：同时运行交易和账户信息同步
// - 交易：从策略服务获取信号，执行买卖操作
// - 账户同步：定时采集各交易所的账户余额、持仓等信息
//
// 交易所配置从数据库 exchange_config 表加载
// 前端可动态管理交易所实例（增删改查、启用禁用）

use std::sync::Arc;
use tracing::{error, info, warn};

mod config;
mod engine;
mod exchange;
mod order;
mod portfolio;
mod risk;
mod service;
mod storage;
mod utils;

use config::Settings;
use engine::signal_poller::{SignalPoller, SignalPollerConfig};
use engine::trading_unit::TradingUnit;
use risk::RiskEngine;
use storage::{Database, ExchangeRepository, OrderRepository, PositionRepository, RedisCache, StopOrderRepository};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    config::load_env();

    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .with_file(true)
        .with_line_number(true)
        .init();

    info!("🚀 Trading Engine starting...");

    let args: Vec<String> = std::env::args().collect();

    match args.get(1).map(|s| s.as_str()) {
        Some("--help") | Some("-h") => {
            print_usage();
            Ok(())
        }
        _ => run().await,
    }
}

fn print_usage() {
    println!("Trading Engine - Automated Trading System");
    println!();
    println!("Usage:");
    println!("  cargo run                 # Start trading engine");
    println!("  cargo run --help          # Show this help message");
    println!();
    println!("Features:");
    println!("  - Auto trading: Execute signals from strategy service");
    println!("  - Account sync: Poll account balances, positions from exchanges");
    println!();
    println!("Configuration:");
    println!("  config/engine-development.toml  # DB/Redis/Risk config");
    println!("  exchange_config table           # Exchange instances (frontend managed)");
    println!();
}

/// 统一启动入口
///
/// 同时启动交易引擎和账户信息同步服务
async fn run() -> Result<(), Box<dyn std::error::Error>> {
    let settings = Settings::new()?;
    info!("✅ Configuration loaded");

    if settings.is_testnet() {
        info!("⚠️  Running in TESTNET mode");
    } else {
        warn!("🔥 Running in LIVE mode - Real money at risk!");
    }

    // 创建数据库连接
    let database = Database::new(&settings.database).await?;
    let pool = database.pool().clone();
    info!("✅ Database connected");

    // 创建 Redis 缓存
    let cache = Arc::new(RedisCache::new(&settings.cache).await?);
    info!("✅ Redis connected");

    // 从数据库加载交易所配置
    let exchange_repo = ExchangeRepository::new(pool.clone());
    let exchange_configs = exchange_repo.load_enabled().await?;

    if exchange_configs.is_empty() {
        error!("❌ No enabled exchanges in database!");
        error!("   Please insert exchange configs into exchange_config table.");
        error!("   Example:");
        error!("   INSERT INTO exchange_config (id, exchange_id, market_type, testnet, enabled, leverage)");
        error!("   VALUES ('binance-futures', 'binance', 'futures', true, true, 10);");
        return Err("No enabled exchanges configured".into());
    }

    info!("📋 Loaded {} enabled exchange configs:", exchange_configs.len());
    for config in &exchange_configs {
        info!("   - {} ({} {}, leverage={}x)",
            config.id, config.exchange_id, config.market_type, config.leverage);
    }

    // ============ 创建共享资源 ============

    // 持仓仓储
    let position_repo = Arc::new(PositionRepository::new(pool.clone()));

    // 止损止盈仓储（持久化）
    let stop_order_repo = Arc::new(StopOrderRepository::new(pool.clone()));
    info!("✅ Stop order repository created");

    // 订单仓储（持久化）
    let order_repo = Arc::new(OrderRepository::new(pool.clone()));
    info!("✅ Order repository created");

    // 风控引擎（所有 TradingUnit 共享）
    let risk_engine = Arc::new(RiskEngine::new(settings.risk_control.clone()));
    info!("✅ Risk engine created");

    // 账户数据仓储
    let account_repo = Arc::new(trading_common::data::account_repository::AccountRepository::new(pool.clone()));
    info!("✅ Account repository created");

    // ============ 启动账户同步服务 ============
    {
        let account_configs = exchange_configs.clone();
        let account_repo = account_repo.clone();

        tokio::spawn(async move {
            let poller = service::account_poller::AccountPoller::new(
                account_configs,
                account_repo,
                service::account_poller::AccountPollerConfig::default(),
            );

            info!("📊 Account Poller starting...");
            poller.start().await;
        });
    }
    info!("✅ Account Poller started (background)");

    // ============ 启动交易引擎 ============

    // 创建 TradingUnit
    let mut trading_units = Vec::new();
    for exchange_config in &exchange_configs {
        match TradingUnit::from_config(
            exchange_config,
            risk_engine.clone(),
            position_repo.clone(),
            cache.clone(),
            Some(stop_order_repo.clone()),
            Some(order_repo.clone()),
        ) {
            Ok(unit) => {
                trading_units.push(Arc::new(unit));
            }
            Err(e) => {
                error!("Failed to create trading unit {}: {}", exchange_config.id, e);
                // 非致命错误，跳过
            }
        }
    }

    if trading_units.is_empty() {
        error!("❌ No trading units created! Check exchange configs and API keys.");
        return Err("No trading units available".into());
    }

    info!("✅ {} trading units created", trading_units.len());

    // 创建信号轮询器（交易引擎唯一主循环）
    let signal_poller = Arc::new(SignalPoller::new(
        pool.clone(),
        risk_engine.clone(),
        trading_units.clone(),
        SignalPollerConfig::default(),
    ));

    info!("🎯 Trading Engine ready");
    info!("   - Trading: Active (signal polling)");
    info!("   - Account sync: Active (60s interval)");
    info!("Starting main loop...");

    // 启动主循环（阻塞）
    signal_poller.start().await;

    database.close().await;
    info!("👋 Trading Engine stopped");

    Ok(())
}
