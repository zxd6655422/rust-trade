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
use rust_decimal::prelude::ToPrimitive;
use sqlx::Row;

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
            total_volume_usd: info.total_volume_usd.to_string(),
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

    // 检查是否是多时间框架策略
    let is_mtf = trading_common::backtest::strategy::is_multi_timeframe_strategy(&request.strategy_id);

    if is_mtf {
        // 多时间框架策略回测
        info!("Using multi-timeframe backtest for strategy: {}", request.strategy_id);

        // 获取 1m K线数据
        let candle_count = request.data_count.max(1000) as u32;
        let klines_1m = match state.repository.get_klines(&request.symbol, candle_count).await {
            Ok(klines) if !klines.is_empty() => klines,
            _ => {
                return Err("No 1m kline data available for multi-timeframe backtest".to_string());
            }
        };

        info!("Loaded {} 1m klines for backtest", klines_1m.len());

        // 创建多时间框架策略
        let strategy = trading_common::backtest::strategy::create_multi_timeframe_strategy(&request.strategy_id)
            .map_err(|e| format!("Failed to create strategy: {}", e))?;

        // 创建并运行多时间框架回测引擎
        let mut engine = trading_common::backtest::MultiTimeframeBacktestEngine::new(
            strategy,
            config,
            request.symbol.clone(),
        )
        .map_err(|e| format!("Failed to create backtest engine: {}", e))?;

        let result = engine.run(klines_1m);

        return Ok(BacktestResponse {
            strategy_name: request.strategy_id,
            initial_capital: format!("${}", initial_capital),
            final_value: format!("${:.2}", result.final_value),
            total_pnl: format!("${:.2}", result.final_value - initial_capital),
            return_percentage: format!("{:.2}%", result.return_percentage),
            total_trades: result.total_trades,
            winning_trades: result.winning_trades,
            losing_trades: result.losing_trades,
            max_drawdown: format!("{:.2}%", result.max_drawdown),
            sharpe_ratio: format!("{:.2}", result.sharpe_ratio),
            volatility: "N/A".to_string(),
            win_rate: format!("{:.2}%", result.win_rate),
            profit_factor: format!("{:.2}", result.profit_factor),
            total_commission: "N/A".to_string(),
            trades: vec![],
            equity_curve: vec![],
            data_source: "1m-klines".to_string(),
        });
    }

    // 单时间框架策略回测
    info!("Creating single-timeframe strategy: {}", request.strategy_id);
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
        "4h" => Timeframe::FourHour,
        "1d" => Timeframe::OneDay,
        "1w" => Timeframe::OneWeek,
        _ => return Err(format!("Invalid timeframe: {}", request.timeframe)),
    };

    // 根据时间框架计算需要多少 1m K 线
    let needed_1m = match timeframe {
        Timeframe::OneMinute => (request.count as u32).max(200),
        Timeframe::FiveMinutes => (request.count as u32 * 5).max(500),
        Timeframe::FifteenMinutes => (request.count as u32 * 15).max(1500),
        Timeframe::ThirtyMinutes => (request.count as u32 * 30).max(2500),
        Timeframe::OneHour => (request.count as u32 * 60).max(5000),
        Timeframe::TwoHour => (request.count as u32 * 120).max(5000),
        Timeframe::FourHour => (request.count as u32 * 240).max(5000),
        Timeframe::ThreeDay => 5000,
        Timeframe::OneDay => 5000,
        Timeframe::OneWeek => 5000,
    };

    let klines_1m = state.repository
        .get_klines(&request.symbol, needed_1m)
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

