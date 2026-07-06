use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use super::{Signal, SignalType, Strategy};
use crate::redis_reader::MarketData;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MacdParams {
    pub fast_period: usize,
    pub slow_period: usize,
    pub signal_period: usize,
    pub histogram_threshold: f64,
}

pub struct MacdStrategy {
    params: MacdParams,
}

#[async_trait]
impl Strategy for MacdStrategy {
    fn name(&self) -> &str {
        "macd"
    }

    async fn analyze(&self, data: &MarketData) -> Option<Signal> {
        let macd_data = data.macd.as_ref()?;
        let current_price = data.current_price;

        let macd = macd_data.macd;
        let signal = macd_data.signal;
        let histogram = macd_data.hist;

        // MACD 金叉：MACD 线上穿信号线
        let is_golden_cross = macd > signal && histogram > self.params.histogram_threshold;

        // MACD 死叉：MACD 线下穿信号线
        let is_death_cross = macd < signal && histogram < -self.params.histogram_threshold;

        if !is_golden_cross && !is_death_cross {
            return None;
        }

        // 计算止损止盈
        let (stop_loss, take_profit) = if is_golden_cross {
            let stop_loss = current_price * 0.97; // 3% 止损
            let take_profit = current_price * 1.06; // 6% 止盈
            (Some(stop_loss), Some(take_profit))
        } else {
            let stop_loss = current_price * 1.03; // 3% 止损
            let take_profit = current_price * 0.94; // 6% 止盈
            (Some(stop_loss), Some(take_profit))
        };

        // 计算信号强度
        let signal_strength = if is_golden_cross {
            histogram.abs() / (current_price * 0.01) // 归一化
        } else {
            histogram.abs() / (current_price * 0.01)
        };
        let signal_strength = signal_strength.min(1.0);

        let market_context = serde_json::json!({
            "macd": macd,
            "signal": signal,
            "histogram": histogram,
            "fast_period": self.params.fast_period,
            "slow_period": self.params.slow_period,
            "signal_period": self.params.signal_period,
            "current_price": current_price,
        });

        if is_golden_cross {
            Some(Signal {
                signal_type: SignalType::Buy,
                signal_strength,
                entry_price: current_price,
                stop_loss,
                take_profit,
                confidence: 0.75,
                reason: format!("MACD 金叉: MACD={:.4}, Signal={:.4}, Hist={:.4}", macd, signal, histogram),
                market_context,
            })
        } else {
            Some(Signal {
                signal_type: SignalType::Sell,
                signal_strength,
                entry_price: current_price,
                stop_loss,
                take_profit,
                confidence: 0.75,
                reason: format!("MACD 死叉: MACD={:.4}, Signal={:.4}, Hist={:.4}", macd, signal, histogram),
                market_context,
            })
        }
    }

    fn from_params(params: &serde_json::Value) -> anyhow::Result<Self> {
        let params: MacdParams = serde_json::from_value(params.clone())?;
        Ok(Self { params })
    }
}
