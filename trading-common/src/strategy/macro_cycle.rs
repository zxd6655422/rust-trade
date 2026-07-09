//! Macro Cycle (MacroCycle) Strategy
//!
//! Ported from strategy-service/src/strategies/macro_cycle.rs
//! Analyzes historical high/low levels (support/resistance), price position
//! within the historical range, moving averages, and ADX to generate
//! buy/sell signals on macro timeframes.

use serde::{Deserialize, Serialize};

use super::rsi::{KlineBar, MarketData, Signal, SignalType};
use super::trend::{calculate_adx, calculate_atr, calculate_multi_ma};

// =================================================================
// Strategy parameters
// =================================================================

/// Macro cycle strategy parameters
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MacroCycleParams {
    /// Moving average periods
    #[serde(default = "default_ma_periods")]
    pub ma_periods: Vec<usize>,
    /// Proximity threshold to historical high/low (percentage)
    #[serde(default = "default_proximity_threshold")]
    pub proximity_threshold: f64,
    /// ADX threshold for trend confirmation
    #[serde(default = "default_adx_threshold")]
    pub adx_threshold: f64,
    /// Number of historical periods to look back
    #[serde(default = "default_lookback_periods")]
    pub lookback_periods: usize,
}

fn default_ma_periods() -> Vec<usize> {
    vec![20, 50, 200]
}

fn default_proximity_threshold() -> f64 {
    5.0
}

fn default_adx_threshold() -> f64 {
    25.0
}

fn default_lookback_periods() -> usize {
    200
}

impl Default for MacroCycleParams {
    fn default() -> Self {
        Self {
            ma_periods: default_ma_periods(),
            proximity_threshold: default_proximity_threshold(),
            adx_threshold: default_adx_threshold(),
            lookback_periods: default_lookback_periods(),
        }
    }
}

// =================================================================
// Internal analysis types
// =================================================================

/// Single timeframe analysis result
#[derive(Debug, Clone)]
struct TimeframeAnalysis {
    historical_high: f64,
    historical_low: f64,
    price_position: f64,
    adx: f64,
    ma_short: f64,
    ma_long: f64,
    is_uptrend: bool,
    is_downtrend: bool,
    near_high: bool,
    near_low: bool,
}

// =================================================================
// Macro Cycle Strategy
// =================================================================

/// Macro cycle strategy
///
/// Analysis logic:
/// 1. Identify historical highs/lows (support/resistance levels)
/// 2. Determine current price position relative to historical range
/// 3. Combine moving averages and ADX for trend confirmation
/// 4. Generate buy/sell signals
pub struct MacroCycleStrategy {
    params: MacroCycleParams,
}

impl MacroCycleStrategy {
    /// Create a new MacroCycle strategy with the given parameters.
    pub fn new(params: MacroCycleParams) -> Self {
        Self { params }
    }

    /// Create from JSON parameters (compatible with strategy-service).
    pub fn from_json(params: &serde_json::Value) -> Result<Self, String> {
        let params: MacroCycleParams =
            serde_json::from_value(params.clone()).map_err(|e| e.to_string())?;
        Ok(Self { params })
    }

    /// Get the strategy name.
    pub fn name(&self) -> &str {
        "macro_cycle"
    }

    /// Find the highest high in the given lookback window.
    fn find_highest_high(&self, klines: &[KlineBar], lookback: usize) -> Option<f64> {
        if klines.is_empty() {
            return None;
        }
        let start = klines.len().saturating_sub(lookback);
        klines[start..].iter().map(|k| k.high).reduce(f64::max)
    }

    /// Find the lowest low in the given lookback window.
    fn find_lowest_low(&self, klines: &[KlineBar], lookback: usize) -> Option<f64> {
        if klines.is_empty() {
            return None;
        }
        let start = klines.len().saturating_sub(lookback);
        klines[start..].iter().map(|k| k.low).reduce(f64::min)
    }

