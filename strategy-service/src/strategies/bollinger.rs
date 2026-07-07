use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use super::{Signal, SignalType, Strategy};
use crate::indicators;
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
        // 使用 indicators 模块计算布林带
        let bollinger = indicators::calculate_bollinger(
            &data.klines,
            self.params.period,
            self.params.std_dev,
        )?;

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

        // 计算其他指标用于上下文
        let rsi = indicators::calculate_rsi(&data.klines, 14).map(|r| r.value);
        let ma_fast = indicators::calculate_ma(&data.klines, 7).map(|r| r.value);

        let market_context = serde_json::json!({
            "upper": bollinger.upper,
            "middle": bollinger.middle,
            "lower": bollinger.lower,
            "bandwidth": bollinger.bandwidth,
            "percent_b": percent_b,
            "is_squeeze": is_squeeze,
            "period": self.params.period,
            "std_dev": self.params.std_dev,
            "rsi": rsi,
            "ma7": ma_fast,
            "current_price": current_price,
            "kline_count": data.klines.len(),
            "timeframe": data.timeframe.as_str(),
        });

        if at_lower_band {
            Some(Signal {
                signal_type: SignalType::Buy,
                signal_strength,
                entry_price: current_price,
                stop_loss,
                take_profit,
                confidence: 0.65,
                reason: format!(
                    "布林带下轨触及: %B={:.2}, 价格接近下轨={:.2} ({}/{})",
                    percent_b, bollinger.lower,
                    self.params.period, self.params.std_dev,
                ),
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
                reason: format!(
                    "布林带上轨触及: %B={:.2}, 价格接近上轨={:.2} ({}/{})",
                    percent_b, bollinger.upper,
                    self.params.period, self.params.std_dev,
                ),
                market_context,
            })
        }
    }

    fn from_params(params: &serde_json::Value) -> anyhow::Result<Self> {
        let params: BollingerParams = serde_json::from_value(params.clone())?;
        Ok(Self { params })
    }
}
