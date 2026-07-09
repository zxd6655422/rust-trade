//! Volume Spike Strategy
//!
//! Ported from strategy-service/src/strategies/volume.rs
//! Detects volume spikes combined with price movement to generate signals.
//! Uses ATR for stop-loss sizing and includes RSI/MA context.

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
// Indicator calculation functions (Wilder smoothing)
// =================================================================

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

/// RSI using Wilder smoothing
pub fn calculate_rsi(klines: &[KlineBar], period: usize) -> Option<RsiResult> {
    if klines.len() < period + 1 || period == 0 {
        return None;
    }

    let closes: Vec<f64> = klines.iter().map(|k| k.close).collect();
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

    let closes: Vec<f64> = klines.iter().map(|k| k.close).collect();
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
// Volume Strategy
// =================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VolumeParams {
    pub volume_ma_period: usize,
    pub volume_spike_threshold: f64,
    pub price_change_threshold: f64,
}

impl Default for VolumeParams {
    fn default() -> Self {
        Self {
            volume_ma_period: 20,
            volume_spike_threshold: 2.0,
            price_change_threshold: 0.005,
        }
    }
}

pub struct VolumeStrategy {
    params: VolumeParams,
}

impl VolumeStrategy {
    /// Create a new Volume strategy with the given parameters.
    pub fn new(params: VolumeParams) -> Self {
        Self { params }
    }

    /// Create from JSON parameters (compatible with strategy-service).
    pub fn from_json(params: &serde_json::Value) -> Result<Self, String> {
        let params: VolumeParams =
            serde_json::from_value(params.clone()).map_err(|e| e.to_string())?;
        Ok(Self { params })
    }

    /// Get the strategy name.
    pub fn name(&self) -> &str {
        "volume"
    }