    /// Calculate price position as a fraction within [low, high].
    fn price_position(&self, current: f64, high: f64, low: f64) -> f64 {
        if (high - low).abs() < f64::EPSILON {
            return 0.5;
        }
        (current - low) / (high - low)
    }

    /// Identify support and resistance levels using a sliding window.
    fn find_support_resistance(&self, klines: &[KlineBar]) -> Vec<(f64, String)> {
        let mut levels: Vec<(f64, String)> = Vec::new();

        if klines.len() < 20 {
            return levels;
        }

        let window = 10;
        for i in window..klines.len() - window {
            let slice = &klines[i - window..i + window + 1];
            let current_high = klines[i].high;
            let current_low = klines[i].low;

            // Local high
            if slice.iter().all(|k| k.high <= current_high) {
                levels.push((current_high, "resistance".to_string()));
            }

            // Local low
            if slice.iter().all(|k| k.low >= current_low) {
                levels.push((current_low, "support".to_string()));
            }
        }

        // Sort and deduplicate
        levels.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
        levels.dedup_by(|a, b| (a.0 - b.0).abs() < 0.001);

        levels
    }

    /// Analyze the current market data and produce a timeframe analysis.
    fn analyze_single(&self, data: &MarketData) -> Option<TimeframeAnalysis> {
        if data.klines.is_empty() {
            return None;
        }

        let current_price = data.current_price;

        // Moving averages
        let multi_ma = calculate_multi_ma(&data.klines, &self.params.ma_periods);
        let ma_short = multi_ma.values.first().map(|(_, v)| *v)?;
        let ma_long = multi_ma.values.last().map(|(_, v)| *v)?;

        // ADX
        let adx = calculate_adx(&data.klines, 14)
            .map(|r| r.adx)
            .unwrap_or(0.0);

        // Historical highs and lows
        let lookback = self.params.lookback_periods.min(data.klines.len());
        let historical_high = self.find_highest_high(&data.klines, lookback)?;
        let historical_low = self.find_lowest_low(&data.klines, lookback)?;

        // Price position within historical range
        let position = self.price_position(current_price, historical_high, historical_low);

        // Distance to historical extremes
        let distance_to_high = (current_price - historical_high) / historical_high;
        let distance_to_low = (current_price - historical_low) / historical_low;

        // Trend determination
        let is_uptrend = ma_short > ma_long && adx > self.params.adx_threshold;
        let is_downtrend = ma_short < ma_long && adx > self.params.adx_threshold;

        // Proximity to key levels
        let near_high = distance_to_high.abs() < self.params.proximity_threshold / 100.0;
        let near_low = distance_to_low.abs() < self.params.proximity_threshold / 100.0;

        Some(TimeframeAnalysis {
            historical_high,
            historical_low,
            price_position: position,
            adx,
            ma_short,
            ma_long,
            is_uptrend,
            is_downtrend,
            near_high,
            near_low,
        })
    }

