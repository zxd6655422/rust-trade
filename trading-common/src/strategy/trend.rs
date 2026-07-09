//! Trend (Multi-MA + ADX Pullback) Strategy
//!
//! Ported from strategy-service/src/strategies/trend.rs
//! Uses three moving averages to identify trend direction, ADX to filter
//! weak trends, and pullback-to-MA entry logic with ATR-based stops.

use serde::{Deserialize, Serialize};

use super::rsi::{AtrResult, KlineBar, MaResult, MarketData, Signal, SignalType};

// =================================================================
// Additional indicator result types
// =================================================================

/// Multi-moving-average result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MultiMaResult {
    pub values: Vec<(usize, f64)>, // (period, value)
}

/// ADX (Average Directional Index) result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdxResult {
    pub adx: f64,
    pub plus_di: f64,
    pub minus_di: f64,
    pub period: usize,
}

// =================================================================
// Indicator calculation functions
// =================================================================

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

/// Calculate SMA for multiple periods
pub fn calculate_multi_ma(klines: &[KlineBar], periods: &[usize]) -> MultiMaResult {
    let values: Vec<(usize, f64)> = periods
        .iter()
        .filter_map(|&period| calculate_ma(klines, period).map(|result| (period, result.value)))
        .collect();

    MultiMaResult { values }
}

/// ATR (Average True Range) using Wilder smoothing
pub fn calculate_atr(klines: &[KlineBar], period: usize) -> Option<AtrResult> {
    if klines.len() < period + 1 || period == 0 {
        return None;
    }

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

    let first_atr: f64 = tr_values[..period].iter().sum::<f64>() / period as f64;

    let mut atr = first_atr;
    for &tr in &tr_values[period..] {
        atr = (atr * (period as f64 - 1.0) + tr) / period as f64;
    }

    Some(AtrResult {
        value: atr,
        period,
    })
}

/// ADX (Average Directional Index) using Wilder smoothing
pub fn calculate_adx(klines: &[KlineBar], period: usize) -> Option<AdxResult> {
    if klines.len() < period * 2 + 1 || period == 0 {
        return None;
    }

    // Compute +DM, -DM, TR
    let mut plus_dm: Vec<f64> = Vec::new();
    let mut minus_dm: Vec<f64> = Vec::new();
    let mut tr: Vec<f64> = Vec::new();

    for i in 1..klines.len() {
        let high_diff = klines[i].high - klines[i - 1].high;
        let low_diff = klines[i - 1].low - klines[i].low;

        let pdm = if high_diff > low_diff && high_diff > 0.0 {
            high_diff
        } else {
            0.0
        };
        let mdm = if low_diff > high_diff && low_diff > 0.0 {
            low_diff
        } else {
            0.0
        };

        let high_low = klines[i].high - klines[i].low;
        let high_prev_close = (klines[i].high - klines[i - 1].close).abs();
        let low_prev_close = (klines[i].low - klines[i - 1].close).abs();
        let tr_val = high_low.max(high_prev_close).max(low_prev_close);

        plus_dm.push(pdm);
        minus_dm.push(mdm);
        tr.push(tr_val);
    }

    if tr.len() < period * 2 {
        return None;
    }

    // Wilder smoothing
    let smooth_plus_dm = wilder_smooth(&plus_dm, period);
    let smooth_minus_dm = wilder_smooth(&minus_dm, period);
    let smooth_tr = wilder_smooth(&tr, period);

    if smooth_tr.is_empty() {
        return None;
    }

    // +DI, -DI
    let plus_di: Vec<f64> = smooth_plus_dm
        .iter()
        .zip(smooth_tr.iter())
        .map(|(&pdm, &t)| if t > 0.0 { 100.0 * pdm / t } else { 0.0 })
        .collect();
    let minus_di: Vec<f64> = smooth_minus_dm
        .iter()
        .zip(smooth_tr.iter())
        .map(|(&mdm, &t)| if t > 0.0 { 100.0 * mdm / t } else { 0.0 })
        .collect();

    // DX
    let dx: Vec<f64> = plus_di
        .iter()
        .zip(minus_di.iter())
        .map(|(&pdi, &mdi)| {
            let sum = pdi + mdi;
            if sum > 0.0 {
                100.0 * (pdi - mdi).abs() / sum
            } else {
                0.0
            }
        })
        .collect();

    // ADX = DX smoothed with Wilder method
    let adx_values = wilder_smooth(&dx, period);

    let adx = adx_values.last().copied().unwrap_or(0.0);
    let plus_di_val = plus_di.last().copied().unwrap_or(0.0);
    let minus_di_val = minus_di.last().copied().unwrap_or(0.0);

    Some(AdxResult {
        adx,
        plus_di: plus_di_val,
        minus_di: minus_di_val,
        period,
    })
}

