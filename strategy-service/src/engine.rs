use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use chrono::Utc;
use rust_decimal::Decimal;
use rust_decimal::prelude::ToPrimitive;
use sqlx::PgPool;
use tokio::sync::RwLock;
use tracing::{debug, error, info, warn};

use crate::db::{signals, strategies as db_strategies, positions};
use crate::exchange;
use crate::kline_store::{KlineBar, KlineManager};
use crate::redis_reader::{MarketData, MultiTimeframeData, Timeframe};
use crate::strategies::{self, SignalType};
use crate::websocket::{WsMessage, WsState};
use crate::alert::{AlertManager, create_signal_alert, create_trade_alert};

/// 持仓状态跟踪（用于止损止盈判断）
#[derive(Debug, Clone)]
struct PositionState {
    /// 持仓方向: "LONG" / "SHORT"
    side: String,
    /// 入场价格
    entry_price: f64,
    /// 持仓期间最高盈利百分比
    max_profit_pct: f64,
}

/// 止盈止损退出原因
#[derive(Debug, Clone)]
enum ExitReason {
    /// MA288 止损: close 穿越 MA288
    Ma288Stop,
    /// 硬止损: 价格触碰固定止损价
    HardStop,
    /// 移动止盈: 盈利达激活阈值后回撤
    TrailingTp,
    /// 趋势反转: MA288 < MA488 (平多) 或 MA288 > MA488 (平空)
    TrendReversal,
}

impl std::fmt::Display for ExitReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ExitReason::Ma288Stop => write!(f, "MA288止损"),
            ExitReason::HardStop => write!(f, "硬止损"),
            ExitReason::TrailingTp => write!(f, "移动止盈"),
            ExitReason::TrendReversal => write!(f, "趋势反转"),
        }
    }
}

/// 策略执行引擎配置
pub struct EngineConfig {
    pub poll_interval_secs: u64,
    pub default_timeframe: Timeframe,
    pub default_kline_limit: usize,
    /// 是否启用多时间框架分析
    pub enable_multi_tf: bool,
}

impl Default for EngineConfig {
    fn default() -> Self {
        Self {
            poll_interval_secs: 5,
            default_timeframe: Timeframe::ThirtyMinutes,
            default_kline_limit: 500,
            enable_multi_tf: true,
        }
    }
}

/// 将 KlineBar 列表转换为 redis_reader::KlineData 列表（供策略层使用）
fn bars_to_kline_data(bars: &[&KlineBar]) -> Vec<crate::redis_reader::KlineData> {
    bars.iter().map(|b| b.to_kline_data()).collect()
}

/// 从 KlineManager 构建 MarketData
fn build_market_data(
    manager: &KlineManager,
    symbol: &str,
    tf: Timeframe,
    limit: usize,
) -> Option<MarketData> {
    let store = manager.get(symbol, tf)?;
    let bars = store.closed_bars(limit);

    if bars.is_empty() {
        return None;
    }

    let klines = bars_to_kline_data(&bars);
    let current_price = store.current_price();

    Some(MarketData {
        klines,
        current_price,
        symbol: symbol.to_string(),
        timeframe: tf,
    })
}

/// 从 KlineManager 构建 MultiTimeframeData
fn build_multi_timeframe_data(
    manager: &KlineManager,
    symbol: &str,
    timeframes: &[Timeframe],
    limit: usize,
) -> Option<MultiTimeframeData> {
    let mut all_data: Vec<MarketData> = Vec::new();

    for &tf in timeframes {
        if let Some(data) = build_market_data(manager, symbol, tf, limit) {
            all_data.push(data);
        }
    }

    if all_data.is_empty() {
        return None;
    }

    // 按时间框架级别排序
    all_data.sort_by_key(|d| d.timeframe.level());

    let primary = all_data.first().cloned()?;
    let secondary = if all_data.len() > 1 {
        Some(all_data[1].clone())
    } else {
        None
    };
    let higher = if all_data.len() > 2 {
        Some(all_data[2].clone())
    } else {
        None
    };

    Some(MultiTimeframeData {
        primary,
        secondary,
        higher,
        all: all_data,
    })
}

pub async fn run(
    pool: PgPool,
    kline_manager: Arc<RwLock<KlineManager>>,
    interval_secs: u64,
    ws_state: Option<Arc<WsState>>,
    alert_manager: Option<Arc<AlertManager>>,
) -> Result<()> {
    let config = EngineConfig {
        poll_interval_secs: interval_secs,
        ..Default::default()
    };

    run_with_config(pool, kline_manager, config, ws_state, alert_manager).await
}

