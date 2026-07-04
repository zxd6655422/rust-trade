pub mod engine;
pub mod market_state;
pub mod metrics;
pub mod multi_symbol;
pub mod multi_timeframe_engine;
pub mod portfolio;
pub mod portfolio_tests;
pub mod strategy;
pub mod walk_forward;

pub use engine::{BacktestConfig, BacktestEngine, BacktestResult};
pub use market_state::{MarketStateAnalyzer, MarketStateReport};
pub use multi_symbol::{MultiSymbolBacktestEngine, MultiSymbolBacktestResult, SymbolBacktestResult};
pub use multi_timeframe_engine::MultiTimeframeBacktestEngine;
pub use portfolio::{Portfolio, Position, PositionSide, Trade};
pub use strategy::{create_strategy, list_strategies, Signal, Strategy, StrategyInfo};
pub use walk_forward::{
    OutOfSampleConfig, OutOfSampleResult, WalkForwardConfig, WalkForwardEngine, WalkForwardResult,
    WalkForwardRoundSummary,
};
