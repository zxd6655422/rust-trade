//! Bollinger Bands Strategy
//!
//! Ported from strategy-service/src/strategies/bollinger.rs
//! Uses SMA-based Bollinger Bands calculation, squeeze detection,
//! and percent-band signal logic.

use serde::{Deserialize, Serialize};

use crate::data::types::{OHLCData, Timeframe};
use rust_decimal::prelude::ToPrimitive;

// =================================================================
// Market data types (portable, no external dependency)
// =================================================================

/// Simplified Kline data for indicator calculations
#[derive(Debug, Clone)]
pub struct KlineBar {
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
    pub volume: f64,
}

impl From<&OHLCData> for KlineBar {
    fn from(ohlc: &OHLCData) -> Self {
        Self {
            open: ohlc.open.to_f64().unwrap_or(0.0),
            high: ohlc.high.to_f64().unwrap_or(0.0),
            low: ohlc.low.to_f64().unwrap_or(0.0),
            close: ohlc.close.to_f64().unwrap_or(0.0),
            volume: ohlc.volume.to_f64().unwrap_or(0.0),
        }
    }
}

/// Market data snapshot consumed by the strategy
#[derive(Debug, Clone)]
pub struct MarketData {
    pub klines: Vec<KlineBar>,
    pub current_price: f64,
    pub symbol: String,
    pub timeframe: Timeframe,
}

// =================================================================
// Indicator result types
// =================================================================

/// Bollinger Bands calculation result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BollingerResult {
    pub upper: f64,
    pub middle: f64,
    pub lower: f64,
    pub bandwidth: f64,
    pub percent_b: f64,
    pub period: usize,
    pub std_dev: f64,
}

/// RSI calculation result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RsiResult {
    pub value: f64,
    pub period: usize,
}

/// MA calculation result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MaResult {
    pub value: f64,
    pub period: usize,
}

// =================================================================
// Indicator calculation functions
// =================================================================

/// Extract closes from KlineBar slice
fn extract_closes(klines: &[KlineBar]) -> Vec<f64> {
    klines.iter().map(|k| k.close).collect()
}

/// Calculate Bollinger Bands
///
/// - middle = SMA of closing prices over `period`
/// - upper  = middle + std_dev * standard deviation
/// - lower  = middle - std_dev * standard deviation
/// - bandwidth = (upper - lower) / middle
/// - percent_b = (current_price - lower) / (upper - lower)
pub fn calculate_bollinger(
    klines: &[KlineBar],
    period: usize,
    std_dev: f64,
) -> Option<BollingerResult> {
    if klines.len() < period || period == 0 {
        return None;
    }

    let closes = extract_closes(klines);
    let recent = &closes[closes.len() - period..];

    // Middle band = SMA
    let middle: f64 = recent.iter().sum::<f64>() / period as f64;

    // Standard deviation
    let variance: f64 = recent
        .iter()
        .map(|x| (x - middle).powi(2))
        .sum::<f64>()
        / period as f64;
    let std = variance.sqrt();

    let upper = middle + std_dev * std;
    let lower = middle - std_dev * std;

    // Bandwidth = (upper - lower) / middle
    let bandwidth = if middle > 0.0 {
        (upper - lower) / middle
    } else {
        0.0
    };

    // %B = (current_price - lower) / (upper - lower)
    let current_price = *closes.last().unwrap_or(&0.0);
    let percent_b = if upper - lower > 0.0 {
        (current_price - lower) / (upper - lower)
    } else {
        0.5
    };

    Some(BollingerResult {
        upper,
        middle,
        lower,
        bandwidth,
        percent_b,
        period,
        std_dev,
    })
}

/// RSI using Wilder smoothing
pub fn calculate_rsi(klines: &[KlineBar], period: usize) -> Option<RsiResult> {
    if klines.len() < period + 1 || period == 0 {
        return None;
    }

    let closes = extract_closes(klines);
    let changes: Vec<f64> = closes.windows(2).map(|w| w[1] - w[0]).collect();

    if changes.len() < period {
        return None;
    }

    // Initial average gain/loss
    let mut avg_gain: f64 = changes[..period]
        .iter()
        .filter(|&&c| c > 0.0)
        .sum::<f64>()
        / period as f64;
    let mut avg_loss: f64 = changes[..period]
        .iter()
        .filter(|&&c| c < 0.0)
        .map(|c| c.abs())
        .sum::<f64>()
        / period as f64;

    // Wilder smoothing
    for &change in &changes[period..] {
        let gain = if change > 0.0 { change } else { 0.0 };
        let loss = if change < 0.0 { change.abs() } else { 0.0 };
        avg_gain = (avg_gain * (period as f64 - 1.0) + gain) / period as f64;
        avg_loss = (avg_loss * (period as f64 - 1.0) + loss) / period as f64;
    }

    let value = if avg_loss == 0.0 {
        100.0
    } else {
        let rs = avg_gain / avg_loss;
        100.0 - (100.0 / (1.0 + rs))
    };

    Some(RsiResult { value, period })
}