pub async fn run_with_config(
    pool: PgPool,
    kline_manager: Arc<RwLock<KlineManager>>,
    config: EngineConfig,
    ws_state: Option<Arc<WsState>>,
    alert_manager: Option<Arc<AlertManager>>,
) -> Result<()> {
    info!(
        "Strategy engine started, polling every {} seconds, timeframe: {}, kline_limit: {}",
        config.poll_interval_secs,
        config.default_timeframe.as_str(),
        config.default_kline_limit
    );

    let mut interval = tokio::time::interval(Duration::from_secs(config.poll_interval_secs));

    // 持仓状态跟踪: key = "instance_id:symbol"
    let mut position_states: HashMap<String, PositionState> = HashMap::new();

    loop {
        interval.tick().await;

        // 获取所有活跃策略实例
        let active_strategies = match db_strategies::list_active_strategies(&pool).await {
            Ok(strategies) => strategies,
            Err(e) => {
                error!("Failed to load active strategies: {}", e);
                continue;
            }
        };

        if active_strategies.is_empty() {
            continue;
        }

        info!("Processing {} active strategies", active_strategies.len());

        for strategy_instance in active_strategies {
            // 遍历策略实例的每个交易对
            for symbol in &strategy_instance.symbols {
                if let Err(e) = process_strategy(
                    &pool,
                    &kline_manager,
                    &strategy_instance,
                    symbol,
                    &config,
                    &ws_state,
                    &alert_manager,
                    &mut position_states,
                )
                .await
                {
                    error!(
                        "Error processing strategy {} for {}: {}",
                        strategy_instance.id, symbol, e
                    );
                }
            }
        }
    }
}

