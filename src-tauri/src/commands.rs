use crate::state::AppState;
use crate::types::*;
use tauri::State;
use trading_common::{
    backtest::{
        engine::{BacktestEngine, BacktestConfig, BacktestResult},
        strategy::create_strategy,
    },
    data::{
        aggregator::KlineAggregator,
        types::{TradeSide, Timeframe},
    },
};
use rust_decimal::Decimal;

use std::str::FromStr;
use tracing::{info, error};

// 默认交易对
const DEFAULT_SYMBOLS: &[&str] = &["BTCUSDT", "ETHUSDT", "SOLUSDT"];

#[tauri::command]
pub async fn get_data_info(
    state: State<'_, AppState>,
) -> Result<DataInfoResponse, String> {
    info!("Getting backtest data info");
    
    let data_info = state.repository
        .get_backtest_data_info()
        .await
        .map_err(|e| {
            error!("Failed to get data info: {}", e);
            e.to_string()
        })?;

    let response = DataInfoResponse {
        total_records: data_info.total_records,
        symbols_count: data_info.symbols_count,
        earliest_time: data_info.earliest_time.map(|t| t.to_rfc3339()),
        latest_time: data_info.latest_time.map(|t| t.to_rfc3339()),
        symbol_info: data_info.symbol_info.into_iter().map(|info| SymbolInfo {
            symbol: info.symbol,
            records_count: info.records_count,
            earliest_time: info.earliest_time.map(|t| t.to_rfc3339()),
            latest_time: info.latest_time.map(|t| t.to_rfc3339()),
            min_price: info.min_price.map(|p| p.to_string()),
            max_price: info.max_price.map(|p| p.to_string()),
        }).collect(),
    };

    info!("Data info retrieved successfully: {} symbols, {} total records", 
          response.symbols_count, response.total_records);
    Ok(response)
}

#[tauri::command]
pub async fn get_available_strategies() -> Result<Vec<StrategyInfo>, String> {
    info!("Getting available strategies");
    
    let strategies = trading_common::backtest::strategy::list_strategies();
    let response: Vec<StrategyInfo> = strategies.into_iter().map(|s| StrategyInfo {
        id: s.id,
        name: s.name,
        description: s.description,
    }).collect();

    info!("Retrieved {} strategies", response.len());
    Ok(response)
}

#[tauri::command]
pub async fn validate_backtest_config(
    state: State<'_, AppState>,
    symbol: String,
    data_count: i64,
) -> Result<bool, String> {
    info!("Validating backtest config for symbol: {}, data_count: {}", symbol, data_count);
    
    let data_info = state.repository
        .get_backtest_data_info()
        .await
        .map_err(|e| e.to_string())?;

    let is_valid = data_info.has_sufficient_data(&symbol, data_count as u64);
    info!("Validation result: {}", is_valid);
    
    Ok(is_valid)
}

#[tauri::command]
pub async fn get_historical_data(
    state: State<'_, AppState>,
    request: HistoricalDataRequest,
) -> Result<Vec<TickDataResponse>, String> {
    info!("Getting historical data for symbol: {}, limit: {:?}", 
          request.symbol, request.limit);
    
    let limit = request.limit.unwrap_or(1000).min(10000);
    let data = state.repository
        .get_recent_ticks_for_backtest(&request.symbol, limit)
        .await
        .map_err(|e| {
            error!("Failed to get historical data: {}", e);
            e.to_string()
        })?;

    let response: Vec<TickDataResponse> = data.into_iter().map(|tick| TickDataResponse {
        timestamp: tick.timestamp.to_rfc3339(),
        symbol: tick.symbol,
        price: tick.price.to_string(),
        quantity: tick.quantity.to_string(),
        side: match tick.side {
            TradeSide::Buy => "Buy".to_string(),
            TradeSide::Sell => "Sell".to_string(),
        },
    }).collect();

    info!("Retrieved {} historical data points", response.len());
    Ok(response)
}

#[tauri::command]
pub async fn run_backtest(
    state: State<'_, AppState>,
    request: BacktestRequest,
) -> Result<BacktestResponse, String> {
    info!("Starting backtest: strategy={}, symbol={}, data_count={}", 
          request.strategy_id, request.symbol, request.data_count);

    let initial_capital = Decimal::from_str(&request.initial_capital)
        .map_err(|_| "Invalid initial capital")?;
    let commission_rate = Decimal::from_str(&request.commission_rate)
        .map_err(|_| "Invalid commission rate")?;

    let mut config = BacktestConfig::new(initial_capital)
        .with_commission_rate(commission_rate);

    for (key, value) in request.strategy_params {
        config = config.with_param(&key, &value);
    }

    info!("Creating strategy: {}", request.strategy_id);
    let temp_strategy = create_strategy(&request.strategy_id)
        .map_err(|e| {
            error!("Failed to create strategy: {}", e);
            e
        })?;

    let mut data_source = "tick".to_string();

    // Check if strategy supports OHLC
    if temp_strategy.supports_ohlc() {
        if let Some(timeframe) = temp_strategy.preferred_timeframe() {
            info!("Strategy supports OHLC, attempting {} timeframe", timeframe.as_str());
            
            // Estimate candle count (roughly data_count / 50, minimum 100)
            let candle_count = (request.data_count / 50).max(100) as u32;
            
            match state.repository.generate_recent_ohlc_for_backtest(
                &request.symbol, 
                timeframe, 
                candle_count
            ).await {
                Ok(ohlc_data) if !ohlc_data.is_empty() => {
                    info!("Generated {} OHLC candles, running OHLC backtest", ohlc_data.len());
                    data_source = format!("OHLC-{}", timeframe.as_str());
                    
                    let strategy = create_strategy(&request.strategy_id)?;
                    let mut engine = BacktestEngine::new(strategy, config)
                        .map_err(|e| {
                            error!("Failed to create backtest engine: {}", e);
                            e
                        })?;

                    let result = engine.run_with_ohlc(ohlc_data);
                    return Ok(create_backtest_response(result, data_source));
                },
                Ok(_) => {
                    info!("No OHLC data available, falling back to tick data");
                },
                Err(e) => {
                    info!("OHLC generation failed: {}, falling back to tick data", e);
                }
            }
        }
    }

    // Fallback to tick data
    info!("Loading tick data for backtest");
    let data = state.repository
        .get_recent_ticks_for_backtest(&request.symbol, request.data_count)
        .await
        .map_err(|e| {
            error!("Failed to load historical data: {}", e);
            e.to_string()
        })?;

    if data.is_empty() {
        return Err("No historical data available for the specified symbol".to_string());
    }

    info!("Loaded {} tick data points, running tick backtest", data.len());

    let strategy = create_strategy(&request.strategy_id)?;
    let mut engine = BacktestEngine::new(strategy, config)
        .map_err(|e| {
            error!("Failed to create backtest engine: {}", e);
            e
        })?;

    let result = engine.run(data);
    Ok(create_backtest_response(result, data_source))
}

