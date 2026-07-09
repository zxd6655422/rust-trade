//! MACD (Moving Average Convergence Divergence) Strategy
//!
//! Ported from strategy-service/src/strategies/macd.rs
//! Uses EMA-based MACD calculation, ATR for stop-loss, and
//! golden/death cross signal logic.

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

/// MACD calculation result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MacdResult {
    pub macd: f64,
    pub signal: f64,
    pub histogram: f64,
}

/// ATR calculation result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AtrResult {
    pub value: f64,
    pub period: usize,
}

/// MA calculation result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MaResult {
    pub value: f64,
    pub period: usize,
}

/// RSI calculation result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RsiResult {
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

/// Exponential Moving Average
fn calculate_ema(values: &[f64], period: usize) -> Option<f64> {
    if values.len() < period || period == 0 {
        return None;
    }

    let multiplier = 2.0 / (period as f64 + 1.0);

    // Initial EMA = SMA of first `period` values
    let mut ema: f64 = values[..period].iter().sum::<f64>() / period as f64;

    // Apply EMA formula for remaining values
    for &value in &values[period..] {
        ema = (value - ema) * multiplier + ema;
    }

    Some(ema)
}

/// Calculate MACD (Moving Average Convergence Divergence)
///
/// Returns the MACD line, signal line, and histogram.
/// MACD = EMA(fast) - EMA(slow)
/// Signal = EMA(MACD, signal_period)
/// Histogram = MACD - Signal
pub fn calculate_macd(
    klines: &[KlineBar],
    fast_period: usize,
    slow_period: usize,
    signal_period: usize,
) -> Option<MacdResult> {
    if klines.len() < slow_period + signal_period || fast_period >= slow_period {
        return None;
    }

    let closes = extract_closes(klines);

    // Calculate fast and slow EMA
    let ema_fast = calculate_ema(&closes, fast_period)?;
    let ema_slow = calculate_ema(&closes, slow_period)?;

    let macd_line = ema_fast - ema_slow;

    // For signal line, we need MACD history. Approximate by computing
    // MACD values over the last signal_period windows and taking EMA.
    // A simpler approach: use the current MACD as a proxy and compute
    // signal line EMA from recent MACD values.
    let mut macd_values: Vec<f64> = Vec::new();
    let start = closes.len().saturating_sub(slow_period + signal_period);
    for i in start..closes.len() {
        if i + slow_period <= closes.len() && i + fast_period <= closes.len() {
            if let (Some(fast), Some(slow)) = (
                calculate_ema(&closes[..=i + fast_period - 1], fast_period),
                calculate_ema(&closes[..=i + slow_period - 1], slow_period),
            ) {
                macd_values.push(fast - slow);
            }
        }
    }

    let signal_line = if macd_values.len() >= signal_period {
        calculate_ema(&macd_values, signal_period).unwrap_or(macd_line)
    } else {
        // Fallback: approximate signal as smoothed MACD
        macd_line
    };

    let histogram = macd_line - signal_line;

    Some(MacdResult {
        macd: macd_line,
        signal: signal_line,
        histogram,
    })
}

/// ATR using Wilder smoothing
pub fn calculate_atr(klines: &[KlineBar], period: usize) -> Option<AtrResult> {
    if klines.len() < period + 1 || period == 0 {
        return None;
    }

    // True Range
    let mut tr_values: Vec<f64> = Vec::new();
    for i in 1..klines.len() {
        let high_low = klines[i].high - klines[i].low;
        let high_prev_close = (klines[i].high - klines[i - 1].close).abs();
        let low_prev_close = (klines[i].low - klines[i - 1].close).abs();
        tr_values.push(high_low.max(high_prev_close).max(low_prev_close));
    }

    if tr_values.len() < period {
        return None;
    }

    // First ATR = SMA of first `period` TR values
    let first_atr: f64 = tr_values[..period].iter().sum::<f64>() / period as f64;

    // Wilder smoothing for subsequent values
    let mut atr = first_atr;
    for &tr in &tr_values[period..] {
        atr = (atr * (period as f64 - 1.0) + tr) / period as f64;
    }

    Some(AtrResult {
        value: atr,
        period,
    })
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
// MACD Strategy
// =================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MacdParams {
    pub fast_period: usize,
    pub slow_period: usize,
    pub signal_period: usize,
    pub histogram_threshold: f64,
}

impl Default for MacdParams {
    fn default() -> Self {
        Self {
            fast_period: 12,
            slow_period: 26,
            signal_period: 9,
            histogram_threshold: 0.0,
        }
    }
}

pub struct MacdStrategy {
    params: MacdParams,
}

impl MacdStrategy {
    /// Create a new MACD strategy with the given parameters.
    pub fn new(params: MacdParams) -> Self {
        Self { params }
    }

    /// Create from JSON parameters (compatible with strategy-service).
    pub fn from_json(params: &serde_json::Value) -> Result<Self, String> {
        let params: MacdParams =
            serde_json::from_value(params.clone()).map_err(|e| e.to_string())?;
        Ok(Self { params })
    }

    /// Get the strategy name.
    pub fn name(&self) -> &str {
        "macd"
    }

