pub mod engine;
pub mod metrics;
pub mod multi_timeframe_engine;
pub mod portfolio;
pub mod strategy;
pub mod walk_forward;

pub use engine::{BacktestConfig, BacktestEngine, BacktestResult};
pub use multi_timeframe_engine::MultiTimeframeBacktestEngine;
pub use portfolio::{Portfolio, Position, PositionSide, Trade};
pub use strategy::{create_strategy, list_strategies, Signal, Strategy, StrategyInfo};
pub use walk_forward::{
    OutOfSampleConfig, OutOfSampleResult, WalkForwardConfig, WalkForwardEngine, WalkForwardResult,
    WalkForwardRoundSummary,
};