// 3. Add helper function to commands.rs
fn create_backtest_response(result: BacktestResult, data_source: String) -> BacktestResponse {
    info!("Backtest completed successfully");

    BacktestResponse {
        strategy_name: result.strategy_name.clone(),
        initial_capital: result.initial_capital.to_string(),
        final_value: result.final_value.to_string(),
        total_pnl: result.total_pnl.to_string(),
        return_percentage: result.return_percentage.to_string(),
        total_trades: result.total_trades,
        winning_trades: result.winning_trades,
        losing_trades: result.losing_trades,
        max_drawdown: result.max_drawdown.to_string(),
        sharpe_ratio: result.sharpe_ratio.to_string(),
        volatility: result.volatility.to_string(),
        win_rate: result.win_rate.to_string(),
        profit_factor: result.profit_factor.to_string(),
        total_commission: result.total_commission.to_string(),
        data_source, // NEW FIELD
        trades: result.trades.into_iter().map(|trade| TradeInfo {
            timestamp: trade.timestamp.to_rfc3339(),
            symbol: trade.symbol,
            side: match trade.side {
                trading_common::data::types::TradeSide::Buy => "Buy".to_string(),
                trading_common::data::types::TradeSide::Sell => "Sell".to_string(),
            },
            quantity: trade.quantity.to_string(),
            price: trade.price.to_string(),
            realized_pnl: trade.realized_pnl.map(|pnl| pnl.to_string()),
            commission: trade.commission.to_string(),
        }).collect(),
        equity_curve: result.equity_curve.into_iter().map(|value| value.to_string()).collect(),
    }
}

#[tauri::command]
pub async fn get_strategy_capabilities() -> Result<Vec<StrategyCapability>, String> {
    info!("Getting strategy capabilities");
    
    let strategies = trading_common::backtest::strategy::list_strategies();
    let mut capabilities = Vec::new();
    
    for strategy_info in strategies {
        // Create temporary strategy instance to check capabilities
        match trading_common::backtest::strategy::create_strategy(&strategy_info.id) {
            Ok(strategy) => {
                capabilities.push(StrategyCapability {
                    id: strategy_info.id,
                    name: strategy_info.name,
                    description: strategy_info.description,
                    supports_ohlc: strategy.supports_ohlc(),
                    preferred_timeframe: strategy.preferred_timeframe().map(|tf| tf.as_str().to_string()),
                });
            }
            Err(e) => {
                info!("Failed to create strategy {}: {}", strategy_info.id, e);
                capabilities.push(StrategyCapability {
                    id: strategy_info.id,
                    name: strategy_info.name,
                    description: strategy_info.description,
                    supports_ohlc: false,
                    preferred_timeframe: None,
                });
            }
        }
    }
    
    info!("Retrieved capabilities for {} strategies", capabilities.len());
    Ok(capabilities)
}

#[tauri::command]
pub async fn get_ohlc_preview(
    state: State<'_, AppState>,
    request: OHLCRequest,
) -> Result<Vec<OHLCPreview>, String> {
    info!("Getting OHLC preview: {} {} count={}",
          request.symbol, request.timeframe, request.count);

    let timeframe = match request.timeframe.as_str() {
        "1m" => Timeframe::OneMinute,
        "5m" => Timeframe::FiveMinutes,
        "15m" => Timeframe::FifteenMinutes,
        "30m" => Timeframe::ThirtyMinutes,
        "1h" => Timeframe::OneHour,
        "4h" => Timeframe::FourHours,
        "1d" => Timeframe::OneDay,
        "1w" => Timeframe::OneWeek,
        _ => return Err(format!("Invalid timeframe: {}", request.timeframe)),
    };

    // 从 kline_1m 表获取数据，然后聚合
    let klines_1m = state.repository
        .get_klines(&request.symbol, 2000)
        .await
        .map_err(|e| {
            error!("Failed to get klines: {}", e);
            e.to_string()
        })?;

    if klines_1m.is_empty() {
        return Err("No OHLC data available for the specified parameters".to_string());
    }

    // 使用聚合器生成指定时间框架的 K 线
    let mut aggregator = KlineAggregator::new();
    for kline in klines_1m {
        aggregator.update(kline);
    }

    let ohlc_data = aggregator.get_klines(timeframe, request.count as usize);

    if ohlc_data.is_empty() {
        return Err("No OHLC data available for the specified parameters".to_string());
    }

    let response: Vec<OHLCPreview> = ohlc_data.into_iter().map(|ohlc| OHLCPreview {
        timestamp: ohlc.timestamp.to_rfc3339(),
        symbol: ohlc.symbol,
        open: ohlc.open.to_string(),
        high: ohlc.high.to_string(),
        low: ohlc.low.to_string(),
        close: ohlc.close.to_string(),
        volume: ohlc.volume.to_string(),
        trade_count: ohlc.trade_count,
    }).collect();

    info!("Generated {} OHLC preview records", response.len());
    Ok(response)
}

// ============ P8: 实时行情 Commands ============

/// 获取实时价格（从数据库最新数据）
#[tauri::command]
pub async fn get_realtime_prices(
    state: State<'_, AppState>,
    symbols: Option<Vec<String>>,
) -> Result<Vec<RealtimePrice>, String> {
    let target_symbols = symbols.unwrap_or_else(|| {
        DEFAULT_SYMBOLS.iter().map(|s| s.to_string()).collect()
    });

    info!("Getting realtime prices for: {:?}", target_symbols);

    let mut prices = Vec::new();

    for symbol in &target_symbols {
        // 从数据库获取最新 tick 数据
        match state.repository.get_latest_tick(symbol).await {
            Ok(Some(tick)) => {
                prices.push(RealtimePrice {
                    symbol: symbol.clone(),
                    price: tick.price.to_string(),
                    change_24h: None,
                    volume_24h: None,
                    high_24h: None,
                    low_24h: None,
                    updated_at: tick.timestamp.to_rfc3339(),
                });
            }
            Ok(None) => {
                info!("No price data for {}", symbol);
            }
            Err(e) => {
                error!("Failed to get price for {}: {}", symbol, e);
            }
        }
    }

    info!("Retrieved {} realtime prices", prices.len());
    Ok(prices)
}

