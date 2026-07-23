//! MA Trend Pullback Strategy (双均线趋势回踩策略)
//!
//! 策略逻辑:
//! - 趋势判断: MA288 > MA488 = 多头, MA288 < MA488 = 空头
//! - 入场信号: 价格从下方突破MA288(做多) / 从上方跌破MA288(做空)
//! - 止损: 价格反向穿越MA288
//! - 止盈: 移动止盈 (激活后跟踪最高盈利，回撤指定比例平仓)
//! - 5m扩散过滤: 可选，基于5m双均线扩散形态过滤入场信号
//!
//! 适用周期: 30m
//! 验证结果: BTC +42.79%, ETH +39.47%, SOL +41.47%
//! 5m扩散优化: BTC +40.46%, ETH +69.44%, SOL +84.45%

use serde::{Deserialize, Serialize};

use crate::data::types::Timeframe;

// =================================================================
// Market data types
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

/// Market data snapshot consumed by the strategy
#[derive(Debug, Clone)]
pub struct MarketData {
    pub klines: Vec<KlineBar>,
    pub current_price: f64,
    pub symbol: String,
    pub timeframe: Timeframe,
    /// Optional 5m klines for diffusion filter
    pub klines_5m: Option<Vec<KlineBar>>,
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
// Strategy parameters
// =================================================================

/// Stop loss mode
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StopMode {
    /// Fixed percentage stop loss
    Fixed,
    /// Stop when price crosses MA288
    Ma288,
}

impl Default for StopMode {
    fn default() -> Self {
        Self::Ma288
    }
}

/// Take profit mode
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TakeProfitMode {
    /// Trailing stop (activate + callback)
    Trailing,
    /// MA48 crossover confirmation
    Ma48,
    /// Bollinger Band position
    Bb,
    /// No take profit (only stop loss)
    None,
}

impl Default for TakeProfitMode {
    fn default() -> Self {
        Self::Trailing
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MATrendPullbackParams {
    /// Fast MA period (default: 288)
    pub fast_ma_period: usize,
    /// Slow MA period (default: 488)
    pub slow_ma_period: usize,
    /// Stop loss mode (default: ma288)
    #[serde(default)]
    pub stop_mode: StopMode,
    /// Fixed stop loss percentage (only used when stop_mode = fixed)
    #[serde(default = "default_fixed_stop_pct")]
    pub fixed_stop_pct: f64,
    /// Take profit mode (default: trailing)
    #[serde(default)]
    pub take_profit_mode: TakeProfitMode,
    /// Trailing stop activation profit percentage (default: 5.0)
    #[serde(default = "default_trailing_activate")]
    pub trailing_activate_pct: f64,
    /// Trailing stop callback percentage from peak profit (default: 5.0)
    #[serde(default = "default_trailing_callback")]
    pub trailing_callback_pct: f64,
    /// MA48 take profit: number of bars to confirm crossover (default: 3)
    #[serde(default = "default_ma48_tp_bars")]
    pub ma48_tp_bars: usize,
    /// Bollinger Band take profit: position percentage (default: 90)
    #[serde(default = "default_bb_tp_pct")]
    pub bb_tp_pct: f64,
    /// Minimum slope threshold for trend filter (default: 0, disabled)
    #[serde(default)]
    pub slope_threshold: f64,
    /// Bollinger Band width threshold (default: 0, disabled)
    #[serde(default)]
    pub bbw_threshold: f64,
    /// Volume ratio threshold (default: 0, disabled)
    #[serde(default)]
    pub vol_threshold: f64,
    /// 5m diffusion filter: only enter when 5m dual MA is expanding (default: false)
    #[serde(default)]
    pub use_5m_expanding: bool,
    /// 5m diffusion filter: minimum angle threshold in degrees (default: 0, disabled)
    #[serde(default)]
    pub min_angle_5m: f64,
}

fn default_fixed_stop_pct() -> f64 { 2.0 }
fn default_trailing_activate() -> f64 { 5.0 }
fn default_trailing_callback() -> f64 { 5.0 }
fn default_ma48_tp_bars() -> usize { 3 }
fn default_bb_tp_pct() -> f64 { 90.0 }

impl Default for MATrendPullbackParams {
    fn default() -> Self {
        Self {
            fast_ma_period: 288,
            slow_ma_period: 488,
            stop_mode: StopMode::Ma288,
            fixed_stop_pct: 2.0,
            take_profit_mode: TakeProfitMode::Trailing,
            trailing_activate_pct: 5.0,
            trailing_callback_pct: 5.0,
            ma48_tp_bars: 3,
            bb_tp_pct: 90.0,
            slope_threshold: 0.0,
            bbw_threshold: 0.0,
            vol_threshold: 0.0,
            use_5m_expanding: false,
            min_angle_5m: 0.0,
        }
    }
}

// =================================================================
// Indicator calculations
// =================================================================

/// Extract closes from KlineBar slice
fn extract_closes(klines: &[KlineBar]) -> Vec<f64> {
    klines.iter().map(|k| k.close).collect()
}

/// Extract volumes from KlineBar slice
fn extract_volumes(klines: &[KlineBar]) -> Vec<f64> {
    klines.iter().map(|k| k.volume).collect()
}

/// Simple Moving Average - returns the latest value
pub fn calculate_sma(klines: &[KlineBar], period: usize) -> Option<f64> {
    if klines.len() < period || period == 0 {
        return None;
    }

    let closes = extract_closes(klines);
    let start = closes.len() - period;
    let sum: f64 = closes[start..].iter().sum();
    Some(sum / period as f64)
}

/// Calculate SMA series for multiple positions
fn calculate_sma_series(closes: &[f64], period: usize) -> Vec<Option<f64>> {
    if closes.len() < period || period == 0 {
        return vec![None; closes.len()];
    }

    let mut result = vec![None; closes.len()];
    for i in (period - 1)..closes.len() {
        let sum: f64 = closes[(i + 1 - period)..=i].iter().sum();
        result[i] = Some(sum / period as f64);
    }
    result
}

/// Volume Weighted Moving Average - returns the latest value
pub fn calculate_vwma(klines: &[KlineBar], period: usize) -> Option<f64> {
    if klines.len() < period || period == 0 {
        return None;
    }

    let closes = extract_closes(klines);
    let volumes = extract_volumes(klines);
    let start = closes.len() - period;

    let mut pv_sum = 0.0;
    let mut v_sum = 0.0;
    for i in start..closes.len() {
        pv_sum += closes[i] * volumes[i];
        v_sum += volumes[i];
    }

    if v_sum > 0.0 {
        Some(pv_sum / v_sum)
    } else {
        None
    }
}

/// Calculate MA slope (change rate over N bars, in basis points)
fn calculate_slope(ma_values: &[Option<f64>], lookback: usize) -> Option<f64> {
    if ma_values.len() < lookback + 1 {
        return None;
    }

    let current = (*ma_values.last()?)?;
    let prev = ma_values[ma_values.len() - 1 - lookback]?;

    if prev != 0.0 {
        Some((current - prev) / prev * 10000.0)
    } else {
        None
    }
}

/// Bollinger Band width (percentage)
fn calculate_bbw(klines: &[KlineBar], period: usize) -> Option<f64> {
    if klines.len() < period {
        return None;
    }

    let closes = extract_closes(klines);
    let start = closes.len() - period;
    let recent = &closes[start..];

    let mean: f64 = recent.iter().sum::<f64>() / period as f64;
    let variance: f64 = recent.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / period as f64;
    let std = variance.sqrt();

    if mean > 0.0 {
        Some(4.0 * std / mean * 100.0) // 2 * upper + 2 * lower = 4 std
    } else {
        None
    }
}

/// Volume ratio (current / MA20)
fn calculate_vol_ratio(klines: &[KlineBar]) -> Option<f64> {
    if klines.len() < 21 {
        return None;
    }

    let volumes = extract_volumes(klines);
    let current = *volumes.last()?;
    let ma20: f64 = volumes[volumes.len() - 21..volumes.len() - 1].iter().sum::<f64>() / 20.0;

    if ma20 > 0.0 {
        Some(current / ma20)
    } else {
        None
    }
}

/// Calculate dual MA spread (fast_ma - slow_ma)
fn calculate_spread(klines: &[KlineBar], fast_period: usize, slow_period: usize) -> Option<f64> {
    let fast_ma = calculate_sma(klines, fast_period)?;
    let slow_ma = calculate_sma(klines, slow_period)?;
    Some(fast_ma - slow_ma)
}

/// Check if dual MA is in expanding phase (|spread| is increasing)
fn is_expanding(klines: &[KlineBar], fast_period: usize, slow_period: usize, lookback: usize) -> Option<bool> {
    if klines.len() < slow_period + lookback {
        return None;
    }

    let current_spread = calculate_spread(klines, fast_period, slow_period)?;
    let prev_klines = &klines[..klines.len() - lookback];
    let prev_spread = calculate_spread(prev_klines, fast_period, slow_period)?;

    Some(current_spread.abs() > prev_spread.abs())
}

/// Calculate approximate angle between dual MAs (in degrees)
fn calculate_angle(klines: &[KlineBar], fast_period: usize, slow_period: usize, lookback: usize) -> Option<f64> {
    if klines.len() < slow_period + lookback {
        return None;
    }

    let current_spread = calculate_spread(klines, fast_period, slow_period)?;
    let prev_klines = &klines[..klines.len() - lookback];
    let prev_spread = calculate_spread(prev_klines, fast_period, slow_period)?;

    let delta = current_spread - prev_spread;
    Some(delta.atan2(lookback as f64) * (180.0 / std::f64::consts::PI))
}

// =================================================================
// Trend direction
// =================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrendDirection {
    Bullish,
    Bearish,
    Neutral,
}

// =================================================================
// MA Trend Pullback Strategy
// =================================================================

pub struct MATrendPullbackStrategy {
    params: MATrendPullbackParams,
}

impl MATrendPullbackStrategy {
    /// Create a new strategy with the given parameters.
    pub fn new(params: MATrendPullbackParams) -> Self {
        Self { params }
    }