async fn process_strategy(
    pool: &PgPool,
    kline_manager: &Arc<RwLock<KlineManager>>,
    strategy_instance: &db_strategies::StrategyInstance,
    symbol: &str,
    config: &EngineConfig,
    ws_state: &Option<Arc<WsState>>,
    alert_manager: &Option<Arc<AlertManager>>,
    position_states: &mut HashMap<String, PositionState>,
) -> Result<()> {
    let state_key = format!("{}:{}", strategy_instance.id, symbol);

    debug!(
        "[{}][{}] 开始分析 (策略ID={}, 市场={})",
        symbol, strategy_instance.strategy_type, strategy_instance.id, strategy_instance.market_type
    );

    // 创建策略实例
    let strategy = strategies::create_strategy(&strategy_instance.strategy_type, &strategy_instance.params)?;

    // 判断是否使用多时间框架分析
    let use_multi_tf = config.enable_multi_tf
        && strategy_instance.strategy_type != "rsi"
        && strategy_instance.strategy_type != "macd"
        && strategy_instance.strategy_type != "bollinger"
        && strategy_instance.strategy_type != "volume";

    // 从 KlineManager 读取数据
    let manager = kline_manager.read().await;

    let (signal, kline_price_f64) = if use_multi_tf {
        let timeframes = strategy.required_timeframes();

        if timeframes.len() > 1 {
            // 多时间框架分析
            let multi_data = build_multi_timeframe_data(
                &manager,
                symbol,
                &timeframes,
                config.default_kline_limit,
            );

            match multi_data {
                Some(data) => {
                    // 日志: 数据概况
                    let tf_summary: Vec<String> = data.all.iter().map(|d| {
                        let bar_count = d.klines.len();
                        format!("{}: {}根", d.timeframe.as_str(), bar_count)
                    }).collect();
                    debug!(
                        "[{}] 数据就绪: {}, 当前价={:.4}",
                        symbol, tf_summary.join(", "), data.primary.current_price
                    );

                    // 数据完整性验证（策略层）
                    if let Err(reason) = validate_strategy_data(&data.primary, symbol) {
                        warn!("[{}] 跳过策略执行: {}", symbol, reason);
                        return Ok(());
                    }

                    let price = data.primary.current_price;
                    let signal = strategy.analyze_multi_tf(&data).await;
                    (signal, price)
                }
                None => {
                    warn!("[{}] 数据不可用，跳过策略执行", symbol);
                    return Ok(());
                }
            }
        } else {
            // 单时间框架
            let market_data = build_market_data(
                &manager,
                symbol,
                config.default_timeframe,
                config.default_kline_limit,
            );

            match market_data {
                Some(data) => {
                    debug!(
                        "[{}] 数据就绪: {} {}根, 当前价={:.4}",
                        symbol, data.timeframe.as_str(), data.klines.len(), data.current_price
                    );
                    if let Err(reason) = validate_strategy_data(&data, symbol) {
                        warn!("[{}] 跳过策略执行: {}", symbol, reason);
                        return Ok(());
                    }

                    let price = data.current_price;
                    let signal = strategy.analyze(&data).await;
                    (signal, price)
                }
                None => {
                    warn!("[{}] 数据不可用，跳过策略执行", symbol);
                    return Ok(());
                }
            }
        }
    } else {
        // 传统单时间框架分析
        let market_data = build_market_data(
            &manager,
            symbol,
            config.default_timeframe,
            config.default_kline_limit,
        );

        match market_data {
            Some(data) => {
                debug!(
                    "[{}] 数据就绪: {} {}根, 当前价={:.4}",
                    symbol, data.timeframe.as_str(), data.klines.len(), data.current_price
                );
                if let Err(reason) = validate_strategy_data(&data, symbol) {
                    warn!("[{}] 跳过策略执行: {}", symbol, reason);
                    return Ok(());
                }

                let price = data.current_price;
                let signal = strategy.analyze(&data).await;
                (signal, price)
            }
            None => {
                warn!("[{}] 数据不可用，跳过策略执行", symbol);
                return Ok(());
            }
        }
    };

    // 保存 30m klines 用于退出条件检查（释放锁前）
    let klines_30m: Vec<KlineBar> = manager.get(symbol, Timeframe::ThirtyMinutes)
        .map(|store| store.closed_bars(500).into_iter().cloned().collect())
        .unwrap_or_default();

    // 释放 KlineManager 读锁
    drop(manager);

    // 获取实时 ticker 价格（公开 API，无需 API Key）
    let current_price_f64 = match exchange::get_ticker_price(symbol, &strategy_instance.market_type).await {
        Ok(price) => {
            tracing::debug!("[{}] 实时价格: {} (K线收盘价: {}, 市场: {})", symbol, price, kline_price_f64, strategy_instance.market_type);
            price
        }
        Err(e) => {
            warn!("[{}] 获取实时价格失败，使用 K 线收盘价: {}", symbol, e);
            kline_price_f64
        }
    };

    // ============================================================
    // 持仓退出检查（止损/止盈/趋势反转）
    // ============================================================
    if let Ok(Some(position)) = positions::get_active_position(
        pool, &strategy_instance.exchange, &strategy_instance.market_type, symbol,
    ).await {
        // 有活跃持仓，检查退出条件
        let exit_reason = check_exit_conditions(
            &position.side,
            position.avg_entry_price.to_f64().unwrap_or(0.0),
            current_price_f64,
            &klines_30m,
            &strategy_instance.params,
            position_states.get_mut(&state_key),
        );

        if let Some(reason) = exit_reason {
            info!(
                "[{}][{}] 🔴 退出信号: {} 价格={:.4} 原因={} (持仓: {} @ {:.4})",
                symbol, strategy_instance.strategy_type,
                if position.side == "LONG" { "平多" } else { "平空" },
                current_price_f64, reason,
                position.side, position.avg_entry_price
            );

            // 产生退出信号
            let exit_direction = if position.side == "LONG" { "bearish" } else { "bullish" };
            let exit_signal_type = if position.side == "LONG" { "SELL" } else { "BUY" };

            let exit_request = signals::CreateSignalRequest {
                strategy_id: strategy_instance.strategy_type.clone(),
                symbol: symbol.to_string(),
                direction: exit_direction.to_string(),
                entry_price: rust_decimal::Decimal::try_from(current_price_f64)?,
                overall_confidence: rust_decimal::Decimal::try_from(1.0)?,
                entry_allowed: true,
                entry_direction: None,
                timeframe_details: None,
                instance_id: Some(strategy_instance.id),
                signal_strength: None,
                market_context: Some(serde_json::json!({
                    "exit_reason": format!("{}", reason),
                    "entry_price": position.avg_entry_price,
                    "position_side": position.side,
                })),
                stop_loss: None,
                take_profit: None,
                market_structure: None,
                key_levels: None,
                trade_setup: None,
                market_type: Some(strategy_instance.market_type.clone()),
                signal_type: Some(exit_signal_type.to_string()),
                signal_intent: Some("exit".to_string()),
            };

            let saved = signals::create_signal(pool, exit_request).await?;
            info!("[{}] 退出信号已保存: id={}", symbol, saved.id);

            // 广播退出信号
            if let Some(ws) = ws_state {
                let ws_msg = WsMessage {
                    msg_type: "signal".to_string(),
                    data: serde_json::json!({
                        "id": saved.id,
                        "symbol": symbol,
                        "direction": exit_direction,
                        "signal_intent": "exit",
                        "exit_reason": format!("{}", reason),
                        "price": current_price_f64,
                    }),
                };
                ws.broadcast_signal(ws_msg);
            }

            // 清除持仓状态
            position_states.remove(&state_key);

            return Ok(());
        }
    }

    // ============================================================
    // 入场信号处理
    // ============================================================
    let signal = match signal {
        Some(signal) => {
            info!(
                "[{}][{}] ✅ 信号产生: {} 价格={:.4} 强度={:.2} 原因={}",
                symbol, strategy_instance.strategy_type,
                match signal.signal_type {
                    SignalType::Buy => "做多",
                    SignalType::Sell => "做空",
                    SignalType::Hold => "持有",
                },
                current_price_f64,
                signal.signal_strength,
                signal.reason
            );
            signal
        },
        None => {
            debug!("[{}][{}] 无信号(详见策略内部日志)", symbol, strategy_instance.strategy_type);
            return Ok(());
        },
    };

    // 检查是否应该忽略信号（避免频繁交易）
    if should_skip_signal(pool, strategy_instance.id, symbol, &signal.signal_type).await? {
        info!(
            "Skipping signal for {} ({}): too soon since last signal",
            symbol, strategy_instance.strategy_type
        );
        return Ok(());
    }

    // 信号追踪：处理方向反转
    let current_direction = match signal.signal_type {
        SignalType::Buy => "bullish",
        SignalType::Sell => "bearish",
        SignalType::Hold => "neutral",
    };

    let active_signals = signals::get_active_signals(pool, strategy_instance.id, symbol).await?;
    let current_price = rust_decimal::Decimal::try_from(current_price_f64)?;

    for old_signal in active_signals {
        let old_direction = &old_signal.direction;

        let is_reversal = (current_direction == "bullish" && old_direction == "bearish")
            || (current_direction == "bearish" && old_direction == "bullish");

        if is_reversal {
            let return_pct = calc_return_pct(old_direction, old_signal.entry_price, current_price);
            signals::supersede_signal(pool, old_signal.id, current_price, return_pct).await?;

            info!(
                "🔄 Signal superseded: {} -> {} for {} (old_signal_id={}, return={}%)",
                old_direction, current_direction, symbol, old_signal.id, return_pct
            );
        }
    }

    // 写入信号到数据库
    let direction = match signal.signal_type {
        SignalType::Buy => "bullish",
        SignalType::Sell => "bearish",
        SignalType::Hold => "neutral",
    };

    let entry_direction = match signal.signal_type {
        SignalType::Buy => Some("long"),
        SignalType::Sell => Some("short"),
        SignalType::Hold => None,
    };

    // 确定 signal_type (BUY/SELL/HOLD)
    let signal_type_str = match signal.signal_type {
        SignalType::Buy => "BUY",
        SignalType::Sell => "SELL",
        SignalType::Hold => "HOLD",
    };

    let signal_request = signals::CreateSignalRequest {
        strategy_id: strategy_instance.strategy_type.clone(),
        symbol: symbol.to_string(),
        direction: direction.to_string(),
        entry_price: rust_decimal::Decimal::try_from(current_price_f64)?,
        overall_confidence: rust_decimal::Decimal::try_from(signal.confidence)?,
        entry_allowed: signal.signal_type != SignalType::Hold,
        entry_direction: entry_direction.map(|s| s.to_string()),
        timeframe_details: None,
        instance_id: Some(strategy_instance.id),
        signal_strength: Some(rust_decimal::Decimal::try_from(signal.signal_strength)?),
        market_context: Some(signal.market_context),
        stop_loss: signal.stop_loss.map(|v| rust_decimal::Decimal::try_from(v)).transpose()?,
        take_profit: signal.take_profit.map(|v| rust_decimal::Decimal::try_from(v)).transpose()?,
        market_structure: signal.market_structure.map(|ms| serde_json::to_value(ms).unwrap_or_default()),
        key_levels: signal.key_levels.map(|kl| serde_json::to_value(kl).unwrap_or_default()),
        trade_setup: signal.trade_setup.map(|ts| serde_json::to_value(ts).unwrap_or_default()),
        market_type: Some(strategy_instance.market_type.clone()),
        signal_type: Some(signal_type_str.to_string()),
        signal_intent: Some("entry".to_string()),
    };

    let saved_signal = signals::create_signal(pool, signal_request).await?;

    info!(
        "Signal generated: {} {} at {} (strength={:.2}, reason={})",
        direction,
        symbol,
        current_price_f64,
        signal.signal_strength,
        signal.reason
    );

    // 更新持仓状态跟踪（入场信号产生时）
    match signal.signal_type {
        SignalType::Buy => {
            position_states.insert(state_key.clone(), PositionState {
                side: "LONG".to_string(),
                entry_price: current_price_f64,
                max_profit_pct: 0.0,
            });
        }
        SignalType::Sell => {
            position_states.insert(state_key.clone(), PositionState {
                side: "SHORT".to_string(),
                entry_price: current_price_f64,
                max_profit_pct: 0.0,
            });
        }
        _ => {}
    }

    // 广播信号到 WebSocket
    if let Some(ws) = ws_state {
        let ws_msg = WsMessage {
            msg_type: "signal".to_string(),
            data: serde_json::json!({
                "id": saved_signal.id,
                "symbol": symbol,
                "strategy": strategy_instance.strategy_type,
                "direction": direction,
                "entry_price": current_price_f64,
                "signal_strength": signal.signal_strength,
                "confidence": signal.confidence,
                "reason": signal.reason,
                "stop_loss": signal.stop_loss,
                "take_profit": signal.take_profit,
                "auto_trade": strategy_instance.auto_trade,
                "created_at": saved_signal.created_at,
            }),
        };
        ws.broadcast_signal(ws_msg);
    }

    // 发送告警
    if let Some(alert_mgr) = alert_manager {
        let alert = create_signal_alert(
            symbol,
            &strategy_instance.strategy_type,
            direction,
            signal.entry_price,
            signal.signal_strength,
            &signal.reason,
        );
        if let Err(e) = alert_mgr.send(&alert).await {
            warn!("Failed to send alert: {}", e);
        }
    }

    // 自动交易标记
    if strategy_instance.auto_trade {
        info!(
            "✅ Auto-trade signal queued: {} {} @ {} (signal={}, stop_loss={:?}, take_profit={:?})",
            direction, symbol, signal.entry_price,
            saved_signal.id, signal.stop_loss, signal.take_profit
        );

        if let Some(alert_mgr) = alert_manager {
            let trade_alert = create_trade_alert(
                symbol,
                direction,
                signal.entry_price,
                &saved_signal.id.to_string(),
            );
            let _ = alert_mgr.send(&trade_alert).await;
        }
    }

    Ok(())
}