/// 获取 K 线历史数据
#[tauri::command]
pub async fn get_kline_history(
    state: State<'_, AppState>,
    request: PriceHistoryRequest,
) -> Result<Vec<KlineData>, String> {
    let timeframe = match request.timeframe.as_str() {
        "1m" => Timeframe::OneMinute,
        "5m" => Timeframe::FiveMinutes,
        "15m" => Timeframe::FifteenMinutes,
        "30m" => Timeframe::ThirtyMinutes,
        "1h" => Timeframe::OneHour,
        "4h" => Timeframe::FourHours,
        "1d" => Timeframe::OneDay,
        _ => return Err(format!("Invalid timeframe: {}", request.timeframe)),
    };

    let count = request.limit.unwrap_or(500).min(2000);

    info!("Getting kline history: {} {} limit={}", request.symbol, request.timeframe, count);

    // 从 kline_1m 表获取数据，然后聚合
    let klines_1m = state.repository
        .get_klines(&request.symbol, 2000)
        .await
        .map_err(|e| {
            error!("Failed to get klines: {}", e);
            e.to_string()
        })?;

    if klines_1m.is_empty() {
        return Ok(Vec::new());
    }

    // 使用聚合器生成指定时间框架的 K 线
    let mut aggregator = KlineAggregator::new();
    for kline in klines_1m {
        aggregator.update(kline);
    }

    let ohlc_data = aggregator.get_klines(timeframe, count as usize);

    let klines: Vec<KlineData> = ohlc_data.into_iter().map(|ohlc| KlineData {
        timestamp: ohlc.timestamp.to_rfc3339(),
        open: ohlc.open.to_string(),
        high: ohlc.high.to_string(),
        low: ohlc.low.to_string(),
        close: ohlc.close.to_string(),
        volume: ohlc.volume.to_string(),
    }).collect();

    info!("Retrieved {} klines", klines.len());
    Ok(klines)
}

/// 获取 24h 统计数据
#[tauri::command]
pub async fn get_24h_stats(
    state: State<'_, AppState>,
    symbol: String,
) -> Result<serde_json::Value, String> {
    info!("Getting 24h stats for: {}", symbol);

    // 获取最近 24h 的 tick 数据统计
    let stats = state.repository
        .get_symbol_stats(&symbol, 24)
        .await
        .map_err(|e| {
            error!("Failed to get 24h stats: {}", e);
            e.to_string()
        })?;

    Ok(stats)
}

// ============ P9: 持仓和交易记录 Commands ============

/// 获取当前持仓列表
#[tauri::command]
pub async fn get_positions(
    state: State<'_, AppState>,
) -> Result<Vec<PositionInfo>, String> {
    info!("Getting positions");

    let positions_data = state.repository
        .get_positions()
        .await
        .map_err(|e| {
            error!("Failed to get positions: {}", e);
            e.to_string()
        })?;

    let positions: Vec<PositionInfo> = positions_data.iter().map(|p| {
        PositionInfo {
            id: p["id"].as_str().unwrap_or_default().to_string(),
            symbol: p["symbol"].as_str().unwrap_or_default().to_string(),
            side: p["side"].as_str().unwrap_or_default().to_string(),
            quantity: p["quantity"].as_str().unwrap_or_default().to_string(),
            avg_entry_price: p["avg_entry_price"].as_str().unwrap_or_default().to_string(),
            current_price: p["current_price"].as_str().map(|s| s.to_string()),
            unrealized_pnl: p["unrealized_pnl"].as_str().map(|s| s.to_string()),
            realized_pnl: p["realized_pnl"].as_str().unwrap_or_default().to_string(),
            opened_at: p["opened_at"].as_str().unwrap_or_default().to_string(),
            updated_at: p["updated_at"].as_str().unwrap_or_default().to_string(),
        }
    }).collect();

    info!("Retrieved {} positions", positions.len());
    Ok(positions)
}

/// 获取交易历史记录
#[tauri::command]
pub async fn get_trade_history(
    state: State<'_, AppState>,
    request: TradeHistoryRequest,
) -> Result<Vec<TradeRecord>, String> {
    let limit = request.limit.unwrap_or(100).min(1000);
    let offset = request.offset.unwrap_or(0);

    info!("Getting trade history: symbol={:?}, limit={}, offset={}",
          request.symbol, limit, offset);

    let trades_data = state.repository
        .get_trade_history(request.symbol.as_deref(), limit, offset)
        .await
        .map_err(|e| {
            error!("Failed to get trade history: {}", e);
            e.to_string()
        })?;

    let trades: Vec<TradeRecord> = trades_data.iter().map(|t| {
        TradeRecord {
            id: t["id"].as_str().unwrap_or_default().to_string(),
            order_id: t["order_id"].as_str().map(|s| s.to_string()),
            symbol: t["symbol"].as_str().unwrap_or_default().to_string(),
            side: t["side"].as_str().unwrap_or_default().to_string(),
            price: t["price"].as_str().unwrap_or_default().to_string(),
            quantity: t["quantity"].as_str().unwrap_or_default().to_string(),
            commission: t["commission"].as_str().unwrap_or_default().to_string(),
            realized_pnl: t["realized_pnl"].as_str().map(|s| s.to_string()),
            strategy_id: t["strategy_id"].as_str().map(|s| s.to_string()),
            trade_time: t["trade_time"].as_str().unwrap_or_default().to_string(),
            created_at: t["created_at"].as_str().unwrap_or_default().to_string(),
        }
    }).collect();

    info!("Retrieved {} trades", trades.len());
    Ok(trades)
}

