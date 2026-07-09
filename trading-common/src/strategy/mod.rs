// trading-common/src/strategy/mod.rs
// Unified Strategy trait and factory for all strategy types
//
// This module provides a single entry point for creating and managing strategies,
// bridging both tick-based (backtest::strategy::Strategy) and multi-timeframe
// (backtest::strategy::MultiTimeframeStrategy) strategy types.

pub mod bollinger;
pub mod macd;
pub mod macro_cycle;
pub mod multi_tf;
pub mod rsi;
pub mod trend;
pub mod volume;

use crate::data::types::{OHLCData, TickData, Timeframe};
use std::collections::HashMap;

// Re-export existing strategy types from backtest module
pub use crate::backtest::strategy::{
    // Core traits
    MultiTimeframeStrategy,
    Strategy,
    // Signal type
    Signal,
    // Multi-timeframe analysis types
    EntryDirection,
    MultiTimeframeAnalysis,
    TrendAnalysis,
    TrendDirection,
    // Info type
    StrategyInfo,
    // Factory functions (for backward compatibility)
    create_multi_timeframe_strategy,
    create_strategy,
    get_strategy_info,
    is_multi_timeframe_strategy,
    list_strategies as list_backtest_strategies,
};

// =================================================================
// Strategy mode: distinguishes tick-based vs multi-timeframe
// =================================================================

/// Strategy execution mode
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StrategyMode {
    /// Tick/OHLC based strategy (backtest-oriented)
    Tick,
    /// Multi-timeframe K-line based strategy (analysis-oriented)
    MultiTimeframe,
}

impl StrategyMode {
    pub fn as_str(&self) -> &'static str {
        match self {
            StrategyMode::Tick => "tick",
            StrategyMode::MultiTimeframe => "multi_timeframe",
        }
    }
}

/// Strategy mode wrapper: wraps either a tick-based or multi-timeframe strategy
pub enum StrategyModeType {
    /// Tick-based strategy (on_tick / on_ohlc)
    Tick(Box<dyn Strategy>),
    /// Multi-timeframe strategy (analyze with K-line data)
    MultiTimeframe(Box<dyn MultiTimeframeStrategy>),
}

// =================================================================
// Unified Strategy trait
// =================================================================

/// Unified strategy interface that all strategies must implement.
///
/// This trait provides a common contract for metadata, initialization, and
/// lifecycle management. Concrete strategies delegate to the appropriate
/// backtest trait (`Strategy` or `MultiTimeframeStrategy`) via `mode()`.
pub trait UnifiedStrategy: Send + Sync {
    /// Returns the execution mode (tick or multi-timeframe).
    fn mode(&self) -> StrategyMode;

    /// Initialize strategy with parameters.
    fn initialize(&mut self, params: HashMap<String, String>) -> Result<(), String>;

    /// Reset strategy internal state for reuse.
    fn reset(&mut self);

    /// Strategy identifier (e.g. "sma", "rsi", "trend").
    fn strategy_id(&self) -> &str;

    /// Human-readable strategy name.
    fn name(&self) -> &str;

    /// Short description of the strategy logic.
    fn description(&self) -> &str;

    // -- Tick-based delegates (only valid when mode() == Tick) --

    /// Process a tick event. Returns `None` if mode is not Tick.
    fn on_tick(&mut self, _tick: &TickData) -> Option<Signal> {
        None
    }

    /// Process an OHLC bar. Returns `None` if mode is not Tick or OHLC is unsupported.
    fn on_ohlc(&mut self, _ohlc: &OHLCData) -> Option<Signal> {
        None
    }

    /// Whether this strategy can consume OHLC data.
    fn supports_ohlc(&self) -> bool {
        false
    }

    /// Preferred OHLC timeframe, if any.
    fn preferred_timeframe(&self) -> Option<Timeframe> {
        None
    }

    // -- Multi-timeframe delegates (only valid when mode() == MultiTimeframe) --

    /// Timeframes required by this strategy (ordered high to low).
    fn required_timeframes(&self) -> Option<Vec<Timeframe>> {
        None
    }

    /// Run multi-timeframe analysis. Returns `None` if mode is not MultiTimeframe.
    fn analyze(&mut self, _klines: &HashMap<Timeframe, Vec<OHLCData>>) -> Option<MultiTimeframeAnalysis> {
        None
    }