/// Wilder smoothing (used by ADX and ATR)
fn wilder_smooth(values: &[f64], period: usize) -> Vec<f64> {
    if values.len() < period {
        return vec![];
    }

    let mut result = Vec::with_capacity(values.len() - period + 1);

    // First value: SMA
    let first: f64 = values[..period].iter().sum::<f64>() / period as f64;
    result.push(first);

    // Subsequent values: Wilder smoothing
    for &val in &values[period..] {
        let prev = *result.last().unwrap();
        result.push((prev * (period as f64 - 1.0) + val) / period as f64);
    }

    result
}

// =================================================================
// Trend Strategy
// =================================================================

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrendParams {
    pub fast_ma: usize,
    pub slow_ma: usize,
    pub trend_ma: usize,
    pub adx_threshold: f64,
}

impl Default for TrendParams {
    fn default() -> Self {
        Self {
            fast_ma: 7,
            slow_ma: 25,
            trend_ma: 99,
            adx_threshold: 25.0,
        }
    }
}

pub struct TrendStrategy {
    params: TrendParams,
}

impl TrendStrategy {
    /// Create a new Trend strategy with the given parameters.
    pub fn new(params: TrendParams) -> Self {
        Self { params }
    }

    /// Create from JSON parameters (compatible with strategy-service).
    pub fn from_json(params: &serde_json::Value) -> Result<Self, String> {
        let params: TrendParams =
            serde_json::from_value(params.clone()).map_err(|e| e.to_string())?;
        Ok(Self { params })
    }

    /// Get the strategy name.
    pub fn name(&self) -> &str {
        "trend"
    }