/// 获取盈亏汇总统计
#[tauri::command]
pub async fn get_pnl_summary(
    state: State<'_, AppState>,
    request: PnlSummaryRequest,
) -> Result<PnlSummary, String> {
    let days = request.days.unwrap_or(30);

    info!("Getting PnL summary for {} days", days);

    let summary_data = state.repository
        .get_pnl_summary(request.symbol.as_deref(), days)
        .await
        .map_err(|e| {
            error!("Failed to get PnL summary: {}", e);
            e.to_string()
        })?;

    let summary = PnlSummary {
        period_days: summary_data["period_days"].as_i64().unwrap_or(days as i64) as i32,
        symbol: summary_data["symbol"].as_str().map(|s| s.to_string()),
        total_trades: summary_data["total_trades"].as_i64().unwrap_or(0),
        winning_trades: summary_data["winning_trades"].as_i64().unwrap_or(0),
        losing_trades: summary_data["losing_trades"].as_i64().unwrap_or(0),
        win_rate: summary_data["win_rate"].as_str().unwrap_or("0.00").to_string(),
        total_pnl: summary_data["total_pnl"].as_str().map(|s| s.to_string()),
        total_commission: summary_data["total_commission"].as_str().map(|s| s.to_string()),
        best_trade: summary_data["best_trade"].as_str().map(|s| s.to_string()),
        worst_trade: summary_data["worst_trade"].as_str().map(|s| s.to_string()),
        avg_pnl: summary_data["avg_pnl"].as_str().map(|s| s.to_string()),
    };

    info!("PnL summary retrieved: {} trades, win rate {}%",
          summary.total_trades, summary.win_rate);
    Ok(summary)
}

// ============ P10: 统计分析 Commands ============

/// 获取资金曲线数据
#[tauri::command]
pub async fn get_equity_curve(
    state: State<'_, AppState>,
    request: EquityCurveRequest,
) -> Result<Vec<EquityCurvePoint>, String> {
    let period = request.period.unwrap_or_else(|| "daily".to_string());
    let days = request.days.unwrap_or(90);

    info!("Getting equity curve: period={}, days={}", period, days);

    let curve_data = state.repository
        .get_equity_curve(request.symbol.as_deref(), &period, days)
        .await
        .map_err(|e| {
            error!("Failed to get equity curve: {}", e);
            e.to_string()
        })?;

    let points: Vec<EquityCurvePoint> = curve_data.iter().map(|p| {
        EquityCurvePoint {
            date: p["date"].as_str().unwrap_or_default().to_string(),
            equity: "0".to_string(), // Will be calculated from initial + cumulative
            pnl: p["pnl"].as_str().unwrap_or("0").to_string(),
            cumulative_pnl: p["cumulative_pnl"].as_str().unwrap_or("0").to_string(),
        }
    }).collect();

    info!("Retrieved {} equity curve points", points.len());
    Ok(points)
}

/// 获取性能指标（夏普比率、最大回撤等）
#[tauri::command]
pub async fn get_performance_metrics(
    state: State<'_, AppState>,
    request: PerformanceRequest,
) -> Result<PerformanceMetrics, String> {
    let days = request.days.unwrap_or(30);

    info!("Getting performance metrics for {} days", days);

    let metrics_data = state.repository
        .get_performance_metrics(request.symbol.as_deref(), days)
        .await
        .map_err(|e| {
            error!("Failed to get performance metrics: {}", e);
            e.to_string()
        })?;

    let metrics = PerformanceMetrics {
        sharpe_ratio: metrics_data["sharpe_ratio"].as_str().unwrap_or("0").to_string(),
        sortino_ratio: metrics_data["sortino_ratio"].as_str().unwrap_or("0").to_string(),
        max_drawdown: metrics_data["max_drawdown"].as_str().unwrap_or("0").to_string(),
        max_drawdown_duration_days: 0, // TODO: Calculate from data
        calmar_ratio: metrics_data["calmar_ratio"].as_str().unwrap_or("0").to_string(),
        volatility: metrics_data["volatility"].as_str().unwrap_or("0").to_string(),
        win_rate: metrics_data["win_rate"].as_str().unwrap_or("0").to_string(),
        profit_factor: metrics_data["profit_factor"].as_str().unwrap_or("0").to_string(),
        avg_trade_duration_hours: 0.0, // TODO: Calculate from data
        total_trades: metrics_data["total_trades"].as_i64().unwrap_or(0),
        winning_trades: metrics_data["winning_trades"].as_i64().unwrap_or(0),
        losing_trades: metrics_data["losing_trades"].as_i64().unwrap_or(0),
        avg_win: metrics_data["avg_win"].as_str().unwrap_or("0").to_string(),
        avg_loss: metrics_data["avg_loss"].as_str().unwrap_or("0").to_string(),
        largest_win: metrics_data["largest_win"].as_str().unwrap_or("0").to_string(),
        largest_loss: metrics_data["largest_loss"].as_str().unwrap_or("0").to_string(),
        consecutive_wins: metrics_data["consecutive_wins"].as_i64().unwrap_or(0) as i32,
        consecutive_losses: metrics_data["consecutive_losses"].as_i64().unwrap_or(0) as i32,
    };

    info!("Performance metrics retrieved: Sharpe={}, MaxDD={}",
          metrics.sharpe_ratio, metrics.max_drawdown);
    Ok(metrics)
}

/// 获取手续费统计
#[tauri::command]
pub async fn get_commission_stats(
    state: State<'_, AppState>,
    request: PerformanceRequest,
) -> Result<CommissionStats, String> {
    let days = request.days.unwrap_or(30);

    info!("Getting commission stats for {} days", days);

    let stats_data = state.repository
        .get_commission_stats(request.symbol.as_deref(), days)
        .await
        .map_err(|e| {
            error!("Failed to get commission stats: {}", e);
            e.to_string()
        })?;

    let by_symbol: Vec<SymbolCommission> = stats_data["commission_by_symbol"]
        .as_array()
        .unwrap_or(&vec![])
        .iter()
        .map(|s| SymbolCommission {
            symbol: s["symbol"].as_str().unwrap_or_default().to_string(),
            total_commission: s["total_commission"].as_str().unwrap_or("0").to_string(),
            trade_count: s["trade_count"].as_i64().unwrap_or(0),
        })
        .collect();

    let by_month: Vec<MonthlyCommission> = stats_data["commission_by_month"]
        .as_array()
        .unwrap_or(&vec![])
        .iter()
        .map(|m| MonthlyCommission {
            month: m["month"].as_str().unwrap_or_default().to_string(),
            total_commission: m["total_commission"].as_str().unwrap_or("0").to_string(),
            trade_count: m["trade_count"].as_i64().unwrap_or(0),
        })
        .collect();

    let stats = CommissionStats {
        total_commission: stats_data["total_commission"].as_str().unwrap_or("0").to_string(),
        avg_commission_per_trade: stats_data["avg_commission_per_trade"].as_str().unwrap_or("0").to_string(),
        commission_by_symbol: by_symbol,
        commission_by_month: by_month,
    };

    info!("Commission stats retrieved: total={}", stats.total_commission);
    Ok(stats)
}