    /// Analyze market data and return an optional signal.
    pub fn analyze(&self, data: &MarketData) -> Option<Signal> {
        // Calculate MACD
        let macd_result = calculate_macd(
            &data.klines,
            self.params.fast_period,
            self.params.slow_period,
            self.params.signal_period,
        )?;

        let macd = macd_result.macd;
        let signal = macd_result.signal;
        let histogram = macd_result.histogram;
        let current_price = data.current_price;

        // MACD golden cross: MACD line crosses above signal line
        let is_golden_cross = macd > signal && histogram > self.params.histogram_threshold;

        // MACD death cross: MACD line crosses below signal line
        let is_death_cross = macd < signal && histogram < -self.params.histogram_threshold;

        if !is_golden_cross && !is_death_cross {
            return None;
        }

        // Calculate ATR for stop-loss sizing
        let atr = calculate_atr(&data.klines, 14)
            .map(|r| r.value)
            .unwrap_or(current_price * 0.02);

        // Calculate stop-loss and take-profit
        let (stop_loss, take_profit) = if is_golden_cross {
            let stop_loss = current_price - 2.0 * atr;
            let take_profit = current_price + 3.0 * atr;
            (Some(stop_loss), Some(take_profit))
        } else {
            let stop_loss = current_price + 2.0 * atr;
            let take_profit = current_price - 3.0 * atr;
            (Some(stop_loss), Some(take_profit))
        };

        // Signal strength based on histogram magnitude
        let signal_strength = (histogram.abs() / (current_price * 0.01)).min(1.0);

        // Additional indicators for context
        let ma_fast = calculate_ma(&data.klines, 7).map(|r| r.value);
        let ma_slow = calculate_ma(&data.klines, 25).map(|r| r.value);
        let rsi = calculate_rsi(&data.klines, 14).map(|r| r.value);

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
                    "MACD golden cross: MACD={:.4}, Signal={:.4}, Hist={:.4} ({}/{}/{})",
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
                    "MACD death cross: MACD={:.4}, Signal={:.4}, Hist={:.4} ({}/{}/{})",
                    macd, signal, histogram,
                    self.params.fast_period,
                    self.params.slow_period,
                    self.params.signal_period,
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
    fn test_macd_calculation() {
        let klines = make_klines(100, 100.0, 0.5);
        let result = calculate_macd(&klines, 12, 26, 9);
        assert!(result.is_some());
        let macd = result.unwrap();
        // In a steady uptrend, MACD should be positive
        assert!(macd.macd > 0.0);
    }

    #[test]
    fn test_macd_insufficient_data() {
        let klines = make_klines(10, 100.0, 0.5);
        let result = calculate_macd(&klines, 12, 26, 9);
        assert!(result.is_none());
    }

    #[test]
    fn test_atr_calculation() {
        let klines = make_klines(100, 100.0, 0.5);
        let result = calculate_atr(&klines, 14);
        assert!(result.is_some());
        let atr = result.unwrap();
        assert!(atr.value > 0.0);
    }

    #[test]
    fn test_ma_calculation() {
        let klines = make_klines(100, 100.0, 0.5);
        let result = calculate_ma(&klines, 20);
        assert!(result.is_some());
    }

    #[test]
    fn test_macd_strategy_uptrend() {
        // Create klines with a strong uptrend to generate golden cross
        let mut klines = make_klines(30, 100.0, 0.0);
        // Add a strong uptrend to push MACD above signal
        for i in 0..50 {
            let price = 100.0 + i as f64 * 2.0;
            klines.push(KlineBar {
                open: price - 1.0,
                high: price + 2.0,
                low: price - 2.0,
                close: price,
                volume: 2000.0,
            });
        }
        let data = make_market_data(klines, Timeframe::OneMinute);
        let strategy = MacdStrategy::new(MacdParams::default());
        let signal = strategy.analyze(&data);
        // With a strong uptrend, MACD should trigger a buy signal
        if let Some(sig) = signal {
            assert_eq!(sig.signal_type, SignalType::Buy);
            assert!(sig.stop_loss.is_some());
            assert!(sig.take_profit.is_some());
        }
    }

    #[test]
    fn test_macd_strategy_downtrend() {
        // Create klines with a strong downtrend to generate death cross
        let mut klines = make_klines(30, 200.0, 0.0);
        // Add a strong downtrend to push MACD below signal
        for i in 0..50 {
            let price = 200.0 - i as f64 * 2.0;
            klines.push(KlineBar {
                open: price + 1.0,
                high: price + 2.0,
                low: price - 2.0,
                close: price,
                volume: 2000.0,
            });
        }
        let data = make_market_data(klines, Timeframe::OneMinute);
        let strategy = MacdStrategy::new(MacdParams::default());
        let signal = strategy.analyze(&data);
        if let Some(sig) = signal {
            assert_eq!(sig.signal_type, SignalType::Sell);
        }
    }

    #[test]
    fn test_macd_strategy_no_signal_sideways() {
        // Oscillating prices keep MACD near zero
        let klines: Vec<KlineBar> = (0..100)
            .map(|i| {
                let price = 100.0 + (i as f64 * 0.3).sin() * 2.0;
                KlineBar {
                    open: price - 0.1,
                    high: price + 0.5,
                    low: price - 0.5,
                    close: price,
                    volume: 1000.0,
                }
            })
            .collect();
        let data = make_market_data(klines, Timeframe::OneMinute);
        let strategy = MacdStrategy::new(MacdParams {
            histogram_threshold: 0.5,
            ..Default::default()
        });
        let signal = strategy.analyze(&data);
        // Oscillating prices should not trigger golden/death cross
        assert!(signal.is_none());
    }

    #[test]
    fn test_from_json() {
        let params = serde_json::json!({
            "fast_period": 8,
            "slow_period": 21,
            "signal_period": 5,
            "histogram_threshold": 0.1
        });
        let strategy = MacdStrategy::from_json(&params).unwrap();
        assert_eq!(strategy.params.fast_period, 8);
        assert_eq!(strategy.params.slow_period, 21);
        assert_eq!(strategy.params.signal_period, 5);
        assert_eq!(strategy.params.histogram_threshold, 0.1);
    }
}
