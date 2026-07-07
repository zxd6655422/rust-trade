#![cfg_attr(
    all(not(debug_assertions), target_os = "windows"),
    windows_subsystem = "windows"
)]

mod commands;
mod state;
mod types;

use commands::*;
use state::AppState;

fn main() {
    let env_file = determine_env_file();

    println!("Loading config: {}", env_file);

    if let Err(_) = dotenvy::from_filename(env_file) {
        println!("Warning: {} not found, trying .env", env_file);
        if let Err(_) = dotenvy::dotenv() {
            println!("Warning: No .env file found");
        }
    }

    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .with_file(true)
        .with_line_number(true)
        .init();

    tracing::info!("Trading Core Tauri Application starting...");

    let runtime = match tokio::runtime::Runtime::new() {
        Ok(rt) => {
            tracing::info!("Tokio runtime created successfully");
            rt
        }
        Err(e) => {
            tracing::error!("Failed to create Tokio runtime: {}", e);
            std::process::exit(1);
        }
    };

    let app_state = runtime.block_on(async {
        match AppState::new().await {
            Ok(state) => {
                tracing::info!("App state initialized successfully");
                state
            }
            Err(e) => {
                tracing::error!("Failed to initialize app state: {}", e);
                tracing::error!("Please check your configuration:");
                tracing::error!("1. Ensure .env file exists with DATABASE_URL and REDIS_URL");
                tracing::error!("2. Ensure PostgreSQL is running and accessible");
                tracing::error!("3. Ensure Redis is running (optional but recommended)");
                tracing::error!("4. Ensure trading_core database and tick_data table exist");
                std::process::exit(1);
            }
        }
    });

    let result = tauri::Builder::default()
        .manage(app_state)
        .invoke_handler(tauri::generate_handler![
            get_data_info,
            get_available_strategies,
            run_backtest,
            get_historical_data,
            validate_backtest_config,
            get_strategy_capabilities,
            get_ohlc_preview,
            // P8: 实时行情
            get_realtime_prices,
            get_kline_history,
            get_24h_stats,
            // P9: 持仓和交易记录
            get_positions,
            get_trade_history,
            get_pnl_summary,
            // P10: 统计分析
            get_equity_curve,
            get_performance_metrics,
            get_commission_stats,
            // P11: 高级回测
            run_multi_timeframe_backtest,
            run_walk_forward_test,
            run_out_of_sample_test,
            run_multi_symbol_backtest,
            analyze_market_state,
            // Paper Trading: 模拟交易
            start_paper_trading,
            stop_paper_trading,
            get_paper_status,
            place_paper_order,
            get_paper_trades,
            get_paper_pending_orders,
            cancel_paper_order,
            reset_paper_trading,
            // 策略实时分析
            get_strategy_analysis,
            get_signal_history,
            get_signal_stats,
            // 交易对管理
            get_symbols,
            add_symbol,
            remove_symbol,
            toggle_symbol,
            // 数据管理
            get_collection_status,
            get_all_collection_status,
            add_symbol_with_collection,
            archive_symbol_data,
            archive_all_symbols,
            // 交易对配置 (trading_pairs)
            get_trading_pairs,
            add_trading_pair,
            update_trading_pair_status,
            delete_trading_pair,
            get_available_symbols_from_data,
            add_to_monitor,
            remove_from_monitor,
            // 策略调度器控制
            get_scheduler_status,
            pause_scheduler,
            resume_scheduler,
            // 系统状态
            check_trading_core_status
        ])
        .setup(|app| {
            tracing::info!("Tauri setup started");
            #[cfg(debug_assertions)]
            {
                let app_handle = app.handle();
                if let Err(e) = app_handle.plugin(tauri_plugin_shell::init()) {
                    tracing::warn!("Failed to initialize shell plugin: {}", e);
                }
                tracing::info!("Debug plugins initialized");
            }
            tracing::info!("Tauri setup completed");
            Ok(())
        })
        .run(tauri::generate_context!());

    match result {
        Ok(_) => tracing::info!("Application exited normally"),
        Err(e) => {
            tracing::error!("Application error: {}", e);
            std::process::exit(1);
        }
    }
}

fn determine_env_file() -> &'static str {
    // 1. 优先使用 RUN_MODE 环境变量
    if let Ok(mode) = std::env::var("RUN_MODE") {
        return match mode.as_str() {
            "production" | "prod" => ".env.production",
            "test" => ".env.test",
            _ => ".env.development",
        };
    }

    // 2. 根据编译模式自动选择
    if cfg!(debug_assertions) {
        ".env.development"  // cargo run → dev
    } else {
        ".env.production"   // cargo run --release → prod
    }
}
