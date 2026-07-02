use chrono::NaiveDate;
use rust_decimal::Decimal;
use sqlx::PgPool;
use std::sync::Arc;
use std::time::Duration;
use tokio::signal;
use tracing::{debug, error, info, warn};
use tracing_subscriber::{fmt, layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

// CLI-specific modules
mod api;
mod config;
mod exchange;
mod live_trading;
mod service;

// Import from trading-common
use trading_common::backtest;
use trading_common::data;

use config::{CollectorMode, Settings};
use data::{cache::TieredCache, repository::TickDataRepository};
use exchange::BinanceExchange;
use live_trading::PaperTradingProcessor;
use service::{BackfillService, MarketDataService};
use trading_common::data::types::{OHLCData, Timeframe};

use data::cache::TickDataCache;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().collect();

    match args.get(1).map(|s| s.as_str()) {
        Some("service") => run_service_mode().await,
        Some("collector") => run_collector_mode().await,
        Some("backtest") => run_backtest_mode().await,
        Some("live") => {
            // Check if paper trading is enabled
            if args.contains(&"--paper-trading".to_string()) {
                run_live_with_paper_trading().await
            } else {
                run_live_mode().await
            }
        }
        None => run_service_mode().await,
        Some("--help") | Some("-h") => {
            print_usage();
            Ok(())
        }
        _ => {
            eprintln!("❌ Unknown command: {}", args[1]);
            print_usage();
            std::process::exit(1);
        }
    }
}

fn print_usage() {
    println!("Trading Core - Cryptocurrency Data Collection & Backtesting System");
    println!();
    println!("Usage:");
    println!("  cargo run service        # Run full service (collector + API + backtest)");
    println!("  cargo run collector      # Run data collector only");
    println!("  cargo run live           # Run live data collection (legacy)");
    println!("  cargo run backtest       # Run backtesting mode (CLI)");
    println!("  cargo run --help         # Show this help message");
    println!();
    println!("Service mode (recommended for 24/7):");
    println!("  - Data collection (candle1m/tick based on config)");
    println!("  - HTTP API for backtest and monitoring");
    println!("  - WebSocket for real-time data");
    println!();
    println!("Collector modes (configured in config file):");
    println!("  disabled  - No data collection (backtest only)");
    println!("  tick      - Collect tick data (high frequency, high resource usage)");
    println!("  candle1m  - Collect 1m candle data (low frequency, minimal resources)");
    println!();
}

async fn run_live_with_paper_trading() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize application environment
    init_application().await?;

    // Load configuration
    let settings = Settings::new()?;

    // Initialize logging with configured level
    init_tracing(&settings.log_level)?;

    info!("🎯 Starting Trading Core Application (Live Mode + Paper Trading)");

    // Check if paper trading is enabled
    if !settings.paper_trading.enabled {
        warn!("⚠️ Paper trading is disabled in config. Set paper_trading.enabled = true");
        warn!("⚠️ Falling back to live data collection only...");
        return run_live_mode().await;
    }

    info!("📋 Configuration loaded successfully");
    info!("📊 Monitoring symbols: {:?}", settings.symbols);
    info!("📝 Log level: {}", settings.log_level);
    info!(
        "🎯 Paper Trading Strategy: {}",
        settings.paper_trading.strategy
    );
    info!(
        "💰 Initial Capital: ${}",
        settings.paper_trading.initial_capital
    );
    info!(
        "🗄️  Database: {} connections",
        settings.database.max_connections
    );
    info!(
        "💾 Cache: Memory({} ticks/{}s) + Redis({} ticks/{}s)",
        settings.cache.memory.max_ticks_per_symbol,
        settings.cache.memory.ttl_seconds,
        settings.cache.redis.max_ticks_per_symbol,
        settings.cache.redis.ttl_seconds
    );

    // Verify strategy exists
    if backtest::strategy::get_strategy_info(&settings.paper_trading.strategy).is_none() {
        error!("❌ Unknown strategy: {}", settings.paper_trading.strategy);
        error!("💡 Available strategies: rsi, sma");
        std::process::exit(1);
    }

    // Create database connection pool
    info!("🔌 Connecting to database...");
    let pool = create_database_pool(&settings).await?;
    test_database_connection(&pool).await?;
    info!("✅ Database connection established");

    // Create cache
    info!("💾 Initializing cache...");
    let cache = create_cache(&settings).await?;
    info!("✅ Cache initialized");

    // Create repository
    let repository = Arc::new(TickDataRepository::new(pool, cache));

    // Create exchange connection
    info!("📡 Initializing exchange connection...");
    let exchange: Arc<dyn exchange::Exchange> = Arc::new(
        BinanceExchange::with_futures_symbols(settings.futures_symbols.clone())
    );
    info!("✅ Exchange connection ready (futures: {:?})", &settings.futures_symbols);

    // Create strategy
    info!(
        "🧠 Initializing strategy: {}",
        settings.paper_trading.strategy
    );
    let strategy = backtest::strategy::create_strategy(&settings.paper_trading.strategy)?;
    info!("✅ Strategy initialized: {}", strategy.name());

    // Create paper trading processor
    let initial_capital = Decimal::try_from(settings.paper_trading.initial_capital)
        .map_err(|e| format!("Invalid initial capital: {}", e))?;
    let paper_trading = Arc::new(tokio::sync::Mutex::new(PaperTradingProcessor::new(
        strategy,
        Arc::clone(&repository),
        initial_capital,
    )));

    // Create market data service
    let service = MarketDataService::new(exchange, repository, settings.symbols.clone())
        .with_paper_trading(paper_trading);

    info!(
        "🎯 Starting market data collection with paper trading for {} symbols",
        settings.symbols.len()
    );
    println!("🚀 Paper trading is now active! Watch for trading signals below...");
    println!(
        "📈 Strategy: {} | Initial Capital: ${}",
        settings.paper_trading.strategy, settings.paper_trading.initial_capital
    );
    println!("{}", "=".repeat(80));

    // Start service
    run_live_application_with_service(service).await?;

    info!("✅ Application stopped gracefully");
    Ok(())
}

