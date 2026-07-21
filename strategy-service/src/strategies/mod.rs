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

// Each trading-common strategy module defines its own KlineBar/MarketData/Signal types.
// They are structurally identical but distinct Rust types, so we need per-module converters.

// --- rsi module types (also used by trend and macro_cycle) ---

fn to_rsi_kline_bar(k: &KlineData) -> trading_common::strategy::rsi::KlineBar {
    trading_common::strategy::rsi::KlineBar {
        open: k.open,
        high: k.high,
        low: k.low,
        close: k.close,
        volume: k.volume,
    }
}

fn to_rsi_market_data(data: &MarketData) -> trading_common::strategy::rsi::MarketData {
    trading_common::strategy::rsi::MarketData {
        klines: data.klines.iter().map(to_rsi_kline_bar).collect(),
        current_price: data.current_price,
        symbol: data.symbol.clone(),
        timeframe: to_tc_timeframe(&data.timeframe),
    }
}

fn from_rsi_signal_type(st: trading_common::strategy::rsi::SignalType) -> SignalType {
    match st {
        trading_common::strategy::rsi::SignalType::Buy => SignalType::Buy,
        trading_common::strategy::rsi::SignalType::Sell => SignalType::Sell,
        trading_common::strategy::rsi::SignalType::Hold => SignalType::Hold,
    }
}

