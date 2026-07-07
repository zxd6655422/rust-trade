use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use super::{Signal, SignalType, Strategy};
use crate::indicators;
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
        // 使用 indicators 模块计算 MACD
        let macd_result = indicators::calculate_macd(
            &data.klines,
            self.params.fast_period,
            self.params.slow_period,
            self.params.signal_period,
        )?;

        let macd = macd_result.macd;
        let signal = macd_result.signal;
        let histogram = macd_result.histogram;
        let current_price = data.current_price;

        // MACD 金叉：MACD 线上穿信号线
        let is_golden_cross = macd > signal && histogram > self.params.histogram_threshold;

        // MACD 死叉：MACD 线下穿信号线
        let is_death_cross = macd < signal && histogram < -self.params.histogram_threshold;

        if !is_golden_cross && !is_death_cross {
            return None;
        }

        // 计算 ATR 用于止损
        let atr = indicators::calculate_atr(&data.klines, 14)
            .map(|r| r.value)
            .unwrap_or(current_price * 0.02);

        // 计算止损止盈
        let (stop_loss, take_profit) = if is_golden_cross {
            let stop_loss = current_price - 2.0 * atr;
            let take_profit = current_price + 3.0 * atr;
            (Some(stop_loss), Some(take_profit))
        } else {
            let stop_loss = current_price + 2.0 * atr;
            let take_profit = current_price - 3.0 * atr;
            (Some(stop_loss), Some(take_profit))
        };

        // 计算信号强度
        let signal_strength = (histogram.abs() / (current_price * 0.01)).min(1.0);

        // 计算其他指标用于上下文
        let ma_fast = indicators::calculate_ma(&data.klines, 7).map(|r| r.value);
        let ma_slow = indicators::calculate_ma(&data.klines, 25).map(|r| r.value);
        let rsi = indicators::calculate_rsi(&data.klines, 14).map(|r| r.value);

        let market_context = serde_json::json!({
            "macd": macd,
            "signal": signal,
            "histogram": histogram,
            "fast_period": self.params.fast_period,
            "slow_period": self.params.slow_period,
            "signal_period": self.params.signal_period,
            "atr": atr,
            "ma7": ma_fast,
            "ma25": ma_slow,
            "rsi": rsi,
            "current_price": current_price,
            "kline_count": data.klines.len(),
            "timeframe": data.timeframe.as_str(),
        });

        if is_golden_cross {
            Some(Signal {
                signal_type: SignalType::Buy,
                signal_strength,
                entry_price: current_price,
                stop_loss,
                take_profit,
                confidence: 0.75,
                reason: format!(
                    "MACD 金叉: MACD={:.4}, Signal={:.4}, Hist={:.4} ({}/{}/{})",
                    macd, signal, histogram,
                    self.params.fast_period,
                    self.params.slow_period,
                    self.params.signal_period,
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
                confidence: 0.75,
                reason: format!(
                    "MACD 死叉: MACD={:.4}, Signal={:.4}, Hist={:.4} ({}/{}/{})",
                    macd, signal, histogram,
                    self.params.fast_period,
                    self.params.slow_period,
                    self.params.signal_period,
                ),
                market_context,
            })
        }
    }

    fn from_params(params: &serde_json::Value) -> anyhow::Result<Self> {
        let params: MacdParams = serde_json::from_value(params.clone())?;
        Ok(Self { params })
    }
}
