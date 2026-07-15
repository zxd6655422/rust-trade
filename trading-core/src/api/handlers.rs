// api/handlers.rs
// HTTP API handlers

use actix_web::{web, HttpResponse};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{error, info};
use uuid::Uuid;

use trading_common::backtest::{
    engine::{BacktestConfig, BacktestEngine},
    strategy::{create_strategy, list_strategies, is_multi_timeframe_strategy},
};
use trading_common::data::repository::TickDataRepository;
use crate::service::backtest_service::{BacktestService, SaveBacktestResultRequest};
use rust_decimal::Decimal;
use std::str::FromStr;

/// 应用状态
pub struct AppState {
    pub repository: Arc<TickDataRepository>,
    pub backtest_lock: Arc<Mutex<()>>,
    pub pool: PgPool,
}

// Safe unwrap for known-good Decimal constants
fn default_commission_decimal() -> Decimal {
    Decimal::from_str("0.001").unwrap_or(Decimal::ZERO)
}

/// 回测请求
#[derive(Debug, Deserialize)]
pub struct BacktestRequest {
    /// 策略实例 ID（可选，优先使用）
    pub instance_id: Option<Uuid>,
    /// 策略类型（当 instance_id 为空时使用）
    pub strategy: Option<String>,
    pub symbol: String,
    #[serde(default = "default_capital")]
    pub capital: f64,
    #[serde(default = "default_data_count")]
    pub data_count: i64,
    #[serde(default = "default_commission")]
    pub commission_rate: f64,
    /// 是否使用 OHLC 数据（如果策略支持）
    #[serde(default)]
    pub use_ohlc: bool,
    /// 策略参数（可选，覆盖实例配置）
    pub strategy_params: Option<std::collections::HashMap<String, String>>,
}

fn default_capital() -> f64 {
    10000.0
}

fn default_data_count() -> i64 {
    10000
}

fn default_commission() -> f64 {
    0.1
}

/// 回测响应
#[derive(Debug, Serialize)]
pub struct BacktestResponse {
    pub success: bool,
    pub message: String,
    pub data: Option<BacktestResult>,
}

/// 回测结果
#[derive(Debug, Serialize)]
pub struct BacktestResult {
    /// 回测结果 ID
    pub id: Option<String>,
    /// 关联的策略实例 ID
    pub instance_id: Option<String>,
    pub strategy: String,
    pub symbol: String,
    pub initial_capital: String,
    pub final_capital: String,
    pub total_return_pct: String,
    pub total_trades: usize,
    pub winning_trades: usize,
    pub losing_trades: usize,
    pub win_rate: String,
    pub max_drawdown: String,
    pub sharpe_ratio: String,
    pub profit_factor: String,
    pub data_points: usize,
    pub data_range_start: String,
    pub data_range_end: String,
}

/// 数据信息响应
#[derive(Debug, Serialize)]
pub struct DataInfoResponse {
    pub total_records: u64,
    pub symbols_count: u64,
    pub earliest_time: Option<String>,
    pub latest_time: Option<String>,
    pub symbol_info: Vec<SymbolInfo>,
}

#[derive(Debug, Serialize)]
pub struct SymbolInfo {
    pub symbol: String,
    pub records_count: u64,
}

/// 策略列表响应
#[derive(Debug, Serialize)]
pub struct StrategiesResponse {
    pub strategies: Vec<StrategyInfo>,
}

#[derive(Debug, Serialize)]
pub struct StrategyInfo {
    pub id: String,
    pub name: String,
    pub description: String,
    pub is_multi_timeframe: bool,
}

/// 采集统计响应
#[derive(Debug, Serialize)]
pub struct CollectorStatsResponse {
    pub status: String,
    pub modes: Vec<String>,
    pub uptime_seconds: u64,
    pub total_ticks_processed: u64,
    pub total_batches_flushed: u64,
    pub last_flush_time: Option<String>,
}

/// 获取数据信息
pub async fn get_data_info(
    data: web::Data<AppState>,
) -> HttpResponse {
    match data.repository.get_backtest_data_info().await {
        Ok(info) => {
            let response = DataInfoResponse {
                total_records: info.total_records,
                symbols_count: info.symbols_count,
                earliest_time: info.earliest_time.map(|t| t.format("%Y-%m-%d %H:%M:%S UTC").to_string()),
                latest_time: info.latest_time.map(|t| t.format("%Y-%m-%d %H:%M:%S UTC").to_string()),
                symbol_info: info.symbol_info.iter().map(|s| SymbolInfo {
                    symbol: s.symbol.clone(),
                    records_count: s.records_count,
                }).collect(),
            };
            HttpResponse::Ok().json(response)
        }
        Err(e) => {
            error!("Failed to get data info: {}", e);
            HttpResponse::InternalServerError().json(serde_json::json!({
                "success": false,
                "message": format!("Failed to get data info: {}", e)
            }))
        }
    }
}

/// 获取策略列表
pub async fn get_strategies() -> HttpResponse {
    let strategies = list_strategies();
    let response = StrategiesResponse {
        strategies: strategies.iter().map(|s| StrategyInfo {
            id: s.id.clone(),
            name: s.name.clone(),
            description: s.description.clone(),
            is_multi_timeframe: s.is_multi_timeframe,
        }).collect(),
    };
    HttpResponse::Ok().json(response)
}