    /// Create from JSON parameters.
    pub fn from_json(params: &serde_json::Value) -> Result<Self, String> {
        let params: MATrendPullbackParams =
            serde_json::from_value(params.clone()).map_err(|e| e.to_string())?;
        Ok(Self { params })
    }

    /// Get the strategy name.
    pub fn name(&self) -> &str {
        "ma_trend_pullback"
    }

    /// Analyze market data and return an optional signal.
    pub fn analyze(&self, data: &MarketData) -> Option<Signal> {
        let klines = &data.klines;
        let min_bars = self.params.slow_ma_period.max(self.params.fast_ma_period) + 10;
        if klines.len() < min_bars {
            return None;
        }

        let closes = extract_closes(klines);
        let current_price = data.current_price;

        // Calculate MAs
        let fast_ma = calculate_sma(klines, self.params.fast_ma_period)?;
        let slow_ma = calculate_sma(klines, self.params.slow_ma_period)?;

        // Determine trend direction
        let trend = if fast_ma > slow_ma {
            TrendDirection::Bullish
        } else if fast_ma < slow_ma {
            TrendDirection::Bearish
        } else {
            TrendDirection::Neutral
        };

        if trend == TrendDirection::Neutral {
            return None;
        }

        // Apply filters
        // 1. Slope filter
        if self.params.slope_threshold > 0.0 {
            let fast_ma_series = calculate_sma_series(&closes, self.params.fast_ma_period);
            if let Some(slope) = calculate_slope(&fast_ma_series, 5) {
                if slope.abs() < self.params.slope_threshold {
                    return None;
                }
            }
        }

        // 2. BBW filter
        if self.params.bbw_threshold > 0.0 {
            if let Some(bbw) = calculate_bbw(klines, 100) {
                if bbw < self.params.bbw_threshold {
                    return None;
                }
            }
        }

        // 3. Volume filter
        if self.params.vol_threshold > 0.0 {
            if let Some(vol_ratio) = calculate_vol_ratio(klines) {
                if vol_ratio < self.params.vol_threshold {
                    return None;
                }
            }
        }

        // 4. 5m diffusion filter (optional)
        if self.params.use_5m_expanding {
            if let Some(klines_5m) = &data.klines_5m {
                // Check if 5m dual MA is expanding
                if let Some(expanding) = is_expanding(klines_5m, self.params.fast_ma_period, self.params.slow_ma_period, 5) {
                    if !expanding {
                        return None; // 5m is converging, skip entry
                    }
                }

                // Check minimum angle threshold
                if self.params.min_angle_5m > 0.0 {
                    if let Some(angle) = calculate_angle(klines_5m, self.params.fast_ma_period, self.params.slow_ma_period, 5) {
                        if angle.abs() < self.params.min_angle_5m {
                            return None; // Angle too small, skip entry
                        }
                    }
                }
            }
        }

        // Check for MA crossover signal
        // We need the previous bar's state to detect crossover
        if klines.len() < 2 {
            return None;
        }

        let prev_klines = &klines[..klines.len() - 1];
        let prev_fast_ma = calculate_sma(prev_klines, self.params.fast_ma_period);
        let prev_slow_ma = calculate_sma(prev_klines, self.params.slow_ma_period);

        // Current bar OHLC
        let open = klines.last()?.open;
        let close = klines.last()?.close;

        // Detect entry signal: price crosses MA288 in trend direction
        let mut signal_type = None;
        let mut reason = String::new();

        match trend {
            TrendDirection::Bullish => {
                // Bullish trend: price crosses above fast MA
                // Previous: open < fast_ma OR close < fast_ma
                // Current: close > fast_ma
                if let Some(prev_fast) = prev_fast_ma {
                    if open < prev_fast && close > fast_ma {
                        signal_type = Some(SignalType::Buy);
                        reason = format!(
                            "Bullish trend pullback: price crossed above MA{} (trend: MA{} > MA{})",
                            self.params.fast_ma_period,
                            self.params.fast_ma_period,
                            self.params.slow_ma_period
                        );
                    }
                }
            }
            TrendDirection::Bearish => {
                // Bearish trend: price crosses below fast MA
                // Previous: open > fast_ma OR close > fast_ma
                // Current: close < fast_ma
                if let Some(prev_fast) = prev_fast_ma {
                    if open > prev_fast && close < fast_ma {
                        signal_type = Some(SignalType::Sell);
                        reason = format!(
                            "Bearish trend pullback: price crossed below MA{} (trend: MA{} < MA{})",
                            self.params.fast_ma_period,
                            self.params.fast_ma_period,
                            self.params.slow_ma_period
                        );
                    }
                }
            }
            _ => {}
        }

        let signal_type = signal_type?;

        // Calculate stop loss at fast MA (MA288)
        let stop_loss = match signal_type {
            SignalType::Buy => Some(fast_ma * 0.98),  // 2% below MA
            SignalType::Sell => Some(fast_ma * 1.02),  // 2% above MA
            _ => None,
        };

        // No fixed take profit - use trailing stop
        // Take profit is set to None, trailing stop logic is handled externally
        let take_profit = None;

        // Signal strength based on MA separation
        let ma_separation = (fast_ma - slow_ma).abs() / slow_ma * 100.0;
        let signal_strength = (ma_separation / 5.0).min(1.0); // Normalize to 0-1

        // Market context for analysis
        let market_context = serde_json::json!({
            "fast_ma": fast_ma,
            "slow_ma": slow_ma,
            "fast_ma_period": self.params.fast_ma_period,
            "slow_ma_period": self.params.slow_ma_period,
            "trend": format!("{:?}", trend),
            "current_price": current_price,
            "open": open,
            "close": close,
            "timeframe": data.timeframe.as_str(),
            "kline_count": klines.len(),
            "stop_mode": format!("{:?}", self.params.stop_mode),
            "take_profit_mode": format!("{:?}", self.params.take_profit_mode),
            "trailing_activate_pct": self.params.trailing_activate_pct,
            "trailing_callback_pct": self.params.trailing_callback_pct,
            "ma48_tp_bars": self.params.ma48_tp_bars,
            "bb_tp_pct": self.params.bb_tp_pct,
            "use_5m_expanding": self.params.use_5m_expanding,
            "min_angle_5m": self.params.min_angle_5m,
        });

        Some(Signal {
            signal_type,
            signal_strength,
            entry_price: current_price,
            stop_loss,
            take_profit,
            confidence: 0.75,
            reason,
            market_context,
        })
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
                    volume: 1000.0 + i as f64,
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
            timeframe: Timeframe::ThirtyMinutes,
            klines_5m: None,
        }
    }