/// Simple Moving Average
pub fn calculate_ma(klines: &[KlineBar], period: usize) -> Option<MaResult> {
    if klines.len() < period || period == 0 {
        return None;
    }

    let closes = extract_closes(klines);
    let start = closes.len() - period;
    let sum: f64 = closes[start..].iter().sum();
    let value = sum / period as f64;

    Some(MaResult { value, period })
}

// =================================================================
// Signal types
// =================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SignalType {
    Buy,
    Sell,
    Hold,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Signal {
    pub signal_type: SignalType,
    pub signal_strength: f64,
    pub entry_price: f64,
    pub stop_loss: Option<f64>,
    pub take_profit: Option<f64>,
    pub confidence: f64,
    pub reason: String,
    pub market_context: serde_json::Value,
}

// =================================================================
// Bollinger Bands Strategy
// =================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BollingerParams {
    pub period: usize,
    pub std_dev: f64,
    pub squeeze_threshold: f64,
}

impl Default for BollingerParams {
    fn default() -> Self {
        Self {
            period: 20,
            std_dev: 2.0,
            squeeze_threshold: 0.04,
        }
    }
}

pub struct BollingerStrategy {
    params: BollingerParams,
}

impl BollingerStrategy {
    /// Create a new Bollinger strategy with the given parameters.
    pub fn new(params: BollingerParams) -> Self {
        Self { params }
    }

    /// Create from JSON parameters (compatible with strategy-service).
    pub fn from_json(params: &serde_json::Value) -> Result<Self, String> {
        let params: BollingerParams =
            serde_json::from_value(params.clone()).map_err(|e| e.to_string())?;
        Ok(Self { params })
    }

    /// Get the strategy name.
    pub fn name(&self) -> &str {
        "bollinger"
    }