/// 执行回测
pub async fn run_backtest(
    data: web::Data<AppState>,
    req: web::Json<BacktestRequest>,
) -> HttpResponse {
    info!("Backtest request: {:?}", req);

    // 解析策略类型和参数：优先使用 instance_id，其次使用 strategy 字段
    let (strategy_type, instance_id, strategy_params) = if let Some(instance_id) = req.instance_id {
        // 从数据库读取策略实例配置
        // 注意：这里需要使用 strategy-service 的数据库查询，但 trading-core 没有直接访问
        // 因此，我们暂时从请求参数中获取策略类型，instance_id 只用于保存结果
        let strategy_type = req.strategy.clone().unwrap_or_else(|| {
            // 如果没有提供 strategy 参数，使用默认值
            // 实际应该从数据库读取，但这里为了简化，使用请求中的值
            "unknown".to_string()
        });
        (strategy_type, Some(instance_id), req.strategy_params.clone())
    } else {
        // 使用 strategy 字段
        let strategy_type = req.strategy.clone().ok_or_else(|| {
            HttpResponse::BadRequest().json(serde_json::json!({
                "success": false,
                "message": "Either instance_id or strategy must be provided"
            }))
        });
        match strategy_type {
            Ok(s) => (s, None, req.strategy_params.clone()),
            Err(resp) => return resp,
        }
    };

    // 验证策略（普通策略或多时间框架策略）
    let is_mtf = is_multi_timeframe_strategy(&strategy_type);
    if !is_mtf && create_strategy(&strategy_type).is_err() {
        return HttpResponse::BadRequest().json(serde_json::json!({
            "success": false,
            "message": format!("Unknown strategy: {}", strategy_type)
        }));
    }
    if is_mtf {
        // 多时间框架策略目前只支持通过专门的 API 调用
        // 后续可以扩展支持
        return HttpResponse::BadRequest().json(serde_json::json!({
            "success": false,
            "message": "Multi-timeframe strategies require dedicated API endpoint. Use /api/backtest/multi-timeframe"
        }));
    }

    // 获取回测锁（同时只允许一个回测）
    let _lock = match data.backtest_lock.try_lock() {
        Ok(lock) => lock,
        Err(_) => {
            return HttpResponse::Conflict().json(serde_json::json!({
                "success": false,
                "message": "Another backtest is running. Please wait."
            }));
        }
    };

    // 检查数据是否充足
    match data.repository.get_backtest_data_info().await {
        Ok(info) => {
            if !info.has_sufficient_data(&req.symbol, 100) {
                return HttpResponse::BadRequest().json(serde_json::json!({
                    "success": false,
                    "message": format!("Insufficient data for symbol: {} (minimum 100 records required)", req.symbol)
                }));
            }
        }
        Err(e) => {
            error!("Failed to check data: {}", e);
            return HttpResponse::InternalServerError().json(serde_json::json!({
                "success": false,
                "message": format!("Failed to check data: {}", e)
            }));
        }
    }

    // 创建策略
    let strategy = match create_strategy(&strategy_type) {
        Ok(s) => s,
        Err(e) => {
            return HttpResponse::BadRequest().json(serde_json::json!({
                "success": false,
                "message": format!("Failed to create strategy: {}", e)
            }));
        }
    };

    // 创建配置
    let initial_capital = Decimal::from_str(&req.capital.to_string()).unwrap_or(Decimal::from(10000));
    let commission_rate = Decimal::from_str(&(req.commission_rate / 100.0).to_string())
        .unwrap_or(default_commission_decimal());
    let mut config = BacktestConfig::new(initial_capital).with_commission_rate(commission_rate);

    // 应用策略参数
    if let Some(params) = &strategy_params {
        for (key, value) in params {
            config = config.with_param(key, value);
        }
    }

    // 尝试使用 OHLC 数据
    if req.use_ohlc && strategy.supports_ohlc() {
        if let Some(timeframe) = strategy.preferred_timeframe() {
            info!("Using OHLC data for backtest (timeframe: {})", timeframe.as_str());

            let candle_count = (req.data_count / 50).max(100) as u32;

            match data.repository.generate_recent_ohlc_for_backtest(&req.symbol, timeframe, candle_count).await {
                Ok(ohlc_data) if !ohlc_data.is_empty() => {
                    let mut engine = match BacktestEngine::new(strategy, config) {
                        Ok(e) => e,
                        Err(e) => {
                            return HttpResponse::InternalServerError().json(serde_json::json!({
                                "success": false,
                                "message": format!("Failed to create backtest engine: {}", e)
                            }));
                        }
                    };

                    let result = engine.run_with_ohlc(ohlc_data.clone());

                    // 保存回测结果到数据库
                    let backtest_service = BacktestService::new(data.pool.clone());
                    let save_request = SaveBacktestResultRequest {
                        instance_id,
                        strategy_id: strategy_type.clone(),
                        symbol: req.symbol.clone(),
                        initial_capital,
                        final_capital: result.final_value,
                        return_pct: result.return_percentage,
                        total_trades: result.total_trades as i32,
                        winning_trades: result.winning_trades as i32,
                        losing_trades: result.losing_trades as i32,
                        win_rate: result.win_rate,
                        max_drawdown: result.max_drawdown,
                        sharpe_ratio: result.sharpe_ratio,
                        profit_factor: result.profit_factor,
                        data_points: ohlc_data.len() as i32,
                        data_start_time: ohlc_data.first().map(|d| d.timestamp),
                        data_end_time: ohlc_data.last().map(|d| d.timestamp),
                        strategy_params: strategy_params.map(|p| serde_json::to_value(p).unwrap_or_default()),
                    };

                    let saved_result = match backtest_service.save_result(save_request).await {
                        Ok(r) => Some(r),
                        Err(e) => {
                            error!("Failed to save backtest result: {}", e);
                            None
                        }
                    };

                    let response = BacktestResponse {
                        success: true,
                        message: "Backtest completed successfully".to_string(),
                        data: Some(BacktestResult {
                            id: saved_result.map(|r| r.id.to_string()),
                            instance_id: instance_id.map(|id| id.to_string()),
                            strategy: strategy_type.clone(),
                            symbol: req.symbol.clone(),
                            initial_capital: format!("${}", initial_capital),
                            final_capital: format!("${:.2}", result.final_value),
                            total_return_pct: format!("{:.2}%", result.return_percentage),
                            total_trades: result.total_trades,
                            winning_trades: result.winning_trades,
                            losing_trades: result.losing_trades,
                            win_rate: format!("{:.2}%", result.win_rate),
                            max_drawdown: format!("{:.2}%", result.max_drawdown),
                            sharpe_ratio: format!("{:.2}", result.sharpe_ratio),
                            profit_factor: format!("{:.2}", result.profit_factor),
                            data_points: ohlc_data.len(),
                            data_range_start: ohlc_data.first().map_or("N/A".to_string(), |d| d.timestamp.format("%Y-%m-%d %H:%M:%S").to_string()),
                            data_range_end: ohlc_data.last().map_or("N/A".to_string(), |d| d.timestamp.format("%Y-%m-%d %H:%M:%S").to_string()),
                        }),
                    };

                    return HttpResponse::Ok().json(response);
                }
                _ => {
                    info!("OHLC data not available, falling back to tick data");
                }
            }
        }
    }

    // 使用 tick 数据
    info!("Using tick data for backtest");

    let data_count = req.data_count.min(50000); // 限制最大数据量
    let tick_data = match data.repository.get_recent_ticks_for_backtest(&req.symbol, data_count).await {
        Ok(d) => d,
        Err(e) => {
            error!("Failed to get tick data: {}", e);
            return HttpResponse::InternalServerError().json(serde_json::json!({
                "success": false,
                "message": format!("Failed to get tick data: {}", e)
            }));
        }
    };

    if tick_data.is_empty() {
        return HttpResponse::BadRequest().json(serde_json::json!({
            "success": false,
            "message": format!("No data found for symbol: {}", req.symbol)
        }));
    }

    let mut engine = match BacktestEngine::new(strategy, config) {
        Ok(e) => e,
        Err(e) => {
            return HttpResponse::InternalServerError().json(serde_json::json!({
                "success": false,
                "message": format!("Failed to create backtest engine: {}", e)
            }));
        }
    };

    let result = engine.run(tick_data.clone());

    // 保存回测结果到数据库
    let backtest_service = BacktestService::new(data.pool.clone());
    let save_request = SaveBacktestResultRequest {
        instance_id,
        strategy_id: strategy_type.clone(),
        symbol: req.symbol.clone(),
        initial_capital,
        final_capital: result.final_value,
        return_pct: result.return_percentage,
        total_trades: result.total_trades as i32,
        winning_trades: result.winning_trades as i32,
        losing_trades: result.losing_trades as i32,
        win_rate: result.win_rate,
        max_drawdown: result.max_drawdown,
        sharpe_ratio: result.sharpe_ratio,
        profit_factor: result.profit_factor,
        data_points: tick_data.len() as i32,
        data_start_time: tick_data.first().map(|d| d.timestamp),
        data_end_time: tick_data.last().map(|d| d.timestamp),
        strategy_params: strategy_params.map(|p| serde_json::to_value(p).unwrap_or_default()),
    };

    let saved_result = match backtest_service.save_result(save_request).await {
        Ok(r) => Some(r),
        Err(e) => {
            error!("Failed to save backtest result: {}", e);
            None
        }
    };

    let response = BacktestResponse {
        success: true,
        message: "Backtest completed successfully".to_string(),
        data: Some(BacktestResult {
            id: saved_result.map(|r| r.id.to_string()),
            instance_id: instance_id.map(|id| id.to_string()),
            strategy: strategy_type.clone(),
            symbol: req.symbol.clone(),
            initial_capital: format!("${}", initial_capital),
            final_capital: format!("${:.2}", result.final_value),
            total_return_pct: format!("{:.2}%", result.return_percentage),
            total_trades: result.total_trades,
            winning_trades: result.winning_trades,
            losing_trades: result.losing_trades,
            win_rate: format!("{:.2}%", result.win_rate),
            max_drawdown: format!("{:.2}%", result.max_drawdown),
            sharpe_ratio: format!("{:.2}", result.sharpe_ratio),
            profit_factor: format!("{:.2}", result.profit_factor),
            data_points: tick_data.len(),
            data_range_start: tick_data.first().map_or("N/A".to_string(), |d| d.timestamp.format("%Y-%m-%d %H:%M:%S").to_string()),
            data_range_end: tick_data.last().map_or("N/A".to_string(), |d| d.timestamp.format("%Y-%m-%d %H:%M:%S").to_string()),
        }),
    };

    HttpResponse::Ok().json(response)
}

