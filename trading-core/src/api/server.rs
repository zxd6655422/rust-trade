// api/server.rs
// Web 服务器

use actix_web::{web, App, HttpServer, middleware};
use actix_cors::Cors;
use sqlx::PgPool;
use std::sync::Arc;
use tokio::sync::{broadcast, Mutex};
use tracing::{error, info};

use trading_common::data::repository::TickDataRepository;
use trading_common::data::types::TickData;

use super::handlers::{self, AppState};
use super::websocket;

/// API 服务器配置
pub struct ApiServerConfig {
    pub host: String,
    pub port: u16,
}

impl Default for ApiServerConfig {
    fn default() -> Self {
        Self {
            host: "0.0.0.0".to_string(),
            port: 8080,
        }
    }
}

/// API 服务器
pub struct ApiServer {
    config: ApiServerConfig,
    repository: Arc<TickDataRepository>,
    tick_tx: broadcast::Sender<TickData>,
    pool: PgPool,
}

impl ApiServer {
    pub fn new(
        config: ApiServerConfig,
        repository: Arc<TickDataRepository>,
        tick_tx: broadcast::Sender<TickData>,
        pool: PgPool,
    ) -> Self {
        Self {
            config,
            repository,
            tick_tx,
            pool,
        }
    }

    /// 启动服务器
    pub async fn start(&self) -> std::io::Result<()> {
        let repository = self.repository.clone();
        let tick_tx = self.tick_tx.clone();
        let pool = self.pool.clone();
        let host = self.config.host.clone();
        let port = self.config.port;

        info!("Starting API server on {}:{}", host, port);

        let account_repo = Arc::new(trading_common::data::account_repository::AccountRepository::new(pool.clone()));

        let app_state = web::Data::new(AppState {
            repository,
            backtest_lock: Arc::new(Mutex::new(())),
            pool,
            account_repo: Some(account_repo),
        });

        let tick_tx_data = web::Data::new(tick_tx);

        HttpServer::new(move || {
            let cors = Cors::default()
                .allow_any_origin()
                .allow_any_method()
                .allow_any_header()
                .max_age(3600);

            App::new()
                .app_data(app_state.clone())
                .app_data(tick_tx_data.clone())
                // 中间件
                .wrap(middleware::Logger::default())
                .wrap(cors)
                // 健康检查
                .route("/health", web::get().to(handlers::health_check))
                // API 路由
                .service(
                    web::scope("/api")
                        .route("/data/info", web::get().to(handlers::get_data_info))
                        .route("/strategies", web::get().to(handlers::get_strategies))
                        // 回测相关 API
                        .route("/backtest", web::post().to(handlers::run_backtest))
                        .route("/backtest/multi-timeframe", web::post().to(handlers::run_multi_timeframe_backtest))
                        .route("/backtest/walk-forward", web::post().to(handlers::run_walk_forward_backtest))
                        .route("/backtest/out-of-sample", web::post().to(handlers::run_out_of_sample_backtest))
                        .route("/backtest/multi-symbol", web::post().to(handlers::run_multi_symbol_backtest))
                        // 回测历史查询 API
                        .route("/backtest/history/{instance_id}", web::get().to(handlers::get_backtest_history_by_instance))
                        .route("/backtest/detail/{id}", web::get().to(handlers::get_backtest_detail))
                        .route("/backtest/stats/{instance_id}", web::get().to(handlers::get_backtest_stats))
                        // 市场分析 API
                        .route("/analysis/market-state", web::post().to(handlers::analyze_market_state))
                        // 账户信息 API
                        .route("/account/overview", web::get().to(handlers::get_account_overview))
                        .route("/account/snapshot", web::get().to(handlers::get_account_snapshot))
                        .route("/account/balances", web::get().to(handlers::get_account_balances))
                        .route("/account/positions", web::get().to(handlers::get_account_positions))
                        .route("/account/history", web::get().to(handlers::get_account_history))
                        .route("/account/uids", web::get().to(handlers::get_account_uids))
                        // 交易事件 API
                        .route("/events/trades", web::get().to(handlers::get_trade_logs))
                        .route("/events/risk", web::get().to(handlers::get_risk_logs))
                        .route("/events/timeline", web::get().to(handlers::get_event_timeline)),
                )
                // WebSocket
                .route("/ws", web::get().to(websocket::ws_handler))
        })
        .bind(format!("{}:{}", host, port))?
        .run()
        .await
    }
}