async fn run_live_application_with_service(
    service: MarketDataService,
) -> Result<(), Box<dyn std::error::Error>> {
    let service_shutdown_tx = service.get_shutdown_tx();

    // Start signal forwarding task
    tokio::spawn(async move {
        signal::ctrl_c().await.expect("Failed to listen for ctrl-c");
        println!("\nReceived Ctrl+C signal, forwarding to service...");
        info!("Received Ctrl+C signal, forwarding to service");
        let _ = service_shutdown_tx.send(());
    });

    // Just wait for service to complete
    match service.start().await {
        Ok(()) => {
            info!("Service stopped successfully");
            Ok(())
        }
        Err(e) => {
            error!("Service stopped with error: {}", e);
            Err(Box::new(e))
        }
    }
}

/// Service mode - full service with data collection + API + backtest
async fn run_service_mode() -> Result<(), Box<dyn std::error::Error>> {
    init_application().await?;

    // Load configuration
    let settings = Settings::new()?;

    // Initialize logging with configured level
    init_tracing(&settings.log_level)?;

    info!("🚀 Starting Trading Core Service Mode");
    info!("📋 Configuration loaded successfully");
    info!("📊 Monitoring symbols: {:?}", settings.symbols);
    info!("📝 Log level: {}", settings.log_level);
    info!(
        "🗄️  Database: {} connections",
        settings.database.max_connections
    );

    // Create database connection pool
    info!("🔌 Connecting to database...");
    let pool = create_database_pool(&settings).await?;
    test_database_connection(&pool).await?;
    info!("✅ Database connection established");

    // Create cache
    info!("💾 Initializing cache...");
    let cache = create_cache(&settings).await?;
    info!("✅ Cache initialized");

    // Create repository
    let repository = Arc::new(TickDataRepository::new(pool, cache));

    // Create broadcast channel for real-time data
    let (tick_tx, _) = tokio::sync::broadcast::channel::<trading_common::data::types::TickData>(1000);

    // Start data collection based on config
    let collection_handle = match settings.collector.mode {
        CollectorMode::Disabled => {
            info!("⚠️ Data collection is disabled");
            None
        }
        CollectorMode::Tick => {
            info!("📊 Starting tick data collection (high frequency)");
            let exchange: Arc<dyn exchange::Exchange> = Arc::new(
                BinanceExchange::with_futures_symbols(settings.futures_symbols.clone())
            );
            let symbols = settings.symbols.clone();
            let tick_tx_clone = tick_tx.clone();
            let shutdown_rx = tick_tx.subscribe();

            Some(tokio::spawn(async move {
                let service = MarketDataService::new(exchange, Arc::new(TickDataRepository::new(
                    // This is a bit of a hack - we need a separate repository for the collection task
                    // In production, you'd want to share the same pool
                    create_database_pool_for_service().await.unwrap(),
                    create_cache_for_service().await.unwrap(),
                )), symbols);

                if let Err(e) = service.start().await {
                    error!("Data collection error: {}", e);
                }
            }))
        }
        CollectorMode::Candle1m => {
            let poll_interval = settings.collector.poll_interval_secs;
            let backfill_enabled = settings.collector.backfill_enabled;
            let backfill_start = settings.collector.backfill_start_date.clone();
            let futures_symbols = settings.futures_symbols.clone();
            info!("📊 Starting candle1m data collection (REST polling every {}s)", poll_interval);
            let exchange: Arc<dyn exchange::Exchange> = Arc::new(
                BinanceExchange::with_futures_symbols(futures_symbols.clone())
            );
            let symbols = settings.symbols.clone();

            Some(tokio::spawn(async move {
                let repo = Arc::new(TickDataRepository::new(
                    create_database_pool_for_service().await.unwrap(),
                    create_cache_for_service().await.unwrap(),
                ));

                // Step 1: Backfill historical data (if enabled)
                // API 限制: Binance 20 req/s, OKX 12 req/s
                // 使用信号量限制并发数，避免超出限制
                if backfill_enabled {
                    match NaiveDate::parse_from_str(&backfill_start, "%Y-%m-%d") {
                        Ok(date) => {
                            let start_dt = date.and_hms_opt(0, 0, 0).unwrap().and_utc();

                            // 限制最大并发数为 5，每个 symbol 内部限速 200ms
                            // 总速率 = 5 * 5 req/s = 25 req/s (远低于 20 req/s 限制)
                            const MAX_CONCURRENT_BACKFILLS: usize = 5;
                            let semaphore = Arc::new(tokio::sync::Semaphore::new(MAX_CONCURRENT_BACKFILLS));

                            info!("🚀 Starting backfill for {} symbols (max {} concurrent)", symbols.len(), MAX_CONCURRENT_BACKFILLS);

                            let mut backfill_handles = Vec::new();
                            for symbol in &symbols {
                                let ex = exchange.clone();
                                let repo_clone = repo.clone();
                                let sym = symbol.clone();
                                let start = start_dt;
                                let sem = semaphore.clone();

                                let handle = tokio::spawn(async move {
                                    // 获取许可，限制并发
                                    let _permit = sem.acquire().await.unwrap();
                                    info!("📊 Starting backfill for {}", sym);

                                    let backfill = BackfillService::new(
                                        ex,
                                        repo_clone,
                                        vec![sym.clone()],
                                        start,
                                    );
                                    backfill.run().await;
                                    info!("✅ Backfill completed for {}", sym);
                                });
                                backfill_handles.push(handle);
                            }

                            // 等待所有 backfill 完成
                            for handle in backfill_handles {
                                if let Err(e) = handle.await {
                                    error!("Backfill task failed: {}", e);
                                }
                            }
                            info!("✅ All backfill tasks completed");
                        }
                        Err(e) => {
                            error!("Invalid backfill_start_date '{}': {}", backfill_start, e);
                        }
                    }
                }

                // Step 2: Start periodic polling - 分批获取，避免超出 API 限制
                // poll_interval 默认 30 秒，每批 5 个 symbol，间隔 500ms
                const POLL_BATCH_SIZE: usize = 5;
                const POLL_BATCH_DELAY_MS: u64 = 500;

                let mut poll_timer = tokio::time::interval(Duration::from_secs(poll_interval));
                poll_timer.tick().await; // skip first immediate tick

                loop {
                    poll_timer.tick().await;

                    // 分批获取 symbol 的 kline 数据
                    for chunk in symbols.chunks(POLL_BATCH_SIZE) {
                        let fetch_futures: Vec<_> = chunk.iter().map(|symbol| {
                            let ex = exchange.clone();
                            let sym = symbol.clone();
                            async move {
                                match ex.fetch_klines(&sym, "1m", 100).await {
                                    Ok(klines) => {
                                        debug!("[{}] 拉取到 {} 条 kline", sym, klines.len());
                                        let ohlc_list: Vec<OHLCData> = klines
                                            .into_iter()
                                            .map(|k| OHLCData {
                                                timestamp: k.timestamp,
                                                symbol: k.symbol,
                                                timeframe: Timeframe::OneMinute,
                                                open: k.open,
                                                high: k.high,
                                                low: k.low,
                                                close: k.close,
                                                volume: k.volume,
                                                trade_count: k.trade_count,
                                            })
                                            .collect();
                                        Some((sym, ohlc_list))
                                    }
                                    Err(e) => {
                                        error!("Failed to fetch klines for {}: {}", sym, e);
                                        None
                                    }
                                }
                            }
                        }).collect();

                        // 等待当前批次完成
                        let results = futures::future::join_all(fetch_futures).await;

                        // 批量插入数据
                        for result in results.into_iter().flatten() {
                            let (symbol, ohlc_list) = result;
                            let count = ohlc_list.len();
                            match repo.batch_insert_klines(ohlc_list).await {
                                Ok(inserted) => {
                                    debug!(
                                        "[{}] kline_1m: fetched {}, upserted {}",
                                        symbol, count, inserted
                                    );
                                }
                                Err(e) => {
                                    error!("[{}] kline_1m 插入失败: {}", symbol, e);
                                }
                            }
                        }

                        // 批次间延迟，避免短时间大量请求
                        tokio::time::sleep(Duration::from_millis(POLL_BATCH_DELAY_MS)).await;
                    }
                }
            }))
        }
    };

    // Start API server
    let api_config = api::server::ApiServerConfig {
        host: "0.0.0.0".to_string(),
        port: 8080,
    };
    let api_server = api::ApiServer::new(api_config, repository.clone(), tick_tx.clone());

    info!("🌐 Starting API server on port 8080");
    info!("📡 WebSocket available at ws://0.0.0.0:8080/ws");
    info!("🔗 REST API available at http://0.0.0.0:8080/api");

    println!("{}", "=".repeat(80));
    println!("🚀 Trading Core Service is running!");
    println!("   Data collection: {}", settings.collector.mode);
    println!("   API server: http://0.0.0.0:8080");
    println!("   WebSocket: ws://0.0.0.0:8080/ws");
    println!("   Press Ctrl+C to stop");
    println!("{}", "=".repeat(80));

    // 等待关闭信号或服务退出
    tokio::select! {
        _ = signal::ctrl_c() => {
            info!("Received Ctrl+C signal, shutting down...");
        }
        result = api_server.start() => {
            match result {
                Ok(_) => info!("API server stopped"),
                Err(e) => error!("API server error: {}", e),
            }
        }
    }

    // 清理
    if let Some(handle) = collection_handle {
        handle.abort();
    }

    info!("✅ Trading Core Service stopped");
    Ok(())
}

