use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use super::{Signal, SignalType, Strategy};
use crate::redis_reader::MarketData;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MultiTfParams {
    pub timeframes: Vec<String>,
    pub min_agreement: usize,
    pub weight_h4: f64,
    pub weight_d1: f64,
    pub weight_h1: f64,
}

pub struct MultiTimeframeStrategy {
    params: MultiTfParams,
}

#[async_trait]
impl Strategy for MultiTimeframeStrategy {
    fn name(&self) -> &str {
        "multi_tf"
    }

    async fn analyze(&self, data: &MarketData) -> Option<Signal> {
        let ma_data = data.ma.as_ref()?;
        let current_price = data.current_price;

        // 简化版本：使用单时间框架的 MA 数据模拟多时间框架分析
        // 实际应该从 Redis 读取不同时间框架的指标数据
        let ma7 = ma_data.ma7;
        let ma25 = ma_data.ma25;
        let ma99 = ma_data.ma99;

        // 模拟不同时间框架的趋势判断
        let h1_trend = if ma7 > ma25 { 1.0 } else { -1.0 };
        let h4_trend = if ma25 > ma99 { 1.0 } else { -1.0 };
        let d1_trend = if ma7 > ma99 { 1.0 } else { -1.0 };

        // 加权计算
        let weighted_score = h1_trend * self.params.weight_h1
            + h4_trend * self.params.weight_h4
            + d1_trend * self.params.weight_d1;

        // 判断一致性
        let bullish_count = [h1_trend, h4_trend, d1_trend].iter().filter(|&&t| t > 0.0).count();
        let bearish_count = [h1_trend, h4_trend, d1_trend].iter().filter(|&&t| t < 0.0).count();

        let is_bullish = bullish_count >= self.params.min_agreement && weighted_score > 0.0;
        let is_bearish = bearish_count >= self.params.min_agreement && weighted_score < 0.0;

        if !is_bullish && !is_bearish {
            return None;
        }

        // 计算止损止盈
        let (stop_loss, take_profit) = if is_bullish {
            let stop_loss = current_price * 0.95; // 5% 止损（多时间框架策略止损更宽）
            let take_profit = current_price * 1.10; // 10% 止盈
            (Some(stop_loss), Some(take_profit))
        } else {
            let stop_loss = current_price * 1.05; // 5% 止损
            let take_profit = current_price * 0.90; // 10% 止盈
            (Some(stop_loss), Some(take_profit))
        };

        // 计算信号强度
        let signal_strength = (weighted_score.abs() / 3.0).min(1.0);

        let market_context = serde_json::json!({
            "h1_trend": h1_trend,
            "h4_trend": h4_trend,
            "d1_trend": d1_trend,
            "weighted_score": weighted_score,
            "bullish_count": bullish_count,
            "bearish_count": bearish_count,
            "min_agreement": self.params.min_agreement,
            "ma7": ma7,
            "ma25": ma25,
            "ma99": ma99,
            "current_price": current_price,
        });

        if is_bullish {
            Some(Signal {
                signal_type: SignalType::Buy,
                signal_strength,
                entry_price: current_price,
                stop_loss,
                take_profit,
                confidence: 0.8,
                reason: format!(
                    "多时间框架看多: H1={}, H4={}, D1={}, 一致性={}/{}",
                    if h1_trend > 0.0 { "↑" } else { "↓" },
                    if h4_trend > 0.0 { "↑" } else { "↓" },
                    if d1_trend > 0.0 { "↑" } else { "↓" },
                    bullish_count,
                    self.params.min_agreement
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
                confidence: 0.8,
                reason: format!(
                    "多时间框架看空: H1={}, H4={}, D1={}, 一致性={}/{}",
                    if h1_trend > 0.0 { "↑" } else { "↓" },
                    if h4_trend > 0.0 { "↑" } else { "↓" },
                    if d1_trend > 0.0 { "↑" } else { "↓" },
                    bearish_count,
                    self.params.min_agreement
                ),
                market_context,
            })
        }
    }

    fn from_params(params: &serde_json::Value) -> anyhow::Result<Self> {
        let params: MultiTfParams = serde_json::from_value(params.clone())?;
        Ok(Self { params })
    }
}