/// 获取实时价格（从 kline_1m 表最新数据）
#[tauri::command]
pub async fn get_realtime_prices(
    state: State<'_, AppState>,
    symbols: Option<Vec<String>>,
) -> Result<Vec<RealtimePrice>, String> {
    // 如果没指定 symbol，从数据库获取所有有数据的 symbol
    let target_symbols = match symbols {
        Some(s) if !s.is_empty() => s,
        _ => state.repository.get_available_symbols().await
            .unwrap_or_else(|_| DEFAULT_SYMBOLS.iter().map(|s| s.to_string()).collect()),
    };

    info!("Getting realtime prices for: {:?}", target_symbols);

    let mut prices = Vec::new();

    for symbol in &target_symbols {
        // 从 kline_1m 获取最新价格和24h统计
        match state.repository.get_kline_with_24h_stats(symbol).await {
            Ok(Some((kline, stats))) => {
                prices.push(RealtimePrice {
                    symbol: symbol.clone(),
                    price: kline.close.to_string(),
                    change_24h: stats.change_pct.map(|v| v.to_string()),
                    volume_24h: stats.volume_24h.map(|v| v.to_string()),
                    high_24h: stats.high_24h.map(|v| v.to_string()),
                    low_24h: stats.low_24h.map(|v| v.to_string()),
                    updated_at: kline.timestamp.to_rfc3339(),
                });
            }
            Ok(None) => {
                info!("No kline data for {}", symbol);
            }
            Err(e) => {
                error!("Failed to get kline for {}: {}", symbol, e);
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
        "4h" => Timeframe::FourHour,
        "1d" => Timeframe::OneDay,
        _ => return Err(format!("Invalid timeframe: {}", request.timeframe)),
    };

    let count = request.limit.unwrap_or(500).min(2000);

    info!("Getting kline history: {} {} limit={}", request.symbol, request.timeframe, count);

    // 根据时间框架计算需要多少 1m K 线
    let needed_1m = match timeframe {
        Timeframe::OneMinute => (count as u32).max(200),
        Timeframe::FiveMinutes => (count as u32 * 5).max(500),
        Timeframe::FifteenMinutes => (count as u32 * 15).max(1500),
        Timeframe::ThirtyMinutes => (count as u32 * 30).max(2500),
        Timeframe::OneHour => (count as u32 * 60).max(5000),
        Timeframe::TwoHour => (count as u32 * 120).max(5000),
        Timeframe::FourHour => (count as u32 * 240).max(5000),
        Timeframe::ThreeDay => 5000,
        Timeframe::OneDay => 5000,
        Timeframe::OneWeek => 5000,
    };

    let klines_1m = state.repository
        .get_klines(&request.symbol, needed_1m)
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

/// 从快照表获取最新账户信息
#[tauri::command]
pub async fn get_account_snapshot(
    state: State<'_, AppState>,
    exchange: Option<String>,
    market_type: Option<String>,
) -> Result<Option<serde_json::Value>, String> {
    let exchange = exchange.unwrap_or_else(|| "binance".to_string());
    let market_type = market_type.unwrap_or_else(|| "futures".to_string());

    info!("Getting account snapshot for exchange={}, market_type={}", exchange, market_type);

    let snapshot = state.repository
        .get_latest_account_snapshot(&exchange, &market_type)
        .await
        .map_err(|e| {
            error!("Failed to get account snapshot: {}", e);
            e.to_string()
        })?;

    match snapshot {
        Some(s) => {
            let equity = s.total_equity.to_string();
            let unrealized = s.unrealized_pnl.to_string();
            info!("Account snapshot: equity={}, unrealized_pnl={}, positions={}", equity, unrealized, s.position_count);
            Ok(Some(serde_json::json!({
                "exchange": s.exchange,
                "market_type": s.market_type,
                "snapshot_at": s.snapshot_at.to_rfc3339(),
                "total_equity": equity,
                "total_balance": s.total_balance.to_string(),
                "available_balance": s.available_balance.to_string(),
                "frozen_balance": s.frozen_balance.to_string(),
                "unrealized_pnl": unrealized,
                "initial_margin": s.initial_margin.map(|v| v.to_string()),
                "maint_margin": s.maint_margin.map(|v| v.to_string()),
                "margin_ratio": s.margin_ratio.map(|v| v.to_string()),
                "position_count": s.position_count,
            })))
        }
        None => {
            info!("No account snapshot found for exchange={}, market_type={}", exchange, market_type);
            Ok(None)
        }
    }
}

/// 从快照表获取最新持仓列表
#[tauri::command]
pub async fn get_account_positions(
    state: State<'_, AppState>,
    exchange: Option<String>,
) -> Result<Vec<serde_json::Value>, String> {
    let exchange = exchange.unwrap_or_else(|| "binance".to_string());

    info!("Getting account positions from snapshot for exchange={}", exchange);

    let positions = state.repository
        .get_latest_positions(&exchange)
        .await
        .map_err(|e| {
            error!("Failed to get account positions: {}", e);
            e.to_string()
        })?;

    let result: Vec<serde_json::Value> = positions.iter().map(|p| {
        serde_json::json!({
            "exchange": p.exchange,
            "symbol": p.symbol,
            "raw_symbol": p.raw_symbol,
            "snapshot_at": p.snapshot_at.to_rfc3339(),
            "position_side": p.position_side.as_str(),
            "position_amt": p.position_amt.to_string(),
            "entry_price": p.entry_price.to_string(),
            "mark_price": p.mark_price.to_string(),
            "unrealized_pnl": p.unrealized_pnl.to_string(),
            "leverage": p.leverage,
            "margin_type": p.margin_type.as_str(),
            "initial_margin": p.initial_margin.to_string(),
            "maint_margin": p.maint_margin.to_string(),
            "liquidation_price": p.liquidation_price.map(|v| v.to_string()),
            "notional": p.notional.to_string(),
            "break_even_price": p.break_even_price.map(|v| v.to_string()),
        })
    }).collect();

    info!("Retrieved {} account positions from snapshot", result.len());
    Ok(result)
}

/// 获取现货资产余额（从 asset_balance 快照表，过滤余额为 0 的资产）
#[tauri::command]
pub async fn get_asset_balances(
    state: State<'_, AppState>,
    exchange: Option<String>,
    market_type: Option<String>,
) -> Result<Vec<AssetBalanceItem>, String> {
    let exchange = exchange.unwrap_or_else(|| "binance".to_string());
    let market_type = market_type.unwrap_or_else(|| "spot".to_string());

    info!("Getting asset balances for exchange={}, market_type={}", exchange, market_type);

    let balances = state.repository
        .get_latest_asset_balances(&exchange, &market_type)
        .await
        .map_err(|e| {
            error!("Failed to get asset balances: {}", e);
            e.to_string()
        })?;

    let result: Vec<AssetBalanceItem> = balances.iter().map(|b| AssetBalanceItem {
        asset: b.asset.clone(),
        total: b.total.to_string(),
        available: b.available.to_string(),
        frozen: b.frozen.to_string(),
    }).collect();

    info!("Retrieved {} asset balances", result.len());
    Ok(result)
}

/// 获取现货实时价格（通过 trading-core 代理获取，不直接访问交易所）
#[tauri::command]
pub async fn get_spot_prices(
    assets: Vec<String>,
    server_url: Option<String>,
) -> Result<std::collections::HashMap<String, String>, String> {
    use std::collections::HashMap;

    // 稳定币直接返回 1:1
    let stablecoins = ["USDT", "USDC", "BUSD", "DAI", "TUSD", "FDUSD", "USDP"];
    let mut prices: HashMap<String, String> = HashMap::new();
    let mut need_fetch: Vec<String> = Vec::new();

    for asset in &assets {
        if stablecoins.contains(&asset.as_str()) {
            prices.insert(asset.clone(), "1".to_string());
        } else {
            need_fetch.push(asset.clone());
        }
    }

    if !need_fetch.is_empty() {
        // 通过 trading-core 代理获取价格
        let base_url = server_url.unwrap_or_else(|| "http://localhost:8080".to_string());
        let assets_param = need_fetch.join(",");
        let url = format!("{}/api/spot/prices?assets={}", base_url, assets_param);

        let client = reqwest::Client::builder()
            .timeout(std::time::Duration::from_secs(15))
            .build()
            .map_err(|e| e.to_string())?;

        let resp = client.get(&url)
            .send()
            .await
            .map_err(|e| format!("Failed to fetch prices from trading-core: {}", e))?;

        let result: serde_json::Value = resp.json()
            .await
            .map_err(|e| format!("Failed to parse prices response: {}", e))?;

        // 解析 trading-core 返回的价格数据
        if let Some(data) = result.get("data").and_then(|d| d.as_object()) {
            for (asset, price_val) in data {
                if let Some(price_str) = price_val.as_str() {
                    prices.insert(asset.clone(), price_str.to_string());
                }
            }
        }
    }

    info!("Fetched prices for {} assets", prices.len());
    Ok(prices)
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

    info!("Getting PnL summary for {} days, exchange={:?}, market_type={:?}", days, request.exchange, request.market_type);

    let summary_data = state.repository
        .get_pnl_summary(
            request.symbol.as_deref(),
            days,
            request.exchange.as_deref(),
            request.market_type.as_deref(),
        )
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

    info!("Getting performance metrics for {} days, exchange={:?}, market_type={:?}", days, request.exchange, request.market_type);

    let metrics_data = state.repository
        .get_performance_metrics(
            request.symbol.as_deref(),
            days,
            request.exchange.as_deref(),
            request.market_type.as_deref(),
        )
        .await
        .map_err(|e| {
            error!("Failed to get performance metrics: {}", e);
            e.to_string()
        })?;

    let metrics = PerformanceMetrics {
        sharpe_ratio: metrics_data["sharpe_ratio"].as_str().unwrap_or("0").to_string(),
        sortino_ratio: metrics_data["sortino_ratio"].as_str().unwrap_or("0").to_string(),
        max_drawdown: metrics_data["max_drawdown"].as_str().unwrap_or("0").to_string(),
        max_drawdown_duration_days: metrics_data["max_drawdown_duration_days"].as_i64().unwrap_or(0),
        calmar_ratio: metrics_data["calmar_ratio"].as_str().unwrap_or("0").to_string(),
        volatility: metrics_data["volatility"].as_str().unwrap_or("0").to_string(),
        win_rate: metrics_data["win_rate"].as_str().unwrap_or("0").to_string(),
        profit_factor: metrics_data["profit_factor"].as_str().unwrap_or("0").to_string(),
        avg_trade_duration_hours: metrics_data["avg_trade_duration_hours"].as_f64().unwrap_or(0.0),
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
            instance_id: None,
            strategy_type: None,
            display_name: None,
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
    server_url: Option<String>,
) -> Result<TradingCoreStatusResponse, String> {
    info!("Checking trading-core status");

    // 检查数据库连接
    let database_ok = state.repository
        .get_backtest_data_info()
        .await
        .is_ok();

    // 使用配置的地址，或默认 localhost:8080
    let url = server_url.unwrap_or_else(|| "http://localhost:8080".to_string());
    let health_url = format!("{}/health", url.trim_end_matches('/'));

    // 真正检查 trading-core HTTP 服务是否在运行
    let trading_core_ok = match reqwest::get(&health_url).await {
        Ok(resp) => resp.status().is_success(),
        Err(_) => false,
    };

    Ok(TradingCoreStatusResponse {
        status: if trading_core_ok { "connected" } else { "disconnected" }.to_string(),
        database: database_ok,
    })
}

// ============ 策略实时分析（带生命周期闭环） ============

/// 获取策略实时分析结果，同时写入 strategy_analysis_log 并处理生命周期
#[tauri::command]
pub async fn get_strategy_analysis(
    state: State<'_, AppState>,
    request: StrategyAnalysisRequest,
) -> Result<StrategyAnalysisResult, String> {
    let strategy_id = request.strategy_id.unwrap_or_else(|| "trend".to_string());
    info!("Strategy analysis: symbol={}, strategy={}", request.symbol, strategy_id);

    if !trading_common::backtest::strategy::is_multi_timeframe_strategy(&strategy_id) {
        return Err(format!("Strategy '{}' does not support real-time analysis.", strategy_id));
    }

    // 1. 拉取 K 线并执行分析
    let klines_1m = state.repository.get_klines(&request.symbol, 2000).await
        .map_err(|e| { error!("get_klines failed: {}", e); e.to_string() })?;
    if klines_1m.is_empty() {
        return Err("No kline data available.".to_string());
    }

    let current_price = klines_1m.last().unwrap().close;

    let mut aggregator = KlineAggregator::new();
    for kline in &klines_1m { aggregator.update(kline.clone()); }

    let mut strategy = trading_common::backtest::strategy::create_multi_timeframe_strategy(&strategy_id)?;
    let mut tf_klines = std::collections::HashMap::new();
    for tf in strategy.required_timeframes() {
        tf_klines.insert(tf, aggregator.get_klines(tf, 200));
    }
    let analysis = strategy.analyze(&tf_klines);

    // 2. 构建 timeframe_details JSON
    let mut tf_details = serde_json::Map::new();
    let tf_map = [
        (Timeframe::FourHour, "4h"),
        (Timeframe::OneHour, "1h"),
        (Timeframe::FifteenMinutes, "15m"),
    ];
    for (tf, label) in &tf_map {
        if let Some(ta) = analysis.timeframe_analyses.get(tf) {
            tf_details.insert(label.to_string(), serde_json::json!({
                "direction": format!("{:?}", ta.direction).to_lowercase(),
                "confidence": ta.confidence.to_string(),
                "description": ta.description
            }));
        }
    }
    let tf_json = serde_json::Value::Object(tf_details);

    let dir_str = match analysis.overall_direction {
        trading_common::backtest::strategy::TrendDirection::Bullish => "bullish",
        trading_common::backtest::strategy::TrendDirection::Bearish => "bearish",
        _ => "neutral",
    };
    let entry_dir = analysis.entry_direction.map(|d| match d {
        trading_common::backtest::strategy::EntryDirection::Long => "long",
        trading_common::backtest::strategy::EntryDirection::Short => "short",
    });

    // 3. 生命周期闭环：检查上一条 pending 分析
    let mut need_save_new = true;
    if let Some(prev) = state.repository.get_pending_analysis(&request.symbol, &strategy_id).await
        .map_err(|e| e.to_string())?
    {
        let is_same_dir = prev.direction == dir_str;
        let age_hours = (chrono::Utc::now() - prev.created_at).num_hours();

        if is_same_dir && age_hours <= 24 {
            // ★ 同方向且未过期 → 不保存新记录，只更新验证状态
            let _ = state.repository.update_analysis_eval(prev.id, current_price).await;
            let return_pct = calc_return_pct(&prev.direction, prev.entry_price, current_price);
            let threshold = Decimal::from_str("0.5").unwrap();
            let confirmed = match prev.direction.as_str() {
                "bullish" => return_pct > threshold,
                "bearish" => return_pct < -threshold,
                _ => false,
            };
            if confirmed {
                let _ = state.repository.close_analysis(
                    prev.id, "confirmed", "price_confirmed", current_price, return_pct
                ).await;
                info!("Confirmed analysis {} (return={}%)", prev.id, return_pct);
                need_save_new = true; // 确认后可以产生新信号
            } else {
                need_save_new = false; // 还在 pending，不重复保存
            }
        } else if !is_same_dir {
            // 方向反转 → 关闭旧信号为 superseded
            let return_pct = calc_return_pct(&prev.direction, prev.entry_price, current_price);
            let _ = state.repository.close_analysis(
                prev.id, "superseded", "direction_changed", current_price, return_pct
            ).await;
            info!("Superseded analysis {} ({} -> {})", prev.id, prev.direction, dir_str);
        } else {
            // 超时 → 关闭为 expired
            let return_pct = calc_return_pct(&prev.direction, prev.entry_price, current_price);
            let _ = state.repository.close_analysis(
                prev.id, "expired", "timeout", current_price, return_pct
            ).await;
            info!("Expired analysis {} ({}h)", prev.id, age_hours);
        }
    }

    // 4. 仅在需要时保存新分析记录
    if need_save_new {
        let _ = state.repository.save_analysis_log(
            &request.symbol, &strategy_id, dir_str, current_price,
            analysis.overall_confidence, analysis.entry_allowed,
            entry_dir, tf_json,
        ).await.map_err(|e| e.to_string())?;
    }

    // 5. 构建返回结果
    let mut timeframes = Vec::new();
    for (tf, label) in &tf_map {
        if let Some(ta) = analysis.timeframe_analyses.get(tf) {
            timeframes.push(TimeframeAnalysis {
                timeframe: label.to_string(),
                direction: format!("{:?}", ta.direction).to_lowercase(),
                confidence: ta.confidence.to_string(),
                description: ta.description.clone(),
            });
        }
    }

    let strategy_info = trading_common::backtest::strategy::get_strategy_info(&strategy_id);

    Ok(StrategyAnalysisResult {
        symbol: request.symbol,
        strategy_id: strategy_id.clone(),
        strategy_name: strategy_info.map(|s| s.name).unwrap_or_else(|| strategy_id),
        timeframes,
        overall_direction: dir_str.to_string(),
        overall_confidence: analysis.overall_confidence.to_string(),
        entry_allowed: analysis.entry_allowed,
        entry_direction: entry_dir.map(|s| s.to_string()),
        analysis_time: chrono::Utc::now().to_rfc3339(),
    })
}

/// 计算信号收益率%
fn calc_return_pct(direction: &str, entry_price: Decimal, current_price: Decimal) -> Decimal {
    if entry_price == Decimal::ZERO { return Decimal::ZERO; }
    let pct = (current_price - entry_price) / entry_price * Decimal::from(100);
    match direction {
        "bullish" => pct,         // 多头：涨=正
        "bearish" => -pct,        // 空头：跌=正
        _ => Decimal::ZERO,
    }
}

/// 获取策略历史信号记录和胜率统计（从 strategy_analysis_log 表）
#[tauri::command]
pub async fn get_signal_history(
    state: State<'_, AppState>,
    request: SignalHistoryRequest,
) -> Result<SignalHistoryResult, String> {
    let limit = request.limit.unwrap_or(50).min(200);
    info!("Signal history: symbol={:?}, limit={}", request.symbol, limit);

    // 从 strategy_analysis_log 表查询
    let records = state.repository.get_analysis_history(
        request.symbol.as_deref(), None, limit,
    ).await.map_err(|e| { error!("get_analysis_history failed: {}", e); e.to_string() })?;

    // 获取统计
    let stats_data = state.repository.get_signal_stats(
        "strategy_analysis_log", request.symbol.as_deref(), None,
    ).await.map_err(|e| e.to_string())?;

    // 转换为前端类型
    let signals: Vec<SignalRecord> = records.iter().map(|r| {
        let (outcome, pnl_str) = match r.actual_return_pct {
            Some(p) if p > Decimal::ZERO => (Some("confirmed".to_string()), Some(format!("+{:.2}%", p))),
            Some(p) if p < Decimal::ZERO => (Some("invalidated".to_string()), Some(format!("{:.2}%", p))),
            Some(_) => (Some("break_even".to_string()), Some("0.00%".to_string())),
            None => match r.status.as_str() {
                "pending" => (Some("pending".to_string()), None),
                "expired" => (Some("expired".to_string()), None),
                "superseded" => (Some("superseded".to_string()), None),
                _ => (Some(r.status.clone()), None),
            },
        };
        SignalRecord {
            id: r.id.to_string(),
            timestamp: r.created_at.to_rfc3339(),
            symbol: r.symbol.clone(),
            direction: r.direction.clone(),
            price: r.entry_price.to_string(),
            outcome,
            pnl: pnl_str,
        }
    }).collect();

    Ok(SignalHistoryResult {
        signals,
        stats: SignalStats {
            total_signals: stats_data.total_signals,
            confirmed: stats_data.confirmed,
            invalidated: stats_data.invalidated,
            expired: stats_data.expired,
            pending: stats_data.pending,
            win_rate: stats_data.confirmation_rate_pct.map(|v| format!("{}%", v)).unwrap_or_else(|| "0%".to_string()),
            avg_return: stats_data.avg_return_pct.map(|v| format!("{:.2}%", v)).unwrap_or_else(|| "0%".to_string()),
        },
    })
}

/// 获取信号统计数据（单独接口，供前端 AutoTradingStatus 调用）
#[tauri::command]
pub async fn get_signal_stats(
    state: State<'_, AppState>,
    request: SignalStatsRequest,
) -> Result<SignalStats, String> {
    let stats_data = state.repository.get_signal_stats(
        &request.table,
        request.symbol.as_deref(),
        request.strategy_id.as_deref(),
    ).await.map_err(|e| e.to_string())?;

    Ok(SignalStats {
        total_signals: stats_data.total_signals,
        confirmed: stats_data.confirmed,
        invalidated: stats_data.invalidated,
        expired: stats_data.expired,
        pending: stats_data.pending,
        win_rate: stats_data.confirmation_rate_pct.map(|v| format!("{}%", v)).unwrap_or_else(|| "0%".to_string()),
        avg_return: stats_data.avg_return_pct.map(|v| format!("{:.2}%", v)).unwrap_or_else(|| "0%".to_string()),
    })
}

// ============ 交易对管理 ============

/// 获取所有交易对（含启用状态）
#[tauri::command]
pub async fn get_symbols(state: State<'_, AppState>) -> Result<Vec<SymbolConfig>, String> {
    let symbols = state.repository.get_all_symbols().await
        .map_err(|e| e.to_string())?;
    Ok(symbols.into_iter().map(|(s, enabled)| SymbolConfig { symbol: s, enabled }).collect())
}

/// 添加交易对
#[tauri::command]
pub async fn add_symbol(state: State<'_, AppState>, symbol: String) -> Result<(), String> {
    let symbol = symbol.to_uppercase();
    if symbol.is_empty() || symbol.len() > 20 {
        return Err("Invalid symbol".to_string());
    }
    state.repository.add_symbol(&symbol).await.map_err(|e| e.to_string())
}

/// 删除交易对
#[tauri::command]
pub async fn remove_symbol(state: State<'_, AppState>, symbol: String) -> Result<(), String> {
    state.repository.remove_symbol(&symbol).await.map_err(|e| e.to_string())
}

/// 启用/禁用交易对
#[tauri::command]
pub async fn toggle_symbol(state: State<'_, AppState>, symbol: String, enabled: bool) -> Result<(), String> {
    state.repository.set_symbol_enabled(&symbol, enabled).await.map_err(|e| e.to_string())
}

// ============ 交易对配置 Commands (trading_pairs) ============

/// 获取所有交易对配置
#[tauri::command]
pub async fn get_trading_pairs(
    state: State<'_, AppState>,
    status: Option<String>,
) -> Result<Vec<TradingPairConfig>, String> {
    let pool = state.repository.get_pool();

    let rows = if let Some(s) = status {
        sqlx::query(
            "SELECT id, symbol, market_type, exchange, status, note, created_at, updated_at \
             FROM trading_pairs WHERE status = $1 ORDER BY symbol"
        )
        .bind(&s)
        .fetch_all(pool)
        .await
        .map_err(|e| e.to_string())?
    } else {
        sqlx::query(
            "SELECT id, symbol, market_type, exchange, status, note, created_at, updated_at \
             FROM trading_pairs ORDER BY symbol"
        )
        .fetch_all(pool)
        .await
        .map_err(|e| e.to_string())?
    };

    let pairs: Vec<TradingPairConfig> = rows.iter().map(|row| TradingPairConfig {
        id: row.get("id"),
        symbol: row.get("symbol"),
        market_type: row.get("market_type"),
        exchange: row.get("exchange"),
        status: row.get("status"),
        note: row.get("note"),
        created_at: row.get::<chrono::DateTime<chrono::Utc>, _>("created_at").format("%Y-%m-%d %H:%M:%S").to_string(),
        updated_at: row.get::<chrono::DateTime<chrono::Utc>, _>("updated_at").format("%Y-%m-%d %H:%M:%S").to_string(),
    }).collect();

    Ok(pairs)
}

/// 添加交易对配置
#[tauri::command]
pub async fn add_trading_pair(
    state: State<'_, AppState>,
    symbol: String,
    market_type: String,
    exchange: String,
    note: Option<String>,
) -> Result<TradingPairConfig, String> {
    let pool = state.repository.get_pool();
    let symbol = symbol.to_uppercase();

    // 验证参数
    if !["spot", "futures"].contains(&market_type.as_str()) {
        return Err("market_type must be 'spot' or 'futures'".to_string());
    }
    if !["binance", "okx"].contains(&exchange.as_str()) {
        return Err("exchange must be 'binance' or 'okx'".to_string());
    }

    // 插入或更新
    let row = sqlx::query(
        "INSERT INTO trading_pairs (symbol, market_type, exchange, status, note) \
         VALUES ($1, $2, $3, 'active', $4) \
         ON CONFLICT (symbol) DO UPDATE SET \
             market_type = EXCLUDED.market_type, \
             exchange = EXCLUDED.exchange, \
             status = 'active', \
             note = EXCLUDED.note, \
             updated_at = NOW() \
         RETURNING id, symbol, market_type, exchange, status, note, created_at, updated_at"
    )
    .bind(&symbol)
    .bind(&market_type)
    .bind(&exchange)
    .bind(&note)
    .fetch_one(pool)
    .await
    .map_err(|e| e.to_string())?;

    info!("Added trading pair: {} ({}/{})", symbol, market_type, exchange);

    Ok(TradingPairConfig {
        id: row.get("id"),
        symbol: row.get("symbol"),
        market_type: row.get("market_type"),
        exchange: row.get("exchange"),
        status: row.get("status"),
        note: row.get("note"),
        created_at: row.get::<chrono::DateTime<chrono::Utc>, _>("created_at").format("%Y-%m-%d %H:%M:%S").to_string(),
        updated_at: row.get::<chrono::DateTime<chrono::Utc>, _>("updated_at").format("%Y-%m-%d %H:%M:%S").to_string(),
    })
}

/// 更新交易对状态
#[tauri::command]
pub async fn update_trading_pair_status(
    state: State<'_, AppState>,
    symbol: String,
    status: String,
) -> Result<(), String> {
    let pool = state.repository.get_pool();

    if !["active", "paused", "archived"].contains(&status.as_str()) {
        return Err("status must be 'active', 'paused', or 'archived'".to_string());
    }

    let affected = sqlx::query(
        "UPDATE trading_pairs SET status = $1, updated_at = NOW() WHERE symbol = $2"
    )
    .bind(&status)
    .bind(&symbol)
    .execute(pool)
    .await
    .map_err(|e| e.to_string())?
    .rows_affected();

    if affected == 0 {
        return Err(format!("Trading pair {} not found", symbol));
    }

    info!("Updated trading pair {} status to {}", symbol, status);
    Ok(())
}

/// 删除交易对配置
#[tauri::command]
pub async fn delete_trading_pair(
    state: State<'_, AppState>,
    symbol: String,
) -> Result<(), String> {
    let pool = state.repository.get_pool();

    let affected = sqlx::query("DELETE FROM trading_pairs WHERE symbol = $1")
        .bind(&symbol)
        .execute(pool)
        .await
        .map_err(|e| e.to_string())?
        .rows_affected();

    if affected == 0 {
        return Err(format!("Trading pair {} not found", symbol));
    }

    info!("Deleted trading pair {}", symbol);
    Ok(())
}

/// 从 kline_1m 获取有数据的交易对（去重）
#[tauri::command]
pub async fn get_available_symbols_from_data(
    state: State<'_, AppState>,
) -> Result<Vec<String>, String> {
    let pool = state.repository.get_pool();

    let rows = sqlx::query_scalar::<_, String>(
        "SELECT DISTINCT symbol FROM kline_1m ORDER BY symbol"
    )
    .fetch_all(pool)
    .await
    .map_err(|e| e.to_string())?;

    Ok(rows)
}

/// 将交易对添加到监控列表 (symbol_config)
#[tauri::command]
pub async fn add_to_monitor(
    state: State<'_, AppState>,
    symbol: String,
) -> Result<(), String> {
    state.repository.add_symbol(&symbol).await.map_err(|e| e.to_string())?;
    info!("Added {} to monitoring list", symbol);
    Ok(())
}

/// 从监控列表移除交易对
#[tauri::command]
pub async fn remove_from_monitor(
    state: State<'_, AppState>,
    symbol: String,
) -> Result<(), String> {
    state.repository.remove_symbol(&symbol).await.map_err(|e| e.to_string())?;
    info!("Removed {} from monitoring list", symbol);
    Ok(())
}

// ============ 策略调度器控制 Commands ============

/// 获取策略调度器状态
#[tauri::command]
pub async fn get_scheduler_status(
    state: State<'_, AppState>,
) -> Result<SchedulerStatus, String> {
    let is_paused = state.repository.is_scheduler_paused().await.unwrap_or(false);
    Ok(SchedulerStatus {
        is_running: !is_paused,
        is_paused,
        strategy_id: "trend".to_string(),
    })
}

/// 暂停策略调度器
#[tauri::command]
pub async fn pause_scheduler(
    state: State<'_, AppState>,
) -> Result<(), String> {
    state.repository.set_scheduler_paused(true).await.map_err(|e| e.to_string())?;
    info!("Strategy scheduler paused");
    Ok(())
}

/// 恢复策略调度器
#[tauri::command]
pub async fn resume_scheduler(
    state: State<'_, AppState>,
) -> Result<(), String> {
    state.repository.set_scheduler_paused(false).await.map_err(|e| e.to_string())?;
    info!("Strategy scheduler resumed");
    Ok(())
}

// ============ 数据管理 Commands ============

/// 交易对配置
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct TradingPairConfig {
    pub id: i32,
    pub symbol: String,
    pub market_type: String,  // spot/futures
    pub exchange: String,     // binance/okx
    pub status: String,       // active/paused/archived
    pub note: Option<String>,
    pub created_at: String,
    pub updated_at: String,
}

/// 数据采集状态
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct CollectionStatus {
    /// 交易对
    pub symbol: String,
    /// 状态
    pub status: String,
    /// 市场类型
    pub market_type: String,
    /// 数据库中已有的记录数
    pub record_count: i64,
    /// 最早数据时间
    pub earliest_time: Option<String>,
    /// 最新数据时间
    pub latest_time: Option<String>,
}

/// 归档结果
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ArchiveResult {
    /// 交易对
    pub symbol: String,
    /// 归档的记录数
    pub archived_count: u64,
    /// Parquet 文件大小 (MB)
    pub file_size_mb: f64,
    /// 是否成功
    pub success: bool,
    /// 错误信息
    pub error: Option<String>,
}

/// 获取数据采集状态
#[tauri::command]
pub async fn get_collection_status(
    state: State<'_, AppState>,
    symbol: String,
) -> Result<CollectionStatus, String> {
    let pool = state.repository.get_pool();

    // 获取记录数
    let count: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM kline_1m WHERE symbol = $1"
    )
    .bind(&symbol)
    .fetch_one(pool)
    .await
    .map_err(|e| e.to_string())?;

    // 获取最早和最新时间
    let time_range: (Option<chrono::NaiveDateTime>, Option<chrono::NaiveDateTime>) = sqlx::query_as(
        "SELECT MIN(timestamp), MAX(timestamp) FROM kline_1m WHERE symbol = $1"
    )
    .bind(&symbol)
    .fetch_one(pool)
    .await
    .map_err(|e| e.to_string())?;

    // 获取交易对配置
    let pair_config: Option<(String, String)> = sqlx::query_as(
        "SELECT market_type, status FROM trading_pairs WHERE symbol = $1"
    )
    .bind(&symbol)
    .fetch_optional(pool)
    .await
    .map_err(|e| e.to_string())?;

    let (market_type, status) = pair_config.unwrap_or(("futures".to_string(), "active".to_string()));

    Ok(CollectionStatus {
        symbol,
        status,
        market_type,
        record_count: count.0,
        earliest_time: time_range.0.map(|t| t.format("%Y-%m-%d %H:%M:%S").to_string()),
        latest_time: time_range.1.map(|t| t.format("%Y-%m-%d %H:%M:%S").to_string()),
    })
}

/// 获取所有交易对的采集状态
#[tauri::command]
pub async fn get_all_collection_status(
    state: State<'_, AppState>,
) -> Result<Vec<CollectionStatus>, String> {
    let pool = state.repository.get_pool();

    // 从 trading_pairs 表获取所有交易对配置
    let rows = sqlx::query(
        "SELECT tp.symbol, tp.market_type, tp.status, \
                COALESCE((SELECT COUNT(*) FROM kline_1m k WHERE k.symbol = tp.symbol), 0) as record_count, \
                (SELECT MIN(timestamp) FROM kline_1m k WHERE k.symbol = tp.symbol) as earliest_time, \
                (SELECT MAX(timestamp) FROM kline_1m k WHERE k.symbol = tp.symbol) as latest_time \
         FROM trading_pairs tp ORDER BY tp.symbol"
    )
    .fetch_all(pool)
    .await
    .map_err(|e| e.to_string())?;

    let statuses: Vec<CollectionStatus> = rows.iter().map(|row| CollectionStatus {
        symbol: row.get("symbol"),
        market_type: row.get("market_type"),
        status: row.get("status"),
        record_count: row.get::<Option<i64>, _>("record_count").unwrap_or(0),
        earliest_time: row.get::<Option<chrono::DateTime<chrono::Utc>>, _>("earliest_time")
            .map(|t| t.format("%Y-%m-%d %H:%M:%S").to_string()),
        latest_time: row.get::<Option<chrono::DateTime<chrono::Utc>>, _>("latest_time")
            .map(|t| t.format("%Y-%m-%d %H:%M:%S").to_string()),
    }).collect();

    Ok(statuses)
}

/// 添加交易对并开始采集
#[tauri::command]
pub async fn add_symbol_with_collection(
    state: State<'_, AppState>,
    symbol: String,
    backfill_days: Option<i64>,
) -> Result<CollectionStatus, String> {
    let symbol = symbol.to_uppercase();

    // 1. 添加到 symbol_config 并启用
    state.repository.add_symbol(&symbol).await.map_err(|e| e.to_string())?;

    info!("Added symbol {} to collection", symbol);

    // 2. 如果指定了回填天数，记录日志（实际回填需要重启服务）
    if let Some(days) = backfill_days {
        if days > 0 {
            info!("Backfill requested for {} ({} days). Will take effect on next service restart.", symbol, days);
        }
    }

    // 3. 返回状态
    get_collection_status(state, symbol).await
}

/// 执行数据归档（导出到 Parquet）
#[tauri::command]
pub async fn archive_symbol_data(
    state: State<'_, AppState>,
    symbol: String,
    days_to_keep: i64,
) -> Result<ArchiveResult, String> {
    let pool = state.repository.get_pool();

    info!("Archiving data for {} (keeping {} days)", symbol, days_to_keep);

    // 1. 查询要归档的数据数量
    let cutoff = chrono::Utc::now() - chrono::Duration::days(days_to_keep);

    let count: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM kline_1m WHERE symbol = $1 AND timestamp < $2"
    )
    .bind(&symbol)
    .bind(cutoff)
    .fetch_one(pool)
    .await
    .map_err(|e| e.to_string())?;

    if count.0 == 0 {
        info!("No data to archive for {}", symbol);
        return Ok(ArchiveResult {
            symbol,
            archived_count: 0,
            file_size_mb: 0.0,
            success: true,
            error: None,
        });
    }

    // 2. 获取要归档的数据（使用 SQL 直接查询）
    let rows = sqlx::query(
        "SELECT timestamp, symbol, open, high, low, close, volume, trade_count \
         FROM kline_1m WHERE symbol = $1 AND timestamp < $2 ORDER BY timestamp ASC"
    )
    .bind(&symbol)
    .bind(cutoff)
    .fetch_all(pool)
    .await
    .map_err(|e| e.to_string())?;

    let klines: Vec<trading_common::data::types::OHLCData> = rows.iter().map(|row| {
        trading_common::data::types::OHLCData {
            timestamp: row.get("timestamp"),
            symbol: row.get("symbol"),
            timeframe: trading_common::data::types::Timeframe::OneMinute,
            open: row.get("open"),
            high: row.get("high"),
            low: row.get("low"),
            close: row.get("close"),
            volume: row.get("volume"),
            trade_count: row.get::<i32, _>("trade_count") as u64,
        }
    }).collect();

    info!("Fetched {} klines for archiving", klines.len());

    // 3. 导出到 Parquet
    let output_dir = std::path::PathBuf::from("data/parquet");
    std::fs::create_dir_all(&output_dir).map_err(|e| e.to_string())?;

    // 使用 Polars 导出
    let config = trading_common::data::polars_repository::PolarsRepositoryConfig {
        parquet_path: output_dir.clone(),
        hot_data_days: days_to_keep,
    };
    let polars_repo = trading_common::data::polars_repository::PolarsRepository::new(config);

    match polars_repo.export_klines(&symbol, &klines) {
        Ok(exported) => {
            info!("Exported {} klines to Parquet for {}", exported, symbol);

            // 获取文件大小（安全处理）
            let file_size_mb = match polars_repo.get_stats(&symbol) {
                Ok(stats) => stats.total_size_bytes as f64 / 1024.0 / 1024.0,
                Err(_) => 0.0,
            };

            Ok(ArchiveResult {
                symbol,
                archived_count: exported as u64,
                file_size_mb,
                success: true,
                error: None,
            })
        }
        Err(e) => {
            error!("Failed to archive {}: {}", symbol, e);
            Ok(ArchiveResult {
                symbol,
                archived_count: 0,
                file_size_mb: 0.0,
                success: false,
                error: Some(e.to_string()),
            })
        }
    }
}

/// 批量归档所有交易对
#[tauri::command]
pub async fn archive_all_symbols(
    state: State<'_, AppState>,
    days_to_keep: i64,
) -> Result<Vec<ArchiveResult>, String> {
    let pool = state.repository.get_pool();

    // 获取所有交易对
    let symbols: Vec<String> = sqlx::query_scalar(
        "SELECT symbol FROM symbol_config ORDER BY symbol"
    )
    .fetch_all(pool)
    .await
    .map_err(|e| e.to_string())?;

    let mut results = Vec::new();
    for symbol in symbols {
        let result = archive_symbol_data(state.clone(), symbol, days_to_keep).await?;
        results.push(result);
    }

    Ok(results)
}
// ============ 策略决策引擎接口 ============

/// 获取策略实例列表
#[tauri::command]
pub async fn get_strategy_instances(
    state: State<'_, AppState>,
) -> Result<Vec<serde_json::Value>, String> {
    info!("Getting strategy instances");

    let pool = state.repository.get_pool();

    let rows = sqlx::query(
        "SELECT id, strategy_type, display_name, params, status, symbols, auto_trade, position_size_pct, exchange, market_type \
         FROM strategy_instances ORDER BY created_at DESC"
    )
    .fetch_all(pool)
    .await
    .map_err(|e| {
        error!("Failed to get strategy instances: {}", e);
        e.to_string()
    })?;

    let instances: Vec<serde_json::Value> = rows.iter().map(|row| {
        let id: sqlx::types::Uuid = row.get("id");
        let position_size_pct: Decimal = row.get("position_size_pct");
        serde_json::json!({
            "id": id.to_string(),
            "strategy_type": row.get::<String, _>("strategy_type"),
            "display_name": row.get::<String, _>("display_name"),
            "params": row.get::<serde_json::Value, _>("params"),
            "status": row.get::<String, _>("status"),
            "symbols": row.get::<Vec<String>, _>("symbols"),
            "auto_trade": row.get::<bool, _>("auto_trade"),
            "position_size_pct": position_size_pct.to_string().parse::<f64>().unwrap_or(10.0),
            "exchange": row.get::<String, _>("exchange"),
            "market_type": row.get::<String, _>("market_type"),
        })
    }).collect();

    info!("Retrieved {} strategy instances", instances.len());
    Ok(instances)
}

/// 获取单个策略的分析结果（简化版，用于前端策略中心）
#[tauri::command]
pub async fn get_strategy_analysis_simple(
    state: State<'_, AppState>,
    strategy_type: String,
    symbol: String,
) -> Result<serde_json::Value, String> {
    info!("Getting strategy analysis: strategy={}, symbol={}", strategy_type, symbol);

    // 1. 拉取 K 线数据
    let klines_1m = state.repository.get_klines(&symbol, 2000).await
        .map_err(|e| { error!("get_klines failed: {}", e); e.to_string() })?;
    if klines_1m.is_empty() {
        return Err("No kline data available.".to_string());
    }

    let current_price = klines_1m.last().unwrap().close;
    let current_price_f64 = current_price.to_f64().unwrap_or(0.0);

    // 2. 聚合多时间框架数据
    let mut aggregator = KlineAggregator::new();
    for kline in &klines_1m { aggregator.update(kline.clone()); }

    // 3. 执行策略分析
    let strategy_id = strategy_type.as_str();

    if trading_common::backtest::strategy::is_multi_timeframe_strategy(strategy_id) {
        // 多时间框架策略
        let mut strategy = trading_common::backtest::strategy::create_multi_timeframe_strategy(strategy_id)?;
        let mut tf_klines = std::collections::HashMap::new();
        for tf in strategy.required_timeframes() {
            tf_klines.insert(tf, aggregator.get_klines(tf, 200));
        }
        let analysis = strategy.analyze(&tf_klines);

        // 转换为 StrategyAnalysis 格式
        let direction = match analysis.overall_direction {
            trading_common::backtest::strategy::TrendDirection::Bullish => "long",
            trading_common::backtest::strategy::TrendDirection::Bearish => "short",
            _ => "neutral",
        };

        let confidence = analysis.overall_confidence.to_string().parse::<f64>().unwrap_or(50.0);

        // 构建关键价位（使用支撑阻力估算）
        let mut support_levels = Vec::new();
        let mut resistance_levels = Vec::new();

        // 从各时间框架分析中提取方向信息来估算关键价位
        for (_, ta) in &analysis.timeframe_analyses {
            // 根据方向估算支撑阻力
            if ta.direction == trading_common::backtest::strategy::TrendDirection::Bullish {
                support_levels.push(current_price_f64 * 0.98);
                resistance_levels.push(current_price_f64 * 1.04);
            } else if ta.direction == trading_common::backtest::strategy::TrendDirection::Bearish {
                support_levels.push(current_price_f64 * 0.96);
                resistance_levels.push(current_price_f64 * 1.02);
            }
        }

        // 去重排序
        support_levels.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        support_levels.dedup();
        resistance_levels.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        resistance_levels.dedup();

        // 计算止损止盈
        let entry_price = current_price_f64;
        let stop_loss = if direction == "long" {
            support_levels.first().copied().unwrap_or(entry_price * 0.98)
        } else {
            resistance_levels.first().copied().unwrap_or(entry_price * 1.02)
        };
        let take_profit = if direction == "long" {
            resistance_levels.first().copied().unwrap_or(entry_price * 1.04)
        } else {
            support_levels.first().copied().unwrap_or(entry_price * 0.96)
        };

        let risk = (entry_price - stop_loss).abs();
        let risk_reward = if risk > 0.0 {
            (take_profit - entry_price).abs() / risk
        } else {
            2.0
        };

        Ok(serde_json::json!({
            "strategy_id": strategy_id,
            "strategy_name": strategy_type,
            "timestamp": chrono::Utc::now().timestamp_millis(),
            "symbol": symbol,
            "market_structure": {
                "structure_type": if direction == "long" { "trending_up" } else if direction == "short" { "trending_down" } else { "ranging" },
                "confidence": confidence,
                "description": format!("{} 置信度 {:.1}%", strategy_type, confidence),
            },
            "key_levels": {
                "support": support_levels,
                "resistance": resistance_levels,
                "pivot": null,
            },
            "bias": {
                "direction": direction,
                "confidence": confidence,
                "reasoning": format!("{} 策略分析: {}", strategy_type, direction),
            },
            "trade_setup": if direction != "neutral" {
                Some(serde_json::json!({
                    "entry_zone": [entry_price * 0.995, entry_price * 1.005],
                    "stop_loss": stop_loss,
                    "take_profit": [take_profit],
                    "risk_reward": risk_reward,
                    "invalidation": "价格突破关键价位",
                }))
            } else {
                None
            },
        }))
    } else {
        Err(format!("Strategy '{}' does not support real-time analysis", strategy_type))
    }
}

/// 获取综合决策结果
#[tauri::command]
pub async fn get_strategy_decision(
    _state: State<'_, AppState>,
    symbol: String,
    analyses: Vec<serde_json::Value>,
) -> Result<serde_json::Value, String> {
    info!("Getting strategy decision for {} with {} analyses", symbol, analyses.len());

    // 解析分析结果
    let parsed_analyses: Vec<trading_common::strategy::analysis::StrategyAnalysis> = analyses
        .iter()
        .filter_map(|a| serde_json::from_value(a.clone()).ok())
        .collect();

    if parsed_analyses.is_empty() {
        return Ok(serde_json::json!({
            "should_trade": false,
            "direction": "neutral",
            "confidence": 0,
            "consensus_strategies": [],
            "trade_setup": null,
            "market_structure": {
                "structure_type": "ranging",
                "confidence": 0,
                "description": "没有有效的策略分析",
            },
            "reasoning": "没有有效的策略分析结果",
        }));
    }

    // 使用决策引擎
    use trading_common::strategy::analysis::TradeDirection;

    // 简单的决策逻辑
    let min_confidence = 70.0;
    let min_consensus = 2;

    // 过滤低置信度
    let valid: Vec<&trading_common::strategy::analysis::StrategyAnalysis> = parsed_analyses
        .iter()
        .filter(|a| a.bias.confidence >= min_confidence)
        .collect();

    // 统计方向
    let long_count = valid.iter().filter(|a| a.bias.direction == TradeDirection::Long).count();
    let short_count = valid.iter().filter(|a| a.bias.direction == TradeDirection::Short).count();

    // 判断共识
    let (direction, consensus_count) = if long_count >= min_consensus && long_count > short_count {
        ("long", long_count)
    } else if short_count >= min_consensus && short_count > long_count {
        ("short", short_count)
    } else {
        ("neutral", 0)
    };

    if direction == "neutral" {
        return Ok(serde_json::json!({
            "should_trade": false,
            "direction": "neutral",
            "confidence": 0,
            "consensus_strategies": [],
            "trade_setup": null,
            "market_structure": {
                "structure_type": "ranging",
                "confidence": 50,
                "description": "策略共识不足",
            },
            "reasoning": format!("做多: {}, 做空: {}, 需要至少{}个策略共识", long_count, short_count, min_consensus),
        }));
    }

    // 收集共识策略
    let consensus_strategies: Vec<String> = valid
        .iter()
        .filter(|a| (a.bias.direction == TradeDirection::Long && direction == "long") ||
                     (a.bias.direction == TradeDirection::Short && direction == "short"))
        .map(|a| a.strategy_name.clone())
        .collect();

    // 计算平均置信度
    let avg_confidence: f64 = valid
        .iter()
        .filter(|a| (a.bias.direction == TradeDirection::Long && direction == "long") ||
                     (a.bias.direction == TradeDirection::Short && direction == "short"))
        .map(|a| a.bias.confidence)
        .sum::<f64>() / consensus_count as f64;

    // 综合交易计划
    let trade_setups: Vec<&trading_common::strategy::analysis::TradeSetup> = valid
        .iter()
        .filter_map(|a| a.trade_setup.as_ref())
        .collect();

    let trade_setup = if !trade_setups.is_empty() {
        let entry_low = trade_setups.iter().map(|s| s.entry_zone.0).sum::<f64>() / trade_setups.len() as f64;
        let entry_high = trade_setups.iter().map(|s| s.entry_zone.1).sum::<f64>() / trade_setups.len() as f64;
        let stop_loss = trade_setups.iter().map(|s| s.stop_loss).sum::<f64>() / trade_setups.len() as f64;
        let take_profit: Vec<f64> = trade_setups.iter().flat_map(|s| s.take_profit.clone()).collect();
        let avg_tp = if take_profit.is_empty() { 0.0 } else { take_profit.iter().sum::<f64>() / take_profit.len() as f64 };

        Some(serde_json::json!({
            "entry_zone": [entry_low, entry_high],
            "stop_loss": stop_loss,
            "take_profit": [avg_tp],
            "risk_reward": if stop_loss > 0.0 { (avg_tp - (entry_low + entry_high) / 2.0).abs() / ((entry_low + entry_high) / 2.0 - stop_loss).abs() } else { 2.0 },
        }))
    } else {
        None
    };

    // 取置信度最高的市场结构
    let market_structure = valid
        .iter()
        .max_by(|a, b| a.market_structure.confidence.partial_cmp(&b.market_structure.confidence).unwrap_or(std::cmp::Ordering::Equal))
        .map(|a| serde_json::json!({
            "structure_type": a.market_structure.structure_type.as_str(),
            "confidence": a.market_structure.confidence,
            "description": a.market_structure.description,
        }))
        .unwrap_or_else(|| serde_json::json!({
            "structure_type": "ranging",
            "confidence": 50,
            "description": "无法确定",
        }));

    Ok(serde_json::json!({
        "should_trade": true,
        "direction": direction,
        "confidence": avg_confidence,
        "consensus_strategies": consensus_strategies,
        "trade_setup": trade_setup,
        "market_structure": market_structure,
        "reasoning": format!("{}个策略共识做{}: {}", consensus_count,
            if direction == "long" { "多" } else { "空" },
            consensus_strategies.join(" + ")),
    }))
}

// ============ 策略配置管理命令 ============

/// 创建策略实例
#[tauri::command]
pub async fn create_strategy_instance(
    state: State<'_, AppState>,
    request: serde_json::Value,
) -> Result<serde_json::Value, String> {
    info!("Creating strategy instance: {:?}", request);

    let pool = state.repository.get_pool();

    let strategy_type = request.get("strategy_type")
        .and_then(|v| v.as_str())
        .ok_or("Missing strategy_type")?;
    let display_name = request.get("display_name")
        .and_then(|v| v.as_str())
        .unwrap_or(strategy_type);
    let params = request.get("params")
        .cloned()
        .unwrap_or(serde_json::json!({}));
    let symbols: Vec<String> = request.get("symbols")
        .and_then(|v| v.as_array())
        .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
        .unwrap_or_else(|| vec!["BTCUSDT".to_string()]);
    let auto_trade = request.get("auto_trade")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let position_size_pct = request.get("position_size_pct")
        .and_then(|v| v.as_f64())
        .unwrap_or(10.0);
    let exchange = request.get("exchange")
        .and_then(|v| v.as_str())
        .unwrap_or("binance");
    let market_type = request.get("market_type")
        .and_then(|v| v.as_str())
        .unwrap_or("futures");
    let note = request.get("note")
        .and_then(|v| v.as_str())
        .unwrap_or("");

    let id: sqlx::types::Uuid = sqlx::query_scalar(
        "INSERT INTO strategy_instances (strategy_type, display_name, params, symbols, auto_trade, position_size_pct, exchange, market_type, note) \
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9) RETURNING id"
    )
    .bind(strategy_type)
    .bind(display_name)
    .bind(params)
    .bind(&symbols)
    .bind(auto_trade)
    .bind(position_size_pct)
    .bind(exchange)
    .bind(market_type)
    .bind(note)
    .fetch_one(pool)
    .await
    .map_err(|e| {
        error!("Failed to create strategy instance: {}", e);
        e.to_string()
    })?;

    info!("Created strategy instance: {}", id);

    Ok(serde_json::json!({
        "id": id.to_string(),
        "success": true
    }))
}

/// 更新策略实例
#[tauri::command]
pub async fn update_strategy_instance(
    state: State<'_, AppState>,
    id: String,
    update: serde_json::Value,
) -> Result<serde_json::Value, String> {
    info!("Updating strategy instance: {} {:?}", id, update);

    let pool = state.repository.get_pool();
    let uuid = sqlx::types::Uuid::parse_str(&id).map_err(|e| e.to_string())?;

    // 构建动态 UPDATE 语句
    let mut sets = Vec::new();
    let mut param_index = 2; // $1 是 id

    if update.get("display_name").is_some() {
        sets.push(format!("display_name = ${}", param_index));
        param_index += 1;
    }
    if update.get("params").is_some() {
        sets.push(format!("params = ${}", param_index));
        param_index += 1;
    }
    if update.get("symbols").is_some() {
        sets.push(format!("symbols = ${}", param_index));
        param_index += 1;
    }
    if update.get("auto_trade").is_some() {
        sets.push(format!("auto_trade = ${}", param_index));
        param_index += 1;
    }
    if update.get("position_size_pct").is_some() {
        sets.push(format!("position_size_pct = ${}", param_index));
        param_index += 1;
    }
    if update.get("exchange").is_some() {
        sets.push(format!("exchange = ${}", param_index));
        param_index += 1;
    }
    if update.get("market_type").is_some() {
        sets.push(format!("market_type = ${}", param_index));
        param_index += 1;
    }
    if update.get("note").is_some() {
        sets.push(format!("note = ${}", param_index));
        param_index += 1;
    }

    sets.push("updated_at = NOW()".to_string());

    let sql = format!(
        "UPDATE strategy_instances SET {} WHERE id = $1",
        sets.join(", ")
    );

    let mut query = sqlx::query(&sql).bind(uuid);

    if let Some(v) = update.get("display_name") {
        query = query.bind(v.as_str().unwrap_or(""));
    }
    if let Some(v) = update.get("params") {
        query = query.bind(v.clone());
    }
    if let Some(v) = update.get("symbols") {
        let symbols: Vec<String> = v.as_array()
            .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
            .unwrap_or_default();
        query = query.bind(symbols);
    }
    if let Some(v) = update.get("auto_trade") {
        query = query.bind(v.as_bool().unwrap_or(false));
    }
    if let Some(v) = update.get("position_size_pct") {
        query = query.bind(v.as_f64().unwrap_or(10.0));
    }
    if let Some(v) = update.get("exchange") {
        query = query.bind(v.as_str().unwrap_or("binance"));
    }
    if let Some(v) = update.get("market_type") {
        query = query.bind(v.as_str().unwrap_or("futures"));
    }
    if let Some(v) = update.get("note") {
        query = query.bind(v.as_str().unwrap_or(""));
    }

    query.execute(pool).await.map_err(|e| {
        error!("Failed to update strategy instance: {}", e);
        e.to_string()
    })?;

    info!("Updated strategy instance: {}", id);

    Ok(serde_json::json!({
        "success": true
    }))
}

/// 更新策略状态
#[tauri::command]
pub async fn update_strategy_status(
    state: State<'_, AppState>,
    id: String,
    status: String,
) -> Result<serde_json::Value, String> {
    info!("Updating strategy status: {} -> {}", id, status);

    let pool = state.repository.get_pool();
    let uuid = sqlx::types::Uuid::parse_str(&id).map_err(|e| e.to_string())?;

    sqlx::query(
        "UPDATE strategy_instances SET status = $2, updated_at = NOW() WHERE id = $1"
    )
    .bind(uuid)
    .bind(&status)
    .execute(pool)
    .await
    .map_err(|e| {
        error!("Failed to update strategy status: {}", e);
        e.to_string()
    })?;

    info!("Updated strategy status: {} -> {}", id, status);

    Ok(serde_json::json!({
        "success": true
    }))
}

/// 删除策略实例
#[tauri::command]
pub async fn delete_strategy_instance(
    state: State<'_, AppState>,
    id: String,
) -> Result<serde_json::Value, String> {
    info!("Deleting strategy instance: {}", id);

    let pool = state.repository.get_pool();
    let uuid = sqlx::types::Uuid::parse_str(&id).map_err(|e| e.to_string())?;

    sqlx::query(
        "DELETE FROM strategy_instances WHERE id = $1"
    )
    .bind(uuid)
    .execute(pool)
    .await
    .map_err(|e| {
        error!("Failed to delete strategy instance: {}", e);
        e.to_string()
    })?;

    info!("Deleted strategy instance: {}", id);

    Ok(serde_json::json!({
        "success": true
    }))
}

/// 获取可用的交易对列表（按市场类型和交易所过滤）
#[tauri::command]
pub async fn get_available_symbols(
    state: State<'_, AppState>,
    market_type: Option<String>,
    exchange: Option<String>,
) -> Result<Vec<String>, String> {
    info!("Getting available symbols, market_type: {:?}, exchange: {:?}", market_type, exchange);

    let pool = state.repository.get_pool();

    let mut sql = String::from("SELECT DISTINCT unified_symbol FROM symbol_mapping WHERE 1=1");
    let mut params: Vec<String> = Vec::new();
    let mut param_index = 1;

    if let Some(ref mt) = market_type {
        sql.push_str(&format!(" AND market_type = ${}", param_index));
        params.push(mt.clone());
        param_index += 1;
    }

    if let Some(ref ex) = exchange {
        sql.push_str(&format!(" AND exchange = ${}", param_index));
        params.push(ex.clone());
    }

    sql.push_str(" ORDER BY unified_symbol");

    let mut query = sqlx::query_scalar(&sql);
    for param in &params {
        query = query.bind(param);
    }

    let symbols: Vec<String> = query
        .fetch_all(pool)
        .await
        .map_err(|e| {
            error!("Failed to get symbols: {}", e);
            e.to_string()
        })?;

    info!("Found {} symbols", symbols.len());
    Ok(symbols)
}