/// 健康检查
pub async fn health_check() -> HttpResponse {
    HttpResponse::Ok().json(serde_json::json!({
        "status": "ok",
        "service": "trading-core"
    }))
}

/// 多时间框架回测请求
#[derive(Debug, Deserialize)]
pub struct MultiTimeframeBacktestRequest {
    pub strategy: String,
    pub symbol: String,
    #[serde(default = "default_capital")]
    pub capital: f64,
    #[serde(default = "default_data_count")]
    pub data_count: i64,
    #[serde(default = "default_commission")]
    pub commission_rate: f64,
    /// 策略参数 (可选)
    #[serde(default)]
    pub strategy_params: Option<std::collections::HashMap<String, String>>,
}

/// 执行多时间框架回测（完整模拟交易）
pub async fn run_multi_timeframe_backtest(
    data: web::Data<AppState>,
    req: web::Json<MultiTimeframeBacktestRequest>,
) -> HttpResponse {
    info!("Multi-timeframe backtest request: {:?}", req);

    // 验证策略
    if !is_multi_timeframe_strategy(&req.strategy) {
        return HttpResponse::BadRequest().json(serde_json::json!({
            "success": false,
            "message": format!("Strategy '{}' is not a multi-timeframe strategy", req.strategy)
        }));
    }

    // 获取回测锁
    let _lock = match data.backtest_lock.try_lock() {
        Ok(lock) => lock,
        Err(_) => {
            return HttpResponse::Conflict().json(serde_json::json!({
                "success": false,
                "message": "Another backtest is running. Please wait."
            }));
        }
    };

    // 检查数据是否充足
    match data.repository.get_backtest_data_info().await {
        Ok(info) => {
            if !info.has_sufficient_data(&req.symbol, 100) {
                return HttpResponse::BadRequest().json(serde_json::json!({
                    "success": false,
                    "message": format!("Insufficient data for symbol: {} (minimum 100 records required)", req.symbol)
                }));
            }
        }
        Err(e) => {
            error!("Failed to check data: {}", e);
            return HttpResponse::InternalServerError().json(serde_json::json!({
                "success": false,
                "message": format!("Failed to check data: {}", e)
            }));
        }
    }

    // 获取 1m K线数据
    let candle_count = req.data_count.max(1000) as u32;
    let klines_1m = match data.repository.get_klines(&req.symbol, candle_count).await {
        Ok(klines) if !klines.is_empty() => klines,
        _ => {
            // 回退到从 tick 数据生成
            let fallback_count = (req.data_count / 50).max(100) as u32;
            match data.repository.generate_recent_ohlc_for_backtest(
                &req.symbol,
                trading_common::data::types::Timeframe::OneMinute,
                fallback_count,
            ).await {
                Ok(klines) => klines,
                Err(e) => {
                    error!("Failed to get 1m klines: {}", e);
                    return HttpResponse::InternalServerError().json(serde_json::json!({
                        "success": false,
                        "message": format!("Failed to get 1m klines: {}", e)
                    }));
                }
            }
        }
    };

    if klines_1m.is_empty() {
        return HttpResponse::BadRequest().json(serde_json::json!({
            "success": false,
            "message": "No 1m kline data available"
        }));
    }

    info!("Loaded {} 1m klines for multi-timeframe backtest", klines_1m.len());

    // 创建多时间框架策略
    let strategy = match trading_common::backtest::strategy::create_multi_timeframe_strategy(&req.strategy) {
        Ok(s) => s,
        Err(e) => {
            return HttpResponse::BadRequest().json(serde_json::json!({
                "success": false,
                "message": format!("Failed to create strategy: {}", e)
            }));
        }
    };

    // 创建配置
    let initial_capital = Decimal::from_str(&req.capital.to_string()).unwrap_or(Decimal::from(10000));
    let commission_rate = Decimal::from_str(&(req.commission_rate / 100.0).to_string())
        .unwrap_or(default_commission_decimal());
    let mut config = trading_common::backtest::engine::BacktestConfig::new(initial_capital)
        .with_commission_rate(commission_rate);

    // 应用策略参数
    if let Some(params) = &req.strategy_params {
        for (key, value) in params {
            config = config.with_param(key, value);
        }
    }

    // 创建并运行多时间框架回测引擎
    let mut engine = match trading_common::backtest::MultiTimeframeBacktestEngine::new(
        strategy,
        config,
        req.symbol.clone(),
    ) {
        Ok(e) => e,
        Err(e) => {
            return HttpResponse::InternalServerError().json(serde_json::json!({
                "success": false,
                "message": format!("Failed to create backtest engine: {}", e)
            }));
        }
    };

    let result = engine.run(klines_1m.clone());

    let response = BacktestResponse {
        success: true,
        message: "Multi-timeframe backtest completed successfully".to_string(),
        data: Some(BacktestResult {
            id: None,
            instance_id: None,
            strategy: req.strategy.clone(),
            symbol: req.symbol.clone(),
            initial_capital: format!("${}", initial_capital),
            final_capital: format!("${:.2}", result.final_value),
            total_return_pct: format!("{:.2}%", result.return_percentage),
            total_trades: result.total_trades,
            winning_trades: result.winning_trades,
            losing_trades: result.losing_trades,
            win_rate: format!("{:.2}%", result.win_rate),
            max_drawdown: format!("{:.2}%", result.max_drawdown),
            sharpe_ratio: format!("{:.2}", result.sharpe_ratio),
            profit_factor: format!("{:.2}", result.profit_factor),
            data_points: klines_1m.len(),
            data_range_start: klines_1m.first().map_or("N/A".to_string(), |d| d.timestamp.format("%Y-%m-%d %H:%M:%S").to_string()),
            data_range_end: klines_1m.last().map_or("N/A".to_string(), |d| d.timestamp.format("%Y-%m-%d %H:%M:%S").to_string()),
        }),
    };

    HttpResponse::Ok().json(response)
}