/// Collector mode - dedicated data collection for 24/7 operation
async fn run_collector_mode() -> Result<(), Box<dyn std::error::Error>> {
    init_application().await?;

    // Load configuration
    let settings = Settings::new()?;

    // Initialize logging with configured level
    init_tracing(&settings.log_level)?;

    info!("🚀 Starting Trading Core Collector Mode");
    info!("📝 Log level: {}", settings.log_level);

    // Check collector mode
    match settings.collector.mode {
        config::CollectorMode::Disabled => {
            info!("⚠️ Collector is disabled in config. Exiting.");
            info!("💡 Set collector.mode = 'candle1m' or 'tick' in config file to enable");
            return Ok(());
        }
        config::CollectorMode::Tick => {
            info!("📊 Collector mode: Tick (high frequency)");
            info!("⚠️ This mode consumes significant resources");
        }
        config::CollectorMode::Candle1m => {
            info!("📊 Collector mode: Candle1m (low frequency)");
            info!("✅ Resource usage: minimal (1 request/minute/symbol)");
        }
    }

    info!("📋 Configuration loaded successfully");
    info!("📊 Monitoring symbols: {:?}", settings.symbols);
    info!(
        "🗄️  Database: {} connections",
        settings.database.max_connections
    );

    // Create database connection pool
    info!("🔌 Connecting to database...");
    let pool = create_database_pool(&settings).await?;
    test_database_connection(&pool).await?;
    info!("✅ Database connection established");

    // Create cache
    info!("💾 Initializing cache...");
    let cache = create_cache(&settings).await?;
    info!("✅ Cache initialized");

    // Create repository
    let repository = Arc::new(TickDataRepository::new(pool, cache));

    // Create exchange connection
    info!("📡 Initializing exchange connection...");
    let exchange: Arc<dyn exchange::Exchange> = Arc::new(
        BinanceExchange::with_futures_symbols(settings.futures_symbols.clone())
    );
    info!("✅ Exchange connection ready (futures: {:?})", &settings.futures_symbols);

    // Create market data service (no paper trading in collector mode)
    let service = MarketDataService::new(exchange, repository, settings.symbols.clone());

    info!(
        "🎯 Starting data collection for {} symbols (mode: {})",
        settings.symbols.len(),
        settings.collector.mode
    );
    println!("🚀 Data collector is now active!");
    println!(
        "📈 Mode: {} | Symbols: {:?}",
        settings.collector.mode, settings.symbols
    );
    println!("{}", "=".repeat(80));

    // Setup signal forwarding
    let service_shutdown_tx = service.get_shutdown_tx();
    tokio::spawn(async move {
        signal::ctrl_c().await.expect("Failed to listen for ctrl-c");
        println!("\nReceived Ctrl+C signal, forwarding to collector...");
        info!("Received Ctrl+C signal, forwarding to collector");
        let _ = service_shutdown_tx.send(());
    });

    // Start service
    match service.start().await {
        Ok(()) => {
            info!("✅ Collector stopped successfully");
            Ok(())
        }
        Err(e) => {
            error!("❌ Collector stopped with error: {}", e);
            Err(Box::new(e))
        }
    }
}