    /// Analyze market data and return an optional signal.
    pub fn analyze(&self, data: &MarketData) -> Option<Signal> {
        if data.klines.len() < self.params.volume_ma_period + 1 {
            return None;
        }

        let current_price = data.current_price;

        // Calculate volume moving average
        let volumes: Vec<f64> = data.klines.iter().map(|k| k.volume).collect();
        let recent_volumes = &volumes[volumes.len() - self.params.volume_ma_period..];
        let volume_ma: f64 = recent_volumes.iter().sum::<f64>() / self.params.volume_ma_period as f64;

        // Current volume
        let current_volume = *volumes.last().unwrap_or(&0.0);

        // Volume ratio
        let volume_ratio = if volume_ma > 0.0 {
            current_volume / volume_ma
        } else {
            0.0
        };

        // Price change
        let closes: Vec<f64> = data.klines.iter().map(|k| k.close).collect();
        let price_change = if closes.len() >= 2 {
            let prev = closes[closes.len() - 2];
            let curr = closes[closes.len() - 1];
            (curr - prev) / prev
        } else {
            0.0
        };

        // Volume spike + price movement = signal
        let is_volume_spike = volume_ratio >= self.params.volume_spike_threshold;
        let is_price_up = price_change > self.params.price_change_threshold;
        let is_price_down = price_change < -self.params.price_change_threshold;

        if !is_volume_spike {
            return None;
        }

        // Calculate ATR for stop-loss sizing
        let atr = calculate_atr(&data.klines, 14)
            .map(|r| r.value)
            .unwrap_or(current_price * 0.02);

        // Calculate stop-loss and take-profit
        let (stop_loss, take_profit) = if is_price_up {
            let stop_loss = current_price - 2.0 * atr;
            let take_profit = current_price + 3.0 * atr;
            (Some(stop_loss), Some(take_profit))
        } else if is_price_down {
            let stop_loss = current_price + 2.0 * atr;
            let take_profit = current_price - 3.0 * atr;
            (Some(stop_loss), Some(take_profit))
        } else {
            return None;
        };

        // Signal strength
        let signal_strength = ((volume_ratio - 1.0) / self.params.volume_spike_threshold).min(1.0);

        // Additional indicators for context
        let rsi = calculate_rsi(&data.klines, 14).map(|r| r.value);
        let ma_fast = calculate_ma(&data.klines, 7).map(|r| r.value);
        let ma_slow = calculate_ma(&data.klines, 25).map(|r| r.value);

        let market_context = serde_json::json!({
            "current_volume": current_volume,
            "volume_ma": volume_ma,
            "volume_ratio": volume_ratio,
            "price_change": price_change,
            "volume_ma_period": self.params.volume_ma_period,
            "price_change_threshold": self.params.price_change_threshold,
            "atr": atr,
            "rsi": rsi,
            "ma7": ma_fast,
            "ma25": ma_slow,
            "current_price": current_price,
            "kline_count": data.klines.len(),
            "timeframe": data.timeframe.as_str(),
        });

        if is_price_up {
            Some(Signal {
                signal_type: SignalType::Buy,
                signal_strength,
                entry_price: current_price,
                stop_loss,
                take_profit,
                confidence: 0.6,
                reason: format!(
                    "Volume spike + price up: ratio={:.2}, change={:.2}%, ATR={:.2}",
                    volume_ratio, price_change * 100.0, atr,
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
                confidence: 0.6,
                reason: format!(
                    "Volume spike + price down: ratio={:.2}, change={:.2}%, ATR={:.2}",
                    volume_ratio, price_change * 100.0, atr,
                ),
                market_context,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_klines(count: usize, base: f64, trend: f64, base_volume: f64) -> Vec<KlineBar> {
        (0..count)
            .map(|i| {
                let price = base + i as f64 * trend;
                KlineBar {
                    open: price - 0.1,
                    high: price + 0.5,
                    low: price - 0.5,
                    close: price,
                    volume: base_volume,
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
    fn test_volume_spike_buy() {
        // Create klines with normal volume, then a volume spike with price up
        let mut klines = make_klines(25, 100.0, 0.0, 1000.0);
        // Add a final kline with a volume spike and price increase
        let last_price = klines.last().map(|k| k.close).unwrap_or(100.0);
        klines.push(KlineBar {
            open: last_price,
            high: last_price + 2.0,
            low: last_price,
            close: last_price + 1.5,
            volume: 5000.0, // 5x normal volume
        });
        let data = make_market_data(klines, Timeframe::OneMinute);
        let strategy = VolumeStrategy::new(VolumeParams::default());
        let signal = strategy.analyze(&data);
        assert!(signal.is_some());
        let sig = signal.unwrap();
        assert_eq!(sig.signal_type, SignalType::Buy);
        assert!(sig.stop_loss.is_some());
        assert!(sig.take_profit.is_some());
        assert!(sig.signal_strength > 0.0);
    }

    #[test]
    fn test_volume_spike_sell() {
        // Create klines with normal volume, then a volume spike with price down
        let mut klines = make_klines(25, 100.0, 0.0, 1000.0);
        let last_price = klines.last().map(|k| k.close).unwrap_or(100.0);
        klines.push(KlineBar {
            open: last_price,
            high: last_price,
            low: last_price - 2.0,
            close: last_price - 1.5,
            volume: 5000.0, // 5x normal volume
        });
        let data = make_market_data(klines, Timeframe::OneMinute);
        let strategy = VolumeStrategy::new(VolumeParams::default());
        let signal = strategy.analyze(&data);
        assert!(signal.is_some());
        let sig = signal.unwrap();
        assert_eq!(sig.signal_type, SignalType::Sell);
    }

    #[test]
    fn test_no_signal_without_volume_spike() {
        // Normal volume, normal price change -> no signal
        let mut klines = make_klines(25, 100.0, 0.0, 1000.0);
        let last_price = klines.last().map(|k| k.close).unwrap_or(100.0);
        klines.push(KlineBar {
            open: last_price,
            high: last_price + 0.5,
            low: last_price - 0.5,
            close: last_price + 0.2,
            volume: 1100.0, // only 1.1x normal, below threshold
        });
        let data = make_market_data(klines, Timeframe::OneMinute);
        let strategy = VolumeStrategy::new(VolumeParams::default());
        let signal = strategy.analyze(&data);
        assert!(signal.is_none());
    }

    #[test]
    fn test_no_signal_with_spike_but_no_price_move() {
        // Volume spike but no significant price movement -> no signal
        let mut klines = make_klines(25, 100.0, 0.0, 1000.0);
        let last_price = klines.last().map(|k| k.close).unwrap_or(100.0);
        klines.push(KlineBar {
            open: last_price,
            high: last_price + 0.1,
            low: last_price - 0.1,
            close: last_price + 0.01,
            volume: 5000.0, // 5x volume, but only 0.01% price change
        });
        let data = make_market_data(klines, Timeframe::OneMinute);
        let strategy = VolumeStrategy::new(VolumeParams::default());
        let signal = strategy.analyze(&data);
        assert!(signal.is_none());
    }

    #[test]
    fn test_insufficient_data() {
        // Not enough klines
        let klines = make_klines(5, 100.0, 0.0, 1000.0);
        let data = make_market_data(klines, Timeframe::OneMinute);
        let strategy = VolumeStrategy::new(VolumeParams::default());
        let signal = strategy.analyze(&data);
        assert!(signal.is_none());
    }

    #[test]
    fn test_from_json() {
        let params = serde_json::json!({
            "volume_ma_period": 30,
            "volume_spike_threshold": 3.0,
            "price_change_threshold": 0.01
        });
        let strategy = VolumeStrategy::from_json(&params).unwrap();
        assert_eq!(strategy.params.volume_ma_period, 30);
        assert_eq!(strategy.params.volume_spike_threshold, 3.0);
        assert_eq!(strategy.params.price_change_threshold, 0.01);
    }

    #[test]
    fn test_default_params() {
        let params = VolumeParams::default();
        assert_eq!(params.volume_ma_period, 20);
        assert_eq!(params.volume_spike_threshold, 2.0);
        assert_eq!(params.price_change_threshold, 0.005);
    }
}