    /// Analyze market data and return an optional signal.
    pub fn analyze(&self, data: &MarketData) -> Option<Signal> {
        let current_price = data.current_price;

        // Calculate multi-MA
        let ma_periods = vec![self.params.fast_ma, self.params.slow_ma, self.params.trend_ma];
        let multi_ma = calculate_multi_ma(&data.klines, &ma_periods);

        // Get MA values
        let ma_fast = multi_ma
            .values
            .iter()
            .find(|(p, _)| *p == self.params.fast_ma)
            .map(|(_, v)| *v)?;
        let ma_slow = multi_ma
            .values
            .iter()
            .find(|(p, _)| *p == self.params.slow_ma)
            .map(|(_, v)| *v)?;
        let ma_trend = multi_ma
            .values
            .iter()
            .find(|(p, _)| *p == self.params.trend_ma)
            .map(|(_, v)| *v)?;

        // Trend direction
        let is_uptrend = ma_fast > ma_slow && ma_slow > ma_trend;
        let is_downtrend = ma_fast < ma_slow && ma_slow < ma_trend;

        if !is_uptrend && !is_downtrend {
            return None;
        }

        // ADX filter: only trade when trend is strong
        let adx_result = calculate_adx(&data.klines, 14);
        let adx_value = adx_result.map(|r| r.adx).unwrap_or(0.0);

        if adx_value < self.params.adx_threshold {
            return None;
        }

        // Pullback detection: price near MA
        let closes: Vec<f64> = data.klines.iter().map(|k| k.close).collect();
        let recent_closes = if closes.len() >= 5 {
            &closes[closes.len() - 5..]
        } else {
            &closes
        };

        let price_vs_ma_slow = (current_price - ma_slow) / ma_slow;

        // Pullback confirmation: price bouncing off MA
        let is_pullback_buy = is_uptrend
            && price_vs_ma_slow.abs() < 0.02
            && current_price > *recent_closes.last().unwrap_or(&0.0);
        let is_pullback_sell = is_downtrend
            && price_vs_ma_slow.abs() < 0.02
            && current_price < *recent_closes.last().unwrap_or(&0.0);

        if !is_pullback_buy && !is_pullback_sell {
            return None;
        }

        // ATR for stop-loss sizing
        let atr = calculate_atr(&data.klines, 14)
            .map(|r| r.value)
            .unwrap_or(current_price * 0.02);

        // Stop-loss and take-profit
        let (stop_loss, take_profit) = if is_pullback_buy {
            let stop_loss = current_price - 2.0 * atr;
            let take_profit = current_price + 3.0 * atr;
            (Some(stop_loss), Some(take_profit))
        } else {
            let stop_loss = current_price + 2.0 * atr;
            let take_profit = current_price - 3.0 * atr;
            (Some(stop_loss), Some(take_profit))
        };

        // Signal strength based on MA spread
        let signal_strength = if is_uptrend {
            ((ma_fast - ma_trend) / ma_trend * 10.0).min(1.0).max(0.0)
        } else {
            ((ma_trend - ma_fast) / ma_trend * 10.0).min(1.0).max(0.0)
        };

        let market_context = serde_json::json!({
            "ma_fast": ma_fast,
            "ma_slow": ma_slow,
            "ma_trend": ma_trend,
            "adx": adx_value,
            "atr": atr,
            "price_vs_ma_slow": price_vs_ma_slow,
            "is_uptrend": is_uptrend,
            "is_downtrend": is_downtrend,
            "fast_ma_period": self.params.fast_ma,
            "slow_ma_period": self.params.slow_ma,
            "trend_ma_period": self.params.trend_ma,
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
                reason: format!(
                    "上升趋势回调买入: MA{}={:.2} > MA{}={:.2} > MA{}={:.2}, ADX={:.1}, 价格接近MA{}",
                    self.params.fast_ma, ma_fast,
                    self.params.slow_ma, ma_slow,
                    self.params.trend_ma, ma_trend,
                    adx_value,
                    self.params.slow_ma,
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
                    "下降趋势反弹卖出: MA{}={:.2} < MA{}={:.2} < MA{}={:.2}, ADX={:.1}, 价格接近MA{}",
                    self.params.fast_ma, ma_fast,
                    self.params.slow_ma, ma_slow,
                    self.params.trend_ma, ma_trend,
                    adx_value,
                    self.params.slow_ma,
                ),
                market_context,
            })
        }
    }
}

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
    fn test_multi_ma() {
        let klines = make_klines(200, 100.0, 0.5);
        let periods = vec![7, 25, 99];
        let result = calculate_multi_ma(&klines, &periods);
        assert_eq!(result.values.len(), 3);
    }

    #[test]
    fn test_adx_calculation() {
        let klines = make_klines(100, 100.0, 0.5);
        let result = calculate_adx(&klines, 14);
        assert!(result.is_some());
        let adx = result.unwrap();
        assert!(adx.adx >= 0.0 && adx.adx <= 100.0);
    }

    #[test]
    fn test_trend_params_default() {
        let params = TrendParams::default();
        assert_eq!(params.fast_ma, 7);
        assert_eq!(params.slow_ma, 25);
        assert_eq!(params.trend_ma, 99);
        assert_eq!(params.adx_threshold, 25.0);
    }

    #[test]
    fn test_from_json() {
        let params = serde_json::json!({
            "fast_ma": 10,
            "slow_ma": 30,
            "trend_ma": 100,
            "adx_threshold": 20.0
        });
        let strategy = TrendStrategy::from_json(&params).unwrap();
        assert_eq!(strategy.params.fast_ma, 10);
        assert_eq!(strategy.params.slow_ma, 30);
        assert_eq!(strategy.params.trend_ma, 100);
        assert_eq!(strategy.params.adx_threshold, 20.0);
    }

    #[test]
    fn test_trend_strategy_name() {
        let strategy = TrendStrategy::new(TrendParams::default());
        assert_eq!(strategy.name(), "trend");
    }
}
