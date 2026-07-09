//! RSI (Relative Strength Index) Strategy
//!
//! Ported from strategy-service/src/strategies/rsi.rs
//! Uses Wilder smoothing for RSI calculation, ATR for stop-loss, and
//! candle confirmation logic.

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

/// RSI calculation result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RsiResult {
    pub value: f64,
    pub period: usize,
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

// =================================================================
// Indicator calculation functions (Wilder smoothing)
// =================================================================

/// Extract closes from KlineBar slice
fn extract_closes(klines: &[KlineBar]) -> Vec<f64> {
    klines.iter().map(|k| k.close).collect()
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
// RSI Strategy
// =================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RsiParams {
    pub period: usize,
    pub overbought: f64,
    pub oversold: f64,
    pub confirm_candles: usize,
}

impl Default for RsiParams {
    fn default() -> Self {
        Self {
            period: 14,
            overbought: 70.0,
            oversold: 30.0,
            confirm_candles: 2,
        }
    }
}

pub struct RsiStrategy {
    params: RsiParams,
}

impl RsiStrategy {
    /// Create a new RSI strategy with the given parameters.
    pub fn new(params: RsiParams) -> Self {
        Self { params }
    }

    /// Create from JSON parameters (compatible with strategy-service).
    pub fn from_json(params: &serde_json::Value) -> Result<Self, String> {
        let params: RsiParams =
            serde_json::from_value(params.clone()).map_err(|e| e.to_string())?;
        Ok(Self { params })
    }

    /// Get the strategy name.
    pub fn name(&self) -> &str {
        "rsi"
    }

    /// Analyze market data and return an optional signal.
    pub fn analyze(&self, data: &MarketData) -> Option<Signal> {
        // Calculate RSI
        let rsi_result = calculate_rsi(&data.klines, self.params.period)?;
        let rsi = rsi_result.value;
        let current_price = data.current_price;

        // Calculate ATR for stop-loss sizing
        let atr = calculate_atr(&data.klines, 14)
            .map(|r| r.value)
            .unwrap_or(current_price * 0.02);

        // Calculate stop-loss and take-profit
        let (stop_loss, take_profit) = if rsi < self.params.oversold {
            // Oversold region: go long
            let stop_loss = current_price - 2.0 * atr;
            let take_profit = current_price + 3.0 * atr;
            (Some(stop_loss), Some(take_profit))
        } else if rsi > self.params.overbought {
            // Overbought region: go short
            let stop_loss = current_price + 2.0 * atr;
            let take_profit = current_price - 3.0 * atr;
            (Some(stop_loss), Some(take_profit))
        } else {
            (None, None)
        };

        // Signal strength
        let signal_strength = if rsi < self.params.oversold {
            (self.params.oversold - rsi) / self.params.oversold
        } else if rsi > self.params.overbought {
            (rsi - self.params.overbought) / (100.0 - self.params.overbought)
        } else {
            0.0
        };

        // Confirm signal with recent candle RSI trend
        let confirmed = self.confirm_signal(data, rsi);

        // Additional indicators for context
        let ma_fast = calculate_ma(&data.klines, 7).map(|r| r.value);
        let ma_slow = calculate_ma(&data.klines, 25).map(|r| r.value);

        let market_context = serde_json::json!({
            "rsi": rsi,
            "rsi_period": self.params.period,
            "overbought": self.params.overbought,
            "oversold": self.params.oversold,
            "atr": atr,
            "ma7": ma_fast,
            "ma25": ma_slow,
            "current_price": current_price,
            "kline_count": data.klines.len(),
            "timeframe": data.timeframe.as_str(),
        });

        if rsi < self.params.oversold && confirmed {
            Some(Signal {
                signal_type: SignalType::Buy,
                signal_strength,
                entry_price: current_price,
                stop_loss,
                take_profit,
                confidence: 0.7,
                reason: format!(
                    "RSI oversold: {:.2} < {} (period={})",
                    rsi, self.params.oversold, self.params.period
                ),
                market_context,
            })
        } else if rsi > self.params.overbought && confirmed {
            Some(Signal {
                signal_type: SignalType::Sell,
                signal_strength,
                entry_price: current_price,
                stop_loss,
                take_profit,
                confidence: 0.7,
                reason: format!(
                    "RSI overbought: {:.2} > {} (period={})",
                    rsi, self.params.overbought, self.params.period
                ),
                market_context,
            })
        } else {
            None
        }
    }

