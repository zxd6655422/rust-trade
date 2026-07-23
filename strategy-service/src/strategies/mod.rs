// strategy-service/src/strategies/mod.rs
//
// Delegates all strategy logic to trading-common strategy implementations.
// Provides adapter types that bridge redis_reader data types to trading-common's
// MarketData/Signal types while preserving the async Strategy trait for engine compatibility.

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::redis_reader::{KlineData, MarketData, MultiTimeframeData, Timeframe};

// =================================================================
// Signal types (re-exported for engine compatibility)
// =================================================================

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum SignalType {
    Buy,
    Sell,
    Hold,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Signal {
    // --- 现有字段保持不变 ---
    pub signal_type: SignalType,
    pub signal_strength: f64,
    pub entry_price: f64,
    pub stop_loss: Option<f64>,
    pub take_profit: Option<f64>,
    pub confidence: f64,
    pub reason: String,
    pub market_context: serde_json::Value,

    // --- 新增字段（向后兼容）---
    /// 市场结构判断
    #[serde(default)]
    pub market_structure: Option<MarketStructure>,
    /// 关键价位
    #[serde(default)]
    pub key_levels: Option<KeyLevels>,
    /// 交易计划
    #[serde(default)]
    pub trade_setup: Option<TradeSetup>,
}

/// 市场结构类型
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum MarketStructureType {
    TrendingUp,
    TrendingDown,
    Ranging,
    Breakout,
    Reversal,
}

/// 市场结构判断
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketStructure {
    pub structure_type: MarketStructureType,
    pub confidence: f64,
    pub description: String,
}

/// 关键价位集合
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyLevels {
    pub support: Vec<f64>,
    pub resistance: Vec<f64>,
    pub pivot: Option<f64>,
}

/// 交易计划
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TradeSetup {
    pub entry_zone: (f64, f64),
    pub stop_loss: f64,
    pub take_profit: Vec<f64>,
    pub risk_reward: f64,
    pub invalidation: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StrategyParams {
    pub strategy_type: String,
    pub params: serde_json::Value,
}

// =================================================================
// Type conversions: redis_reader types <-> trading-common types
// =================================================================

/// Convert redis_reader Timeframe to trading-common Timeframe
fn to_tc_timeframe(tf: &Timeframe) -> trading_common::data::types::Timeframe {
    match tf {
        Timeframe::OneMinute => trading_common::data::types::Timeframe::OneMinute,
        Timeframe::FiveMinutes => trading_common::data::types::Timeframe::FiveMinutes,
        Timeframe::FifteenMinutes => trading_common::data::types::Timeframe::FifteenMinutes,
        Timeframe::ThirtyMinutes => trading_common::data::types::Timeframe::ThirtyMinutes,
        Timeframe::OneHour => trading_common::data::types::Timeframe::OneHour,
        Timeframe::TwoHour => trading_common::data::types::Timeframe::TwoHour,
        Timeframe::FourHour => trading_common::data::types::Timeframe::FourHour,
        Timeframe::OneDay => trading_common::data::types::Timeframe::OneDay,
        Timeframe::ThreeDay => trading_common::data::types::Timeframe::ThreeDay,
        Timeframe::OneWeek => trading_common::data::types::Timeframe::OneWeek,
    }
}

// =================================================================
// Strategy trait (async, preserved for engine compatibility)
// =================================================================

#[async_trait]
pub trait Strategy: Send + Sync {
    /// Strategy name
    fn name(&self) -> &str;

    /// Analyze market data and return a signal (single timeframe)
    async fn analyze(&self, data: &MarketData) -> Option<Signal>;

    /// Multi-timeframe analysis
    async fn analyze_multi_tf(&self, data: &MultiTimeframeData) -> Option<Signal> {
        self.analyze(&data.primary).await
    }

    /// Required timeframes for this strategy
    fn required_timeframes(&self) -> Vec<Timeframe> {
        vec![Timeframe::OneMinute]
    }

    /// Create strategy from JSON params
    fn from_params(params: &serde_json::Value) -> anyhow::Result<Self>
    where
        Self: Sized;
}

// =================================================================
// MA Trend Pullback Strategy adapter
// =================================================================

fn to_matp_kline_bar(k: &KlineData) -> trading_common::strategy::ma_trend_pullback::KlineBar {
    trading_common::strategy::ma_trend_pullback::KlineBar {
        open: k.open,
        high: k.high,
        low: k.low,
        close: k.close,
        volume: k.volume,
    }
}

fn to_matp_market_data(data: &MarketData, klines_5m: Option<&Vec<KlineData>>) -> trading_common::strategy::ma_trend_pullback::MarketData {
    trading_common::strategy::ma_trend_pullback::MarketData {
        klines: data.klines.iter().map(to_matp_kline_bar).collect(),
        current_price: data.current_price,
        symbol: data.symbol.clone(),
        timeframe: to_tc_timeframe(&data.timeframe),
        klines_5m: klines_5m.map(|klines| klines.iter().map(to_matp_kline_bar).collect()),
    }
}

fn from_matp_signal(s: trading_common::strategy::ma_trend_pullback::Signal) -> Signal {
    Signal {
        signal_type: match s.signal_type {
            trading_common::strategy::ma_trend_pullback::SignalType::Buy => SignalType::Buy,
            trading_common::strategy::ma_trend_pullback::SignalType::Sell => SignalType::Sell,
            trading_common::strategy::ma_trend_pullback::SignalType::Hold => SignalType::Hold,
        },
        signal_strength: s.signal_strength,
        entry_price: s.entry_price,
        stop_loss: s.stop_loss,
        take_profit: s.take_profit,
        confidence: s.confidence,
        reason: s.reason,
        market_context: s.market_context,
        market_structure: None,
        key_levels: None,
        trade_setup: None,
    }
}

struct MATrendPullbackAdapter(trading_common::strategy::ma_trend_pullback::MATrendPullbackStrategy);

#[async_trait]
impl Strategy for MATrendPullbackAdapter {
    fn name(&self) -> &str {
        "ma_trend_pullback"
    }

    async fn analyze(&self, data: &MarketData) -> Option<Signal> {
        let tc_data = to_matp_market_data(data, None);
        self.0.analyze(&tc_data).map(from_matp_signal)
    }

    async fn analyze_multi_tf(&self, data: &MultiTimeframeData) -> Option<Signal> {
        // Extract 5m klines if available
        let klines_5m = data.all.iter()
            .find(|d| d.timeframe == Timeframe::FiveMinutes)
            .map(|d| &d.klines);

        let tc_data = to_matp_market_data(&data.primary, klines_5m);
        self.0.analyze(&tc_data).map(from_matp_signal)
    }

    fn required_timeframes(&self) -> Vec<Timeframe> {
        vec![Timeframe::ThirtyMinutes, Timeframe::FiveMinutes]
    }

    fn from_params(params: &serde_json::Value) -> anyhow::Result<Self> {
        let s = trading_common::strategy::ma_trend_pullback::MATrendPullbackStrategy::from_json(params)
            .map_err(|e| anyhow::anyhow!(e))?;
        Ok(MATrendPullbackAdapter(s))
    }
}

// =================================================================
// Factory
// =================================================================

/// Create a strategy instance by type name and JSON parameters.
///
/// Delegates to trading-common strategy implementations.
pub fn create_strategy(
    strategy_type: &str,
    params: &serde_json::Value,
) -> anyhow::Result<Box<dyn Strategy>> {
    match strategy_type {
        "ma_trend_pullback" => Ok(Box::new(MATrendPullbackAdapter::from_params(params)?)),
        _ => Err(anyhow::anyhow!("Unknown strategy type: {}", strategy_type)),
    }
}

/// Get the timeframes required by a strategy type.
///
/// Used by the engine to determine which K-line data to fetch from Redis.
pub fn get_strategy_timeframes(strategy_type: &str, params: &serde_json::Value) -> Vec<Timeframe> {
    match strategy_type {
        "ma_trend_pullback" => {
            // MA Trend Pullback strategy: 30m primary + 5m for diffusion filter
            let primary = params
                .get("primary_timeframe")
                .and_then(|v| v.as_str())
                .and_then(Timeframe::from_str)
                .unwrap_or(Timeframe::ThirtyMinutes);
            vec![primary, Timeframe::FiveMinutes]
        }
        _ => {
            // Other strategies: default single timeframe
            vec![Timeframe::OneMinute]
        }
    }
}
