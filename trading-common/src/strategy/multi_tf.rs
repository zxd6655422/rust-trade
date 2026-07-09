//! Multi-Timeframe Strategy
//!
//! Ported from strategy-service/src/strategies/multi_tf.rs
//! Uses multiple moving averages to simulate different timeframes (1h/4h/1d),
//! ADX to filter weak trends, and weighted consensus logic with ATR-based stops.
//!
//! The `analyze` method works on a single timeframe by using MA periods as proxies
//! for higher timeframes. The `analyze_multi_tf` method accepts true multi-timeframe
//! data when available.

use serde::{Deserialize, Serialize};

use super::rsi::{KlineBar, MarketData, Signal, SignalType};
use super::trend::{calculate_adx, calculate_atr, calculate_ma, calculate_multi_ma};

// =================================================================
// Multi-timeframe data types
// =================================================================

/// Multi-timeframe market data container
#[derive(Debug, Clone)]
pub struct MultiTimeframeData {
    /// Primary (lowest) timeframe data
    pub primary: MarketData,
    /// All timeframes including primary, each with its own klines
    pub all: Vec<TimeframeMarketData>,
}

/// Market data for a single timeframe within multi-tf analysis
#[derive(Debug, Clone)]
pub struct TimeframeMarketData {
    pub klines: Vec<KlineBar>,
    pub current_price: f64,
    pub symbol: String,
    pub timeframe: String,
}

// =================================================================
// Parameters
// =================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MultiTfParams {
    /// Timeframe list, e.g. ["1h", "4h", "1d"]
    pub timeframes: Vec<String>,
    /// Minimum number of timeframes that must agree
    pub min_agreement: usize,
    /// Per-timeframe weights
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

// =================================================================
// Internal analysis types
// =================================================================

/// Trend analysis result for a single timeframe
#[derive(Debug, Clone)]
struct TimeframeAnalysis {
    timeframe: String,
    trend: f64,     // 1.0 = up, -1.0 = down, 0.0 = neutral
    strength: f64,  // 0.0 - 1.0
    ma_fast: f64,
    ma_slow: f64,
    adx: f64,
}

// =================================================================
// Multi-Timeframe Strategy
// =================================================================

pub struct MultiTimeframeStrategy {
    params: MultiTfParams,
}

impl MultiTimeframeStrategy {
    /// Create a new MultiTimeframe strategy with the given parameters.
    pub fn new(params: MultiTfParams) -> Self {
        Self { params }
    }

    /// Create from JSON parameters (compatible with strategy-service).
    pub fn from_json(params: &serde_json::Value) -> Result<Self, String> {
        let params: MultiTfParams =
            serde_json::from_value(params.clone()).map_err(|e| e.to_string())?;
        Ok(Self { params })
    }

    /// Get the strategy name.
    pub fn name(&self) -> &str {
        "multi_tf"
    }