/// Real-time mode entry
async fn run_live_mode() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize environment and logging
    init_application().await?;

    // Load configuration
    let settings = Settings::new()?;

    // Initialize logging with configured level
    init_tracing(&settings.log_level)?;

    info!("🚀 Starting Trading Core Application (Live Mode)");
    info!("📋 Configuration loaded successfully");
    info!("📊 Monitoring symbols: {:?}", settings.symbols);
    info!("📝 Log level: {}", settings.log_level);
    info!(
        "🗄️  Database: {} connections",
        settings.database.max_connections
    );
    info!(
        "💾 Cache: Memory({} ticks/{}s) + Redis({} ticks/{}s)",
        settings.cache.memory.max_ticks_per_symbol,
        settings.cache.memory.ttl_seconds,
        settings.cache.redis.max_ticks_per_symbol,
        settings.cache.redis.ttl_seconds
    );

    // Create and start the application
    run_live_application(settings).await?;

    info!("✅ Application stopped gracefully");
    Ok(())
}

/// Backtesting mode entry
async fn run_backtest_mode() -> Result<(), Box<dyn std::error::Error>> {
    init_application().await?;

    let settings = Settings::new()?;

    // Initialize logging with configured level
    init_tracing(&settings.log_level)?;

    info!("🔬 Starting Trading Core Application (Backtest Mode)");
    info!("📋 Configuration loaded successfully");
    info!("📝 Log level: {}", settings.log_level);

    let pool = create_database_pool(&settings).await?;
    test_database_connection(&pool).await?;
    info!("✅ Database connection established");

    let cache = create_backtest_cache(&settings).await?;
    info!("✅ Cache initialized for backtest");

    let repository = TickDataRepository::new(pool, cache);

    run_backtest_interactive(repository).await?;

    info!("✅ Backtest completed successfully");
    Ok(())
}