fn from_rsi_signal(s: trading_common::strategy::rsi::Signal) -> Signal {
    Signal {
        signal_type: from_rsi_signal_type(s.signal_type),
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

// --- macd module types ---

fn to_macd_kline_bar(k: &KlineData) -> trading_common::strategy::macd::KlineBar {
    trading_common::strategy::macd::KlineBar {
        open: k.open,
        high: k.high,
        low: k.low,
        close: k.close,
        volume: k.volume,
    }
}

fn to_macd_market_data(data: &MarketData) -> trading_common::strategy::macd::MarketData {
    trading_common::strategy::macd::MarketData {
        klines: data.klines.iter().map(to_macd_kline_bar).collect(),
        current_price: data.current_price,
        symbol: data.symbol.clone(),
        timeframe: to_tc_timeframe(&data.timeframe),
    }
}

fn from_macd_signal(s: trading_common::strategy::macd::Signal) -> Signal {
    Signal {
        signal_type: match s.signal_type {
            trading_common::strategy::macd::SignalType::Buy => SignalType::Buy,
            trading_common::strategy::macd::SignalType::Sell => SignalType::Sell,
            trading_common::strategy::macd::SignalType::Hold => SignalType::Hold,
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

// --- bollinger module types ---

fn to_bollinger_kline_bar(k: &KlineData) -> trading_common::strategy::bollinger::KlineBar {
    trading_common::strategy::bollinger::KlineBar {
        open: k.open,
        high: k.high,
        low: k.low,
        close: k.close,
        volume: k.volume,
    }
}

fn to_bollinger_market_data(data: &MarketData) -> trading_common::strategy::bollinger::MarketData {
    trading_common::strategy::bollinger::MarketData {
        klines: data.klines.iter().map(to_bollinger_kline_bar).collect(),
        current_price: data.current_price,
        symbol: data.symbol.clone(),
        timeframe: to_tc_timeframe(&data.timeframe),
    }
}

fn from_bollinger_signal(s: trading_common::strategy::bollinger::Signal) -> Signal {
    Signal {
        signal_type: match s.signal_type {
            trading_common::strategy::bollinger::SignalType::Buy => SignalType::Buy,
            trading_common::strategy::bollinger::SignalType::Sell => SignalType::Sell,
            trading_common::strategy::bollinger::SignalType::Hold => SignalType::Hold,
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

// --- volume module types ---

fn to_volume_kline_bar(k: &KlineData) -> trading_common::strategy::volume::KlineBar {
    trading_common::strategy::volume::KlineBar {
        open: k.open,
        high: k.high,
        low: k.low,
        close: k.close,
        volume: k.volume,
    }
}

fn to_volume_market_data(data: &MarketData) -> trading_common::strategy::volume::MarketData {
    trading_common::strategy::volume::MarketData {
        klines: data.klines.iter().map(to_volume_kline_bar).collect(),
        current_price: data.current_price,
        symbol: data.symbol.clone(),
        timeframe: to_tc_timeframe(&data.timeframe),
    }
}

fn from_volume_signal(s: trading_common::strategy::volume::Signal) -> Signal {
    Signal {
        signal_type: match s.signal_type {
            trading_common::strategy::volume::SignalType::Buy => SignalType::Buy,
            trading_common::strategy::volume::SignalType::Sell => SignalType::Sell,
            trading_common::strategy::volume::SignalType::Hold => SignalType::Hold,
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

// --- multi_tf module types ---
// multi_tf imports Signal/MarketData from rsi, so they are the same types.
// We only need a converter for TimeframeMarketData and MultiTimeframeData.

fn to_multi_tf_tf_market_data(
    data: &MarketData,
) -> trading_common::strategy::multi_tf::TimeframeMarketData {
    trading_common::strategy::multi_tf::TimeframeMarketData {
        klines: data.klines.iter().map(to_rsi_kline_bar).collect(),
        current_price: data.current_price,
        symbol: data.symbol.clone(),
        timeframe: data.timeframe.as_str().to_string(),
    }
}

fn to_multi_tf_data(
    data: &MultiTimeframeData,
) -> trading_common::strategy::multi_tf::MultiTimeframeData {
    let primary = to_rsi_market_data(&data.primary);
    let all: Vec<trading_common::strategy::multi_tf::TimeframeMarketData> =
        data.all.iter().map(to_multi_tf_tf_market_data).collect();
    trading_common::strategy::multi_tf::MultiTimeframeData { primary, all }
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
// Strategy adapters: wrap trading-common strategies
// =================================================================

// --- RSI ---

struct RsiAdapter(trading_common::strategy::rsi::RsiStrategy);

#[async_trait]
impl Strategy for RsiAdapter {
    fn name(&self) -> &str {
        "rsi"
    }

    async fn analyze(&self, data: &MarketData) -> Option<Signal> {
        let tc_data = to_rsi_market_data(data);
        self.0.analyze(&tc_data).map(from_rsi_signal)
    }

    fn from_params(params: &serde_json::Value) -> anyhow::Result<Self> {
        let s = trading_common::strategy::rsi::RsiStrategy::from_json(params)
            .map_err(|e| anyhow::anyhow!(e))?;
        Ok(RsiAdapter(s))
    }
}

// --- MACD ---

struct MacdAdapter(trading_common::strategy::macd::MacdStrategy);

#[async_trait]
impl Strategy for MacdAdapter {
    fn name(&self) -> &str {
        "macd"
    }

    async fn analyze(&self, data: &MarketData) -> Option<Signal> {
        let tc_data = to_macd_market_data(data);
        self.0.analyze(&tc_data).map(from_macd_signal)
    }

    fn from_params(params: &serde_json::Value) -> anyhow::Result<Self> {
        let s = trading_common::strategy::macd::MacdStrategy::from_json(params)
            .map_err(|e| anyhow::anyhow!(e))?;
        Ok(MacdAdapter(s))
    }
}

// --- Bollinger ---

struct BollingerAdapter(trading_common::strategy::bollinger::BollingerStrategy);

#[async_trait]
impl Strategy for BollingerAdapter {
    fn name(&self) -> &str {
        "bollinger"
    }

    async fn analyze(&self, data: &MarketData) -> Option<Signal> {
        let tc_data = to_bollinger_market_data(data);
        self.0.analyze(&tc_data).map(from_bollinger_signal)
    }

    fn from_params(params: &serde_json::Value) -> anyhow::Result<Self> {
        let s = trading_common::strategy::bollinger::BollingerStrategy::from_json(params)
            .map_err(|e| anyhow::anyhow!(e))?;
        Ok(BollingerAdapter(s))
    }
}

// --- Volume ---

struct VolumeAdapter(trading_common::strategy::volume::VolumeStrategy);

#[async_trait]
impl Strategy for VolumeAdapter {
    fn name(&self) -> &str {
        "volume"
    }

    async fn analyze(&self, data: &MarketData) -> Option<Signal> {
        let tc_data = to_volume_market_data(data);
        self.0.analyze(&tc_data).map(from_volume_signal)
    }

    fn from_params(params: &serde_json::Value) -> anyhow::Result<Self> {
        let s = trading_common::strategy::volume::VolumeStrategy::from_json(params)
            .map_err(|e| anyhow::anyhow!(e))?;
        Ok(VolumeAdapter(s))
    }
}

// --- Trend ---

struct TrendAdapter(trading_common::strategy::trend::TrendStrategy);

#[async_trait]
impl Strategy for TrendAdapter {
    fn name(&self) -> &str {
        "trend"
    }

    async fn analyze(&self, data: &MarketData) -> Option<Signal> {
        let tc_data = to_rsi_market_data(data);
        self.0.analyze(&tc_data).map(from_rsi_signal)
    }

    fn required_timeframes(&self) -> Vec<Timeframe> {
        vec![Timeframe::OneHour, Timeframe::FourHour]
    }

    fn from_params(params: &serde_json::Value) -> anyhow::Result<Self> {
        let s = trading_common::strategy::trend::TrendStrategy::from_json(params)
            .map_err(|e| anyhow::anyhow!(e))?;
        Ok(TrendAdapter(s))
    }
}

// --- Multi-Timeframe ---

struct MultiTfAdapter(trading_common::strategy::multi_tf::MultiTimeframeStrategy);

#[async_trait]
impl Strategy for MultiTfAdapter {
    fn name(&self) -> &str {
        "multi_tf"
    }

    async fn analyze(&self, data: &MarketData) -> Option<Signal> {
        let tc_data = to_rsi_market_data(data);
        self.0.analyze(&tc_data).map(from_rsi_signal)
    }

    async fn analyze_multi_tf(&self, data: &MultiTimeframeData) -> Option<Signal> {
        let tc_data = to_multi_tf_data(data);
        self.0.analyze_multi_tf(&tc_data).map(from_rsi_signal)
    }

    fn required_timeframes(&self) -> Vec<Timeframe> {
        vec![Timeframe::OneHour, Timeframe::FourHour, Timeframe::OneDay]
    }

    fn from_params(params: &serde_json::Value) -> anyhow::Result<Self> {
        let s = trading_common::strategy::multi_tf::MultiTimeframeStrategy::from_json(params)
            .map_err(|e| anyhow::anyhow!(e))?;
        Ok(MultiTfAdapter(s))
    }
}

// --- Macro Cycle ---

struct MacroCycleAdapter(trading_common::strategy::macro_cycle::MacroCycleStrategy);

#[async_trait]
impl Strategy for MacroCycleAdapter {
    fn name(&self) -> &str {
        "macro_cycle"
    }

    async fn analyze(&self, data: &MarketData) -> Option<Signal> {
        let tc_data = to_rsi_market_data(data);
        self.0.analyze(&tc_data).map(from_rsi_signal)
    }

    fn required_timeframes(&self) -> Vec<Timeframe> {
        vec![Timeframe::OneDay, Timeframe::OneWeek]
    }

    fn from_params(params: &serde_json::Value) -> anyhow::Result<Self> {
        let s = trading_common::strategy::macro_cycle::MacroCycleStrategy::from_json(params)
            .map_err(|e| anyhow::anyhow!(e))?;
        Ok(MacroCycleAdapter(s))
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
        "rsi" => Ok(Box::new(RsiAdapter::from_params(params)?)),
        "macd" => Ok(Box::new(MacdAdapter::from_params(params)?)),
        "bollinger" => Ok(Box::new(BollingerAdapter::from_params(params)?)),
        "volume" => Ok(Box::new(VolumeAdapter::from_params(params)?)),
        "trend" => Ok(Box::new(TrendAdapter::from_params(params)?)),
        "multi_tf" => Ok(Box::new(MultiTfAdapter::from_params(params)?)),
        "macro_cycle" => Ok(Box::new(MacroCycleAdapter::from_params(params)?)),
        _ => Err(anyhow::anyhow!("Unknown strategy type: {}", strategy_type)),
    }
}

/// Get the timeframes required by a strategy type.
///
/// Used by the engine to determine which K-line data to fetch from Redis.
pub fn get_strategy_timeframes(strategy_type: &str, params: &serde_json::Value) -> Vec<Timeframe> {
    match strategy_type {
        "multi_tf" => {
            // Multi-timeframe strategy: read from params
            if let Some(tfs) = params.get("timeframes").and_then(|v| v.as_array()) {
                let mut timeframes: Vec<Timeframe> = tfs
                    .iter()
                    .filter_map(|v| v.as_str().and_then(Timeframe::from_str))
                    .collect();
                timeframes.sort_by_key(|tf| tf.level());
                timeframes
            } else {
                // Default: 1h + 4h + 1d
                vec![Timeframe::OneHour, Timeframe::FourHour, Timeframe::OneDay]
            }
        }
        "macro_cycle" => {
            // Macro cycle strategy
            let primary = params
                .get("primary_timeframe")
                .and_then(|v| v.as_str())
                .and_then(Timeframe::from_str)
                .unwrap_or(Timeframe::OneDay);

            let secondary = params
                .get("secondary_timeframe")
                .and_then(|v| v.as_str())
                .and_then(Timeframe::from_str)
                .unwrap_or(Timeframe::OneWeek);

            let mut tfs = vec![primary, secondary];
            tfs.sort_by_key(|tf| tf.level());
            tfs.dedup();
            tfs
        }
        "trend" => {
            // Trend strategy
            vec![Timeframe::OneHour, Timeframe::FourHour]
        }
        _ => {
            // Other strategies: default single timeframe
            vec![Timeframe::OneMinute]
        }
    }
}
