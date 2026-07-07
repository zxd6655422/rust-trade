use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use super::{Signal, SignalType, Strategy};
use crate::indicators;
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
        // 使用 indicators 模块计算 RSI
        let rsi_result = indicators::calculate_rsi(&data.klines, self.params.period)?;
        let rsi = rsi_result.value;
        let current_price = data.current_price;

        // 计算 ATR 用于止损
        let atr = indicators::calculate_atr(&data.klines, 14)
            .map(|r| r.value)
            .unwrap_or(current_price * 0.02);

        // 计算止损止盈
        let (stop_loss, take_profit) = if rsi < self.params.oversold {
            // 超卖区域，做多
            let stop_loss = current_price - 2.0 * atr;
            let take_profit = current_price + 3.0 * atr;
            (Some(stop_loss), Some(take_profit))
        } else if rsi > self.params.overbought {
            // 超买区域，做空
            let stop_loss = current_price + 2.0 * atr;
            let take_profit = current_price - 3.0 * atr;
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

        // 计算其他指标用于上下文
        let ma_fast = indicators::calculate_ma(&data.klines, 7).map(|r| r.value);
        let ma_slow = indicators::calculate_ma(&data.klines, 25).map(|r| r.value);

        let market_context = serde_json::json!({
            "rsi": rsi,
            "rsi_period": self.params.period,
            "overbought": self.params.overbought,
            "oversold": self.params.oversold,
            "atr": atr,
            "ma7": ma_fast,
            "ma25": ma_slow,
            "current_price": current_price,
            "kline_count": data.klines.len(),
            "timeframe": data.timeframe.as_str(),
        });

        if rsi < self.params.oversold && confirmed {
            Some(Signal {
                signal_type: SignalType::Buy,
                signal_strength,
                entry_price: current_price,
                stop_loss,
                take_profit,
                confidence: 0.7,
                reason: format!(
                    "RSI 超卖: {:.2} < {} (period={})",
                    rsi, self.params.oversold, self.params.period
                ),
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
                reason: format!(
                    "RSI 超买: {:.2} > {} (period={})",
                    rsi, self.params.overbought, self.params.period
                ),
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
        // 计算前一根 K 线的 RSI
        if let Some(prev_rsi) = indicators::calculate_rsi(
            &data.klines[..data.klines.len() - 1],
            self.params.period,
        ) {
            // 如果 RSI 在超卖区域且继续下降，或者在超买区域且继续上升，则确认
            if _current_rsi < self.params.oversold {
                _current_rsi <= prev_rsi.value // RSI 继续下降或持平
            } else if _current_rsi > self.params.overbought {
                _current_rsi >= prev_rsi.value // RSI 继续上升或持平
            } else {
                true
            }
        } else {
            true // 数据不足时默认确认
        }
    }
}
