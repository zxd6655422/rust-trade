pub mod rsi;
pub mod macd;
pub mod bollinger;
pub mod volume;
pub mod trend;
pub mod multi_tf;
pub mod macro_cycle;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::redis_reader::{MarketData, MultiTimeframeData, Timeframe};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Signal {
    pub signal_type: SignalType,
    pub signal_strength: f64,
    pub entry_price: f64,
    pub stop_loss: Option<f64>,
    pub take_profit: Option<f64>,
    pub confidence: f64,
    pub reason: String,
    pub market_context: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum SignalType {
    Buy,
    Sell,
    Hold,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StrategyParams {
    pub strategy_type: String,
    pub params: serde_json::Value,
}

#[async_trait]
pub trait Strategy: Send + Sync {
    /// 策略名称
    fn name(&self) -> &str;

    /// 分析市场数据并返回信号（单时间框架，向后兼容）
    async fn analyze(&self, data: &MarketData) -> Option<Signal>;

    /// 多时间框架分析（可选实现）
    ///
    /// 默认实现：使用主时间框架数据调用单时间框架分析
    /// 子类可以覆盖此方法实现真正的多时间框架分析
    async fn analyze_multi_tf(&self, data: &MultiTimeframeData) -> Option<Signal> {
        self.analyze(&data.primary).await
    }

    /// 获取策略需要的时间框架列表
    ///
    /// 默认返回单时间框架
    /// 多时间框架策略应覆盖此方法
    fn required_timeframes(&self) -> Vec<Timeframe> {
        vec![Timeframe::OneMinute]
    }

    /// 从 JSON 参数创建策略实例
    fn from_params(params: &serde_json::Value) -> anyhow::Result<Self>
    where
        Self: Sized;
}

/// 根据策略类型创建策略实例
pub fn create_strategy(
    strategy_type: &str,
    params: &serde_json::Value,
) -> anyhow::Result<Box<dyn Strategy>> {
    match strategy_type {
        "rsi" => Ok(Box::new(rsi::RsiStrategy::from_params(params)?)),
        "macd" => Ok(Box::new(macd::MacdStrategy::from_params(params)?)),
        "bollinger" => Ok(Box::new(bollinger::BollingerStrategy::from_params(params)?)),
        "volume" => Ok(Box::new(volume::VolumeStrategy::from_params(params)?)),
        "trend" => Ok(Box::new(trend::TrendStrategy::from_params(params)?)),
        "multi_tf" => Ok(Box::new(multi_tf::MultiTimeframeStrategy::from_params(params)?)),
        "macro_cycle" => Ok(Box::new(macro_cycle::MacroCycleStrategy::from_params(params)?)),
        _ => Err(anyhow::anyhow!("Unknown strategy type: {}", strategy_type)),
    }
}

/// 获取策略类型需要的时间框架
pub fn get_strategy_timeframes(strategy_type: &str, params: &serde_json::Value) -> Vec<Timeframe> {
    match strategy_type {
        "multi_tf" => {
            // 多时间框架策略：从参数中读取
            if let Some(tfs) = params.get("timeframes").and_then(|v| v.as_array()) {
                let mut timeframes: Vec<Timeframe> = tfs
                    .iter()
                    .filter_map(|v| v.as_str().and_then(Timeframe::from_str))
                    .collect();
                timeframes.sort_by_key(|tf| tf.level());
                timeframes
            } else {
                // 默认：1h + 4h + 1d
                vec![Timeframe::OneHour, Timeframe::FourHour, Timeframe::OneDay]
            }
        }
        "macro_cycle" => {
            // 大周期策略
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
            // 趋势策略
            vec![Timeframe::OneHour, Timeframe::FourHour]
        }
        _ => {
            // 其他策略：默认使用单一时间框架
            vec![Timeframe::OneMinute]
        }
    }
}