// ============ P11: 高级回测 Commands ============

/// 执行多时间框架回测（完整模拟交易）
#[tauri::command]
pub async fn run_multi_timeframe_backtest(
    state: State<'_, AppState>,
    request: MultiTimeframeBacktestRequest,
) -> Result<BacktestResponse, String> {
    info!("Starting multi-timeframe backtest: strategy={}, symbol={}, data_count={}",
          request.strategy, request.symbol, request.data_count);

    // 验证策略
    if !trading_common::backtest::strategy::is_multi_timeframe_strategy(&request.strategy) {
        return Err(format!("Strategy '{}' is not a multi-timeframe strategy", request.strategy));
    }

    let initial_capital = Decimal::from_str(&request.capital.to_string())
        .map_err(|_| "Invalid initial capital")?;
    let commission_rate = Decimal::from_str(&(request.commission_rate / 100.0).to_string())
        .map_err(|_| "Invalid commission rate")?;

    let mut config = trading_common::backtest::engine::BacktestConfig::new(initial_capital)
        .with_commission_rate(commission_rate);

    if let Some(params) = &request.strategy_params {
        for (key, value) in params {
            config = config.with_param(key, value);
        }
    }

    // 获取 1m K线数据
    let candle_count = request.data_count.max(1000) as u32;
    let klines_1m = state.repository
        .get_klines(&request.symbol, candle_count)
        .await
        .map_err(|e| {
            error!("Failed to get klines: {}", e);
            e.to_string()
        })?;

    if klines_1m.is_empty() {
        return Err("No 1m kline data available. Ensure candle1m data collection is running.".to_string());
    }

    info!("Loaded {} 1m klines for multi-timeframe backtest", klines_1m.len());

    // 创建策略和引擎
    let strategy = trading_common::backtest::strategy::create_multi_timeframe_strategy(&request.strategy)?;
    let mut engine = trading_common::backtest::MultiTimeframeBacktestEngine::new(
        strategy,
        config,
        request.symbol.clone(),
    ).map_err(|e| {
        error!("Failed to create backtest engine: {}", e);
        e
    })?;

    let result = engine.run(klines_1m.clone());

    info!("Multi-timeframe backtest completed: {} trades, return={:.2}%",
          result.total_trades, result.return_percentage);

    Ok(BacktestResponse {
        strategy_name: format!("{} (Multi-TF)", request.strategy),
        initial_capital: initial_capital.to_string(),
        final_value: result.final_value.to_string(),
        total_pnl: result.total_pnl.to_string(),
        return_percentage: result.return_percentage.to_string(),
        total_trades: result.total_trades,
        winning_trades: result.winning_trades,
        losing_trades: result.losing_trades,
        max_drawdown: result.max_drawdown.to_string(),
        sharpe_ratio: result.sharpe_ratio.to_string(),
        volatility: result.volatility.to_string(),
        win_rate: result.win_rate.to_string(),
        profit_factor: result.profit_factor.to_string(),
        total_commission: result.total_commission.to_string(),
        data_source: "1m-kline".to_string(),
        trades: result.trades.into_iter().map(|trade| TradeInfo {
            timestamp: trade.timestamp.to_rfc3339(),
            symbol: trade.symbol,
            side: match trade.side {
                trading_common::data::types::TradeSide::Buy => "Buy".to_string(),
                trading_common::data::types::TradeSide::Sell => "Sell".to_string(),
            },
            quantity: trade.quantity.to_string(),
            price: trade.price.to_string(),
            realized_pnl: trade.realized_pnl.map(|pnl| pnl.to_string()),
            commission: trade.commission.to_string(),
        }).collect(),
        equity_curve: result.equity_curve.into_iter().map(|v| v.to_string()).collect(),
    })
}

/// 执行滚动前进测试
#[tauri::command]
pub async fn run_walk_forward_test(
    state: State<'_, AppState>,
    request: WalkForwardRequest,
) -> Result<WalkForwardResult, String> {
    info!("Starting walk-forward test: strategy={}, symbol={}", request.strategy, request.symbol);

    if !trading_common::backtest::strategy::is_multi_timeframe_strategy(&request.strategy) {
        return Err(format!("Strategy '{}' is not a multi-timeframe strategy", request.strategy));
    }

    let initial_capital = Decimal::from_str(&request.capital.to_string())
        .map_err(|_| "Invalid initial capital")?;
    let commission_rate = Decimal::from_str(&(request.commission_rate / 100.0).to_string())
        .map_err(|_| "Invalid commission rate")?;

    let mut bt_config = trading_common::backtest::BacktestConfig::new(initial_capital)
        .with_commission_rate(commission_rate);
    if let Some(params) = &request.strategy_params {
        for (key, value) in params {
            bt_config = bt_config.with_param(key, value);
        }
    }

    let wf_config = trading_common::backtest::WalkForwardConfig::default()
        .with_train_candles(request.train_candles)
        .with_test_candles(request.test_candles)
        .with_step_candles(request.step_candles);

    // 获取 1m K线数据
    let klines_1m = state.repository
        .get_klines(&request.symbol, request.data_count)
        .await
        .map_err(|e| {
            error!("Failed to get klines: {}", e);
            e.to_string()
        })?;

    if klines_1m.is_empty() {
        return Err("No 1m kline data available for walk-forward analysis".to_string());
    }

    info!("Loaded {} 1m klines for walk-forward", klines_1m.len());

    let strategy_id = request.strategy.clone();
    let result = trading_common::backtest::WalkForwardEngine::run(
        || trading_common::backtest::strategy::create_multi_timeframe_strategy(&strategy_id)
            .unwrap_or_else(|e| {
                error!("Failed to create strategy '{}': {}", strategy_id, e);
                trading_common::backtest::strategy::create_multi_timeframe_strategy("trend")
                    .expect("Fallback strategy 'trend' must exist")
            }),
        &bt_config,
        &wf_config,
        &klines_1m,
        &request.symbol,
    ).map_err(|e| {
        error!("Walk-forward failed: {}", e);
        e
    })?;

    let rounds: Vec<WalkForwardRoundSummary> = result.round_summaries.iter().map(|r| {
        WalkForwardRoundSummary {
            round: r.round,
            train_start: r.train_start.to_rfc3339(),
            train_end: r.train_end.to_rfc3339(),
            test_start: r.test_start.to_rfc3339(),
            test_end: r.test_end.to_rfc3339(),
            train_return_pct: format!("{:.2}%", r.train_return_pct),
            train_sharpe: format!("{:.2}", r.train_sharpe),
            train_trades: r.train_trades,
            test_return_pct: format!("{:.2}%", r.test_return_pct),
            test_sharpe: format!("{:.2}", r.test_sharpe),
            test_trades: r.test_trades,
            test_win_rate: format!("{:.2}%", r.test_win_rate),
            test_max_drawdown: format!("{:.2}%", r.test_max_drawdown),
            overfit_ratio: format!("{:.2}", r.overfit_ratio),
        }
    }).collect();

    Ok(WalkForwardResult {
        total_rounds: result.total_rounds,
        profitable_rounds: result.profitable_rounds,
        overall_test_return_pct: format!("{:.2}%", result.overall_test_return_pct),
        overall_test_sharpe: format!("{:.2}", result.overall_test_sharpe),
        overall_test_max_drawdown: format!("{:.2}%", result.overall_test_max_drawdown),
        overall_test_win_rate: format!("{:.2}%", result.overall_test_win_rate),
        avg_overfit_ratio: format!("{:.2}", result.avg_overfit_ratio),
        is_overfit: result.is_overfit,
        rounds,
    })
}