// =================================================================
// 滚动前进测试 + 样本外测试
// =================================================================

/// 滚动前进测试请求
#[derive(Debug, Deserialize)]
pub struct WalkForwardRequest {
    pub strategy: String,
    pub symbol: String,
    #[serde(default = "default_capital")]
    pub capital: f64,
    #[serde(default = "default_commission")]
    pub commission_rate: f64,
    /// 训练窗口大小（1m K线数量），默认 43200 (30天)
    #[serde(default = "default_train_candles")]
    pub train_candles: usize,
    /// 测试窗口大小（1m K线数量），默认 10080 (7天)
    #[serde(default = "default_test_candles")]
    pub test_candles: usize,
    /// 滚动步长（1m K线数量），默认 10080 (7天)
    #[serde(default = "default_step_candles")]
    pub step_candles: usize,
    /// 总数据量（1m K线数量）
    #[serde(default = "default_wf_data_count")]
    pub data_count: u32,
    /// 策略参数 (可选)
    #[serde(default)]
    pub strategy_params: Option<std::collections::HashMap<String, String>>,
}

fn default_train_candles() -> usize {
    43200
}
fn default_test_candles() -> usize {
    10080
}
fn default_step_candles() -> usize {
    10080
}
fn default_wf_data_count() -> u32 {
    100000
}

