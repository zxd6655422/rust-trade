use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use super::{Signal, SignalType, Strategy};
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
        let ma_data = data.ma.as_ref()?;
        let current_price = data.current_price;

        // 使用 MA 数据判断趋势
        let ma7 = ma_data.ma7;
        let ma25 = ma_data.ma25;
        let ma99 = ma_data.ma99;

        // 趋势判断
        let is_uptrend = ma7 > ma25 && ma25 > ma99;
        let is_downtrend = ma7 < ma25 && ma25 < ma99;

        if !is_uptrend && !is_downtrend {
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
        let price_vs_ma25 = (current_price - ma25) / ma25;

        // 回调确认：价格从 MA25 附近反弹
        let is_pullback_buy = is_uptrend && price_vs_ma25.abs() < 0.02 && current_price > *recent_closes.last().unwrap_or(&0.0);
        let is_pullback_sell = is_downtrend && price_vs_ma25.abs() < 0.02 && current_price < *recent_closes.last().unwrap_or(&0.0);

        if !is_pullback_buy && !is_pullback_sell {
            return None;
        }

        // 计算止损止盈
        let (stop_loss, take_profit) = if is_pullback_buy {
            let stop_loss = ma25 * 0.97; // MA25 下方 3%
            let take_profit = current_price * 1.05; // 5% 止盈
            (Some(stop_loss), Some(take_profit))
        } else {
            let stop_loss = ma25 * 1.03; // MA25 上方 3%
            let take_profit = current_price * 0.95; // 5% 止盈
            (Some(stop_loss), Some(take_profit))
        };

        // 计算信号强度
        let signal_strength = if is_uptrend {
            (ma7 - ma99) / ma99 * 10.0 // 趋势强度
        } else {
            (ma99 - ma7) / ma99 * 10.0
        };
        let signal_strength = signal_strength.min(1.0).max(0.0);

        let market_context = serde_json::json!({
            "ma7": ma7,
            "ma25": ma25,
            "ma99": ma99,
            "price_vs_ma25": price_vs_ma25,
            "is_uptrend": is_uptrend,
            "is_downtrend": is_downtrend,
            "fast_ma": self.params.fast_ma,
            "slow_ma": self.params.slow_ma,
            "trend_ma": self.params.trend_ma,
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
                reason: format!("上升趋势回调买入: MA7={:.2} > MA25={:.2} > MA99={:.2}, 价格接近MA25", ma7, ma25, ma99),
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
                reason: format!("下降趋势反弹卖出: MA7={:.2} < MA25={:.2} < MA99={:.2}, 价格接近MA25", ma7, ma25, ma99),
                market_context,
            })
        }
    }

    fn from_params(params: &serde_json::Value) -> anyhow::Result<Self> {
        let params: TrendParams = serde_json::from_value(params.clone())?;
        Ok(Self { params })
    }
}