    /// Analyze market data and return an optional signal.
    pub fn analyze(&self, data: &MarketData) -> Option<Signal> {
        let analysis = self.analyze_single(data)?;
        let current_price = data.current_price;

        // ATR for stop-loss sizing
        let atr = calculate_atr(&data.klines, 14)
            .map(|r| r.value)
            .unwrap_or(current_price * 0.02);

        // Identify support/resistance levels for context
        let levels = self.find_support_resistance(&data.klines);

        // Distance to historical extremes
        let distance_to_high =
            (current_price - analysis.historical_high) / analysis.historical_high;
        let distance_to_low =
            (current_price - analysis.historical_low) / analysis.historical_low;

        // Signal generation
        if analysis.near_high && analysis.is_downtrend {
            // Near historical resistance with downtrend -> Sell
            let stop_loss = current_price + 2.0 * atr;
            let take_profit = current_price - 3.0 * atr;
            let signal_strength = (1.0 - analysis.price_position) * 0.8;

            Some(Signal {
                signal_type: SignalType::Sell,
                signal_strength,
                entry_price: current_price,
                stop_loss: Some(stop_loss),
                take_profit: Some(take_profit),
                confidence: 0.75,
                reason: format!(
                    "接近历史阻力位 {:.2}（距离高点 {:.1}%），趋势向下，ADX={:.1}",
                    analysis.historical_high,
                    distance_to_high * 100.0,
                    analysis.adx,
                ),
                market_context: serde_json::json!({
                    "current_price": current_price,
                    "historical_high": analysis.historical_high,
                    "historical_low": analysis.historical_low,
                    "distance_to_high_pct": distance_to_high * 100.0,
                    "distance_to_low_pct": distance_to_low * 100.0,
                    "price_position": analysis.price_position,
                    "adx": analysis.adx,
                    "atr": atr,
                    "ma_short": analysis.ma_short,
                    "ma_long": analysis.ma_long,
                    "is_uptrend": analysis.is_uptrend,
                    "is_downtrend": analysis.is_downtrend,
                    "support_resistance_levels": levels.len(),
                    "timeframe": data.timeframe.as_str(),
                }),
            })
        } else if analysis.near_low && analysis.is_uptrend {
            // Near historical support with uptrend -> Buy
            let stop_loss = current_price - 2.0 * atr;
            let take_profit = current_price + 3.0 * atr;
            let signal_strength = analysis.price_position * 0.8;

            Some(Signal {
                signal_type: SignalType::Buy,
                signal_strength,
                entry_price: current_price,
                stop_loss: Some(stop_loss),
                take_profit: Some(take_profit),
                confidence: 0.75,
                reason: format!(
                    "接近历史支撑位 {:.2}（距离低点 {:.1}%），趋势向上，ADX={:.1}",
                    analysis.historical_low,
                    distance_to_low.abs() * 100.0,
                    analysis.adx,
                ),
                market_context: serde_json::json!({
                    "current_price": current_price,
                    "historical_high": analysis.historical_high,
                    "historical_low": analysis.historical_low,
                    "distance_to_high_pct": distance_to_high * 100.0,
                    "distance_to_low_pct": distance_to_low * 100.0,
                    "price_position": analysis.price_position,
                    "adx": analysis.adx,
                    "atr": atr,
                    "ma_short": analysis.ma_short,
                    "ma_long": analysis.ma_long,
                    "is_uptrend": analysis.is_uptrend,
                    "is_downtrend": analysis.is_downtrend,
                    "support_resistance_levels": levels.len(),
                    "timeframe": data.timeframe.as_str(),
                }),
            })
        } else if analysis.is_uptrend && analysis.price_position < 0.3 {
            // Uptrend with price in low region -> Buy
            let stop_loss = current_price - 2.0 * atr;
            let take_profit = current_price + 3.0 * atr;
            let signal_strength = (1.0 - analysis.price_position) * 0.6;

            Some(Signal {
                signal_type: SignalType::Buy,
                signal_strength,
                entry_price: current_price,
                stop_loss: Some(stop_loss),
                take_profit: Some(take_profit),
                confidence: 0.65,
                reason: format!(
                    "上升趋势中价格处于低位区域（位置 {:.1}%），ADX={:.1}",
                    analysis.price_position * 100.0,
                    analysis.adx,
                ),
                market_context: serde_json::json!({
                    "current_price": current_price,
                    "historical_high": analysis.historical_high,
                    "historical_low": analysis.historical_low,
                    "price_position": analysis.price_position,
                    "adx": analysis.adx,
                    "atr": atr,
                    "timeframe": data.timeframe.as_str(),
                }),
            })
        } else if analysis.is_downtrend && analysis.price_position > 0.7 {
            // Downtrend with price in high region -> Sell
            let stop_loss = current_price + 2.0 * atr;
            let take_profit = current_price - 3.0 * atr;
            let signal_strength = analysis.price_position * 0.6;

            Some(Signal {
                signal_type: SignalType::Sell,
                signal_strength,
                entry_price: current_price,
                stop_loss: Some(stop_loss),
                take_profit: Some(take_profit),
                confidence: 0.65,
                reason: format!(
                    "下降趋势中价格处于高位区域（位置 {:.1}%），ADX={:.1}",
                    analysis.price_position * 100.0,
                    analysis.adx,
                ),
                market_context: serde_json::json!({
                    "current_price": current_price,
                    "historical_high": analysis.historical_high,
                    "historical_low": analysis.historical_low,
                    "price_position": analysis.price_position,
                    "adx": analysis.adx,
                    "atr": atr,
                    "timeframe": data.timeframe.as_str(),
                }),
            })
        } else {
            None
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
            timeframe: Timeframe::OneDay,
        }
    }