/// 样本外测试请求
#[derive(Debug, Deserialize)]
pub struct OutOfSampleRequest {
    pub strategy: String,
    pub symbol: String,
    #[serde(default = "default_capital")]
    pub capital: f64,
    #[serde(default = "default_commission")]
    pub commission_rate: f64,
    /// 训练集比例，默认 0.7
    #[serde(default = "default_train_ratio")]
    pub train_ratio: f64,
    /// 总数据量
    #[serde(default = "default_wf_data_count")]
    pub data_count: u32,
    /// 策略参数 (可选)
    #[serde(default)]
    pub strategy_params: Option<std::collections::HashMap<String, String>>,
}

fn default_train_ratio() -> f64 {
    0.7
}

/// 执行滚动前进测试
pub async fn run_walk_forward_backtest(
    data: web::Data<AppState>,
    req: web::Json<WalkForwardRequest>,
) -> HttpResponse {
    info!("Walk-forward backtest request: {:?}", req);

    if !is_multi_timeframe_strategy(&req.strategy) {
        return HttpResponse::BadRequest().json(serde_json::json!({
            "success": false,
            "message": format!("Strategy '{}' is not a multi-timeframe strategy", req.strategy)
        }));
    }

    let _lock = match data.backtest_lock.try_lock() {
        Ok(lock) => lock,
        Err(_) => {
            return HttpResponse::Conflict().json(serde_json::json!({
                "success": false,
                "message": "Another backtest is running. Please wait."
            }));
        }
    };

    // 获取 1m K线数据
    let klines_1m = match data.repository.get_klines(&req.symbol, req.data_count).await {
        Ok(klines) if !klines.is_empty() => klines,
        _ => {
            return HttpResponse::BadRequest().json(serde_json::json!({
                "success": false,
                "message": "Insufficient 1m kline data for walk-forward analysis"
            }));
        }
    };

    info!("Loaded {} 1m klines for walk-forward", klines_1m.len());

    let initial_capital = Decimal::from_str(&req.capital.to_string()).unwrap_or(Decimal::from(10000));
    let commission_rate = Decimal::from_str(&(req.commission_rate / 100.0).to_string())
        .unwrap_or(default_commission_decimal());
    let mut bt_config = trading_common::backtest::BacktestConfig::new(initial_capital)
        .with_commission_rate(commission_rate);
    if let Some(params) = &req.strategy_params {
        for (key, value) in params {
            bt_config = bt_config.with_param(key, value);
        }
    }

    let wf_config = trading_common::backtest::WalkForwardConfig::default()
        .with_train_candles(req.train_candles)
        .with_test_candles(req.test_candles)
        .with_step_candles(req.step_candles);

    let strategy_id = req.strategy.clone();
    let result = trading_common::backtest::WalkForwardEngine::run(
        || trading_common::backtest::strategy::create_multi_timeframe_strategy(&strategy_id)
            .unwrap_or_else(|e| {
                error!("Failed to create strategy '{}': {}", strategy_id, e);
                // Fallback to trend strategy to prevent panic
                trading_common::backtest::strategy::create_multi_timeframe_strategy("trend")
                    .expect("Fallback strategy 'trend' must exist")
            }),
        &bt_config,
        &wf_config,
        &klines_1m,
        &req.symbol,
    );

    match result {
        Ok(wf_result) => {
            let rounds_json: Vec<serde_json::Value> = wf_result
                .round_summaries
                .iter()
                .map(|r| {
                    serde_json::json!({
                        "round": r.round,
                        "train_start": r.train_start.format("%Y-%m-%d %H:%M:%S").to_string(),
                        "train_end": r.train_end.format("%Y-%m-%d %H:%M:%S").to_string(),
                        "test_start": r.test_start.format("%Y-%m-%d %H:%M:%S").to_string(),
                        "test_end": r.test_end.format("%Y-%m-%d %H:%M:%S").to_string(),
                        "train_return_pct": format!("{:.2}%", r.train_return_pct),
                        "train_sharpe": format!("{:.2}", r.train_sharpe),
                        "train_trades": r.train_trades,
                        "test_return_pct": format!("{:.2}%", r.test_return_pct),
                        "test_sharpe": format!("{:.2}", r.test_sharpe),
                        "test_trades": r.test_trades,
                        "test_win_rate": format!("{:.2}%", r.test_win_rate),
                        "test_max_drawdown": format!("{:.2}%", r.test_max_drawdown),
                        "overfit_ratio": format!("{:.2}", r.overfit_ratio),
                    })
                })
                .collect();

            HttpResponse::Ok().json(serde_json::json!({
                "success": true,
                "message": "Walk-forward analysis completed",
                "data": {
                    "total_rounds": wf_result.total_rounds,
                    "profitable_rounds": wf_result.profitable_rounds,
                    "overall_test_return_pct": format!("{:.2}%", wf_result.overall_test_return_pct),
                    "overall_test_sharpe": format!("{:.2}", wf_result.overall_test_sharpe),
                    "overall_test_max_drawdown": format!("{:.2}%", wf_result.overall_test_max_drawdown),
                    "overall_test_win_rate": format!("{:.2}%", wf_result.overall_test_win_rate),
                    "avg_overfit_ratio": format!("{:.2}", wf_result.avg_overfit_ratio),
                    "is_overfit": wf_result.is_overfit,
                    "rounds": rounds_json
                }
            }))
        }
        Err(e) => HttpResponse::BadRequest().json(serde_json::json!({
            "success": false,
            "message": format!("Walk-forward analysis failed: {}", e)
        })),
    }
}