/// 验证策略数据完整性（策略层）
fn validate_strategy_data(
    market_data: &MarketData,
    symbol: &str,
) -> Result<(), String> {
    let tf = &market_data.timeframe;

    if market_data.klines.is_empty() {
        return Err(format!("[{}:{}] K线数据为空", symbol, tf.as_str()));
    }

    let min_bars = tf.min_warmup_bars();
    if market_data.klines.len() < min_bars {
        return Err(format!(
            "[{}:{}] 数据不足: 需要 {} 条，实际 {} 条",
            symbol,
            tf.as_str(),
            min_bars,
            market_data.klines.len()
        ));
    }

    if let Some(latest) = market_data.klines.last() {
        let now_ms = chrono::Utc::now().timestamp_millis();
        let age_ms = now_ms - latest.timestamp;
        let max_age_ms = tf.as_duration().num_milliseconds() * 2;

        if age_ms > max_age_ms {
            let age_minutes = age_ms / 60000;
            let latest_dt = chrono::DateTime::from_timestamp_millis(latest.timestamp)
                .map(|dt| dt.format("%H:%M:%S").to_string())
                .unwrap_or_else(|| "unknown".to_string());
            return Err(format!(
                "[{}:{}] 数据过旧: 最新时间戳延迟 {} 分钟 (latest={}, bars={})",
                symbol,
                tf.as_str(),
                age_minutes,
                latest_dt,
                market_data.klines.len()
            ));
        }
    }

    Ok(())
}

