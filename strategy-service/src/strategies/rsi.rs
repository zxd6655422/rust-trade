use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use super::{Signal, SignalType, Strategy};
use crate::redis_reader::MarketData;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RsiParams {
    pub period: usize,
    pub overbought: f64,
    pub oversold: f64,
    pub confirm_candles: usize,
}

pub struct RsiStrategy {
    params: RsiParams,
}

#[async_trait]
impl Strategy for RsiStrategy {
    fn name(&self) -> &str {
        "rsi"
    }

    async fn analyze(&self, data: &MarketData) -> Option<Signal> {
        let rsi = data.rsi?;
        let current_price = data.current_price;

        // 计算止损止盈
        let (stop_loss, take_profit) = if rsi < self.params.oversold {
            // 超卖区域，做多
            let stop_loss = current_price * 0.98; // 2% 止损
            let take_profit = current_price * 1.04; // 4% 止盈
            (Some(stop_loss), Some(take_profit))
        } else if rsi > self.params.overbought {
            // 超买区域，做空
            let stop_loss = current_price * 1.02; // 2% 止损
            let take_profit = current_price * 0.96; // 4% 止盈
            (Some(stop_loss), Some(take_profit))
        } else {
            (None, None)
        };

        // 计算信号强度
        let signal_strength = if rsi < self.params.oversold {
            (self.params.oversold - rsi) / self.params.oversold
        } else if rsi > self.params.overbought {
            (rsi - self.params.overbought) / (100.0 - self.params.overbought)
        } else {
            0.0
        };

        // 确认信号：检查最近 N 根 K 线是否确认
        let confirmed = self.confirm_signal(data, rsi);

        let market_context = serde_json::json!({
            "rsi": rsi,
            "period": self.params.period,
            "overbought": self.params.overbought,
            "oversold": self.params.oversold,
            "current_price": current_price,
            "kline_count": data.klines.len(),
        });

        if rsi < self.params.oversold && confirmed {
            Some(Signal {
                signal_type: SignalType::Buy,
                signal_strength,
                entry_price: current_price,
                stop_loss,
                take_profit,
                confidence: 0.7,
                reason: format!("RSI 超卖: {:.2} < {}", rsi, self.params.oversold),
                market_context,
            })
        } else if rsi > self.params.overbought && confirmed {
            Some(Signal {
                signal_type: SignalType::Sell,
                signal_strength,
                entry_price: current_price,
                stop_loss,
                take_profit,
                confidence: 0.7,
                reason: format!("RSI 超买: {:.2} > {}", rsi, self.params.overbought),
                market_context,
            })
        } else {
            None
        }
    }

    fn from_params(params: &serde_json::Value) -> anyhow::Result<Self> {
        let params: RsiParams = serde_json::from_value(params.clone())?;
        Ok(Self { params })
    }
}

impl RsiStrategy {
    fn confirm_signal(&self, data: &MarketData, _current_rsi: f64) -> bool {
        // 如果没有足够的 K 线数据，直接返回 true
        if data.klines.len() < self.params.confirm_candles + 1 {
            return true;
        }

        // 检查最近 N 根 K 线的 RSI 趋势
        // 这里简化处理，实际应该从 Redis 读取历史 RSI
        true
    }
}