/// 执行样本外测试
pub async fn run_out_of_sample_backtest(
    data: web::Data<AppState>,
    req: web::Json<OutOfSampleRequest>,
) -> HttpResponse {
    info!("Out-of-sample backtest request: {:?}", req);

    if !is_multi_timeframe_strategy(&req.strategy) {
        return HttpResponse::BadRequest().json(serde_json::json!({
            "success": false,
            "message": format!("Strategy '{}' is not a multi-timeframe strategy", req.strategy)
        }));
    }

    let _lock = match data.backtest_lock.try_lock() {
        Ok(lock) => lock,
        Err(_) => {
            return HttpResponse::Conflict().json(serde_json::json!({
                "success": false,
                "message": "Another backtest is running. Please wait."
            }));
        }
    };

    let klines_1m = match data.repository.get_klines(&req.symbol, req.data_count).await {
        Ok(klines) if !klines.is_empty() => klines,
        _ => {
            return HttpResponse::BadRequest().json(serde_json::json!({
                "success": false,
                "message": "Insufficient 1m kline data for out-of-sample analysis"
            }));
        }
    };

    info!("Loaded {} 1m klines for out-of-sample", klines_1m.len());

    let initial_capital = Decimal::from_str(&req.capital.to_string()).unwrap_or(Decimal::from(10000));
    let commission_rate = Decimal::from_str(&(req.commission_rate / 100.0).to_string())
        .unwrap_or(default_commission_decimal());
    let mut bt_config = trading_common::backtest::BacktestConfig::new(initial_capital)
        .with_commission_rate(commission_rate);
    if let Some(params) = &req.strategy_params {
        for (key, value) in params {
            bt_config = bt_config.with_param(key, value);
        }
    }

    let os_config = trading_common::backtest::OutOfSampleConfig {
        train_ratio: Decimal::from_str(&req.train_ratio.to_string())
            .unwrap_or(Decimal::from_str("0.7").unwrap()),
    };

    let strategy_id = req.strategy.clone();
    let result = trading_common::backtest::WalkForwardEngine::run_out_of_sample(
        || trading_common::backtest::strategy::create_multi_timeframe_strategy(&strategy_id)
            .unwrap_or_else(|e| {
                error!("Failed to create strategy '{}': {}", strategy_id, e);
                trading_common::backtest::strategy::create_multi_timeframe_strategy("trend")
                    .expect("Fallback strategy 'trend' must exist")
            }),
        &bt_config,
        &os_config,
        &klines_1m,
        &req.symbol,
    );

    match result {
        Ok(os_result) => {
            HttpResponse::Ok().json(serde_json::json!({
                "success": true,
                "message": "Out-of-sample analysis completed",
                "data": {
                    "train": {
                        "return_pct": format!("{:.2}%", os_result.train_result.return_percentage),
                        "sharpe": format!("{:.2}", os_result.train_sharpe),
                        "max_drawdown": format!("{:.2}%", os_result.train_result.max_drawdown),
                        "win_rate": format!("{:.2}%", os_result.train_result.win_rate),
                        "total_trades": os_result.train_result.total_trades,
                        "profit_factor": format!("{:.2}", os_result.train_result.profit_factor),
                    },
                    "test": {
                        "return_pct": format!("{:.2}%", os_result.test_result.return_percentage),
                        "sharpe": format!("{:.2}", os_result.test_sharpe),
                        "max_drawdown": format!("{:.2}%", os_result.test_result.max_drawdown),
                        "win_rate": format!("{:.2}%", os_result.test_result.win_rate),
                        "total_trades": os_result.test_result.total_trades,
                        "profit_factor": format!("{:.2}", os_result.test_result.profit_factor),
                    },
                    "overfit_ratio": format!("{:.2}", os_result.overfit_ratio),
                    "is_overfit": os_result.is_overfit
                }
            }))
        }
        Err(e) => HttpResponse::BadRequest().json(serde_json::json!({
            "success": false,
            "message": format!("Out-of-sample analysis failed: {}", e)
        })),
    }
}

// =================================================================
// 多交易对回测 + 市场状态分析
// =================================================================

/// 多交易对回测请求
#[derive(Debug, Deserialize)]
pub struct MultiSymbolBacktestRequest {
    pub strategy: String,
    /// 交易对列表，为空则自动获取所有可用交易对
    #[serde(default)]
    pub symbols: Vec<String>,
    #[serde(default = "default_capital")]
    pub capital: f64,
    #[serde(default = "default_commission")]
    pub commission_rate: f64,
    /// 每个 symbol 的数据量
    #[serde(default = "default_wf_data_count")]
    pub data_count: u32,
    /// 市场状态分析窗口大小
    #[serde(default = "default_market_window")]
    pub market_state_window: usize,
    /// 策略参数 (可选)
    #[serde(default)]
    pub strategy_params: Option<std::collections::HashMap<String, String>>,
}

fn default_market_window() -> usize {
    50
}

/// 市场状态分析请求
#[derive(Debug, Deserialize)]
pub struct MarketStateRequest {
    pub symbol: String,
    /// 数据量
    #[serde(default = "default_wf_data_count")]
    pub data_count: u32,
    /// 分析窗口大小
    #[serde(default = "default_market_window")]
    pub window: usize,
}