/// 检查是否应该跳过信号（避免频繁交易）
async fn should_skip_signal(
    pool: &PgPool,
    instance_id: uuid::Uuid,
    symbol: &str,
    signal_type: &SignalType,
) -> Result<bool> {
    let last_signal = signals::get_last_signal(pool, instance_id, symbol).await?;

    if let Some(last) = last_signal {
        let now = Utc::now();
        let age = now - last.created_at;

        let base_cooldown = chrono::Duration::minutes(5);
        if age < base_cooldown {
            info!(
                "[{}] 信号冷却中: 距上次信号 {} 分钟 (最小间隔 5 分钟)",
                symbol,
                age.num_minutes()
            );
            return Ok(true);
        }

        let same_direction = match (signal_type, last.direction.as_str()) {
            (SignalType::Buy, "bullish") => true,
            (SignalType::Sell, "bearish") => true,
            _ => false,
        };

        if same_direction {
            let extended_cooldown = chrono::Duration::minutes(15);
            if age < extended_cooldown {
                info!(
                    "[{}] 同方向信号冷却中: 距上次同方向信号 {} 分钟 (最小间隔 15 分钟)",
                    symbol,
                    age.num_minutes()
                );
                return Ok(true);
            }
        }

        if !same_direction {
            info!(
                "[{}] 检测到反向信号: {} -> {}, 允许生成",
                symbol,
                last.direction,
                match signal_type {
                    SignalType::Buy => "bullish",
                    SignalType::Sell => "bearish",
                    SignalType::Hold => "neutral",
                }
            );
        }
    }

    Ok(false)
}