    /// Analyze market data and return an optional signal.
    pub fn analyze(&self, data: &MarketData) -> Option<Signal> {
        // Calculate Bollinger Bands
        let bollinger = calculate_bollinger(
            &data.klines,
            self.params.period,
            self.params.std_dev,
        )?;

        let current_price = data.current_price;

        // Check squeeze state (Bollinger band narrowing)
        let is_squeeze = bollinger.bandwidth < self.params.squeeze_threshold;

        // Check price position
        let percent_b = bollinger.percent_b;

        // Price touching lower band (oversold)
        let at_lower_band = percent_b < 0.1;

        // Price touching upper band (overbought)
        let at_upper_band = percent_b > 0.9;

        if !at_lower_band && !at_upper_band {
            return None;
        }

        // Calculate stop-loss and take-profit
        let (stop_loss, take_profit) = if at_lower_band {
            let stop_loss = bollinger.lower * 0.98; // 2% below lower band
            let take_profit = bollinger.middle; // Middle band
            (Some(stop_loss), Some(take_profit))
        } else {
            let stop_loss = bollinger.upper * 1.02; // 2% above upper band
            let take_profit = bollinger.middle; // Middle band
            (Some(stop_loss), Some(take_profit))
        };

        // Signal strength
        let signal_strength = if at_lower_band {
            1.0 - percent_b // Closer to 0 = stronger
        } else {
            percent_b - 0.9 // Closer to 1 = stronger
        };
        let signal_strength = signal_strength.min(1.0).max(0.0);

        // Additional indicators for context
        let rsi = calculate_rsi(&data.klines, 14).map(|r| r.value);
        let ma_fast = calculate_ma(&data.klines, 7).map(|r| r.value);

        let market_context = serde_json::json!({
            "upper": bollinger.upper,
            "middle": bollinger.middle,
            "lower": bollinger.lower,
            "bandwidth": bollinger.bandwidth,
            "percent_b": percent_b,
            "is_squeeze": is_squeeze,
            "period": self.params.period,
            "std_dev": self.params.std_dev,
            "rsi": rsi,
            "ma7": ma_fast,
            "current_price": current_price,
            "kline_count": data.klines.len(),
            "timeframe": data.timeframe.as_str(),
        });

        if at_lower_band {
            Some(Signal {
                signal_type: SignalType::Buy,
                signal_strength,
                entry_price: current_price,
                stop_loss,
                take_profit,
                confidence: 0.65,
                reason: format!(
                    "Bollinger lower band hit: %B={:.2}, lower={:.2} ({}/{})",
                    percent_b, bollinger.lower,
                    self.params.period, self.params.std_dev,
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
                confidence: 0.65,
                reason: format!(
                    "Bollinger upper band hit: %B={:.2}, upper={:.2} ({}/{})",
                    percent_b, bollinger.upper,
                    self.params.period, self.params.std_dev,
                ),
                market_context,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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

    fn make_market_data(klines: Vec<KlineBar>, timeframe: Timeframe) -> MarketData {
        let current_price = klines.last().map(|k| k.close).unwrap_or(0.0);
        MarketData {
            klines,
            current_price,
            symbol: "BTCUSDT".to_string(),
            timeframe,
        }
    }

    #[test]
    fn test_bollinger_calculation() {
        let klines = make_klines(100, 100.0, 0.5);
        let result = calculate_bollinger(&klines, 20, 2.0);
        assert!(result.is_some());
        let boll = result.unwrap();
        assert!(boll.upper > boll.middle);
        assert!(boll.middle > boll.lower);
        assert!(boll.bandwidth > 0.0);
        assert!(boll.percent_b >= 0.0);
    }

    #[test]
    fn test_bollinger_insufficient_data() {
        let klines = make_klines(5, 100.0, 0.5);
        let result = calculate_bollinger(&klines, 20, 2.0);
        assert!(result.is_none());
    }

    #[test]
    fn test_rsi_calculation() {
        let klines = make_klines(100, 100.0, 0.5);
        let result = calculate_rsi(&klines, 14);
        assert!(result.is_some());
        let rsi = result.unwrap();
        assert!(rsi.value >= 0.0 && rsi.value <= 100.0);
    }

    #[test]
    fn test_ma_calculation() {
        let klines = make_klines(100, 100.0, 0.5);
        let result = calculate_ma(&klines, 20);
        assert!(result.is_some());
    }

    #[test]
    fn test_bollinger_strategy_lower_band() {
        // Create klines with a sharp drop to push price to lower band
        let mut klines = make_klines(50, 100.0, 0.0);
        // Add a sharp drop so %B goes below 0.1
        for i in 0..20 {
            let price = 100.0 - i as f64 * 3.0;
            klines.push(KlineBar {
                open: price + 1.0,
                high: price + 2.0,
                low: price - 2.0,
                close: price,
                volume: 2000.0,
            });
        }
        let data = make_market_data(klines, Timeframe::OneMinute);
        let strategy = BollingerStrategy::new(BollingerParams::default());
        let signal = strategy.analyze(&data);
        // With a sharp drop, price should hit lower band
        if let Some(sig) = signal {
            assert_eq!(sig.signal_type, SignalType::Buy);
            assert!(sig.stop_loss.is_some());
            assert!(sig.take_profit.is_some());
        }
    }

    #[test]
    fn test_bollinger_strategy_upper_band() {
        // Create klines with a sharp rise to push price to upper band
        let mut klines = make_klines(50, 100.0, 0.0);
        for i in 0..20 {
            let price = 100.0 + i as f64 * 3.0;
            klines.push(KlineBar {
                open: price - 1.0,
                high: price + 2.0,
                low: price - 2.0,
                close: price,
                volume: 2000.0,
            });
        }
        let data = make_market_data(klines, Timeframe::OneMinute);
        let strategy = BollingerStrategy::new(BollingerParams::default());
        let signal = strategy.analyze(&data);
        if let Some(sig) = signal {
            assert_eq!(sig.signal_type, SignalType::Sell);
        }
    }

    #[test]
    fn test_bollinger_strategy_no_signal_in_neutral() {
        // Oscillating prices keep %B in the middle range
        let klines: Vec<KlineBar> = (0..100)
            .map(|i| {
                let price = 100.0 + (i as f64 * 0.3).sin() * 1.0;
                KlineBar {
                    open: price - 0.1,
                    high: price + 0.3,
                    low: price - 0.3,
                    close: price,
                    volume: 1000.0,
                }
            })
            .collect();
        let data = make_market_data(klines, Timeframe::OneMinute);
        let strategy = BollingerStrategy::new(BollingerParams::default());
        let signal = strategy.analyze(&data);
        // Oscillating prices should not trigger upper/lower band signals
        assert!(signal.is_none());
    }

    #[test]
    fn test_from_json() {
        let params = serde_json::json!({
            "period": 20,
            "std_dev": 2.5,
            "squeeze_threshold": 0.03
        });
        let strategy = BollingerStrategy::from_json(&params).unwrap();
        assert_eq!(strategy.params.period, 20);
        assert_eq!(strategy.params.std_dev, 2.5);
        assert_eq!(strategy.params.squeeze_threshold, 0.03);
    }
}
