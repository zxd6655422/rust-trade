mod config;
mod db;
pub mod decision_engine;
mod redis_reader;
mod strategies;
mod engine;
mod api;
pub mod indicators;
pub mod websocket;
pub mod alert;
pub mod exchange;
pub mod kline_store;
pub mod kline_loader;
pub mod ws_feed;

use std::sync::Arc;
use anyhow::Result;
use tokio::sync::RwLock;
use tracing::{info, error, warn};
use tracing_subscriber::{fmt, EnvFilter};

use kline_store::KlineManager;
use kline_loader::{collect_data_requirements, hybrid_load};

#[tokio::main]
async fn main() -> Result<()> {
    // 初始化日志
    fmt()
        .with_env_filter(EnvFilter::from_default_env().add_directive("strategy_service=info".parse()?))
        .init();

    info!("Starting strategy-service...");

    // 加载 .env 文件（统一从 config/ 目录加载）
    let run_mode = std::env::var("RUN_MODE").unwrap_or_else(|_| "development".to_string());
    let env_file = match run_mode.as_str() {
        "production" | "prod" => "config/.env.production",
        "test" => "config/.env.test",
        _ => "config/.env.development",
    };

    // 尝试加载环境特定的 .env 文件，失败则加载 config/.env
    if dotenv::from_filename(env_file).is_err() {
        if dotenv::from_filename("config/.env").is_err() {
            dotenv::dotenv().ok();
        }
    }

    info!("Loading config from: {}", env_file);

    // 加载配置
    let config = config::AppConfig::load()?;
    info!("Config loaded: {:?}", config);

    // 初始化数据库连接池
    let db_pool = db::create_pool(&config.database).await?;
    info!("Database connected");

    // 初始化 Redis 连接（保留用于其他模块）
    let redis_conn = redis_reader::create_connection(&config.redis).await?;
    info!("Redis connected");

    // ============================================================
    // Phase 1: KlineManager 初始化 — 混合加载
    // ============================================================
    info!("Initializing KlineManager...");

    // 1. 查询所有活跃策略
    let active_strategies = db::strategies::list_active_strategies(&db_pool).await?;
    info!("Found {} active strategies", active_strategies.len());

    // 2. 收集所有 (symbol, timeframe, market_type) 对并计算 max_bars
    let (pairs, max_bars) = collect_data_requirements(&active_strategies);
    let max_bars = max_bars.max(config.kline.default_max_bars);

    info!(
        "Data requirements: {} pairs, max_bars={}",
        pairs.len(),
        max_bars
    );
    for (symbol, tf, market_type) in &pairs {
        info!("  - {} {} ({})", symbol, tf.as_str(), market_type);
    }

    // 创建 KlineManager 并初始化 stores
    let mut manager = KlineManager::new(max_bars);
    manager.init_stores(&pairs);

    // 从交易所加载数据（按策略配置的市场）
    for (symbol, tf, market_type) in &pairs {
        if let Err(e) = hybrid_load(
            &mut manager,
            symbol,
            *tf,
            max_bars,
            market_type,
        )
        .await
        {
            error!(
                "Failed to load data for {} {} ({}): {}",
                symbol,
                tf.as_str(),
                market_type,
                e
            );
        }
    }

    // 打印加载结果（用于启动时检查 K 线是否加载完整）
    let mut loaded_complete = 0usize;
    for (symbol, tf, market_type) in &pairs {
        match manager.get(symbol, *tf, market_type) {
            Some(store) => {
                let total = store.total_count();
                if total >= max_bars {
                    loaded_complete += 1;
                    info!("[K线] {} {} ({}) 加载完整: {} bars (已收盘 {})", symbol, tf.as_str(), market_type, total, store.closed_count());
                } else {
                    warn!("[K线] {} {} ({}) 加载不完整: {} / {} bars (已收盘 {})", symbol, tf.as_str(), market_type, total, max_bars, store.closed_count());
                }
            }
            None => {
                warn!("[K线] {} {} ({}) store 未创建", symbol, tf.as_str(), market_type);
            }
        }
    }
    info!("[K线] 加载完成: {}/{} 个 store 数据完整 (target={} bars)", loaded_complete, pairs.len(), max_bars);

    let manager = Arc::new(RwLock::new(manager));

    // 启动引擎、WS 数据源和 HTTP 服务
    start_services(config, db_pool, redis_conn, manager, pairs).await?;

    Ok(())
}

async fn start_services(
    config: config::AppConfig,
    db_pool: sqlx::PgPool,
    _redis_conn: redis::aio::ConnectionManager,
    kline_manager: Arc<RwLock<KlineManager>>,
    ws_subscriptions: Vec<(String, redis_reader::Timeframe, String)>,
) -> Result<()> {
    // 初始化 WebSocket 状态（信号广播）
    let ws_state = Arc::new(websocket::WsState::new());
    info!("WebSocket state initialized");

    // 初始化告警管理器
    let alert_config = alert::AlertConfig::default();
    let alert_manager = Arc::new(alert::AlertManager::new(alert_config));
    info!("Alert manager initialized");

    // ============================================================
    // Phase 2: 启动 Binance WebSocket 实时数据源
    // ============================================================
    if !ws_subscriptions.is_empty() {
        info!(
            "Starting Binance WebSocket feed with {} subscriptions...",
            ws_subscriptions.len()
        );
        let _ws_feed_receiver = ws_feed::start_ws_feed(
            ws_subscriptions,
            kline_manager.clone(),
        )
        .await;
        info!("Binance WebSocket feed started");
    }

    // ============================================================
    // Phase 3: 启动健康检查（每 60 秒，快速发现过旧数据）
    // ============================================================
    let _health_handle = kline_loader::start_health_check(
        kline_manager.clone(),
        60, // 1 分钟
    );
    info!("Health check started (interval: 60s)");

    // ============================================================
    // Phase 4: 启动动态 Store 管理（每 60 秒）
    // ============================================================
    let _dynamic_handle = kline_loader::start_dynamic_manager(
        db_pool.clone(),
        kline_manager.clone(),
        60, // 1 分钟
    );
    info!("Dynamic store manager started (interval: 60s)");

    // 启动策略执行引擎（使用 KlineManager）
    let engine_handle = {
        let pool = db_pool.clone();
        let km = kline_manager.clone();
        let interval = config.engine.poll_interval_secs;
        let ws = ws_state.clone();
        let alert = alert_manager.clone();
        tokio::spawn(async move {
            if let Err(e) = engine::run(pool, km, interval, Some(ws), Some(alert)).await {
                error!("Strategy engine error: {}", e);
            }
        })
    };

    // 启动 HTTP + WebSocket 服务
    let ws_router = websocket::create_ws_router(ws_state.clone());
    let app = api::create_router(db_pool.clone(), Some(kline_manager.clone())).merge(ws_router);

    let listener = tokio::net::TcpListener::bind(&format!("{}:{}", config.server.host, config.server.port)).await?;
    info!("HTTP server listening on {}:{}", config.server.host, config.server.port);
    info!("WebSocket available at ws://{}:{}/ws/signals", config.server.host, config.server.port);

    // 并行运行所有服务
    tokio::select! {
        result = axum::serve(listener, app) => {
            if let Err(e) = result {
                error!("HTTP server error: {}", e);
            }
        }
        result = engine_handle => {
            if let Err(e) = result {
                error!("Engine task error: {}", e);
            }
        }
    }

    Ok(())
}
