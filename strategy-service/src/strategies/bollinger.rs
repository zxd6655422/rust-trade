use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use super::{Signal, SignalType, Strategy};
use crate::redis_reader::MarketData;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BollingerParams {
    pub period: usize,
    pub std_dev: f64,
    pub squeeze_threshold: f64,
}

pub struct BollingerStrategy {
    params: BollingerParams,
}

#[async_trait]
impl Strategy for BollingerStrategy {
    fn name(&self) -> &str {
        "bollinger"
    }

    async fn analyze(&self, data: &MarketData) -> Option<Signal> {
        let bollinger = data.bollinger.as_ref()?;
        let current_price = data.current_price;

        // 检查是否在挤压状态（布林带收窄）
        let is_squeeze = bollinger.bandwidth < self.params.squeeze_threshold;

        // 检查价格位置
        let percent_b = bollinger.percent_b;

        // 价格触及下轨（超卖）
        let at_lower_band = percent_b < 0.1;

        // 价格触及上轨（超买）
        let at_upper_band = percent_b > 0.9;

        if !at_lower_band && !at_upper_band {
            return None;
        }

        // 计算止损止盈
        let (stop_loss, take_profit) = if at_lower_band {
            let stop_loss = bollinger.lower * 0.98; // 下轨下方 2%
            let take_profit = bollinger.middle; // 中轨
            (Some(stop_loss), Some(take_profit))
        } else {
            let stop_loss = bollinger.upper * 1.02; // 上轨上方 2%
            let take_profit = bollinger.middle; // 中轨
            (Some(stop_loss), Some(take_profit))
        };

        // 计算信号强度
        let signal_strength = if at_lower_band {
            1.0 - percent_b // 越接近 0 越强
        } else {
            percent_b - 0.9 // 越接近 1 越强
        };
        let signal_strength = signal_strength.min(1.0).max(0.0);

        let market_context = serde_json::json!({
            "upper": bollinger.upper,
            "middle": bollinger.middle,
            "lower": bollinger.lower,
            "bandwidth": bollinger.bandwidth,
            "percent_b": percent_b,
            "is_squeeze": is_squeeze,
            "period": self.params.period,
            "std_dev": self.params.std_dev,
            "current_price": current_price,
        });

        if at_lower_band {
            Some(Signal {
                signal_type: SignalType::Buy,
                signal_strength,
                entry_price: current_price,
                stop_loss,
                take_profit,
                confidence: 0.65,
                reason: format!("布林带下轨触及: %B={:.2}, 价格接近下轨={:.2}", percent_b, bollinger.lower),
                market_context,
            })
        } else {
            Some(Signal {
                signal_type: SignalType::Sell,
                signal_strength,
                entry_price: current_price,
                stop_loss,
                take_profit,
                confidence: 0.65,
                reason: format!("布林带上轨触及: %B={:.2}, 价格接近上轨={:.2}", percent_b, bollinger.upper),
                market_context,
            })
        }
    }

    fn from_params(params: &serde_json::Value) -> anyhow::Result<Self> {
        let params: BollingerParams = serde_json::from_value(params.clone())?;
        Ok(Self { params })
    }
}
