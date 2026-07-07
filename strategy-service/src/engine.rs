use std::time::Duration;

use anyhow::Result;
use chrono::Utc;
use redis::aio::ConnectionManager;
use sqlx::PgPool;
use tracing::{error, info, warn};

use crate::db::{signals, strategies as db_strategies};
use crate::redis_reader::{self, Timeframe};
use crate::strategies::{self, SignalType};

/// 策略执行引擎配置
pub struct EngineConfig {
    pub poll_interval_secs: u64,
    pub default_timeframe: Timeframe,
    pub default_kline_limit: usize,
}

impl Default for EngineConfig {
    fn default() -> Self {
        Self {
            poll_interval_secs: 5,
            default_timeframe: Timeframe::OneMinute,
            default_kline_limit: 500, // 默认读取 500 根 K 线用于指标计算
        }
    }
}

pub async fn run(pool: PgPool, mut redis: ConnectionManager, interval_secs: u64) -> Result<()> {
    let config = EngineConfig {
        poll_interval_secs: interval_secs,
        ..Default::default()
    };

    run_with_config(pool, redis, config).await
}

pub async fn run_with_config(
    pool: PgPool,
    mut redis: ConnectionManager,
    config: EngineConfig,
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
) -> Result<()> {
    // 获取市场数据
    let market_data = redis_reader::get_market_data(
        redis,
        symbol,
        &config.default_timeframe,
        config.default_kline_limit,
    )
    .await?;

    if market_data.klines.is_empty() {
        warn!("No kline data for {}", symbol);
        return Ok(());
    }

    // 创建策略实例
    let strategy = strategies::create_strategy(&strategy_instance.strategy_type, &strategy_instance.params)?;

    // 分析市场数据
    let signal = match strategy.analyze(&market_data).await {
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
        entry_price: rust_decimal::Decimal::try_from(signal.entry_price)?,
        overall_confidence: rust_decimal::Decimal::try_from(signal.confidence)?,
        entry_allowed: signal.signal_type != SignalType::Hold,
        entry_direction: entry_direction.map(|s| s.to_string()),
        timeframe_details: None,
        instance_id: Some(strategy_instance.id),
        signal_strength: Some(rust_decimal::Decimal::try_from(signal.signal_strength)?),
        market_context: Some(signal.market_context),
        stop_loss: signal.stop_loss.map(|v| rust_decimal::Decimal::try_from(v)).transpose()?,
        take_profit: signal.take_profit.map(|v| rust_decimal::Decimal::try_from(v)).transpose()?,
    };

    let saved_signal = signals::create_signal(pool, signal_request).await?;

    info!(
        "Signal generated: {} {} at {} (strength={:.2}, reason={})",
        direction,
        symbol,
        signal.entry_price,
        signal.signal_strength,
        signal.reason
    );

    // 如果启用了自动交易，可以在这里触发交易执行
    if strategy_instance.auto_trade {
        info!(
            "Auto-trade enabled for strategy {}, signal {} would trigger trade",
            strategy_instance.id, saved_signal.id
        );
        // TODO: 调用 trading-engine 执行交易
    }

    Ok(())
}

/// 检查是否应该跳过信号（避免频繁交易）
async fn should_skip_signal(
    pool: &PgPool,
    instance_id: uuid::Uuid,
    symbol: &str,
    _signal_type: &SignalType,
) -> Result<bool> {
    // 查询最近的信号
    let recent_signals = signals::get_signals_by_instance(pool, instance_id, Some(10)).await?;

    // 检查同一交易对的最近信号时间
    let now = Utc::now();
    let min_interval = chrono::Duration::minutes(5); // 最小间隔 5 分钟

    for recent in recent_signals {
        if recent.symbol == symbol && recent.created_at + min_interval > now {
            return Ok(true); // 太近了，跳过
        }
    }

    Ok(false)
}
