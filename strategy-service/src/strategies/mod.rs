pub mod rsi;
pub mod macd;
pub mod bollinger;
pub mod volume;
pub mod trend;
pub mod multi_tf;

use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use crate::redis_reader::MarketData;

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

    /// 分析市场数据并返回信号
    async fn analyze(&self, data: &MarketData) -> Option<Signal>;

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
        _ => Err(anyhow::anyhow!("Unknown strategy type: {}", strategy_type)),
    }
}