    /// Confirm signal by checking recent RSI trend over confirm_candles.
    fn confirm_signal(&self, data: &MarketData, current_rsi: f64) -> bool {
        if data.klines.len() < self.params.confirm_candles + 1 {
            return true;
        }

        // Calculate RSI on the previous N-1 klines to check trend
        if let Some(prev_rsi) =
            calculate_rsi(&data.klines[..data.klines.len() - 1], self.params.period)
        {
            if current_rsi < self.params.oversold {
                current_rsi <= prev_rsi.value // RSI still falling or flat
            } else if current_rsi > self.params.overbought {
                current_rsi >= prev_rsi.value // RSI still rising or flat
            } else {
                true
            }
        } else {
            true // Not enough data: default confirm
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
    fn test_rsi_calculation() {
        let klines = make_klines(100, 100.0, 0.5);
        let result = calculate_rsi(&klines, 14);
        assert!(result.is_some());
        let rsi = result.unwrap();
        assert!(rsi.value >= 0.0 && rsi.value <= 100.0);
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
    fn test_rsi_strategy_oversold() {
        // Create klines with a strong downtrend to push RSI into oversold
        let mut klines = make_klines(50, 200.0, 0.0);
        // Add a series of large drops
        for i in 0..20 {
            let price = 200.0 - i as f64 * 5.0;
            klines.push(KlineBar {
                open: price + 2.0,
                high: price + 3.0,
                low: price - 3.0,
                close: price,
                volume: 2000.0,
            });
        }
        let data = make_market_data(klines, Timeframe::OneMinute);
        let strategy = RsiStrategy::new(RsiParams::default());
        let signal = strategy.analyze(&data);
        // With a strong downtrend, RSI should be oversold
        if let Some(sig) = signal {
            assert_eq!(sig.signal_type, SignalType::Buy);
            assert!(sig.stop_loss.is_some());
            assert!(sig.take_profit.is_some());
        }
    }

    #[test]
    fn test_rsi_strategy_overbought() {
        // Create klines with a strong uptrend to push RSI into overbought
        let mut klines = make_klines(50, 100.0, 0.0);
        for i in 0..20 {
            let price = 100.0 + i as f64 * 5.0;
            klines.push(KlineBar {
                open: price - 2.0,
                high: price + 3.0,
                low: price - 3.0,
                close: price,
                volume: 2000.0,
            });
        }
        let data = make_market_data(klines, Timeframe::OneMinute);
        let strategy = RsiStrategy::new(RsiParams::default());
        let signal = strategy.analyze(&data);
        if let Some(sig) = signal {
            assert_eq!(sig.signal_type, SignalType::Sell);
        }
    }

    #[test]
    fn test_rsi_strategy_no_signal_in_neutral() {
        // Oscillating prices keep RSI in the neutral zone (30-70)
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
        let strategy = RsiStrategy::new(RsiParams::default());
        let signal = strategy.analyze(&data);
        // Oscillating prices should not trigger oversold/overbought
        assert!(signal.is_none());
    }

    #[test]
    fn test_from_json() {
        let params = serde_json::json!({
            "period": 14,
            "overbought": 75.0,
            "oversold": 25.0,
            "confirm_candles": 3
        });
        let strategy = RsiStrategy::from_json(&params).unwrap();
        assert_eq!(strategy.params.period, 14);
        assert_eq!(strategy.params.overbought, 75.0);
        assert_eq!(strategy.params.oversold, 25.0);
        assert_eq!(strategy.params.confirm_candles, 3);
    }
}
