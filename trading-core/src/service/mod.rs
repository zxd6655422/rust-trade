pub mod aggregator;
pub mod backfill;
pub mod backtest_service;
pub mod errors;
pub mod market_data;
pub mod market_sentiment;
pub mod strategy_scheduler;
pub mod types;

// Re-export main interfaces
pub use aggregator::HighTfAggregator;
pub use backfill::BackfillService;
pub use backtest_service::BacktestService;
pub use errors::ServiceError;
pub use market_data::MarketDataService;
pub use market_sentiment::MarketSentimentCollector;
pub use strategy_scheduler::{StrategyAnalysisScheduler, StrategySchedulerConfig};
pub use types::*;