/// Backtesting interactive interface
async fn run_backtest_interactive(
    repository: TickDataRepository,
) -> Result<(), Box<dyn std::error::Error>> {
    use backtest::{
        engine::{BacktestConfig, BacktestEngine},
        strategy::{create_strategy, list_strategies},
    };
    use rust_decimal::Decimal;
    use std::io::{self, Write};
    use std::str::FromStr;

    println!("{}", "=".repeat(60));
    println!("🎯 TRADING CORE BACKTESTING SYSTEM");
    println!("{}", "=".repeat(60));

    // Display statistics
    println!("📊 Loading data statistics...");
    let data_info = repository.get_backtest_data_info().await?;

    println!("\n📈 Available Data:");
    println!("  Total Records: {}", data_info.total_records);
    println!("  Available Symbols: {}", data_info.symbols_count);

    if let Some(earliest) = data_info.earliest_time {
        println!(
            "  Earliest Data: {}",
            earliest.format("%Y-%m-%d %H:%M:%S UTC")
        );
    }
    if let Some(latest) = data_info.latest_time {
        println!("  Latest Data: {}", latest.format("%Y-%m-%d %H:%M:%S UTC"));
    }

    println!("\n📋 Symbol Details:");
    for (i, symbol_info) in data_info.symbol_info.iter().take(10).enumerate() {
        println!(
            "  {}: {} ({} records)",
            i + 1,
            symbol_info.symbol,
            symbol_info.records_count
        );
    }

    if data_info.symbol_info.len() > 10 {
        println!(
            "  ... and {} more symbols",
            data_info.symbol_info.len() - 10
        );
    }

    // Strategy Selection
    println!("\n🎯 Available Strategies:");
    let strategies = list_strategies();
    for (i, strategy) in strategies.iter().enumerate() {
        println!("  {}) {} - {}", i + 1, strategy.name, strategy.description);
    }

    print!("\nSelect strategy (1-{}): ", strategies.len());
    io::stdout().flush()?;

    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    let choice: usize = input.trim().parse().unwrap_or(0);

    if choice == 0 || choice > strategies.len() {
        println!("❌ Invalid selection");
        return Ok(());
    }

    let selected_strategy = &strategies[choice - 1];
    println!("✅ Selected Strategy: {}", selected_strategy.name);

    // Trading pair selection
    println!("\n📊 Symbol Selection:");
    let available_symbols = data_info.get_available_symbols();

    // Display the first 10 symbols for quick selection
    for (i, symbol) in available_symbols.iter().take(10).enumerate() {
        let symbol_info = data_info.get_symbol_info(symbol).unwrap();
        println!(
            "  {}) {} ({} records)",
            i + 1,
            symbol,
            symbol_info.records_count
        );
    }

    print!(
        "\nSelect symbol (1-{}) or enter custom symbol: ",
        available_symbols.len().min(10)
    );
    io::stdout().flush()?;

    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    let input = input.trim();

    let symbol = if let Ok(choice) = input.parse::<usize>() {
        if choice > 0 && choice <= available_symbols.len().min(10) {
            available_symbols[choice - 1].clone()
        } else {
            println!("❌ Invalid selection");
            return Ok(());
        }
    } else if input.is_empty() {
        "BTCUSDT".to_string()
    } else {
        input.to_uppercase()
    };

    // Verify whether the selected transaction pair has data
    if !data_info.has_sufficient_data(&symbol, 100) {
        println!(
            "❌ Insufficient data for symbol: {} (minimum 100 records required)",
            symbol
        );
        return Ok(());
    }

    let symbol_info = data_info.get_symbol_info(&symbol).unwrap();
    println!(
        "✅ Selected Symbol: {} ({} records available)",
        symbol, symbol_info.records_count
    );

    // Data quantity selection
    print!(
        "\nEnter number of records to backtest (default: 10000, max: {}): ",
        symbol_info.records_count
    );
    io::stdout().flush()?;

    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    let data_count: i64 = if input.trim().is_empty() {
        10000.min(symbol_info.records_count as i64)
    } else {
        input
            .trim()
            .parse()
            .unwrap_or(10000)
            .min(symbol_info.records_count as i64)
    };

    // Initial Funding Setup
    print!("\nEnter initial capital (default: $10000): $");
    io::stdout().flush()?;

    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    let initial_capital = if input.trim().is_empty() {
        Decimal::from(10000)
    } else {
        Decimal::from_str(input.trim()).unwrap_or(Decimal::from(10000))
    };

    // Commission rate setting
    print!("\nEnter commission rate % (default: 0.1%): ");
    io::stdout().flush()?;

    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    let commission_rate = if input.trim().is_empty() {
        Decimal::from_str("0.001").unwrap() // 0.1%
    } else {
        let rate = input.trim().parse::<f64>().unwrap_or(0.1);
        Decimal::from_str(&format!("{}", rate / 100.0))
            .unwrap_or(Decimal::from_str("0.001").unwrap())
    };

    // Check if strategy supports OHLC
    let temp_strategy = create_strategy(&selected_strategy.id)?;
    if temp_strategy.supports_ohlc() {
        if let Some(timeframe) = temp_strategy.preferred_timeframe() {
            println!(
                "\n🔄 Strategy supports OHLC, using {} timeframe for better performance",
                timeframe.as_str()
            );

            // Estimate candle count needed (roughly data_count / 50, minimum 100)
            let candle_count = (data_count / 50).max(100) as u32;

            println!("🔍 Loading {} OHLC candles for {}...", candle_count, symbol);

            match repository
                .generate_recent_ohlc_for_backtest(&symbol, timeframe, candle_count)
                .await
            {
                Ok(ohlc_data) if !ohlc_data.is_empty() => {
                    println!("✅ Loaded {} OHLC candles", ohlc_data.len());
                    println!(
                        "📅 Data range: {} to {}",
                        ohlc_data
                            .first()
                            .unwrap()
                            .timestamp
                            .format("%Y-%m-%d %H:%M:%S"),
                        ohlc_data
                            .last()
                            .unwrap()
                            .timestamp
                            .format("%Y-%m-%d %H:%M:%S")
                    );

                    let config =
                        BacktestConfig::new(initial_capital).with_commission_rate(commission_rate);

                    let strategy = create_strategy(&selected_strategy.id)?;

                    println!("\n{}", "=".repeat(60));
                    let mut engine = BacktestEngine::new(strategy, config)?;
                    let result = engine.run_with_ohlc(ohlc_data);

                    // Show results
                    println!("\n");
                    result.print_summary();

                    // Ask whether to display detailed transaction analysis
                    print!("\nShow detailed trade analysis? (y/N): ");
                    io::stdout().flush()?;

                    let mut input = String::new();
                    io::stdin().read_line(&mut input)?;
                    if input.trim().to_lowercase() == "y" || input.trim().to_lowercase() == "yes" {
                        result.print_trade_analysis();
                    }

                    println!("\n🎉 Backtest completed successfully!");
                    return Ok(());
                }
                Ok(_) => {
                    println!(
                        "⚠️ No OHLC data available for timeframe {}, falling back to tick data",
                        timeframe.as_str()
                    );
                }
                Err(e) => {
                    println!(
                        "⚠️ OHLC generation failed: {}, falling back to tick data",
                        e
                    );
                }
            }
        }
    }

    // Fallback to tick data (original logic)
    println!(
        "\n🔍 Loading historical tick data: {} latest {} records...",
        symbol, data_count
    );

    let data = repository
        .get_recent_ticks_for_backtest(&symbol, data_count)
        .await?;

    if data.is_empty() {
        println!("❌ No historical data found for symbol: {}", symbol);
        return Ok(());
    }

    println!("✅ Loaded {} tick data points", data.len());
    println!(
        "📅 Data range: {} to {}",
        data.first().unwrap().timestamp.format("%Y-%m-%d %H:%M:%S"),
        data.last().unwrap().timestamp.format("%Y-%m-%d %H:%M:%S")
    );

    let config = BacktestConfig::new(initial_capital).with_commission_rate(commission_rate);

    let strategy = create_strategy(&selected_strategy.id)?;

    println!("\n{}", "=".repeat(60));
    let mut engine = BacktestEngine::new(strategy, config)?;
    let result = engine.run(data);

    // Show results
    println!("\n");
    result.print_summary();

    // Ask whether to display detailed transaction analysis
    print!("\nShow detailed trade analysis? (y/N): ");
    io::stdout().flush()?;

    let mut input = String::new();
    io::stdin().read_line(&mut input)?;
    if input.trim().to_lowercase() == "y" || input.trim().to_lowercase() == "yes" {
        result.print_trade_analysis();
    }

    println!("\n🎉 Backtest completed successfully!");

    Ok(())
}

