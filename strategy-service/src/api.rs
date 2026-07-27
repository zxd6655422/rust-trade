use std::sync::Arc;
use axum::{
    extract::{Path, Query},
    http::StatusCode,
    response::Json,
    routing::{get, put},
    Router,
};
use serde::Deserialize;
use sqlx::PgPool;
use tokio::sync::RwLock;
use uuid::Uuid;

use crate::db::{signals, strategies, trades, performance};
use crate::kline_store::KlineManager;

pub fn create_router(pool: PgPool, kline_manager: Option<Arc<RwLock<KlineManager>>>) -> Router {
    let km = kline_manager.unwrap_or_else(|| Arc::new(RwLock::new(KlineManager::new(0))));

    Router::new()
        // 策略管理
        .route("/api/strategies", get(list_strategies).post(create_strategy))
        .route(
            "/api/strategies/:id",
            get(get_strategy).put(update_strategy).delete(delete_strategy),
        )
        .route("/api/strategies/:id/status", put(update_strategy_status))
        // 策略选择和默认策略
        .route("/api/strategies/selectable", get(get_selectable_strategies))
        .route("/api/strategies/defaults", get(get_all_defaults))
        .route("/api/strategies/defaults/:default_for", get(get_default_by_scenario))
        .route("/api/strategies/:id/set-default/:default_for", put(set_default_strategy))
        .route("/api/strategies/:id/unset-default", put(unset_default_strategy))
        // 信号查询
        .route("/api/signals", get(query_signals))
        .route("/api/strategies/:id/signals", get(get_strategy_signals))
        // 交易记录
        .route("/api/trades", get(query_trades))
        .route("/api/strategies/:id/trades", get(get_strategy_trades))
        // 策略统计
        .route("/api/strategies/:id/performance", get(get_performance))
        .route("/api/strategies/:id/summary", get(get_summary))
        // K线内存状态
        .route("/api/kline-status", get(kline_status))
        .with_state((pool, km))
}

// ==================== 策略管理 ====================

type AppState = (PgPool, Arc<RwLock<KlineManager>>);