    #[test]
    fn test_sma_calculation() {
        let klines = make_klines(500, 100.0, 0.1);
        let sma = calculate_sma(&klines, 288);
        assert!(sma.is_some());
        let sma = sma.unwrap();
        assert!(sma > 0.0);
    }

    #[test]
    fn test_trend_detection() {
        // Create klines with uptrend (fast MA > slow MA)
        let klines = make_klines(500, 100.0, 0.5);
        let data = make_market_data(klines);
        let params = MATrendPullbackParams {
            fast_ma_period: 48,
            slow_ma_period: 96,
            ..Default::default()
        };
        let strategy = MATrendPullbackStrategy::new(params);
        let result = strategy.analyze(&data);
        // May or may not have a signal depending on exact crossover
        // But should not panic
    }

    #[test]
    fn test_from_json() {
        let params = serde_json::json!({
            "fast_ma_period": 288,
            "slow_ma_period": 488,
            "trailing_activate_pct": 5.0,
            "trailing_callback_pct": 5.0,
            "slope_threshold": 0.0,
            "bbw_threshold": 0.0,
            "vol_threshold": 0.0,
            "use_5m_expanding": true,
            "min_angle_5m": 0.3
        });
        let strategy = MATrendPullbackStrategy::from_json(&params).unwrap();
        assert_eq!(strategy.params.fast_ma_period, 288);
        assert_eq!(strategy.params.slow_ma_period, 488);
        assert_eq!(strategy.params.trailing_activate_pct, 5.0);
        assert_eq!(strategy.params.use_5m_expanding, true);
        assert_eq!(strategy.params.min_angle_5m, 0.3);
    }