/// Initialize application environment and logging
async fn init_application() -> Result<(), Box<dyn std::error::Error>> {
    // 根据编译模式自动选择环境:
    // - cargo run (debug) → development环境
    // - cargo run --release (release) → production环境
    let env_file = if cfg!(debug_assertions) {
        ".env.development"
    } else {
        ".env.production"
    };

    // 允许通过 RUN_MODE 环境变量覆盖
    let override_mode = std::env::var("RUN_MODE").ok();
    let env_file = match override_mode.as_deref() {
        Some("production") | Some("prod") => ".env.production",
        Some("test") => ".env.test",
        Some("development") | Some("dev") => ".env.development",
        _ => env_file,
    };

    println!("Loading config: {}", env_file);

    // Load environment variables from .env file
    if let Err(_) = dotenv::from_filename(env_file) {
        println!("Warning: {} not found, trying .env", env_file);
        dotenv::dotenv().ok();
    }

    info!("🔧 Application environment initialized");
    Ok(())
}

/// Initialize tracing subscriber for logging
fn init_tracing(log_level: &str) -> Result<(), Box<dyn std::error::Error>> {
    // 优先使用环境变量 RUST_LOG，否则使用配置文件的 log_level
    // sqlx 慢查询日志默认关闭（设为 error 只在真正出错时才打印）
    let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| {
        let level = match log_level {
            "trace" => "trading_core=trace,sqlx=debug,tokio=trace,hyper=debug",
            "debug" => "trading_core=debug,sqlx=warn,tokio=info,hyper=info",
            "info" => "trading_core=info,sqlx=error,tokio=warn,hyper=warn",
            "warn" => "trading_core=warn,sqlx=error,tokio=error,hyper=error",
            "error" => "trading_core=error,sqlx=error,tokio=error,hyper=error",
            _ => "trading_core=info,sqlx=error,tokio=warn,hyper=warn",
        };
        EnvFilter::new(level)
    });

    // Setup tracing subscriber with structured logging
    tracing_subscriber::registry()
        .with(env_filter)
        .with(
            fmt::layer()
                .with_target(true)
                .with_thread_ids(true)
                .with_file(true)
                .with_line_number(true)
                .compact(),
        )
        .init();

    Ok(())
}