async fn list_strategies(
    axum::extract::State((pool, _)): axum::extract::State<AppState>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    match strategies::list_strategies(&pool).await {
        Ok(strategies) => Ok(Json(serde_json::json!({
            "success": true,
            "data": strategies
        }))),
        Err(e) => {
            tracing::error!("Failed to list strategies: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

async fn get_strategy(
    axum::extract::State((pool, _)): axum::extract::State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    match strategies::get_strategy(&pool, id).await {
        Ok(Some(strategy)) => Ok(Json(serde_json::json!({
            "success": true,
            "data": strategy
        }))),
        Ok(None) => Err(StatusCode::NOT_FOUND),
        Err(e) => {
            tracing::error!("Failed to get strategy: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

async fn create_strategy(
    axum::extract::State((pool, _)): axum::extract::State<AppState>,
    Json(req): Json<strategies::CreateStrategyRequest>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    match strategies::create_strategy(&pool, req).await {
        Ok(strategy) => Ok(Json(serde_json::json!({
            "success": true,
            "data": strategy
        }))),
        Err(e) => {
            tracing::error!("Failed to create strategy: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

async fn update_strategy(
    axum::extract::State((pool, _)): axum::extract::State<AppState>,
    Path(id): Path<Uuid>,
    Json(req): Json<strategies::UpdateStrategyRequest>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    match strategies::update_strategy(&pool, id, req).await {
        Ok(Some(strategy)) => Ok(Json(serde_json::json!({
            "success": true,
            "data": strategy
        }))),
        Ok(None) => Err(StatusCode::NOT_FOUND),
        Err(e) => {
            tracing::error!("Failed to update strategy: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

async fn update_strategy_status(
    axum::extract::State((pool, _)): axum::extract::State<AppState>,
    Path(id): Path<Uuid>,
    Json(req): Json<strategies::UpdateStatusRequest>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    match strategies::update_strategy_status(&pool, id, &req.status).await {
        Ok(Some(strategy)) => Ok(Json(serde_json::json!({
            "success": true,
            "data": strategy
        }))),
        Ok(None) => Err(StatusCode::NOT_FOUND),
        Err(e) => {
            tracing::error!("Failed to update strategy status: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

async fn delete_strategy(
    axum::extract::State((pool, _)): axum::extract::State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    match strategies::delete_strategy(&pool, id).await {
        Ok(true) => Ok(Json(serde_json::json!({
            "success": true,
            "message": "Strategy deleted"
        }))),
        Ok(false) => Err(StatusCode::NOT_FOUND),
        Err(e) => {
            tracing::error!("Failed to delete strategy: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

// ==================== 信号查询 ====================

#[derive(Debug, Deserialize)]
struct SignalQueryParams {
    strategy_id: Option<String>,
    instance_id: Option<Uuid>,
    symbol: Option<String>,
    start: Option<chrono::DateTime<chrono::Utc>>,
    end: Option<chrono::DateTime<chrono::Utc>>,
    status: Option<String>,
    limit: Option<i64>,
}

async fn query_signals(
    axum::extract::State((pool, _)): axum::extract::State<AppState>,
    Query(params): Query<SignalQueryParams>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let query = signals::SignalQuery {
        strategy_id: params.strategy_id,
        instance_id: params.instance_id,
        symbol: params.symbol,
        start: params.start,
        end: params.end,
        status: params.status,
        limit: params.limit,
    };

    match signals::query_signals(&pool, query).await {
        Ok(signals) => Ok(Json(serde_json::json!({
            "success": true,
            "data": signals
        }))),
        Err(e) => {
            tracing::error!("Failed to query signals: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

async fn get_strategy_signals(
    axum::extract::State((pool, _)): axum::extract::State<AppState>,
    Path(id): Path<Uuid>,
    Query(params): Query<SignalQueryParams>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    match signals::get_signals_by_instance(&pool, id, params.limit).await {
        Ok(signals) => Ok(Json(serde_json::json!({
            "success": true,
            "data": signals
        }))),
        Err(e) => {
            tracing::error!("Failed to get strategy signals: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

// ==================== 交易记录 ====================

#[derive(Debug, Deserialize)]
struct TradeQueryParams {
    strategy_id: Option<String>,
    signal_id: Option<Uuid>,
    symbol: Option<String>,
    start: Option<chrono::DateTime<chrono::Utc>>,
    end: Option<chrono::DateTime<chrono::Utc>>,
    limit: Option<i64>,
}

async fn query_trades(
    axum::extract::State((pool, _)): axum::extract::State<AppState>,
    Query(params): Query<TradeQueryParams>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let query = trades::TradeQuery {
        strategy_id: params.strategy_id,
        signal_id: params.signal_id,
        symbol: params.symbol,
        start: params.start,
        end: params.end,
        limit: params.limit,
    };

    match trades::query_trades(&pool, query).await {
        Ok(trades) => Ok(Json(serde_json::json!({
            "success": true,
            "data": trades
        }))),
        Err(e) => {
            tracing::error!("Failed to query trades: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

async fn get_strategy_trades(
    axum::extract::State((pool, _)): axum::extract::State<AppState>,
    Path(id): Path<Uuid>,
    Query(params): Query<TradeQueryParams>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    match trades::get_trades_by_instance(&pool, id, params.limit).await {
        Ok(trades) => Ok(Json(serde_json::json!({
            "success": true,
            "data": trades
        }))),
        Err(e) => {
            tracing::error!("Failed to get strategy trades: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

// ==================== 策略统计 ====================

#[derive(Debug, Deserialize)]
struct PerformanceQueryParams {
    start: Option<chrono::DateTime<chrono::Utc>>,
    end: Option<chrono::DateTime<chrono::Utc>>,
}

async fn get_performance(
    axum::extract::State((pool, _)): axum::extract::State<AppState>,
    Path(id): Path<Uuid>,
    Query(params): Query<PerformanceQueryParams>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    match performance::get_performance(&pool, id, params.start, params.end).await {
        Ok(performances) => Ok(Json(serde_json::json!({
            "success": true,
            "data": performances
        }))),
        Err(e) => {
            tracing::error!("Failed to get performance: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

async fn get_summary(
    axum::extract::State((pool, _)): axum::extract::State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    match performance::get_summary(&pool, id).await {
        Ok(Some(summary)) => Ok(Json(serde_json::json!({
            "success": true,
            "data": summary
        }))),
        Ok(None) => Ok(Json(serde_json::json!({
            "success": true,
            "data": null,
            "message": "No trades found for this strategy"
        }))),
        Err(e) => {
            tracing::error!("Failed to get summary: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

// ==================== 策略选择和默认策略 ====================

#[derive(Debug, Deserialize)]
struct SelectableQueryParams {
    market_type: Option<String>,
}

/// 获取可用于选择的策略列表（活跃状态）
async fn get_selectable_strategies(
    axum::extract::State((pool, _)): axum::extract::State<AppState>,
    Query(params): Query<SelectableQueryParams>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    match strategies::list_selectable_strategies(&pool, params.market_type.as_deref()).await {
        Ok(strategies) => Ok(Json(serde_json::json!({
            "success": true,
            "data": strategies
        }))),
        Err(e) => {
            tracing::error!("Failed to list selectable strategies: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

/// 获取所有默认策略配置
async fn get_all_defaults(
    axum::extract::State((pool, _)): axum::extract::State<AppState>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    match strategies::get_all_default_strategies(&pool).await {
        Ok(strategies) => {
            // 按场景分组
            let mut defaults = serde_json::Map::new();
            for strategy in strategies {
                if let Some(ref default_for) = strategy.default_for {
                    defaults.insert(default_for.clone(), serde_json::json!(strategy));
                }
            }
            Ok(Json(serde_json::json!({
                "success": true,
                "data": defaults
            })))
        }
        Err(e) => {
            tracing::error!("Failed to get default strategies: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

/// 获取指定场景的默认策略
async fn get_default_by_scenario(
    axum::extract::State((pool, _)): axum::extract::State<AppState>,
    Path(default_for): Path<String>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    match strategies::get_default_strategy(&pool, &default_for).await {
        Ok(Some(strategy)) => Ok(Json(serde_json::json!({
            "success": true,
            "data": strategy
        }))),
        Ok(None) => Ok(Json(serde_json::json!({
            "success": true,
            "data": null,
            "message": format!("No default strategy configured for '{}'", default_for)
        }))),
        Err(e) => {
            tracing::error!("Failed to get default strategy: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

/// 设置策略为某个场景的默认策略
async fn set_default_strategy(
    axum::extract::State((pool, _)): axum::extract::State<AppState>,
    Path((id, default_for)): Path<(Uuid, String)>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    match strategies::set_default_strategy(&pool, id, &default_for).await {
        Ok(Some(strategy)) => Ok(Json(serde_json::json!({
            "success": true,
            "data": strategy,
            "message": format!("Strategy set as default for '{}'", default_for)
        }))),
        Ok(None) => Err(StatusCode::NOT_FOUND),
        Err(e) => {
            tracing::error!("Failed to set default strategy: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

/// 取消策略的默认设置
async fn unset_default_strategy(
    axum::extract::State((pool, _)): axum::extract::State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    match strategies::unset_default_strategy(&pool, id).await {
        Ok(Some(strategy)) => Ok(Json(serde_json::json!({
            "success": true,
            "data": strategy,
            "message": "Default strategy unset"
        }))),
        Ok(None) => Err(StatusCode::NOT_FOUND),
        Err(e) => {
            tracing::error!("Failed to unset default strategy: {}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

// ==================== K线内存状态 ====================

/// 查看 KlineManager 内存中的数据状态
///
/// GET /api/kline-status
///
/// 返回每个 (symbol, timeframe) 的：
/// - closed_count: 已完成K线数量
/// - latest_time: 最新K线时间戳
/// - latest_time_str: 可读的时间
/// - age_seconds: 距最新数据的秒数
/// - current_price: 当前价格
/// - is_stale: 是否过旧（> 2 个周期）
async fn kline_status(
    axum::extract::State((_, km)): axum::extract::State<AppState>,
) -> Result<Json<serde_json::Value>, StatusCode> {
    let manager = km.read().await;
    let now_ms = chrono::Utc::now().timestamp_millis();

    let mut stores = Vec::new();
    for (symbol, tf) in manager.keys() {
        if let Some(store) = manager.get(&symbol, tf) {
            let latest = store.latest_closed_time();
            let duration_ms = store.timeframe_duration_ms();

            let (age_seconds, is_stale, latest_time_str) = if let Some(latest_time) = latest {
                let age_ms = now_ms - latest_time;
                let age_s = age_ms / 1000;
                let stale = age_ms > duration_ms * 2;
                let dt = chrono::DateTime::from_timestamp_millis(latest_time)
                    .map(|d| d.format("%Y-%m-%d %H:%M:%S").to_string())
                    .unwrap_or_else(|| "unknown".to_string());
                (Some(age_s), stale, Some(dt))
            } else {
                (None, true, None)
            };

            stores.push(serde_json::json!({
                "symbol": symbol,
                "timeframe": tf.as_str(),
                "closed_count": store.closed_count(),
                "current_price": store.current_price(),
                "latest_time": latest,
                "latest_time_str": latest_time_str,
                "age_seconds": age_seconds,
                "is_stale": is_stale,
            }));
        }
    }

    Ok(Json(serde_json::json!({
        "success": true,
        "data": {
            "stores": stores,
            "total_stores": stores.len(),
            "stale_count": stores.iter().filter(|s| s["is_stale"].as_bool().unwrap_or(false)).count(),
        }
    })))
}
