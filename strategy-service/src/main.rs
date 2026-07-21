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

use std::sync::Arc;
use anyhow::Result;
use tracing::{info, error, warn};
use tracing_subscriber::{fmt, EnvFilter};

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

    // 初始化 Redis 连接
    let redis_conn = redis_reader::create_connection(&config.redis).await?;
    info!("Redis connected");

    // 初始化 WebSocket 状态
    let ws_state = Arc::new(websocket::WsState::new());
    info!("WebSocket state initialized");

    // 初始化告警管理器
    let alert_config = alert::AlertConfig::default();
    let alert_manager = Arc::new(alert::AlertManager::new(alert_config));
    info!("Alert manager initialized");

    // 启动策略执行引擎
    let engine_handle = {
        let pool = db_pool.clone();
        let redis = redis_conn.clone();
        let interval = config.engine.poll_interval_secs;
        let ws = ws_state.clone();
        let alert = alert_manager.clone();
        tokio::spawn(async move {
            if let Err(e) = engine::run(pool, redis, interval, Some(ws), Some(alert)).await {
                error!("Strategy engine error: {}", e);
            }
        })
    };

    // 启动 HTTP + WebSocket 服务
    let ws_router = websocket::create_ws_router(ws_state.clone());
    let app = api::create_router(db_pool.clone()).merge(ws_router);

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