/// Main application runtime (original live mode)
async fn run_live_application(settings: Settings) -> Result<(), Box<dyn std::error::Error>> {
    // Validate basic configuration
    if settings.symbols.is_empty() {
        error!("❌ No symbols configured for monitoring");
        std::process::exit(1);
    }

    if settings.database.max_connections == 0 {
        error!("❌ Database max_connections must be greater than 0");
        std::process::exit(1);
    }

    // Create database connection pool
    info!("🔌 Connecting to database...");
    let pool = create_database_pool(&settings).await?;

    // Test database connectivity
    test_database_connection(&pool).await?;
    info!("✅ Database connection established");

    // Create cache
    info!("💾 Initializing cache...");
    let cache = create_cache(&settings).await?;
    info!("✅ Cache initialized");

    // Create repository
    let repository = Arc::new(TickDataRepository::new(pool, cache));

    // Create exchange
    info!("📡 Initializing exchange connection...");
    let exchange: Arc<dyn exchange::Exchange> = Arc::new(
        BinanceExchange::with_futures_symbols(settings.futures_symbols.clone())
    );
    info!("✅ Exchange connection ready (futures: {:?})", &settings.futures_symbols);

    // Create market data service
    let service = MarketDataService::new(exchange, repository, settings.symbols.clone());

    info!(
        "🎯 Starting market data collection for {} symbols",
        settings.symbols.len()
    );

    // Setup signal forwarding to service
    let service_shutdown_tx = service.get_shutdown_tx();
    tokio::spawn(async move {
        signal::ctrl_c().await.expect("Failed to listen for ctrl-c");
        println!("\nReceived Ctrl+C signal, forwarding to service...");
        info!("Received Ctrl+C signal, forwarding to service");
        let _ = service_shutdown_tx.send(());
    });

    // Start service and wait for completion
    match service.start().await {
        Ok(()) => {
            info!("✅ Service stopped successfully");
            Ok(())
        }
        Err(e) => {
            error!("❌ Service stopped with error: {}", e);
            Err(Box::new(e))
        }
    }
}

