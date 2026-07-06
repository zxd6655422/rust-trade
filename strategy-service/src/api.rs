use axum::{
    extract::{Path, Query},
    http::StatusCode,
    response::Json,
    routing::{get, put},
    Router,
};
use serde::Deserialize;
use sqlx::PgPool;
use uuid::Uuid;

use crate::db::{signals, strategies, trades, performance};

pub fn create_router(pool: PgPool) -> Router {
    Router::new()
        // 策略管理
        .route("/api/strategies", get(list_strategies).post(create_strategy))
        .route(
            "/api/strategies/:id",
            get(get_strategy).put(update_strategy).delete(delete_strategy),
        )
        .route("/api/strategies/:id/status", put(update_strategy_status))
        // 信号查询
        .route("/api/signals", get(query_signals))
        .route("/api/strategies/:id/signals", get(get_strategy_signals))
        // 交易记录
        .route("/api/trades", get(query_trades))
        .route("/api/strategies/:id/trades", get(get_strategy_trades))
        // 策略统计
        .route("/api/strategies/:id/performance", get(get_performance))
        .route("/api/strategies/:id/summary", get(get_summary))
        .with_state(pool)
}

// ==================== 策略管理 ====================

async fn list_strategies(
    axum::extract::State(pool): axum::extract::State<PgPool>,
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
    axum::extract::State(pool): axum::extract::State<PgPool>,
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
    axum::extract::State(pool): axum::extract::State<PgPool>,
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
    axum::extract::State(pool): axum::extract::State<PgPool>,
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
    axum::extract::State(pool): axum::extract::State<PgPool>,
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
    axum::extract::State(pool): axum::extract::State<PgPool>,
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
    axum::extract::State(pool): axum::extract::State<PgPool>,
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
    axum::extract::State(pool): axum::extract::State<PgPool>,
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
    axum::extract::State(pool): axum::extract::State<PgPool>,
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
    axum::extract::State(pool): axum::extract::State<PgPool>,
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
    axum::extract::State(pool): axum::extract::State<PgPool>,
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
    axum::extract::State(pool): axum::extract::State<PgPool>,
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
