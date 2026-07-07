use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use super::{Signal, SignalType, Strategy};
use crate::indicators;
use crate::redis_reader::MarketData;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VolumeParams {
    pub volume_ma_period: usize,
    pub volume_spike_threshold: f64,
    pub price_change_threshold: f64,
}

pub struct VolumeStrategy {
    params: VolumeParams,
}

#[async_trait]
impl Strategy for VolumeStrategy {
    fn name(&self) -> &str {
        "volume"
    }

    async fn analyze(&self, data: &MarketData) -> Option<Signal> {
        if data.klines.len() < self.params.volume_ma_period + 1 {
            return None;
        }

        let current_price = data.current_price;

        // 计算成交量移动平均
        let volumes: Vec<f64> = data.klines.iter().map(|k| k.volume).collect();
        let recent_volumes = &volumes[volumes.len() - self.params.volume_ma_period..];
        let volume_ma: f64 = recent_volumes.iter().sum::<f64>() / self.params.volume_ma_period as f64;

        // 当前成交量
        let current_volume = *volumes.last().unwrap_or(&0.0);

        // 成交量倍数
        let volume_ratio = if volume_ma > 0.0 {
            current_volume / volume_ma
        } else {
            0.0
        };

        // 价格变化
        let closes: Vec<f64> = data.klines.iter().map(|k| k.close).collect();
        let price_change = if closes.len() >= 2 {
            let prev = closes[closes.len() - 2];
            let curr = closes[closes.len() - 1];
            (curr - prev) / prev
        } else {
            0.0
        };

        // 成交量放大 + 价格变动 = 信号
        let is_volume_spike = volume_ratio >= self.params.volume_spike_threshold;
        let is_price_up = price_change > self.params.price_change_threshold;
        let is_price_down = price_change < -self.params.price_change_threshold;

        if !is_volume_spike {
            return None;
        }

        // 计算 ATR 用于止损
        let atr = indicators::calculate_atr(&data.klines, 14)
            .map(|r| r.value)
            .unwrap_or(current_price * 0.02);

        // 计算止损止盈
        let (stop_loss, take_profit) = if is_price_up {
            let stop_loss = current_price - 2.0 * atr;
            let take_profit = current_price + 3.0 * atr;
            (Some(stop_loss), Some(take_profit))
        } else if is_price_down {
            let stop_loss = current_price + 2.0 * atr;
            let take_profit = current_price - 3.0 * atr;
            (Some(stop_loss), Some(take_profit))
        } else {
            return None;
        };

        // 计算信号强度
        let signal_strength = ((volume_ratio - 1.0) / self.params.volume_spike_threshold).min(1.0);

        // 计算其他指标用于上下文
        let rsi = indicators::calculate_rsi(&data.klines, 14).map(|r| r.value);
        let ma_fast = indicators::calculate_ma(&data.klines, 7).map(|r| r.value);
        let ma_slow = indicators::calculate_ma(&data.klines, 25).map(|r| r.value);

        let market_context = serde_json::json!({
            "current_volume": current_volume,
            "volume_ma": volume_ma,
            "volume_ratio": volume_ratio,
            "price_change": price_change,
            "volume_ma_period": self.params.volume_ma_period,
            "price_change_threshold": self.params.price_change_threshold,
            "atr": atr,
            "rsi": rsi,
            "ma7": ma_fast,
            "ma25": ma_slow,
            "current_price": current_price,
            "kline_count": data.klines.len(),
            "timeframe": data.timeframe.as_str(),
        });

        if is_price_up {
            Some(Signal {
                signal_type: SignalType::Buy,
                signal_strength,
                entry_price: current_price,
                stop_loss,
                take_profit,
                confidence: 0.6,
                reason: format!(
                    "成交量放大+价格上涨: 量比={:.2}, 涨幅={:.2}%, ATR={:.2}",
                    volume_ratio, price_change * 100.0, atr,
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
                confidence: 0.6,
                reason: format!(
                    "成交量放大+价格下跌: 量比={:.2}, 跌幅={:.2}%, ATR={:.2}",
                    volume_ratio, price_change * 100.0, atr,
                ),
                market_context,
            })
        }
    }

    fn from_params(params: &serde_json::Value) -> anyhow::Result<Self> {
        let params: VolumeParams = serde_json::from_value(params.clone())?;
        Ok(Self { params })
    }
}