    /// Analyze a single timeframe using multiple MAs as proxies for higher timeframes.
    ///
    /// This mode works with only one timeframe of data by using different MA periods
    /// to approximate the trend at different time scales:
    /// - MA7/MA25  ~ 1h trend
    /// - MA25/MA50 ~ 4h trend
    /// - MA50/MA99 ~ 1d trend
    pub fn analyze(&self, data: &MarketData) -> Option<Signal> {
        let current_price = data.current_price;

        let ma_periods = vec![7, 25, 50, 99];
        let multi_ma = calculate_multi_ma(&data.klines, &ma_periods);

        let ma7 = multi_ma.values.iter().find(|(p, _)| *p == 7).map(|(_, v)| *v)?;
        let ma25 = multi_ma.values.iter().find(|(p, _)| *p == 25).map(|(_, v)| *v)?;
        let ma50 = multi_ma
            .values
            .iter()
            .find(|(p, _)| *p == 50)
            .map(|(_, v)| *v)
            .unwrap_or(ma25);
        let ma99 = multi_ma
            .values
            .iter()
            .find(|(p, _)| *p == 99)
            .map(|(_, v)| *v)
            .unwrap_or(ma50);

        // Simulate multi-timeframe trend from MA crosses
        let h1_trend = if ma7 > ma25 {
            1.0
        } else if ma7 < ma25 {
            -1.0
        } else {
            0.0
        };
        let h4_trend = if ma25 > ma50 {
            1.0
        } else if ma25 < ma50 {
            -1.0
        } else {
            0.0
        };
        let d1_trend = if ma50 > ma99 {
            1.0
        } else if ma50 < ma99 {
            -1.0
        } else {
            0.0
        };

        let adx = calculate_adx(&data.klines, 14)
            .map(|r| r.adx)
            .unwrap_or(0.0);

        let weights = self.params.weights.clone().unwrap_or_default();
        let weighted_score = h1_trend * weights.weight_1h
            + h4_trend * weights.weight_4h
            + d1_trend * weights.weight_1d;

        let bullish_count = [h1_trend, h4_trend, d1_trend]
            .iter()
            .filter(|&&t| t > 0.0)
            .count();
        let bearish_count = [h1_trend, h4_trend, d1_trend]
            .iter()
            .filter(|&&t| t < 0.0)
            .count();

        let is_bullish = bullish_count >= self.params.min_agreement && weighted_score > 0.0;
        let is_bearish = bearish_count >= self.params.min_agreement && weighted_score < 0.0;

        if !is_bullish && !is_bearish {
            return None;
        }

        // ADX filter: skip weak trends
        if adx < 20.0 {
            return None;
        }

        let atr = calculate_atr(&data.klines, 14)
            .map(|r| r.value)
            .unwrap_or(current_price * 0.02);

        // Multi-TF uses wider stops (3x ATR stop, 5x ATR target)
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
                    "Multi-TF bullish (sim): MA7={:.2}>MA25={:.2}>MA50={:.2}, ADX={:.1}, agreement={}/{}",
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
                    "Multi-TF bearish (sim): MA7={:.2}<MA25={:.2}<MA50={:.2}, ADX={:.1}, agreement={}/{}",
                    ma7, ma25, ma50, adx, bearish_count, self.params.min_agreement,
                ),
                market_context,
            })
        }
    }

    /// True multi-timeframe analysis using actual data from different timeframes.
    ///
    /// Each timeframe is analyzed independently with MA pairs and ADX, then results
    /// are combined using weighted consensus.
    pub fn analyze_multi_tf(&self, data: &MultiTimeframeData) -> Option<Signal> {
        let current_price = data.primary.current_price;

        // Analyze each timeframe
        let mut analyses: Vec<TimeframeAnalysis> = Vec::new();

        for tf_data in &data.all {
            if let Some(analysis) = self.analyze_single_timeframe(&tf_data.klines, &tf_data.timeframe) {
                analyses.push(analysis);
            }
        }

        if analyses.is_empty() {
            return None;
        }

        // Calculate weighted score
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

        // Check minimum agreement
        let is_bullish = bullish_count >= self.params.min_agreement && weighted_score > 0.0;
        let is_bearish = bearish_count >= self.params.min_agreement && weighted_score < 0.0;

        if !is_bullish && !is_bearish {
            return None;
        }

        // Use primary timeframe for ADX and ATR
        let primary_klines = &data.primary.klines;
        let adx = calculate_adx(primary_klines, 14)
            .map(|r| r.adx)
            .unwrap_or(0.0);

        // ADX filter
        if adx < 20.0 {
            return None;
        }

        let atr = calculate_atr(primary_klines, 14)
            .map(|r| r.value)
            .unwrap_or(current_price * 0.02);

        // Wider stops for multi-TF (3x ATR stop, 5x ATR target)
        let (stop_loss, take_profit) = if is_bullish {
            (Some(current_price - 3.0 * atr), Some(current_price + 5.0 * atr))
        } else {
            (Some(current_price + 3.0 * atr), Some(current_price - 5.0 * atr))
        };

        let signal_strength = (weighted_score.abs() * (adx / 50.0)).min(1.0);

        // Build per-timeframe detail
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
                    "Multi-TF bullish: {}/{} agree, weighted={:.3}, ADX={:.1}",
                    bullish_count,
                    analyses.len(),
                    weighted_score,
                    adx,
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
                    "Multi-TF bearish: {}/{} agree, weighted={:.3}, ADX={:.1}",
                    bearish_count,
                    analyses.len(),
                    weighted_score,
                    adx,
                ),
                market_context,
            })
        }
    }

    /// Analyze trend for a single timeframe using MA pairs and ADX.
    fn analyze_single_timeframe(&self, klines: &[KlineBar], tf: &str) -> Option<TimeframeAnalysis> {
        if klines.len() < 100 {
            return None;
        }

        // Different MA periods per timeframe
        let (fast_period, slow_period) = match tf {
            "1h" => (7, 25),
            "4h" => (20, 50),
            "1d" => (10, 30),
            "1w" => (10, 30),
            _ => (7, 25),
        };

        let ma_fast = calculate_ma(klines, fast_period).map(|r| r.value)?;
        let ma_slow = calculate_ma(klines, slow_period).map(|r| r.value)?;

        let adx = calculate_adx(klines, 14)
            .map(|r| r.adx)
            .unwrap_or(0.0);

        // Trend direction
        let trend = if ma_fast > ma_slow {
            1.0
        } else if ma_fast < ma_slow {
            -1.0
        } else {
            0.0
        };

        // Strength: blend of ADX and MA spread
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

    /// Get the weight for a given timeframe string.
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

// =================================================================
// Tests
// =================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data::types::Timeframe;

    fn make_klines(count: usize, base: f64, trend: f64) -> Vec<KlineBar> {
        (0..count)
            .map(|i| {
                let price = base + i as f64 * trend;
                KlineBar {
                    open: price - 0.1,
                    high: price + 0.5,
                    low: price - 0.5,
                    close: price,
                    volume: 1000.0,
                }
            })
            .collect()
    }

    fn make_market_data(klines: Vec<KlineBar>) -> MarketData {
        let current_price = klines.last().map(|k| k.close).unwrap_or(0.0);
        MarketData {
            klines,
            current_price,
            symbol: "BTCUSDT".to_string(),
            timeframe: Timeframe::OneMinute,
        }
    }

    #[test]
    fn test_params_default() {
        let weights = TimeframeWeights::default();
        assert_eq!(weights.weight_1h, 0.3);
        assert_eq!(weights.weight_4h, 0.4);
        assert_eq!(weights.weight_1d, 0.3);
    }

    #[test]
    fn test_from_json() {
        let params = serde_json::json!({
            "timeframes": ["1h", "4h", "1d"],
            "min_agreement": 2,
            "weights": {
                "weight_1h": 0.2,
                "weight_4h": 0.5,
                "weight_1d": 0.3
            }
        });
        let strategy = MultiTimeframeStrategy::from_json(&params).unwrap();
        assert_eq!(strategy.params.timeframes.len(), 3);
        assert_eq!(strategy.params.min_agreement, 2);
        let w = strategy.params.weights.as_ref().unwrap();
        assert_eq!(w.weight_4h, 0.5);
    }

    #[test]
    fn test_strategy_name() {
        let params = MultiTfParams {
            timeframes: vec!["1h".to_string(), "4h".to_string()],
            min_agreement: 2,
            weights: None,
        };
        let strategy = MultiTimeframeStrategy::new(params);
        assert_eq!(strategy.name(), "multi_tf");
    }

    #[test]
    fn test_analyze_insufficient_data() {
        let params = MultiTfParams {
            timeframes: vec!["1h".to_string()],
            min_agreement: 2,
            weights: None,
        };
        let strategy = MultiTimeframeStrategy::new(params);
        // 20 klines: MA25 cannot be calculated, so all MA lookups return None,
        // causing analyze() to bail out early.
        let klines = make_klines(20, 100.0, 0.5);
        let data = make_market_data(klines);
        assert!(strategy.analyze(&data).is_none());
    }

    #[test]
    fn test_analyze_uptrend() {
        let params = MultiTfParams {
            timeframes: vec!["1h".to_string(), "4h".to_string(), "1d".to_string()],
            min_agreement: 2,
            weights: None,
        };
        let strategy = MultiTimeframeStrategy::new(params);
        let klines = make_klines(200, 100.0, 0.5);
        let data = make_market_data(klines);
        // With a steady uptrend, should produce a Buy signal
        let signal = strategy.analyze(&data);
        // May or may not trigger depending on ADX; just verify no panic
        if let Some(sig) = signal {
            assert!(sig.signal_strength >= 0.0 && sig.signal_strength <= 1.0);
            assert!(sig.confidence > 0.0);
        }
    }

    #[test]
    fn test_get_weight() {
        let params = MultiTfParams {
            timeframes: vec![],
            min_agreement: 1,
            weights: None,
        };
        let strategy = MultiTimeframeStrategy::new(params);
        assert_eq!(strategy.get_weight("1h"), 0.3);
        assert_eq!(strategy.get_weight("4h"), 0.4);
        assert_eq!(strategy.get_weight("1d"), 0.3);
        assert_eq!(strategy.get_weight("5m"), 0.2);
    }

    #[test]
    fn test_get_weight_custom() {
        let params = MultiTfParams {
            timeframes: vec![],
            min_agreement: 1,
            weights: Some(TimeframeWeights {
                weight_1h: 0.1,
                weight_4h: 0.6,
                weight_1d: 0.3,
            }),
        };
        let strategy = MultiTimeframeStrategy::new(params);
        assert_eq!(strategy.get_weight("1h"), 0.1);
        assert_eq!(strategy.get_weight("4h"), 0.6);
        assert_eq!(strategy.get_weight("1d"), 0.3);
    }

    #[test]
    fn test_analyze_multi_tf_no_data() {
        let params = MultiTfParams {
            timeframes: vec!["1h".to_string()],
            min_agreement: 1,
            weights: None,
        };
        let strategy = MultiTimeframeStrategy::new(params);
        let primary = make_market_data(make_klines(200, 100.0, 0.5));
        let data = MultiTimeframeData {
            primary,
            all: vec![],
        };
        assert!(strategy.analyze_multi_tf(&data).is_none());
    }
}