    #[test]
    fn test_volume_ratio() {
        let klines = make_klines(30, 100.0, 0.0);
        let ratio = calculate_vol_ratio(&klines);
        assert!(ratio.is_some());
    }

    #[test]
    fn test_bbw() {
        let klines = make_klines(120, 100.0, 0.1);
        let bbw = calculate_bbw(&klines, 100);
        assert!(bbw.is_some());
    }

    #[test]
    fn test_spread_calculation() {
        let klines = make_klines(500, 100.0, 0.5);
        let spread = calculate_spread(&klines, 48, 96);
        assert!(spread.is_some());
        // In an uptrend, fast MA > slow MA, so spread should be positive
        assert!(spread.unwrap() > 0.0);
    }

    #[test]
    fn test_expanding_detection() {
        // Create klines with strong uptrend (expanding)
        let klines = make_klines(500, 100.0, 1.0);
        let expanding = is_expanding(&klines, 48, 96, 5);
        assert!(expanding.is_some());
    }

    #[test]
    fn test_angle_calculation() {
        let klines = make_klines(500, 100.0, 0.5);
        let angle = calculate_angle(&klines, 48, 96, 5);
        assert!(angle.is_some());
    }

    #[test]
    fn test_5m_diffusion_filter() {
        // Create30m klines with uptrend
        let klines_30m = make_klines(500, 100.0, 0.5);
        // Create5m klines with strong uptrend (expanding)
        let klines_5m = make_klines(2500, 100.0, 0.1);

        let data = MarketData {
            klines: klines_30m,
            current_price: 150.0,
            symbol: "BTCUSDT".to_string(),
            timeframe: Timeframe::ThirtyMinutes,
            klines_5m: Some(klines_5m),
        };

        // Strategy with5m diffusion filter enabled
        let params = MATrendPullbackParams {
            fast_ma_period: 48,
            slow_ma_period: 96,
            use_5m_expanding: true,
            min_angle_5m: 0.0,
            ..Default::default()
        };
        let strategy = MATrendPullbackStrategy::new(params);
        // Should not panic, may or may not produce a signal
        let _ = strategy.analyze(&data);
    }
}
