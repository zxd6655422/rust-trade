use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use chrono::Utc;
use redis::aio::ConnectionManager;
use rust_decimal::Decimal;
use sqlx::PgPool;
use tracing::{error, info, warn};

use crate::db::{signals, strategies as db_strategies};
use crate::exchange;
use crate::redis_reader::{self, MultiTimeframeData, Timeframe};
use crate::strategies::{self, SignalType};
use crate::websocket::{WsMessage, WsState};
use crate::alert::{AlertManager, AlertConfig, create_signal_alert, create_trade_alert};

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
            default_timeframe: Timeframe::OneMinute,
            default_kline_limit: 500, // 默认读取 500 根 K 线用于指标计算
            enable_multi_tf: true,
        }
    }
}

pub async fn run(
    pool: PgPool,
    mut redis: ConnectionManager,
    interval_secs: u64,
    ws_state: Option<Arc<WsState>>,
    alert_manager: Option<Arc<AlertManager>>,
) -> Result<()> {
    let config = EngineConfig {
        poll_interval_secs: interval_secs,
        ..Default::default()
    };

    run_with_config(pool, redis, config, ws_state, alert_manager).await
}

pub async fn run_with_config(
    pool: PgPool,
    mut redis: ConnectionManager,
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
                    &mut redis,
                    &strategy_instance,
                    symbol,
                    &config,
                    &ws_state,
                    &alert_manager,
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
    redis: &mut ConnectionManager,
    strategy_instance: &db_strategies::StrategyInstance,
    symbol: &str,
    config: &EngineConfig,
    ws_state: &Option<Arc<WsState>>,
    alert_manager: &Option<Arc<AlertManager>>,
) -> Result<()> {
    // 创建策略实例
    let strategy = strategies::create_strategy(&strategy_instance.strategy_type, &strategy_instance.params)?;

    // 判断是否使用多时间框架分析
    let use_multi_tf = config.enable_multi_tf
        && strategy_instance.strategy_type != "rsi"
        && strategy_instance.strategy_type != "macd"
        && strategy_instance.strategy_type != "bollinger"
        && strategy_instance.strategy_type != "volume";

    // 获取市场数据和信号（同时保存 current_price 用于信号追踪）
    let (signal, kline_price_f64) = if use_multi_tf {
        // 获取策略需要的时间框架
        let timeframes = strategy.required_timeframes();

        if timeframes.len() > 1 {
            // 多时间框架分析
            let multi_data = redis_reader::get_multi_timeframe_data(
                redis,
                symbol,
                &timeframes,
                config.default_kline_limit,
            )
            .await?;

            // 数据完整性验证（策略层）
            if let Err(reason) = validate_strategy_data(&multi_data.primary, symbol) {
                warn!("[{}] 跳过策略执行: {}", symbol, reason);
                return Ok(());
            }

            let price = multi_data.primary.current_price;
            let signal = strategy.analyze_multi_tf(&multi_data).await;
            (signal, price)
        } else {
            // 单时间框架
            let market_data = redis_reader::get_market_data(
                redis,
                symbol,
                &config.default_timeframe,
                config.default_kline_limit,
            )
            .await?;

            // 数据完整性验证（策略层）
            if let Err(reason) = validate_strategy_data(&market_data, symbol) {
                warn!("[{}] 跳过策略执行: {}", symbol, reason);
                return Ok(());
            }

            let price = market_data.current_price;
            let signal = strategy.analyze(&market_data).await;
            (signal, price)
        }
    } else {
        // 传统单时间框架分析
        let market_data = redis_reader::get_market_data(
            redis,
            symbol,
            &config.default_timeframe,
            config.default_kline_limit,
        )
        .await?;

        // 数据完整性验证（策略层）
        if let Err(reason) = validate_strategy_data(&market_data, symbol) {
            warn!("[{}] 跳过策略执行: {}", symbol, reason);
            return Ok(());
        }

        let price = market_data.current_price;
        let signal = strategy.analyze(&market_data).await;
        (signal, price)
    };

    // 获取实时 ticker 价格（公开 API，无需 API Key）
    // 根据 market_type 选择正确的 API（现货/合约）
    // 用于信号追踪和收益率计算，比 K 线收盘价更准确
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

    let signal = match signal {
        Some(signal) => signal,
        None => return Ok(()), // 没有信号
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
    // 使用实时 ticker 价格（公开 API）而不是 K 线收盘价
    let current_price = rust_decimal::Decimal::try_from(current_price_f64)?;

    for old_signal in active_signals {
        let old_direction = &old_signal.direction;

        // 检查是否方向反转
        let is_reversal = (current_direction == "bullish" && old_direction == "bearish")
            || (current_direction == "bearish" && old_direction == "bullish");

        if is_reversal {
            // 计算收益率
            let return_pct = calc_return_pct(old_direction, old_signal.entry_price, current_price);

            // 关闭旧信号
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

    let signal_request = signals::CreateSignalRequest {
        strategy_id: strategy_instance.strategy_type.clone(),
        symbol: symbol.to_string(),
        direction: direction.to_string(),
        // 使用实时 ticker 价格作为入场价，比 K 线收盘价更准确
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
        // V7 新增字段：策略分析详情
        market_structure: signal.market_structure.map(|ms| serde_json::to_value(ms).unwrap_or_default()),
        key_levels: signal.key_levels.map(|kl| serde_json::to_value(kl).unwrap_or_default()),
        trade_setup: signal.trade_setup.map(|ts| serde_json::to_value(ts).unwrap_or_default()),
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

    // 如果启用了自动交易，标记信号为待执行
    // 实际下单由 trading-engine 从 strategy_signals 表轮询执行
    if strategy_instance.auto_trade {
        info!(
            "✅ Auto-trade signal queued: {} {} @ {} (signal={}, stop_loss={:?}, take_profit={:?})",
            direction, symbol, signal.entry_price,
            saved_signal.id, signal.stop_loss, signal.take_profit
        );

        // 发送交易执行告警
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
///
/// 检查项：
/// 1. K线数据是否为空
/// 2. 数据条数是否满足最低预热要求
/// 3. 最新数据是否过旧（超过2个周期）
///
/// 返回 Ok(()) 表示数据可用，Err 包含跳过原因
fn validate_strategy_data(
    market_data: &redis_reader::MarketData,
    symbol: &str,
) -> Result<(), String> {
    let tf = &market_data.timeframe;

    // 1. 检查数据是否为空
    if market_data.klines.is_empty() {
        return Err(format!("[{}:{}] K线数据为空", symbol, tf.as_str()));
    }

    // 2. 检查数据条数是否满足最低预热要求
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

    // 3. 检查最新数据是否过旧
    if let Some(latest) = market_data.klines.last() {
        let now_ms = chrono::Utc::now().timestamp_millis();
        let age_ms = now_ms - latest.timestamp;
        let max_age_ms = tf.as_duration().num_milliseconds() * 2;

        if age_ms > max_age_ms {
            let age_minutes = age_ms / 60000;
            return Err(format!(
                "[{}:{}] 数据过旧: 最新时间戳延迟 {} 分钟",
                symbol,
                tf.as_str(),
                age_minutes
            ));
        }
    }

    Ok(())
}

/// 检查是否应该跳过信号（避免频繁交易）
///
/// 信号去重逻辑：
/// 1. 5分钟内跳过所有信号（无论方向）
/// 2. 同方向信号延长冷却期到15分钟
/// 3. 反向信号允许立即生成（已过基础冷却期）
async fn should_skip_signal(
    pool: &PgPool,
    instance_id: uuid::Uuid,
    symbol: &str,
    signal_type: &SignalType,
) -> Result<bool> {
    // 获取最近一个信号
    let last_signal = signals::get_last_signal(pool, instance_id, symbol).await?;

    if let Some(last) = last_signal {
        let now = Utc::now();
        let age = now - last.created_at;

        // 1. 基础冷却期：5分钟内跳过所有信号
        let base_cooldown = chrono::Duration::minutes(5);
        if age < base_cooldown {
            info!(
                "[{}] 信号冷却中: 距上次信号 {} 分钟 (最小间隔 5 分钟)",
                symbol,
                age.num_minutes()
            );
            return Ok(true);
        }

        // 2. 同方向信号：延长冷却期到15分钟
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

        // 3. 反向信号：允许立即生成（已过基础冷却期）
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
///
/// 用于方向反转时计算旧信号的盈亏
fn calc_return_pct(direction: &str, entry_price: Decimal, current_price: Decimal) -> Decimal {
    if entry_price == Decimal::ZERO {
        return Decimal::ZERO;
    }
    let pct = (current_price - entry_price) / entry_price * Decimal::from(100);
    match direction {
        "bullish" => pct,   // 多头：涨=正
        "bearish" => -pct,  // 空头：跌=正
        _ => Decimal::ZERO,
    }
}
