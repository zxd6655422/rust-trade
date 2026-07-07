use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use super::{Signal, SignalType, Strategy};
use crate::indicators;
use crate::redis_reader::{MarketData, Timeframe};

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

/// 单个时间框架的趋势分析结果
#[derive(Debug, Clone)]
struct TimeframeAnalysis {
    timeframe: String,
    trend: f64,       // 1.0 = 上涨, -1.0 = 下跌, 0.0 = 中性
    strength: f64,    // 趋势强度 0.0 - 1.0
    ma_fast: f64,
    ma_slow: f64,
    adx: f64,
}

impl MultiTimeframeStrategy {
    /// 分析单个时间框架的趋势
    fn analyze_timeframe(&self, klines: &[crate::redis_reader::KlineData], tf: &str) -> Option<TimeframeAnalysis> {
        if klines.len() < 100 {
            return None;
        }

        // 根据时间框架选择不同的均线参数
        let (fast_period, slow_period) = match tf {
            "1h" => (7, 25),
            "4h" => (20, 50),
            "1d" => (10, 30),
            _ => (7, 25),
        };

        let ma_fast = indicators::calculate_ma(klines, fast_period).map(|r| r.value)?;
        let ma_slow = indicators::calculate_ma(klines, slow_period).map(|r| r.value)?;

        // 计算 ADX
        let adx = indicators::calculate_adx(klines, 14).map(|r| r.adx).unwrap_or(0.0);

        // 趋势判断
        let trend = if ma_fast > ma_slow {
            1.0
        } else if ma_fast < ma_slow {
            -1.0
        } else {
            0.0
        };

        // 趋势强度：基于 ADX 和均线差距
        let ma_diff_pct = ((ma_fast - ma_slow) / ma_slow).abs();
        let strength = (adx / 100.0 * 0.7 + ma_diff_pct * 10.0 * 0.3).min(1.0);

        Some(TimeframeAnalysis {
            timeframe: tf.to_string(),
            trend,
            strength,
            ma_fast,
            ma_slow,
            adx,
        })
    }
}

#[async_trait]
impl Strategy for MultiTimeframeStrategy {
    fn name(&self) -> &str {
        "multi_tf"
    }

    async fn analyze(&self, data: &MarketData) -> Option<Signal> {
        let current_price = data.current_price;

        // 分析当前时间框架
        let current_tf_analysis = self.analyze_timeframe(&data.klines, data.timeframe.as_str());

        // 注意：这里只能分析当前时间框架的数据
        // 完整的多时间框架分析需要从 Redis 读取多个时间框架的 K 线
        // 这部分需要在 engine.rs 中实现，传递多个时间框架的数据

        // 简化版本：使用单时间框架的多均线模拟
        let ma_periods = vec![7, 25, 50, 99];
        let multi_ma = indicators::calculate_multi_ma(&data.klines, &ma_periods);

        let ma7 = multi_ma.values.iter().find(|(p, _)| *p == 7).map(|(_, v)| *v)?;
        let ma25 = multi_ma.values.iter().find(|(p, _)| *p == 25).map(|(_, v)| *v)?;
        let ma50 = multi_ma.values.iter().find(|(p, _)| *p == 50).map(|(_, v)| *v).unwrap_or(ma25);
        let ma99 = multi_ma.values.iter().find(|(p, _)| *p == 99).map(|(_, v)| *v).unwrap_or(ma50);

        // 模拟不同时间框架的趋势判断
        let h1_trend = if ma7 > ma25 { 1.0 } else if ma7 < ma25 { -1.0 } else { 0.0 };
        let h4_trend = if ma25 > ma50 { 1.0 } else if ma25 < ma50 { -1.0 } else { 0.0 };
        let d1_trend = if ma50 > ma99 { 1.0 } else if ma50 < ma99 { -1.0 } else { 0.0 };

        // 计算 ADX
        let adx = indicators::calculate_adx(&data.klines, 14).map(|r| r.adx).unwrap_or(0.0);

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

        // ADX 过滤：只在趋势明确时交易
        if adx < 20.0 {
            return None;
        }

        // 计算 ATR 用于止损
        let atr = indicators::calculate_atr(&data.klines, 14)
            .map(|r| r.value)
            .unwrap_or(current_price * 0.02);

        // 计算止损止盈（多时间框架策略止损更宽）
        let (stop_loss, take_profit) = if is_bullish {
            let stop_loss = current_price - 3.0 * atr;
            let take_profit = current_price + 5.0 * atr;
            (Some(stop_loss), Some(take_profit))
        } else {
            let stop_loss = current_price + 3.0 * atr;
            let take_profit = current_price - 5.0 * atr;
            (Some(stop_loss), Some(take_profit))
        };

        // 计算信号强度
        let signal_strength = (weighted_score.abs() / 3.0 * (adx / 50.0)).min(1.0);

        let market_context = serde_json::json!({
            "h1_trend": h1_trend,
            "h4_trend": h4_trend,
            "d1_trend": d1_trend,
            "weighted_score": weighted_score,
            "bullish_count": bullish_count,
            "bearish_count": bearish_count,
            "min_agreement": self.params.min_agreement,
            "adx": adx,
            "atr": atr,
            "ma7": ma7,
            "ma25": ma25,
            "ma50": ma50,
            "ma99": ma99,
            "current_price": current_price,
            "kline_count": data.klines.len(),
            "timeframe": data.timeframe.as_str(),
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
                    "多时间框架看多: MA7={:.2}>MA25={:.2}>MA50={:.2}, ADX={:.1}, 一致性={}/{}",
                    ma7, ma25, ma50, adx,
                    bullish_count, self.params.min_agreement,
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
                    "多时间框架看空: MA7={:.2}<MA25={:.2}<MA50={:.2}, ADX={:.1}, 一致性={}/{}",
                    ma7, ma25, ma50, adx,
                    bearish_count, self.params.min_agreement,
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