/// 计算信号收益率%
fn calc_return_pct(direction: &str, entry_price: Decimal, current_price: Decimal) -> Decimal {
    if entry_price == Decimal::ZERO {
        return Decimal::ZERO;
    }
    let pct = (current_price - entry_price) / entry_price * Decimal::from(100);
    match direction {
        "bullish" => pct,
        "bearish" => -pct,
        _ => Decimal::ZERO,
    }
}

// ============================================================
// 退出条件检查（止损/止盈/趋势反转）
// ============================================================

/// 计算 SMA（简单移动平均）
fn calc_sma(klines: &[KlineBar], period: usize) -> Option<f64> {
    if klines.len() < period || period == 0 {
        return None;
    }
    let start = klines.len() - period;
    let sum: f64 = klines[start..].iter().map(|k| k.close).sum();
    Some(sum / period as f64)
}

/// 检查 MA288 止损条件（动态交叉检查，与 JS 回测一致）
///
/// 做多持仓: 前一根 close > 前一根 MA288 且 当前 close < 当前 MA288
/// 做空持仓: 前一根 close < 前一根 MA288 且 当前 close > 当前 MA288
fn check_ma288_stop(
    side: &str,
    klines: &[KlineBar],
    ma_period: usize,
) -> bool {
    if klines.len() < ma_period + 1 {
        return false;
    }

    // 当前 MA288 (最后 ma_period 根 K 线)
    let cur_ma = match calc_sma(klines, ma_period) {
        Some(v) => v,
        None => return false,
    };

    // 前一根 MA288 (去掉最后一根 K 线)
    let prev_klines = &klines[..klines.len() - 1];
    let prev_ma = match calc_sma(prev_klines, ma_period) {
        Some(v) => v,
        None => return false,
    };

    let cur_close = klines.last().unwrap().close;
    let prev_close = klines[klines.len() - 2].close;

    match side {
        "LONG" => prev_close > prev_ma && cur_close < cur_ma,
        "SHORT" => prev_close < prev_ma && cur_close > cur_ma,
        _ => false,
    }
}

/// 检查移动止盈条件
///
/// 盈利 >= trailing_activate_pct 后，从最高盈利回撤 >= trailing_callback_pct 时触发
fn check_trailing_tp(
    side: &str,
    entry_price: f64,
    current_price: f64,
    max_profit_pct: &mut f64,
    trailing_activate_pct: f64,
    trailing_callback_pct: f64,
) -> bool {
    if entry_price <= 0.0 {
        return false;
    }

    let pnl_pct = match side {
        "LONG" => (current_price - entry_price) / entry_price * 100.0,
        "SHORT" => (entry_price - current_price) / entry_price * 100.0,
        _ => return false,
    };

    *max_profit_pct = max_profit_pct.max(pnl_pct);

    if *max_profit_pct < trailing_activate_pct {
        return false; // 未达到激活阈值
    }

    let drawdown = *max_profit_pct - pnl_pct;
    drawdown >= trailing_callback_pct
}