/// 执行样本外测试
#[tauri::command]
pub async fn run_out_of_sample_test(
    state: State<'_, AppState>,
    request: OutOfSampleRequest,
) -> Result<OutOfSampleResult, String> {
    info!("Starting out-of-sample test: strategy={}, symbol={}", request.strategy, request.symbol);

    if !trading_common::backtest::strategy::is_multi_timeframe_strategy(&request.strategy) {
        return Err(format!("Strategy '{}' is not a multi-timeframe strategy", request.strategy));
    }

    let initial_capital = Decimal::from_str(&request.capital.to_string())
        .map_err(|_| "Invalid initial capital")?;
    let commission_rate = Decimal::from_str(&(request.commission_rate / 100.0).to_string())
        .map_err(|_| "Invalid commission rate")?;

    let mut bt_config = trading_common::backtest::BacktestConfig::new(initial_capital)
        .with_commission_rate(commission_rate);
    if let Some(params) = &request.strategy_params {
        for (key, value) in params {
            bt_config = bt_config.with_param(key, value);
        }
    }

    let os_config = trading_common::backtest::OutOfSampleConfig {
        train_ratio: Decimal::from_str(&request.train_ratio.to_string())
            .unwrap_or_else(|_| Decimal::new(7, 1)),
    };

    let klines_1m = state.repository
        .get_klines(&request.symbol, request.data_count)
        .await
        .map_err(|e| {
            error!("Failed to get klines: {}", e);
            e.to_string()
        })?;

    if klines_1m.is_empty() {
        return Err("No 1m kline data available for out-of-sample analysis".to_string());
    }

    info!("Loaded {} 1m klines for out-of-sample", klines_1m.len());

    let strategy_id = request.strategy.clone();
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
        &request.symbol,
    ).map_err(|e| {
        error!("Out-of-sample failed: {}", e);
        e
    })?;

    Ok(OutOfSampleResult {
        train_return_pct: format!("{:.2}%", result.train_result.return_percentage),
        train_sharpe: format!("{:.2}", result.train_sharpe),
        train_max_drawdown: format!("{:.2}%", result.train_result.max_drawdown),
        train_win_rate: format!("{:.2}%", result.train_result.win_rate),
        train_trades: result.train_result.total_trades,
        train_profit_factor: format!("{:.2}", result.train_result.profit_factor),
        test_return_pct: format!("{:.2}%", result.test_result.return_percentage),
        test_sharpe: format!("{:.2}", result.test_sharpe),
        test_max_drawdown: format!("{:.2}%", result.test_result.max_drawdown),
        test_win_rate: format!("{:.2}%", result.test_result.win_rate),
        test_trades: result.test_result.total_trades,
        test_profit_factor: format!("{:.2}", result.test_result.profit_factor),
        overfit_ratio: format!("{:.2}", result.overfit_ratio),
        is_overfit: result.is_overfit,
    })
}