/// 执行多交易对回测
pub async fn run_multi_symbol_backtest(
    data: web::Data<AppState>,
    req: web::Json<MultiSymbolBacktestRequest>,
) -> HttpResponse {
    info!("Multi-symbol backtest request: {:?}", req);

    if !is_multi_timeframe_strategy(&req.strategy) {
        return HttpResponse::BadRequest().json(serde_json::json!({
            "success": false,
            "message": format!("Strategy '{}' is not a multi-timeframe strategy", req.strategy)
        }));
    }

    let _lock = match data.backtest_lock.try_lock() {
        Ok(lock) => lock,
        Err(_) => {
            return HttpResponse::Conflict().json(serde_json::json!({
                "success": false,
                "message": "Another backtest is running. Please wait."
            }));
        }
    };

    // 确定 symbol 列表
    let symbols = if req.symbols.is_empty() {
        match data.repository.get_backtest_data_info().await {
            Ok(info) => info.get_available_symbols(),
            Err(e) => {
                return HttpResponse::InternalServerError().json(serde_json::json!({
                    "success": false,
                    "message": format!("Failed to get available symbols: {}", e)
                }));
            }
        }
    } else {
        req.symbols.clone()
    };

    if symbols.is_empty() {
        return HttpResponse::BadRequest().json(serde_json::json!({
            "success": false,
            "message": "No symbols available for backtest"
        }));
    }

    info!("Multi-symbol backtest with {} symbols: {:?}", symbols.len(), symbols);

    // 加载每个 symbol 的数据
    let mut symbol_data = std::collections::HashMap::new();
    for symbol in &symbols {
        match data.repository.get_klines(symbol, req.data_count).await {
            Ok(klines) if !klines.is_empty() => {
                info!("Loaded {} klines for {}", klines.len(), symbol);
                symbol_data.insert(symbol.clone(), klines);
            }
            _ => {
                info!("No kline data for {}, skipping", symbol);
            }
        }
    }

    if symbol_data.is_empty() {
        return HttpResponse::BadRequest().json(serde_json::json!({
            "success": false,
            "message": "No kline data available for any symbol"
        }));
    }

    let initial_capital = Decimal::from_str(&req.capital.to_string()).unwrap_or(Decimal::from(10000));
    let commission_rate = Decimal::from_str(&(req.commission_rate / 100.0).to_string())
        .unwrap_or(default_commission_decimal());
    let mut bt_config = trading_common::backtest::BacktestConfig::new(initial_capital)
        .with_commission_rate(commission_rate);
    if let Some(params) = &req.strategy_params {
        for (key, value) in params {
            bt_config = bt_config.with_param(key, value);
        }
    }

    let strategy_id = req.strategy.clone();
    let result = trading_common::backtest::MultiSymbolBacktestEngine::run(
        move || trading_common::backtest::strategy::create_multi_timeframe_strategy(&strategy_id)
            .unwrap_or_else(|e| {
                error!("Failed to create strategy '{}': {}", strategy_id, e);
                trading_common::backtest::strategy::create_multi_timeframe_strategy("trend")
                    .expect("Fallback strategy 'trend' must exist")
            }),
        &bt_config,
        &symbol_data,
        req.market_state_window,
    );

    match result {
        Ok(ms_result) => {
            let symbols_json: Vec<serde_json::Value> = ms_result
                .results
                .iter()
                .map(|r| {
                    serde_json::json!({
                        "symbol": r.symbol,
                        "return_pct": format!("{:.2}%", r.result.return_percentage),
                        "sharpe": format!("{:.2}", r.result.sharpe_ratio),
                        "win_rate": format!("{:.2}%", r.result.win_rate),
                        "max_drawdown": format!("{:.2}%", r.result.max_drawdown),
                        "total_trades": r.result.total_trades,
                        "profit_factor": format!("{:.2}", r.result.profit_factor),
                        "market_state": r.market_state.summary,
                        "data_quality": format!("{:.0}/100", r.market_state.data_quality_score),
                    })
                })
                .collect();

            HttpResponse::Ok().json(serde_json::json!({
                "success": true,
                "message": "Multi-symbol backtest completed",
                "data": {
                    "total_symbols": ms_result.total_symbols,
                    "profitable_symbols": ms_result.profitable_symbols,
                    "losing_symbols": ms_result.losing_symbols,
                    "avg_return_pct": format!("{:.2}%", ms_result.avg_return_pct),
                    "avg_sharpe": format!("{:.2}", ms_result.avg_sharpe),
                    "avg_win_rate": format!("{:.2}%", ms_result.avg_win_rate),
                    "avg_max_drawdown": format!("{:.2}%", ms_result.avg_max_drawdown),
                    "total_trades": ms_result.total_trades,
                    "best_symbol": ms_result.best_symbol,
                    "best_return_pct": format!("{:.2}%", ms_result.best_return_pct),
                    "worst_symbol": ms_result.worst_symbol,
                    "worst_return_pct": format!("{:.2}%", ms_result.worst_return_pct),
                    "cross_symbol_correlation": format!("{:.2}", ms_result.cross_symbol_correlation),
                    "symbols": symbols_json
                }
            }))
        }
        Err(e) => HttpResponse::BadRequest().json(serde_json::json!({
            "success": false,
            "message": format!("Multi-symbol backtest failed: {}", e)
        })),
    }
}

/// 分析市场状态
pub async fn analyze_market_state(
    data: web::Data<AppState>,
    req: web::Json<MarketStateRequest>,
) -> HttpResponse {
    info!("Market state analysis request: {:?}", req);

    let klines_1m = match data.repository.get_klines(&req.symbol, req.data_count).await {
        Ok(klines) if !klines.is_empty() => klines,
        _ => {
            return HttpResponse::BadRequest().json(serde_json::json!({
                "success": false,
                "message": format!("No kline data available for {}", req.symbol)
            }));
        }
    };

    info!("Loaded {} klines for market state analysis", klines_1m.len());

    let report = trading_common::backtest::MarketStateAnalyzer::analyze(&klines_1m, req.window);

    let state_dist_json: serde_json::Value = report
        .state_percentages
        .iter()
        .map(|(k, v)| (k.clone(), serde_json::json!(format!("{:.1}%", v))))
        .collect::<serde_json::Map<_, _>>()
        .into();

    HttpResponse::Ok().json(serde_json::json!({
        "success": true,
        "message": "Market state analysis completed",
        "data": {
            "symbol": req.symbol,
            "total_candles": report.total_candles,
            "analysis_window": report.analysis_window,
            "state_distribution": state_dist_json,
            "avg_volatility": format!("{:.2}%", report.avg_volatility),
            "avg_trend_strength": format!("{:.2}", report.avg_trend_strength),
            "trend_ratio": format!("{:.1}%", report.trend_ratio),
            "ranging_ratio": format!("{:.1}%", report.ranging_ratio),
            "data_quality_score": format!("{:.0}/100", report.data_quality_score),
            "summary": report.summary
        }
    }))
}