/// Create database connection pool
async fn create_database_pool(settings: &Settings) -> Result<PgPool, Box<dyn std::error::Error>> {
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(settings.database.max_connections)
        .min_connections(settings.database.min_connections)
        .max_lifetime(Duration::from_secs(settings.database.max_lifetime))
        .acquire_timeout(Duration::from_secs(30))
        .idle_timeout(Duration::from_secs(600))
        .connect(&settings.database.url)
        .await?;

    Ok(pool)
}

/// Test database connection
async fn test_database_connection(pool: &PgPool) -> Result<(), Box<dyn std::error::Error>> {
    // Simple connectivity test
    sqlx::query("SELECT 1").execute(pool).await?;

    // Check if tick_data table exists
    let table_exists = sqlx::query_scalar::<_, bool>(
        "SELECT EXISTS (
            SELECT FROM information_schema.tables 
            WHERE table_schema = 'public' 
            AND table_name = 'tick_data'
        )",
    )
    .fetch_one(pool)
    .await?;

    if !table_exists {
        error!("❌ Required table 'tick_data' does not exist in database");
        error!("💡 Please run the database migration scripts first");
        std::process::exit(1);
    }

    info!("✅ Database schema validation passed");
    Ok(())
}

/// Create cache instance (original live mode)
async fn create_cache(settings: &Settings) -> Result<TieredCache, Box<dyn std::error::Error>> {
    let memory_config = (
        settings.cache.memory.max_ticks_per_symbol,
        settings.cache.memory.ttl_seconds,
    );

    let redis_config = (
        settings.cache.redis.url.as_str(),
        settings.cache.redis.max_ticks_per_symbol,
        settings.cache.redis.ttl_seconds,
    );

    let cache = TieredCache::new(memory_config, redis_config).await?;

    // Test cache connectivity
    test_cache_connection(&cache).await?;

    Ok(cache)
}

/// Create simplified cache for backtest mode
async fn create_backtest_cache(
    settings: &Settings,
) -> Result<TieredCache, Box<dyn std::error::Error>> {
    // Creating a minimal cache configuration for backtesting
    let memory_config = (10, 60);
    let redis_config = (settings.cache.redis.url.as_str(), 10, 60);

    let cache = TieredCache::new(memory_config, redis_config).await?;

    // Simple connection test (not required to be completely normal, because backtesting mainly uses the database)
    if let Err(e) = test_cache_connection(&cache).await {
        warn!("⚠️ Cache test failed (this is OK for backtest mode): {}", e);
    }

    Ok(cache)
}

/// Test cache connection
async fn test_cache_connection(cache: &TieredCache) -> Result<(), Box<dyn std::error::Error>> {
    // Test cache by getting symbols (should return empty list initially)
    cache.get_symbols().await?;
    info!("✅ Cache connectivity test passed");
    Ok(())
}

/// Create database pool for service mode (separate instance)
async fn create_database_pool_for_service() -> Result<PgPool, Box<dyn std::error::Error>> {
    let settings = Settings::new()?;
    create_database_pool(&settings).await
}

/// Create cache for service mode (separate instance)
async fn create_cache_for_service() -> Result<TieredCache, Box<dyn std::error::Error>> {
    let settings = Settings::new()?;
    create_cache(&settings).await
}