/// 检查趋势反转（MA288 < MA488 时平多，MA288 > MA488 时平空）
fn check_trend_reversal(side: &str, klines: &[KlineBar], fast_period: usize, slow_period: usize) -> bool {
    let fast_ma = match calc_sma(klines, fast_period) {
        Some(v) => v,
        None => return false,
    };
    let slow_ma = match calc_sma(klines, slow_period) {
        Some(v) => v,
        None => return false,
    };

    match side {
        "LONG" => fast_ma < slow_ma,   // MA288 < MA488 = 趋势转空
        "SHORT" => fast_ma > slow_ma,  // MA288 > MA488 = 趋势转多
        _ => false,
    }
}

/// 综合检查退出条件
///
/// 优先级: 硬止损 > MA288止损 > 移动止盈 > 趋势反转
fn check_exit_conditions(
    side: &str,
    entry_price: f64,
    current_price: f64,
    klines_30m: &[KlineBar],
    params: &serde_json::Value,
    position_state: Option<&mut PositionState>,
) -> Option<ExitReason> {
    // 解析策略参数
    let hard_stop_pct = params.get("hard_stop_pct")
        .and_then(|v| v.as_f64())
        .unwrap_or(0.0);
    let stop_mode = params.get("stop_mode")
        .and_then(|v| v.as_str())
        .unwrap_or("ma288");
    let fast_ma_period = params.get("fast_ma_period")
        .and_then(|v| v.as_u64())
        .unwrap_or(288) as usize;
    let slow_ma_period = params.get("slow_ma_period")
        .and_then(|v| v.as_u64())
        .unwrap_or(488) as usize;
    let take_profit_mode = params.get("take_profit_mode")
        .and_then(|v| v.as_str())
        .unwrap_or("none");
    let trailing_activate_pct = params.get("trailing_activate_pct")
        .and_then(|v| v.as_f64())
        .unwrap_or(5.0);
    let trailing_callback_pct = params.get("trailing_callback_pct")
        .and_then(|v| v.as_f64())
        .unwrap_or(5.0);

    // 1. 硬止损（优先级最高）
    if hard_stop_pct > 0.0 {
        let hard_stop_price = match side {
            "LONG" => entry_price * (1.0 - hard_stop_pct / 100.0),
            "SHORT" => entry_price * (1.0 + hard_stop_pct / 100.0),
            _ => 0.0,
        };
        let triggered = match side {
            "LONG" => current_price <= hard_stop_price,
            "SHORT" => current_price >= hard_stop_price,
            _ => false,
        };
        if triggered {
            debug!(
                "[退出] 硬止损触发: {} entry={:.4} current={:.4} stop={:.4}",
                side, entry_price, current_price, hard_stop_price
            );
            return Some(ExitReason::HardStop);
        }
    }

    // 2. MA288 止损（动态交叉检查）
    if stop_mode == "ma288" {
        if check_ma288_stop(side, klines_30m, fast_ma_period) {
            debug!(
                "[退出] MA288止损触发: {} entry={:.4} current={:.4}",
                side, entry_price, current_price
            );
            return Some(ExitReason::Ma288Stop);
        }
    }

    // 3. 移动止盈
    if take_profit_mode == "trailing" {
        // 需要持仓状态来跟踪 max_profit_pct
        if let Some(state) = position_state {
            if check_trailing_tp(
                side,
                entry_price,
                current_price,
                &mut state.max_profit_pct,
                trailing_activate_pct,
                trailing_callback_pct,
            ) {
                debug!(
                    "[退出] 移动止盈触发: {} entry={:.4} current={:.4} max_profit={:.2}%",
                    side, entry_price, current_price, state.max_profit_pct
                );
                return Some(ExitReason::TrailingTp);
            }
        }
    }

    // 4. 趋势反转
    if check_trend_reversal(side, klines_30m, fast_ma_period, slow_ma_period) {
        debug!(
            "[退出] 趋势反转触发: {} entry={:.4} current={:.4}",
            side, entry_price, current_price
        );
        return Some(ExitReason::TrendReversal);
    }

    None
}
