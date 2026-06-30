pub(crate) mod base;
pub mod multi_timeframe;
mod rsi;
mod sma;
mod trend_strategy;

pub use base::{Signal, Strategy};
pub use multi_timeframe::{
    EntryDirection, MultiTimeframeAnalysis, MultiTimeframeStrategy, TrendAnalysis, TrendDirection,
};
pub use trend_strategy::TrendStrategy;

use rsi::RsiStrategy;
use sma::SmaStrategy;

#[derive(Debug, Clone)]
pub struct StrategyInfo {
    pub id: String,
    pub name: String,
    pub description: String,
    pub is_multi_timeframe: bool,
}

pub fn create_strategy(strategy_id: &str) -> Result<Box<dyn Strategy>, String> {
    match strategy_id {
        "sma" => Ok(Box::new(SmaStrategy::new())),
        "rsi" => Ok(Box::new(RsiStrategy::new())),
        _ => Err(format!("Unknown strategy: {}", strategy_id)),
    }
}

/// 创建多时间框架策略
pub fn create_multi_timeframe_strategy(
    strategy_id: &str,
) -> Result<Box<dyn MultiTimeframeStrategy>, String> {
    match strategy_id {
        "trend" => Ok(Box::new(TrendStrategy::new())),
        _ => Err(format!("Unknown multi-timeframe strategy: {}", strategy_id)),
    }
}

pub fn list_strategies() -> Vec<StrategyInfo> {
    vec![
        StrategyInfo {
            id: "sma".to_string(),
            name: "Simple Moving Average".to_string(),
            description: "Trading strategy based on short and long-term moving average crossover"
                .to_string(),
            is_multi_timeframe: false,
        },
        StrategyInfo {
            id: "rsi".to_string(),
            name: "RSI Strategy".to_string(),
            description: "Trading strategy based on Relative Strength Index (RSI)".to_string(),
            is_multi_timeframe: false,
        },
        StrategyInfo {
            id: "trend".to_string(),
            name: "Multi-Timeframe Trend".to_string(),
            description:
                "Multi-timeframe trend strategy: 4h for trend, 1h for confirmation, 15m for entry"
                    .to_string(),
            is_multi_timeframe: true,
        },
    ]
}

pub fn get_strategy_info(strategy_id: &str) -> Option<StrategyInfo> {
    list_strategies()
        .into_iter()
        .find(|info| info.id == strategy_id)
}

/// 检查策略是否是多时间框架策略
pub fn is_multi_timeframe_strategy(strategy_id: &str) -> bool {
    get_strategy_info(strategy_id)
        .map(|info| info.is_multi_timeframe)
        .unwrap_or(false)
}