    /// Check whether the strategy should enter a position.
    fn should_enter(&self, _analysis: &MultiTimeframeAnalysis) -> bool {
        false
    }

    /// Check whether the strategy should exit a position.
    fn should_exit(&self, _analysis: &MultiTimeframeAnalysis, _is_long: bool) -> bool {
        false
    }
}

// =================================================================
// Strategy Factory
// =================================================================

/// Strategy constructor function type.
type StrategyCreator = Box<dyn Fn() -> Result<StrategyModeType, String> + Send + Sync>;

/// Factory for creating strategy instances by identifier.
///
/// Built-in strategies ("sma", "rsi", "trend") are registered by default.
/// Call `register()` to add custom strategies at runtime.
pub struct StrategyFactory {
    registry: HashMap<String, StrategyCreator>,
}

impl StrategyFactory {
    /// Create an empty factory (no built-in strategies).
    pub fn new() -> Self {
        Self {
            registry: HashMap::new(),
        }
    }

    /// Create a factory with all built-in strategies registered.
    pub fn with_defaults() -> Self {
        let mut factory = Self::new();
        factory.register_builtins();
        factory
    }

    /// Register built-in strategies: sma, rsi, trend.
    fn register_builtins(&mut self) {
        self.register("sma", || {
            create_strategy("sma").map(StrategyModeType::Tick)
        });
        self.register("rsi", || {
            create_strategy("rsi").map(StrategyModeType::Tick)
        });
        self.register("trend", || {
            create_multi_timeframe_strategy("trend").map(StrategyModeType::MultiTimeframe)
        });
    }

    /// Register a custom strategy creator under the given identifier.
    pub fn register<F>(&mut self, id: &str, creator: F)
    where
        F: Fn() -> Result<StrategyModeType, String> + Send + Sync + 'static,
    {
        self.registry.insert(id.to_string(), Box::new(creator));
    }

    /// Create a strategy instance by its identifier.
    pub fn create(&self, strategy_id: &str) -> Result<StrategyModeType, String> {
        let creator = self
            .registry
            .get(strategy_id)
            .ok_or_else(|| format!("Unknown strategy: {}", strategy_id))?;
        creator()
    }

    /// Get metadata for a registered strategy.
    pub fn get_info(&self, strategy_id: &str) -> Option<StrategyInfo> {
        if !self.registry.contains_key(strategy_id) {
            return None;
        }
        // Delegate to the backtest module's info function; fall back to
        // a generic entry for custom-registered strategies.
        get_strategy_info(strategy_id).or_else(|| {
            self.registry.get_key_value(strategy_id).map(|(id, _)| StrategyInfo {
                id: id.clone(),
                name: id.clone(),
                description: format!("Custom strategy: {}", id),
                is_multi_timeframe: self.is_multi_timeframe(strategy_id),
            })
        })
    }

    /// List metadata for all registered strategies.
    pub fn list(&self) -> Vec<StrategyInfo> {
        self.registry
            .keys()
            .filter_map(|id| self.get_info(id))
            .collect()
    }

    /// Check whether a registered strategy is multi-timeframe.
    pub fn is_multi_timeframe(&self, strategy_id: &str) -> bool {
        // For built-in strategies, delegate to the backtest module.
        if is_multi_timeframe_strategy(strategy_id) {
            return true;
        }
        // For custom strategies, attempt creation and inspect the mode.
        if let Ok(mut strategy) = self.create(strategy_id) {
            let result = match &mut strategy {
                StrategyModeType::Tick(_) => false,
                StrategyModeType::MultiTimeframe(_) => true,
            };
            return result;
        }
        false
    }
}

impl Default for StrategyFactory {
    fn default() -> Self {
        Self::with_defaults()
    }
}

// =================================================================
// Convenience functions
// =================================================================

/// Create a strategy using the default factory.
///
/// Returns the strategy wrapped in a `StrategyModeType` enum.
///
/// # Examples
/// ```ignore
/// use trading_common::strategy::create;
///
/// let strategy = create("sma").unwrap();
/// let strategy = create("trend").unwrap();
/// ```
pub fn create(strategy_id: &str) -> Result<StrategyModeType, String> {
    StrategyFactory::default().create(strategy_id)
}

/// List all strategies registered in the default factory.
pub fn list_all() -> Vec<StrategyInfo> {
    StrategyFactory::default().list()
}