    #[test]
    fn test_params_default() {
        let params = MacroCycleParams::default();
        assert_eq!(params.ma_periods, vec![20, 50, 200]);
        assert_eq!(params.proximity_threshold, 5.0);
        assert_eq!(params.adx_threshold, 25.0);
        assert_eq!(params.lookback_periods, 200);
    }

    #[test]
    fn test_from_json() {
        let params = serde_json::json!({
            "ma_periods": [10, 30, 100],
            "proximity_threshold": 3.0,
            "adx_threshold": 20.0,
            "lookback_periods": 150
        });
        let strategy = MacroCycleStrategy::from_json(&params).unwrap();
        assert_eq!(strategy.params.ma_periods, vec![10, 30, 100]);
        assert_eq!(strategy.params.proximity_threshold, 3.0);
        assert_eq!(strategy.params.adx_threshold, 20.0);
        assert_eq!(strategy.params.lookback_periods, 150);
    }

    #[test]
    fn test_strategy_name() {
        let strategy = MacroCycleStrategy::new(MacroCycleParams::default());
        assert_eq!(strategy.name(), "macro_cycle");
    }

    #[test]
    fn test_find_highest_lowest() {
        let klines = make_klines(100, 100.0, 0.5);
        let strategy = MacroCycleStrategy::new(MacroCycleParams::default());
        let high = strategy.find_highest_high(&klines, 50);
        let low = strategy.find_lowest_low(&klines, 50);
        assert!(high.is_some());
        assert!(low.is_some());
        assert!(high.unwrap() > low.unwrap());
    }

    #[test]
    fn test_price_position() {
        let strategy = MacroCycleStrategy::new(MacroCycleParams::default());
        let pos = strategy.price_position(150.0, 200.0, 100.0);
        assert!((pos - 0.5).abs() < f64::EPSILON);
    }

    #[test]
    fn test_price_position_equal_bounds() {
        let strategy = MacroCycleStrategy::new(MacroCycleParams::default());
        let pos = strategy.price_position(100.0, 100.0, 100.0);
        assert!((pos - 0.5).abs() < f64::EPSILON);
    }

    #[test]
    fn test_support_resistance() {
        // Create klines with a clear peak and trough
        let mut klines = make_klines(50, 100.0, 0.0);
        // Add a peak
        for i in 0..5 {
            klines.push(KlineBar {
                open: 100.0 + i as f64 * 10.0,
                high: 105.0 + i as f64 * 10.0,
                low: 95.0 + i as f64 * 10.0,
                close: 100.0 + i as f64 * 10.0,
                volume: 2000.0,
            });
        }
        let strategy = MacroCycleStrategy::new(MacroCycleParams::default());
        let levels = strategy.find_support_resistance(&klines);
        // Should find at least some levels
        assert!(!levels.is_empty());
    }

    #[test]
    fn test_no_signal_for_insufficient_data() {
        let klines = make_klines(5, 100.0, 0.0);
        let data = make_market_data(klines);
        let strategy = MacroCycleStrategy::new(MacroCycleParams::default());
        let signal = strategy.analyze(&data);
        assert!(signal.is_none());
    }
}
