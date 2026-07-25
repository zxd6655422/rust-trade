pub(crate) mod base;
pub mod ma_trend_pullback;
pub mod multi_timeframe;

pub use base::{Signal, SignalIntent, Strategy};
pub use multi_timeframe::{
    EntryDirection, MultiTimeframeAnalysis, MultiTimeframeStrategy, TrendAnalysis, TrendDirection,
};

use ma_trend_pullback::MATrendPullbackBacktestStrategy;

#[derive(Debug, Clone)]
pub struct StrategyInfo {
    pub id: String,
    pub name: String,
    pub description: String,
    pub is_multi_timeframe: bool,
}

pub fn create_strategy(strategy_id: &str) -> Result<Box<dyn Strategy>, String> {
    match strategy_id {
        "ma_trend_pullback" => Ok(Box::new(MATrendPullbackBacktestStrategy::new())),
        _ => Err(format!("Unknown strategy: {}", strategy_id)),
    }
}

/// 创建多时间框架策略
pub fn create_multi_timeframe_strategy(
    strategy_id: &str,
) -> Result<Box<dyn MultiTimeframeStrategy>, String> {
    match strategy_id {
        _ => Err(format!("Unknown multi-timeframe strategy: {}", strategy_id)),
    }
}

pub fn list_strategies() -> Vec<StrategyInfo> {
    vec![
        StrategyInfo {
            id: "ma_trend_pullback".to_string(),
            name: "MA Trend Pullback".to_string(),
            description: "Dual MA trend pullback strategy: MA288/MA488 trend detection with MA288 crossover entry and trailing stop"
                .to_string(),
            is_multi_timeframe: false,
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
