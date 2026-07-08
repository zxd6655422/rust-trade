use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use super::{Signal, SignalType, Strategy};
use crate::indicators;
use crate::redis_reader::{KlineData, MarketData, MultiTimeframeData, Timeframe};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MultiTfParams {
    /// 时间框架列表，例如 ["1h", "4h", "1d"]
    pub timeframes: Vec<String>,
    /// 最少需要几个时间框架达成一致
    pub min_agreement: usize,
    /// 各时间框架权重
    pub weights: Option<TimeframeWeights>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimeframeWeights {
    pub weight_1h: f64,
    pub weight_4h: f64,
    pub weight_1d: f64,
}

impl Default for TimeframeWeights {
    fn default() -> Self {
        Self {
            weight_1h: 0.3,
            weight_4h: 0.4,
            weight_1d: 0.3,
        }
    }
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
    fn analyze_timeframe(&self, klines: &[KlineData], tf: &str) -> Option<TimeframeAnalysis> {
        if klines.len() < 100 {
            return None;
        }

        // 根据时间框架选择不同的均线参数
        let (fast_period, slow_period) = match tf {
            "1h" => (7, 25),
            "4h" => (20, 50),
            "1d" => (10, 30),
            "1w" => (10, 30),
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

    /// 获取时间框架的权重
    fn get_weight(&self, tf: &str) -> f64 {
        match &self.params.weights {
            Some(weights) => match tf {
                "1h" => weights.weight_1h,
                "4h" => weights.weight_4h,
                "1d" => weights.weight_1d,
                _ => 0.2,
            },
            None => match tf {
                "1h" => 0.3,
                "4h" => 0.4,
                "1d" => 0.3,
                _ => 0.2,
            },
        }
    }
}

#[async_trait]
impl Strategy for MultiTimeframeStrategy {
    fn name(&self) -> &str {
        "multi_tf"
    }

    async fn analyze(&self, data: &MarketData) -> Option<Signal> {
        // 单时间框架分析（向后兼容）
        // 使用多均线模拟不同时间框架
        let current_price = data.current_price;

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

        let adx = indicators::calculate_adx(&data.klines, 14).map(|r| r.adx).unwrap_or(0.0);

        let weights = self.params.weights.clone().unwrap_or_default();
        let weighted_score = h1_trend * weights.weight_1h
            + h4_trend * weights.weight_4h
            + d1_trend * weights.weight_1d;

        let bullish_count = [h1_trend, h4_trend, d1_trend].iter().filter(|&&t| t > 0.0).count();
        let bearish_count = [h1_trend, h4_trend, d1_trend].iter().filter(|&&t| t < 0.0).count();

        let is_bullish = bullish_count >= self.params.min_agreement && weighted_score > 0.0;
        let is_bearish = bearish_count >= self.params.min_agreement && weighted_score < 0.0;

        if !is_bullish && !is_bearish {
            return None;
        }

        if adx < 20.0 {
            return None;
        }

        let atr = indicators::calculate_atr(&data.klines, 14)
            .map(|r| r.value)
            .unwrap_or(current_price * 0.02);

        let (stop_loss, take_profit) = if is_bullish {
            (Some(current_price - 3.0 * atr), Some(current_price + 5.0 * atr))
        } else {
            (Some(current_price + 3.0 * atr), Some(current_price - 5.0 * atr))
        };

        let signal_strength = (weighted_score.abs() / 3.0 * (adx / 50.0)).min(1.0);

        let market_context = serde_json::json!({
            "h1_trend": h1_trend,
            "h4_trend": h4_trend,
            "d1_trend": d1_trend,
            "weighted_score": weighted_score,
            "bullish_count": bullish_count,
            "bearish_count": bearish_count,
            "adx": adx,
            "atr": atr,
            "ma7": ma7,
            "ma25": ma25,
            "ma50": ma50,
            "ma99": ma99,
            "mode": "single_tf_simulation",
        });

        if is_bullish {
            Some(Signal {
                signal_type: SignalType::Buy,
                signal_strength,
                entry_price: current_price,
                stop_loss,
                take_profit,
                confidence: 0.7,
                reason: format!(
                    "多时间框架看多(模拟): MA7={:.2}>MA25={:.2}>MA50={:.2}, ADX={:.1}, 一致性={}/{}",
                    ma7, ma25, ma50, adx, bullish_count, self.params.min_agreement,
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
                    "多时间框架看空(模拟): MA7={:.2}<MA25={:.2}<MA50={:.2}, ADX={:.1}, 一致性={}/{}",
                    ma7, ma25, ma50, adx, bearish_count, self.params.min_agreement,
                ),
                market_context,
            })
        }
    }

    /// 真正的多时间框架分析
    async fn analyze_multi_tf(&self, data: &MultiTimeframeData) -> Option<Signal> {
        let current_price = data.primary.current_price;

        // 分析每个时间框架
        let mut analyses: Vec<TimeframeAnalysis> = Vec::new();

        for market_data in &data.all {
            if let Some(analysis) = self.analyze_timeframe(&market_data.klines, market_data.timeframe.as_str()) {
                analyses.push(analysis);
            }
        }

        if analyses.is_empty() {
            return None;
        }

        // 计算加权得分
        let mut weighted_score = 0.0;
        let mut total_weight = 0.0;
        let mut bullish_count = 0;
        let mut bearish_count = 0;

        for analysis in &analyses {
            let weight = self.get_weight(&analysis.timeframe);
            weighted_score += analysis.trend * weight * analysis.strength;
            total_weight += weight;

            if analysis.trend > 0.0 {
                bullish_count += 1;
            } else if analysis.trend < 0.0 {
                bearish_count += 1;
            }
        }

        if total_weight > 0.0 {
            weighted_score /= total_weight;
        }

        // 判断是否满足最小一致性要求
        let is_bullish = bullish_count >= self.params.min_agreement && weighted_score > 0.0;
        let is_bearish = bearish_count >= self.params.min_agreement && weighted_score < 0.0;

        if !is_bullish && !is_bearish {
            return None;
        }

        // 使用主时间框架的 ADX 和 ATR
        let primary_klines = &data.primary.klines;
        let adx = indicators::calculate_adx(primary_klines, 14)
            .map(|r| r.adx)
            .unwrap_or(0.0);

        // ADX 过滤
        if adx < 20.0 {
            return None;
        }

        let atr = indicators::calculate_atr(primary_klines, 14)
            .map(|r| r.value)
            .unwrap_or(current_price * 0.02);

        // 止损止盈（多时间框架策略止损更宽）
        let (stop_loss, take_profit) = if is_bullish {
            (Some(current_price - 3.0 * atr), Some(current_price + 5.0 * atr))
        } else {
            (Some(current_price + 3.0 * atr), Some(current_price - 5.0 * atr))
        };

        let signal_strength = (weighted_score.abs() * (adx / 50.0)).min(1.0);

        // 构建分析详情
        let tf_details: Vec<serde_json::Value> = analyses
            .iter()
            .map(|a| {
                serde_json::json!({
                    "timeframe": a.timeframe,
                    "trend": a.trend,
                    "strength": a.strength,
                    "adx": a.adx,
                    "ma_fast": a.ma_fast,
                    "ma_slow": a.ma_slow,
                })
            })
            .collect();

        let market_context = serde_json::json!({
            "timeframe_analyses": tf_details,
            "weighted_score": weighted_score,
            "bullish_count": bullish_count,
            "bearish_count": bearish_count,
            "min_agreement": self.params.min_agreement,
            "adx": adx,
            "atr": atr,
            "current_price": current_price,
            "mode": "multi_tf",
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
                    "多时间框架看多: {}/{}一致, 加权得分={:.3}, ADX={:.1}",
                    bullish_count, analyses.len(), weighted_score, adx,
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
                    "多时间框架看空: {}/{}一致, 加权得分={:.3}, ADX={:.1}",
                    bearish_count, analyses.len(), weighted_score, adx,
                ),
                market_context,
            })
        }
    }

    fn required_timeframes(&self) -> Vec<Timeframe> {
        self.params
            .timeframes
            .iter()
            .filter_map(|tf| Timeframe::from_str(tf))
            .collect()
    }

    fn from_params(params: &serde_json::Value) -> anyhow::Result<Self> {
        let params: MultiTfParams = serde_json::from_value(params.clone())?;
        Ok(Self { params })
    }
}