// =================================================================
// 回测历史查询 API
// =================================================================

/// 获取策略实例的回测历史
pub async fn get_backtest_history_by_instance(
    data: web::Data<AppState>,
    path: web::Path<String>,
) -> HttpResponse {
    let instance_id_str = path.into_inner();
    let instance_id = match Uuid::parse_str(&instance_id_str) {
        Ok(id) => id,
        Err(_) => {
            return HttpResponse::BadRequest().json(serde_json::json!({
                "success": false,
                "message": "Invalid instance_id format"
            }));
        }
    };

    let backtest_service = BacktestService::new(data.pool.clone());

    match backtest_service.get_by_instance(instance_id, Some(50)).await {
        Ok(results) => {
            let results_json: Vec<serde_json::Value> = results.iter().map(|r| {
                serde_json::json!({
                    "id": r.id,
                    "instance_id": r.instance_id,
                    "strategy_id": r.strategy_id,
                    "symbol": r.symbol,
                    "initial_capital": r.initial_capital,
                    "final_capital": r.final_capital,
                    "return_pct": r.return_pct,
                    "total_trades": r.total_trades,
                    "winning_trades": r.winning_trades,
                    "losing_trades": r.losing_trades,
                    "win_rate": r.win_rate,
                    "max_drawdown": r.max_drawdown,
                    "sharpe_ratio": r.sharpe_ratio,
                    "profit_factor": r.profit_factor,
                    "data_points": r.data_points,
                    "strategy_params": r.strategy_params,
                    "created_at": r.created_at,
                })
            }).collect();

            HttpResponse::Ok().json(serde_json::json!({
                "success": true,
                "data": {
                    "instance_id": instance_id,
                    "total": results.len(),
                    "backtests": results_json
                }
            }))
        }
        Err(e) => {
            error!("Failed to fetch backtest history: {}", e);
            HttpResponse::InternalServerError().json(serde_json::json!({
                "success": false,
                "message": format!("Failed to fetch backtest history: {}", e)
            }))
        }
    }
}

/// 获取回测结果详情
pub async fn get_backtest_detail(
    data: web::Data<AppState>,
    path: web::Path<String>,
) -> HttpResponse {
    let backtest_id_str = path.into_inner();
    let backtest_id = match Uuid::parse_str(&backtest_id_str) {
        Ok(id) => id,
        Err(_) => {
            return HttpResponse::BadRequest().json(serde_json::json!({
                "success": false,
                "message": "Invalid backtest_id format"
            }));
        }
    };

    let backtest_service = BacktestService::new(data.pool.clone());

    match backtest_service.get_by_id(backtest_id).await {
        Ok(Some(result)) => {
            HttpResponse::Ok().json(serde_json::json!({
                "success": true,
                "data": {
                    "id": result.id,
                    "instance_id": result.instance_id,
                    "strategy_id": result.strategy_id,
                    "symbol": result.symbol,
                    "initial_capital": result.initial_capital,
                    "final_capital": result.final_capital,
                    "return_pct": result.return_pct,
                    "total_trades": result.total_trades,
                    "winning_trades": result.winning_trades,
                    "losing_trades": result.losing_trades,
                    "win_rate": result.win_rate,
                    "max_drawdown": result.max_drawdown,
                    "sharpe_ratio": result.sharpe_ratio,
                    "profit_factor": result.profit_factor,
                    "data_points": result.data_points,
                    "data_start_time": result.data_start_time,
                    "data_end_time": result.data_end_time,
                    "strategy_params": result.strategy_params,
                    "created_at": result.created_at,
                }
            }))
        }
        Ok(None) => {
            HttpResponse::NotFound().json(serde_json::json!({
                "success": false,
                "message": "Backtest result not found"
            }))
        }
        Err(e) => {
            error!("Failed to fetch backtest detail: {}", e);
            HttpResponse::InternalServerError().json(serde_json::json!({
                "success": false,
                "message": format!("Failed to fetch backtest detail: {}", e)
            }))
        }
    }
}

/// 获取策略实例的回测统计
pub async fn get_backtest_stats(
    data: web::Data<AppState>,
    path: web::Path<String>,
) -> HttpResponse {
    let instance_id_str = path.into_inner();
    let instance_id = match Uuid::parse_str(&instance_id_str) {
        Ok(id) => id,
        Err(_) => {
            return HttpResponse::BadRequest().json(serde_json::json!({
                "success": false,
                "message": "Invalid instance_id format"
            }));
        }
    };

    let backtest_service = BacktestService::new(data.pool.clone());

    match backtest_service.get_instance_stats(instance_id).await {
        Ok(Some(stats)) => {
            HttpResponse::Ok().json(serde_json::json!({
                "success": true,
                "data": {
                    "instance_id": instance_id,
                    "total_backtests": stats.total_backtests,
                    "avg_return_pct": stats.avg_return_pct,
                    "best_return_pct": stats.best_return_pct,
                    "worst_return_pct": stats.worst_return_pct,
                    "avg_win_rate": stats.avg_win_rate,
                    "avg_sharpe_ratio": stats.avg_sharpe_ratio,
                    "avg_max_drawdown": stats.avg_max_drawdown,
                    "avg_profit_factor": stats.avg_profit_factor,
                }
            }))
        }
        Ok(None) => {
            HttpResponse::Ok().json(serde_json::json!({
                "success": true,
                "data": {
                    "instance_id": instance_id,
                    "total_backtests": 0,
                    "message": "No backtest results found for this instance"
                }
            }))
        }
        Err(e) => {
            error!("Failed to fetch backtest stats: {}", e);
            HttpResponse::InternalServerError().json(serde_json::json!({
                "success": false,
                "message": format!("Failed to fetch backtest stats: {}", e)
            }))
        }
    }
}
