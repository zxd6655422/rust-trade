use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use super::{Signal, SignalType, Strategy};
use crate::indicators;
use crate::redis_reader::MarketData;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrendParams {
    pub fast_ma: usize,
    pub slow_ma: usize,
    pub trend_ma: usize,
    pub adx_threshold: f64,
}

pub struct TrendStrategy {
    params: TrendParams,
}

#[async_trait]
impl Strategy for TrendStrategy {
    fn name(&self) -> &str {
        "trend"
    }

    async fn analyze(&self, data: &MarketData) -> Option<Signal> {
        let current_price = data.current_price;

        // 计算多均线
        let ma_periods = vec![self.params.fast_ma, self.params.slow_ma, self.params.trend_ma];
        let multi_ma = indicators::calculate_multi_ma(&data.klines, &ma_periods);

        // 获取均线值
        let ma_fast = multi_ma.values.iter().find(|(p, _)| *p == self.params.fast_ma).map(|(_, v)| *v)?;
        let ma_slow = multi_ma.values.iter().find(|(p, _)| *p == self.params.slow_ma).map(|(_, v)| *v)?;
        let ma_trend = multi_ma.values.iter().find(|(p, _)| *p == self.params.trend_ma).map(|(_, v)| *v)?;

        // 趋势判断
        let is_uptrend = ma_fast > ma_slow && ma_slow > ma_trend;
        let is_downtrend = ma_fast < ma_slow && ma_slow < ma_trend;

        if !is_uptrend && !is_downtrend {
            return None;
        }

        // 计算 ADX（如果数据足够）
        let adx_result = indicators::calculate_adx(&data.klines, 14);
        let adx_value = adx_result.map(|r| r.adx).unwrap_or(0.0);

        // ADX 过滤：只在趋势明确时交易
        if adx_value < self.params.adx_threshold {
            return None;
        }

        // 检查价格回调/反弹
        let closes: Vec<f64> = data.klines.iter().map(|k| k.close).collect();
        let recent_closes = if closes.len() >= 5 {
            &closes[closes.len() - 5..]
        } else {
            &closes
        };

        // 计算最近价格相对于 MA 的位置
        let price_vs_ma_slow = (current_price - ma_slow) / ma_slow;

        // 回调确认：价格从 MA 附近反弹
        let is_pullback_buy = is_uptrend && price_vs_ma_slow.abs() < 0.02 && current_price > *recent_closes.last().unwrap_or(&0.0);
        let is_pullback_sell = is_downtrend && price_vs_ma_slow.abs() < 0.02 && current_price < *recent_closes.last().unwrap_or(&0.0);

        if !is_pullback_buy && !is_pullback_sell {
            return None;
        }

        // 计算 ATR 用于止损
        let atr = indicators::calculate_atr(&data.klines, 14).map(|r| r.value).unwrap_or(current_price * 0.02);

        // 计算止损止盈
        let (stop_loss, take_profit) = if is_pullback_buy {
            let stop_loss = current_price - 2.0 * atr; // 2 倍 ATR 止损
            let take_profit = current_price + 3.0 * atr; // 3 倍 ATR 止盈
            (Some(stop_loss), Some(take_profit))
        } else {
            let stop_loss = current_price + 2.0 * atr;
            let take_profit = current_price - 3.0 * atr;
            (Some(stop_loss), Some(take_profit))
        };

        // 计算信号强度
        let signal_strength = if is_uptrend {
            ((ma_fast - ma_trend) / ma_trend * 10.0).min(1.0).max(0.0)
        } else {
            ((ma_trend - ma_fast) / ma_trend * 10.0).min(1.0).max(0.0)
        };

        let market_context = serde_json::json!({
            "ma_fast": ma_fast,
            "ma_slow": ma_slow,
            "ma_trend": ma_trend,
            "adx": adx_value,
            "atr": atr,
            "price_vs_ma_slow": price_vs_ma_slow,
            "is_uptrend": is_uptrend,
            "is_downtrend": is_downtrend,
            "fast_ma_period": self.params.fast_ma,
            "slow_ma_period": self.params.slow_ma,
            "trend_ma_period": self.params.trend_ma,
            "current_price": current_price,
        });

        if is_pullback_buy {
            Some(Signal {
                signal_type: SignalType::Buy,
                signal_strength,
                entry_price: current_price,
                stop_loss,
                take_profit,
                confidence: 0.7,
                reason: format!(
                    "上升趋势回调买入: MA{}={:.2} > MA{}={:.2} > MA{}={:.2}, ADX={:.1}, 价格接近MA{}",
                    self.params.fast_ma, ma_fast,
                    self.params.slow_ma, ma_slow,
                    self.params.trend_ma, ma_trend,
                    adx_value,
                    self.params.slow_ma,
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
                confidence: 0.7,
                reason: format!(
                    "下降趋势反弹卖出: MA{}={:.2} < MA{}={:.2} < MA{}={:.2}, ADX={:.1}, 价格接近MA{}",
                    self.params.fast_ma, ma_fast,
                    self.params.slow_ma, ma_slow,
                    self.params.trend_ma, ma_trend,
                    adx_value,
                    self.params.slow_ma,
                ),
                market_context,
            })
        }
    }

    fn from_params(params: &serde_json::Value) -> anyhow::Result<Self> {
        let params: TrendParams = serde_json::from_value(params.clone())?;
        Ok(Self { params })
    }
}