/// 执行多交易对回测
#[tauri::command]
pub async fn run_multi_symbol_backtest(
    state: State<'_, AppState>,
    request: MultiSymbolBacktestRequest,
) -> Result<MultiSymbolBacktestResult, String> {
    info!("Starting multi-symbol backtest: strategy={}, symbols={:?}",
          request.strategy, request.symbols);

    if !trading_common::backtest::strategy::is_multi_timeframe_strategy(&request.strategy) {
        return Err(format!("Strategy '{}' is not a multi-timeframe strategy", request.strategy));
    }

    let initial_capital = Decimal::from_str(&request.capital.to_string())
        .map_err(|_| "Invalid initial capital")?;
    let commission_rate = Decimal::from_str(&(request.commission_rate / 100.0).to_string())
        .map_err(|_| "Invalid commission rate")?;

    let mut bt_config = trading_common::backtest::BacktestConfig::new(initial_capital)
        .with_commission_rate(commission_rate);
    if let Some(params) = &request.strategy_params {
        for (key, value) in params {
            bt_config = bt_config.with_param(key, value);
        }
    }

    // 确定 symbol 列表
    let symbols = if request.symbols.is_empty() {
        let data_info = state.repository.get_backtest_data_info()
            .await
            .map_err(|e| e.to_string())?;
        data_info.get_available_symbols()
    } else {
        request.symbols.clone()
    };

    if symbols.is_empty() {
        return Err("No symbols available for backtest".to_string());
    }

    // 加载每个 symbol 的数据
    let mut symbol_data = std::collections::HashMap::new();
    for symbol in &symbols {
        match state.repository.get_klines(symbol, request.data_count).await {
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
        return Err("No kline data available for any symbol".to_string());
    }

    let strategy_id = request.strategy.clone();
    let result = trading_common::backtest::MultiSymbolBacktestEngine::run(
        move || trading_common::backtest::strategy::create_multi_timeframe_strategy(&strategy_id)
            .unwrap_or_else(|e| {
                error!("Failed to create strategy '{}': {}", strategy_id, e);
                trading_common::backtest::strategy::create_multi_timeframe_strategy("trend")
                    .expect("Fallback strategy 'trend' must exist")
            }),
        &bt_config,
        &symbol_data,
        request.market_state_window,
    ).map_err(|e| {
        error!("Multi-symbol backtest failed: {}", e);
        e
    })?;

    let symbols_result: Vec<SymbolBacktestResultItem> = result.results.iter().map(|r| {
        SymbolBacktestResultItem {
            symbol: r.symbol.clone(),
            return_pct: format!("{:.2}%", r.result.return_percentage),
            sharpe: format!("{:.2}", r.result.sharpe_ratio),
            win_rate: format!("{:.2}%", r.result.win_rate),
            max_drawdown: format!("{:.2}%", r.result.max_drawdown),
            total_trades: r.result.total_trades,
            profit_factor: format!("{:.2}", r.result.profit_factor),
            market_state: r.market_state.summary.clone(),
            data_quality: format!("{:.0}/100", r.market_state.data_quality_score),
        }
    }).collect();

    Ok(MultiSymbolBacktestResult {
        total_symbols: result.total_symbols,
        profitable_symbols: result.profitable_symbols,
        losing_symbols: result.losing_symbols,
        avg_return_pct: format!("{:.2}%", result.avg_return_pct),
        avg_sharpe: format!("{:.2}", result.avg_sharpe),
        avg_win_rate: format!("{:.2}%", result.avg_win_rate),
        avg_max_drawdown: format!("{:.2}%", result.avg_max_drawdown),
        total_trades: result.total_trades,
        best_symbol: result.best_symbol,
        best_return_pct: format!("{:.2}%", result.best_return_pct),
        worst_symbol: result.worst_symbol,
        worst_return_pct: format!("{:.2}%", result.worst_return_pct),
        cross_symbol_correlation: format!("{:.2}", result.cross_symbol_correlation),
        symbols: symbols_result,
    })
}

/// 分析市场状态
#[tauri::command]
pub async fn analyze_market_state(
    state: State<'_, AppState>,
    request: MarketStateAnalysisRequest,
) -> Result<MarketStateResult, String> {
    info!("Analyzing market state: symbol={}, data_count={}, window={}",
          request.symbol, request.data_count, request.window);

    let klines_1m = state.repository
        .get_klines(&request.symbol, request.data_count)
        .await
        .map_err(|e| {
            error!("Failed to get klines: {}", e);
            e.to_string()
        })?;

    if klines_1m.is_empty() {
        return Err(format!("No kline data available for {}", request.symbol));
    }

    info!("Loaded {} klines for market state analysis", klines_1m.len());

    let report = trading_common::backtest::MarketStateAnalyzer::analyze(&klines_1m, request.window);

    let state_distribution: std::collections::HashMap<String, String> = report
        .state_percentages
        .iter()
        .map(|(k, v)| (k.clone(), format!("{:.1}%", v)))
        .collect();

    Ok(MarketStateResult {
        symbol: request.symbol,
        total_candles: report.total_candles,
        analysis_window: report.analysis_window,
        state_distribution,
        avg_volatility: format!("{:.2}%", report.avg_volatility),
        avg_trend_strength: format!("{:.2}", report.avg_trend_strength),
        trend_ratio: format!("{:.1}%", report.trend_ratio),
        ranging_ratio: format!("{:.1}%", report.ranging_ratio),
        data_quality_score: format!("{:.0}/100", report.data_quality_score),
        summary: report.summary,
    })
}
// ===== Paper Trading Commands =====

/// 启动模拟交易
#[tauri::command]
pub async fn start_paper_trading(
    state: State<'_, AppState>,
    request: Option<PaperStartRequest>,
) -> Result<PaperStatusResponse, String> {
    info!("Starting paper trading");

    let mut trader = state.paper_trader.write().await;

    // 如果已在运行，先停止
    if trader.is_running() {
        trader.stop();
    }

    // 如果提供了新配置，重新创建 trader
    if let Some(req) = request {
        let config = trading_common::paper::PaperTraderConfig {
            initial_capital: req.initial_capital
                .and_then(|s| Decimal::from_str(&s).ok())
                .unwrap_or(Decimal::from(10000)),
            commission_rate: req.commission_rate
                .and_then(|s| Decimal::from_str(&s).ok())
                .unwrap_or(Decimal::from_str("0.001").unwrap()),
            slippage_pct: req.slippage_pct
                .and_then(|s| Decimal::from_str(&s).ok())
                .unwrap_or(Decimal::from_str("0.0001").unwrap()),
            symbols: req.symbols.unwrap_or_else(|| vec!["BTCUSDT".to_string(), "ETHUSDT".to_string()]),
        };
        *trader = trading_common::paper::PaperTrader::new(config);
    }

    // 尝试从数据库获取最新价格作为初始价格
    let symbols = trader.get_config().symbols.clone();
    for symbol in &symbols {
        if let Ok(Some(tick)) = state.repository.get_latest_tick(symbol).await {
            trader.update_price(symbol, tick.price);
        }
    }

    trader.start();
    let status = trader.get_status();

    Ok(convert_paper_status(status))
}

/// 停止模拟交易
#[tauri::command]
pub async fn stop_paper_trading(
    state: State<'_, AppState>,
) -> Result<PaperStatusResponse, String> {
    info!("Stopping paper trading");

    let mut trader = state.paper_trader.write().await;
    trader.stop();
    let status = trader.get_status();

    Ok(convert_paper_status(status))
}

/// 获取模拟交易状态
#[tauri::command]
pub async fn get_paper_status(
    state: State<'_, AppState>,
) -> Result<PaperStatusResponse, String> {
    let mut trader = state.paper_trader.write().await;

    // 更新最新价格
    let symbols = trader.get_config().symbols.clone();
    for symbol in &symbols {
        if let Ok(Some(tick)) = state.repository.get_latest_tick(symbol).await {
            trader.update_price(symbol, tick.price);
        }
    }

    let status = trader.get_status();
    Ok(convert_paper_status(status))
}

/// 模拟交易手动下单
#[tauri::command]
pub async fn place_paper_order(
    state: State<'_, AppState>,
    request: PaperOrderRequest,
) -> Result<PaperTradeResponse, String> {
    info!("Paper order: {} {} {}", request.side, request.quantity, request.symbol);

    let mut trader = state.paper_trader.write().await;

    // 更新价格
    if let Ok(Some(tick)) = state.repository.get_latest_tick(&request.symbol).await {
        trader.update_price(&request.symbol, tick.price);
    }

    let quantity = Decimal::from_str(&request.quantity)
        .map_err(|e| format!("Invalid quantity: {}", e))?;

    let side = match request.side.to_lowercase().as_str() {
        "buy" => TradeSide::Buy,
        "sell" => TradeSide::Sell,
        _ => return Err("Invalid side: must be 'buy' or 'sell'".to_string()),
    };

    let order = match request.order_type.as_deref().unwrap_or("market") {
        "market" => trader.place_market_order(&request.symbol, side, quantity),
        "limit" => {
            let price = request.price
                .and_then(|s| Decimal::from_str(&s).ok())
                .ok_or("Limit order requires price")?;
            trader.place_limit_order(&request.symbol, side, quantity, price)
        }
        "stop_loss" => {
            let price = request.price
                .and_then(|s| Decimal::from_str(&s).ok())
                .ok_or("Stop loss order requires price")?;
            trader.place_stop_loss_order(&request.symbol, side, quantity, price)
        }
        "take_profit" => {
            let price = request.price
                .and_then(|s| Decimal::from_str(&s).ok())
                .ok_or("Take profit order requires price")?;
            trader.place_take_profit_order(&request.symbol, side, quantity, price)
        }
        _ => return Err("Invalid order type: market, limit, stop_loss, take_profit".to_string()),
    };

    order.map(convert_paper_trade)
}

/// 获取模拟交易记录
#[tauri::command]
pub async fn get_paper_trades(
    state: State<'_, AppState>,
) -> Result<Vec<PaperTradeResponse>, String> {
    let trader = state.paper_trader.read().await;
    let trades = trader.get_trades();

    Ok(trades.into_iter().map(convert_paper_trade).collect())
}

/// 获取模拟交易挂单
#[tauri::command]
pub async fn get_paper_pending_orders(
    state: State<'_, AppState>,
) -> Result<Vec<PaperTradeResponse>, String> {
    let trader = state.paper_trader.read().await;
    let orders = trader.get_pending_orders();

    Ok(orders.into_iter().map(convert_paper_trade).collect())
}

/// 取消模拟交易挂单
#[tauri::command]
pub async fn cancel_paper_order(
    state: State<'_, AppState>,
    order_id: String,
) -> Result<(), String> {
    let mut trader = state.paper_trader.write().await;
    trader.cancel_order(&order_id)
}

/// 重置模拟交易
#[tauri::command]
pub async fn reset_paper_trading(
    state: State<'_, AppState>,
) -> Result<PaperStatusResponse, String> {
    info!("Resetting paper trading");

    let mut trader = state.paper_trader.write().await;
    trader.reset();
    let status = trader.get_status();

    Ok(convert_paper_status(status))
}

// ===== Paper Trading 辅助函数 =====

fn convert_paper_status(status: trading_common::paper::PaperTraderStatus) -> PaperStatusResponse {
    PaperStatusResponse {
        running: status.running,
        initial_capital: status.initial_capital.to_string(),
        cash: status.cash.to_string(),
        total_value: status.total_value.to_string(),
        total_pnl: status.total_pnl.to_string(),
        total_pnl_pct: format!("{:.2}", status.total_pnl_pct),
        realized_pnl: status.realized_pnl.to_string(),
        unrealized_pnl: status.unrealized_pnl.to_string(),
        total_commission: status.total_commission.to_string(),
        total_trades: status.total_trades,
        win_rate: format!("{:.1}", status.win_rate),
        positions: status.positions.into_iter().map(|p| PaperPositionResponse {
            symbol: p.symbol,
            side: p.side,
            quantity: p.quantity.to_string(),
            avg_price: p.avg_price.to_string(),
            current_price: p.current_price.to_string(),
            market_value: p.market_value.to_string(),
            unrealized_pnl: p.unrealized_pnl.to_string(),
            unrealized_pnl_pct: format!("{:.2}", p.unrealized_pnl_pct),
        }).collect(),
        pending_orders: status.pending_orders,
        latest_prices: status.latest_prices.into_iter()
            .map(|(k, v)| (k, v.to_string()))
            .collect(),
        started_at: status.started_at.map(|t| t.to_rfc3339()),
    }
}

fn convert_paper_trade(order: trading_common::paper::PaperOrder) -> PaperTradeResponse {
    PaperTradeResponse {
        order_id: order.order_id,
        symbol: order.symbol,
        side: match order.side {
            TradeSide::Buy => "Buy".to_string(),
            TradeSide::Sell => "Sell".to_string(),
        },
        order_type: match order.order_type {
            trading_common::paper::PaperOrderType::Market => "Market".to_string(),
            trading_common::paper::PaperOrderType::Limit => "Limit".to_string(),
            trading_common::paper::PaperOrderType::StopLoss => "StopLoss".to_string(),
            trading_common::paper::PaperOrderType::TakeProfit => "TakeProfit".to_string(),
        },
        quantity: order.quantity.to_string(),
        price: order.price.map(|p| p.to_string()),
        status: match order.status {
            trading_common::paper::PaperOrderStatus::Pending => "Pending".to_string(),
            trading_common::paper::PaperOrderStatus::Filled => "Filled".to_string(),
            trading_common::paper::PaperOrderStatus::Canceled => "Canceled".to_string(),
            trading_common::paper::PaperOrderStatus::Rejected => "Rejected".to_string(),
        },
        filled_price: order.filled_price.map(|p| p.to_string()),
        commission: order.commission.to_string(),
        created_at: order.created_at.to_rfc3339(),
        filled_at: order.filled_at.map(|t| t.to_rfc3339()),
        reject_reason: order.reject_reason,
    }
}

/// 检查 trading-core 服务状态
#[tauri::command]
pub async fn check_trading_core_status(
    state: State<'_, AppState>,
) -> Result<TradingCoreStatusResponse, String> {
    info!("Checking trading-core status");

    // 检查数据库连接
    let database_ok = state.repository
        .get_backtest_data_info()
        .await
        .is_ok();

    Ok(TradingCoreStatusResponse {
        status: "connected".to_string(),
        database: database_ok,
    })
}
