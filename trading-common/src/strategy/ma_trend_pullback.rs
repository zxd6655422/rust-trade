//! MA Trend Pullback Strategy (双均线趋势回踩策略)
//!
//! ===========================================================================
//! 策略概述
//! ===========================================================================
//!
//! 基于双均线(MA288/MA488)判断趋势方向，价格回踩MA288时入场的趋势跟踪策略。
//!
//! ===========================================================================
//! 核心逻辑
//! ===========================================================================
//!
//! 1. 趋势判断 (30m K线):
//!    - 多头趋势: MA288 > MA488
//!    - 空头趋势: MA288 < MA488
//!
//! 2. 入场信号 (30m或5m K线):
//!    - 做多: 趋势为多头 + 开盘价在MA288下方 + 收盘价突破MA288上方
//!    - 做空: 趋势为空头 + 开盘价在MA288上方 + 收盘价跌破MA288下方
//!
//! 3. 止损逻辑:
//!    - MA288止损: 做多时收盘价跌破MA288 / 做空时收盘价突破MA288
//!    - 固定止损: 亏损超过指定百分比(默认2%)
//!
//! 4. 止盈逻辑:
//!    - 移动止盈(默认): 盈利达到激活阈值(5%)后启动跟踪，回撤指定比例(5%)平仓
//!    - MA48止盈: 连续N根K线收盘价穿越MA48时平仓
//!    - BB止盈: 价格触及布林带指定位置(90%)时平仓
//!
//! ===========================================================================
//! 下单操作 (开仓)
//! ===========================================================================
//!
//! 当策略返回 Buy/Sell 信号时，执行以下操作:
//!
//! 【做多信号 (SignalType::Buy)】
//! - 条件: 30m趋势为多头(MA288>MA488) + 价格从下方突破MA288
//! - 操作: 开多仓 (买入)
//! - 止损价: MA288 * 0.98 (MA288下方2%)
//!
//! 【做空信号 (SignalType::Sell)】
//! - 条件: 30m趋势为空头(MA288<MA488) + 价格从上方跌破MA288
//! - 操作: 开空仓 (卖出)
//! - 止损价: MA288 * 1.02 (MA288上方2%)
//!
//! ===========================================================================
//! 平仓操作 (止损/止盈)
//! ===========================================================================
//!
//! 【止损平仓】
//! - MA288止损: 做多持仓时，收盘价跌破MA288 → 平多
//!              做空持仓时，收盘价突破MA288 → 平空
//! - 固定止损: 亏损超过 fixed_stop_pct (默认2%) → 平仓
//!
//! 【止盈平仓】
//! - 移动止盈: 盈利达到 trailing_activate_pct (5%)后启动跟踪
//!             从最高盈利回撤 trailing_callback_pct (5%)时平仓
//! - MA48止盈: 连续 ma48_tp_bars (3)根K线收盘价穿越MA48时平仓
//! - BB止盈: 价格在布林带位置达到 bb_tp_pct (90%)时平仓
//!
//! 【趋势反转平仓】
//! - 做多持仓时，MA288跌破MA488(趋势转空) → 平多
//! - 做空持仓时，MA288突破MA488(趋势转多) → 平空
//!
//! ===========================================================================
//! 反手操作说明 (重要)
//! ===========================================================================
//!
//! 根据回测验证，30m入场策略不建议反手操作:
//!
//! 【30m入场 - 不反手】(推荐)
//! - 当持有反向仓位时，收到入场信号 → 只平旧仓，不开新仓
//! - 等待下一个信号再开仓
//! - 回测结果: SOL +93.97%, ETH +66.68%, BTC +51.45%
//!
//! 【5m入场 - 可反手】
//! - 当持有反向仓位时，收到入场信号 → 平旧仓 + 开新仓
//! - 回测结果: ETH +69.44%, BTC +40.46%
//!
//! ===========================================================================
//! 5m扩散过滤 (第十三次分析优化)
//! ===========================================================================
//!
//! 可选功能，通过检测5m双均线扩散形态过滤入场信号:
//!
//! - use_5m_expanding: 启用5m扩散过滤
//!   - 只在5m MA288/MA488价差扩大时入场
//!   - 过滤收敛阶段的假信号
//!
//! - min_angle_5m: 最小夹角阈值
//!   - 0: 不限制夹角
//!   - 1.0: 只保留强趋势(推荐ETH)
//!
//! - entry_timeframe: 入场K线周期
//!   - "30m": 用30m K线检测入场信号(默认)
//!   - "5m": 用5m K线检测入场信号，趋势仍用30m判断
//!
//! ===========================================================================
//! 参数配置推荐
//! ===========================================================================
//!
//! 【BTC配置】
//! - entry_timeframe: "30m"
//! - use_5m_expanding: false
//! - 预期收益: +42.79% ~ +51.45%
//!
//! 【ETH配置】(推荐5m入场+扩散)
//! - entry_timeframe: "5m"
//! - use_5m_expanding: true
//! - min_angle_5m: 1.0
//! - 预期收益: +54.63% ~ +69.44%
//!
//! 【SOL配置】(推荐30m入场+扩散)
//! - entry_timeframe: "30m"
//! - use_5m_expanding: true
//! - min_angle_5m: 0
//! - 预期收益: +84.45% ~ +93.97%
//!
//! ===========================================================================
//! 回测验证结果
//! ===========================================================================
//!
//! 基础策略 (30m + MA288止损, 无扩散过滤):
//! - BTC: +42.79% (胜率15.2%, 盈亏比10.75)
//! - ETH: +39.47% (胜率21.4%, 盈亏比7.23)
//! - SOL: +41.47% (胜率15.3%, 盈亏比2.87)
//!
//! 优化策略 (30m + MA288止损 + 5m扩散 + 不反手):
//! - BTC: +51.45% (胜率16.7%, 盈亏比10.75)
//! - ETH: +66.68% (胜率14.6%, 盈亏比3.40)
//! - SOL: +93.97% (胜率25.2%, 盈亏比2.01)
//!
//! 5m入场策略 (5m入场 + 30m趋势 + 5m扩散 + 反手):
//! - BTC: +40.46% (胜率18.4%)
//! - ETH: +69.44% (胜率23.3%)

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
    /// Hard stop loss percentage from entry price (default: 0, disabled)
    /// Set to 1.0-2.5 depending on coin volatility
    /// BTC: 1.0, ETH: 1.5, SOL: 2.5
    #[serde(default)]
    pub hard_stop_pct: f64,
    /// Take profit mode (default: trailing)
    #[serde(default)]
    pub take_profit_mode: TakeProfitMode,
    /// Trailing stop activation profit percentage (default: 5.0)
    #[serde(default = "default_trailing_activate")]
    pub trailing_activate_pct: f64,
    /// Trailing stop callback percentage from peak profit (default: 5.0)
    #[serde(default = "default_trailing_callback")]
    pub trailing_callback_pct: f64,
    /// Minimum slope threshold for trend filter (default: 0, disabled)
    #[serde(default)]
    pub slope_threshold: f64,
    /// Bollinger Band width threshold (default: 0, disabled)
    #[serde(default)]
    pub bbw_threshold: f64,
    /// Volume ratio threshold (default: 0, disabled)
    #[serde(default)]
    pub vol_threshold: f64,
    /// Realized volatility 48-period threshold (default: 0, disabled)
    /// Skip entry when realized_vol_48 >= threshold
    /// Per-coin recommended values from studies/001:
    /// BTC: 0.426, ETH: 0.445, SOL: 0.790, BNB: 0.488, SUI: 0.788, HYPE: 0.646
    #[serde(default)]
    pub realized_vol_threshold: f64,
    /// 30m diffusion filter: only enter when 30m dual MA is expanding (default: false)
    #[serde(default)]
    pub use_30m_expanding: bool,
    /// 5m diffusion filter: only enter when 5m dual MA is expanding (default: false)
    #[serde(default)]
    pub use_5m_expanding: bool,
    /// 5m diffusion filter: minimum angle threshold in degrees (default: 0, disabled)
    #[serde(default)]
    pub min_angle_5m: f64,
    /// Entry timeframe: "30m" or "5m" (default: "30m")
    #[serde(default = "default_entry_timeframe")]
    pub entry_timeframe: String,
}

fn default_entry_timeframe() -> String { "30m".to_string() }

fn default_fixed_stop_pct() -> f64 { 2.0 }
fn default_trailing_activate() -> f64 { 5.0 }
fn default_trailing_callback() -> f64 { 5.0 }

impl Default for MATrendPullbackParams {
    fn default() -> Self {
        Self {
            fast_ma_period: 288,
            slow_ma_period: 488,
            stop_mode: StopMode::Ma288,
            fixed_stop_pct: 2.0,
            hard_stop_pct: 0.0,  // Disabled by default, enable per coin
            take_profit_mode: TakeProfitMode::Trailing,
            trailing_activate_pct: 5.0,
            trailing_callback_pct: 5.0,
            slope_threshold: 0.0,
            bbw_threshold: 0.0,
            vol_threshold: 0.0,
            realized_vol_threshold: 0.0,
            use_30m_expanding: false,
            use_5m_expanding: false,
            min_angle_5m: 0.0,
            entry_timeframe: "30m".to_string(),
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

/// Realized volatility over 48 periods (population std of simple returns * 100)
/// Matches the Python implementation in indicators.py::_rolling_std_returns
fn calculate_realized_vol_48(klines: &[KlineBar]) -> Option<f64> {
    if klines.len() < 49 {
        return None;
    }

    let closes = extract_closes(klines);
    let n = closes.len();

    // Calculate simple returns: rets[i] = closes[i] / closes[i-1] - 1
    let mut rets = vec![0.0f64; n];
    for i in 1..n {
        if closes[i - 1] != 0.0 {
            rets[i] = closes[i] / closes[i - 1] - 1.0;
        }
    }

    // Rolling population std over last 48 returns (inclusive of current bar)
    let window = 48usize;
    if n < window {
        return None;
    }

    let start = n - window;
    let sum: f64 = rets[start..n].iter().sum();
    let sum_sq: f64 = rets[start..n].iter().map(|x| x * x).sum();
    let mean = sum / window as f64;
    let variance = (sum_sq / window as f64 - mean * mean).max(0.0);
    Some(variance.sqrt() * 100.0)
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
            tracing::debug!(
                "[{}] 数据不足: 需要{}根K线, 实际{}根",
                data.symbol, min_bars, klines.len()
            );
            return None;
        }

        let closes = extract_closes(klines);
        let current_price = data.current_price;

        // Calculate MAs on 30m (for trend direction)
        let fast_ma = calculate_sma(klines, self.params.fast_ma_period)?;
        let slow_ma = calculate_sma(klines, self.params.slow_ma_period)?;

        // Determine trend direction using 30m MAs
        let trend = if fast_ma > slow_ma {
            TrendDirection::Bullish
        } else if fast_ma < slow_ma {
            TrendDirection::Bearish
        } else {
            TrendDirection::Neutral
        };

        let spread_pct = (fast_ma - slow_ma).abs() / slow_ma * 100.0;
        tracing::debug!(
            "[{}] 趋势分析: MA{}={:.4}, MA{}={:.4}, 价差={:.4}%, 方向={:?}, 当前价={:.4}",
            data.symbol, self.params.fast_ma_period, fast_ma,
            self.params.slow_ma_period, slow_ma, spread_pct, trend, current_price
        );

        if trend == TrendDirection::Neutral {
            tracing::debug!("[{}] 跳过: 双均线交叉(中性), 无趋势方向", data.symbol);
            return None;
        }

        // Apply filters (on30m klines)
        // 1. Slope filter
        if self.params.slope_threshold > 0.0 {
            let fast_ma_series = calculate_sma_series(&closes, self.params.fast_ma_period);
            if let Some(slope) = calculate_slope(&fast_ma_series, 5) {
                tracing::debug!(
                    "[{}] 过滤器1-斜率: slope={:.6}, 阈值={:.6}, {}",
                    data.symbol, slope.abs(), self.params.slope_threshold,
                    if slope.abs() < self.params.slope_threshold { "❌ 未通过" } else { "✅ 通过" }
                );
                if slope.abs() < self.params.slope_threshold {
                    return None;
                }
            }
        }

        // 2. BBW filter
        if self.params.bbw_threshold > 0.0 {
            if let Some(bbw) = calculate_bbw(klines, 100) {
                tracing::debug!(
                    "[{}] 过滤器2-BBW: bbw={:.6}, 阈值={:.6}, {}",
                    data.symbol, bbw, self.params.bbw_threshold,
                    if bbw < self.params.bbw_threshold { "❌ 未通过" } else { "✅ 通过" }
                );
                if bbw < self.params.bbw_threshold {
                    return None;
                }
            }
        }

        // 3. Volume filter
        if self.params.vol_threshold > 0.0 {
            if let Some(vol_ratio) = calculate_vol_ratio(klines) {
                tracing::debug!(
                    "[{}] 过滤器3-成交量: vol_ratio={:.4}, 阈值={:.4}, {}",
                    data.symbol, vol_ratio, self.params.vol_threshold,
                    if vol_ratio < self.params.vol_threshold { "❌ 未通过" } else { "✅ 通过" }
                );
                if vol_ratio < self.params.vol_threshold {
                    return None;
                }
            }
        }

        // 3b. Realized volatility filter (skip high-vol entries)
        if self.params.realized_vol_threshold > 0.0 {
            if let Some(rv48) = calculate_realized_vol_48(klines) {
                tracing::debug!(
                    "[{}] 过滤器3b-波动率: realized_vol_48={:.4}, 阈值={:.4}, {}",
                    data.symbol, rv48, self.params.realized_vol_threshold,
                    if rv48 >= self.params.realized_vol_threshold { "❌ 未通过(高波动)" } else { "✅ 通过" }
                );
                if rv48 >= self.params.realized_vol_threshold {
                    return None;
                }
            }
        }

        // 4. 30m diffusion filter (optional)
        if self.params.use_30m_expanding {
            if let Some(expanding) = is_expanding(klines, self.params.fast_ma_period, self.params.slow_ma_period, 5) {
                tracing::debug!(
                    "[{}] 过滤器4-30m扩散: expanding={}, {}",
                    data.symbol, expanding,
                    if !expanding { "❌ 未通过(收敛)" } else { "✅ 通过(扩散)" }
                );
                if !expanding {
                    return None; // 30m is converging, skip entry
                }
            }
        }

        // 5. 5m diffusion filter (optional)
        if self.params.use_5m_expanding {
            if let Some(klines_5m) = &data.klines_5m {
                // Check if 5m dual MA is expanding
                if let Some(expanding) = is_expanding(klines_5m, self.params.fast_ma_period, self.params.slow_ma_period, 5) {
                    tracing::debug!(
                        "[{}] 过滤器5a-5m扩散: expanding={}, {}",
                        data.symbol, expanding,
                        if !expanding { "❌ 未通过(收敛)" } else { "✅ 通过(扩散)" }
                    );
                    if !expanding {
                        return None; // 5m is converging, skip entry
                    }
                }

                // Check minimum angle threshold
                if self.params.min_angle_5m > 0.0 {
                    if let Some(angle) = calculate_angle(klines_5m, self.params.fast_ma_period, self.params.slow_ma_period, 5) {
                        tracing::debug!(
                            "[{}] 过滤器5b-5m角度: angle={:.4}°, 阈值={:.4}°, {}",
                            data.symbol, angle.abs(), self.params.min_angle_5m,
                            if angle.abs() < self.params.min_angle_5m { "❌ 未通过" } else { "✅ 通过" }
                        );
                        if angle.abs() < self.params.min_angle_5m {
                            return None; // Angle too small, skip entry
                        }
                    }
                }
            } else {
                tracing::debug!("[{}] 过滤器5-5m扩散: 无5m数据, 跳过此过滤器", data.symbol);
            }
        }

        // Determine entry klines based on entry_timeframe parameter
        let entry_klines: &Vec<KlineBar> = if self.params.entry_timeframe == "5m" {
            // Use 5m klines for entry signal if available
            match &data.klines_5m {
                Some(klines_5m) if klines_5m.len() >= 2 => klines_5m,
                _ => {
                    tracing::debug!("[{}] 入场时间框架回退: 5m数据不足, 使用30m", data.symbol);
                    klines
                },
            }
        } else {
            klines // Use 30m klines for entry
        };

        // Check for MA crossover signal on entry klines
        if entry_klines.len() < 2 {
            tracing::debug!("[{}] 入场K线不足: 需要至少2根, 实际{}根", data.symbol, entry_klines.len());
            return None;
        }

        let entry_fast_ma = calculate_sma(entry_klines, self.params.fast_ma_period)?;
        let prev_entry_klines = &entry_klines[..entry_klines.len() - 1];
        let prev_entry_fast_ma = calculate_sma(prev_entry_klines, self.params.fast_ma_period);

        // Current entry bar OHLC + previous close
        // 使用前一根K线的close（而非当前open）判断穿越前状态，与JS回测保持一致
        let prev_close = entry_klines[entry_klines.len() - 2].close;
        let open = entry_klines.last()?.open;
        let close = entry_klines.last()?.close;

        tracing::debug!(
            "[{}] 入场条件检查: {}入场, prev_close={:.4}, open={:.4}, close={:.4}, MA{}={:.4}, 前一根MA{}={:.4}",
            data.symbol, self.params.entry_timeframe,
            prev_close, open, close, self.params.fast_ma_period, entry_fast_ma,
            self.params.fast_ma_period, prev_entry_fast_ma.unwrap_or(0.0)
        );

        // Detect entry signal: price crosses MA288 in trend direction
        // 使用 prev_close（前一根收盘价）判断穿越前状态，与回测逻辑一致
        let mut signal_type = None;
        let mut reason = String::new();

        match trend {
            TrendDirection::Bullish => {
                // Bullish trend: price crosses above fast MA
                if let Some(prev_fast) = prev_entry_fast_ma {
                    let crossed = prev_close < prev_fast && close > entry_fast_ma;
                    tracing::debug!(
                        "[{}] 做多条件: prev_close({:.4}) < 前MA{}({:.4})? {} AND close({:.4}) > MA{}({:.4})? {} → {}",
                        data.symbol, prev_close, self.params.fast_ma_period, prev_fast,
                        prev_close < prev_fast, close, self.params.fast_ma_period, entry_fast_ma,
                        close > entry_fast_ma,
                        if crossed { "✅ 触发做多信号" } else { "❌ 未触发" }
                    );
                    if crossed {
                        signal_type = Some(SignalType::Buy);
                        reason = format!(
                            "Bullish trend pullback: price crossed above MA{} on {} (trend: MA{} > MA{})",
                            self.params.fast_ma_period,
                            self.params.entry_timeframe,
                            self.params.fast_ma_period,
                            self.params.slow_ma_period
                        );
                    }
                } else {
                    tracing::debug!("[{}] 做多条件: 无法计算前一根MA{}, 跳过", data.symbol, self.params.fast_ma_period);
                }
            }
            TrendDirection::Bearish => {
                // Bearish trend: price crosses below fast MA
                if let Some(prev_fast) = prev_entry_fast_ma {
                    let crossed = prev_close > prev_fast && close < entry_fast_ma;
                    tracing::debug!(
                        "[{}] 做空条件: prev_close({:.4}) > 前MA{}({:.4})? {} AND close({:.4}) < MA{}({:.4})? {} → {}",
                        data.symbol, prev_close, self.params.fast_ma_period, prev_fast,
                        prev_close > prev_fast, close, self.params.fast_ma_period, entry_fast_ma,
                        close < entry_fast_ma,
                        if crossed { "✅ 触发做空信号" } else { "❌ 未触发" }
                    );
                    if crossed {
                        signal_type = Some(SignalType::Sell);
                        reason = format!(
                            "Bearish trend pullback: price crossed below MA{} on {} (trend: MA{} < MA{})",
                            self.params.fast_ma_period,
                            self.params.entry_timeframe,
                            self.params.fast_ma_period,
                            self.params.slow_ma_period
                        );
                    }
                } else {
                    tracing::debug!("[{}] 做空条件: 无法计算前一根MA{}, 跳过", data.symbol, self.params.fast_ma_period);
                }
            }
            _ => {}
        }

        let signal_type = match signal_type {
            Some(s) => s,
            None => {
                tracing::debug!("[{}] 最终结果: 无入场信号, 等待下一根K线", data.symbol);
                return None;
            }
        };

        // Calculate stop loss (仅用于 Engine 层静态止损，作为盘中安全网)
        // 注意: MA288 穿越止损由 strategy-service 的 check_exit_conditions() 独立处理，
        // 两层同时生效，互不覆盖。
        let stop_loss = if self.params.hard_stop_pct > 0.0 {
            // Hard stop: based on entry price
            match signal_type {
                SignalType::Buy => Some(current_price * (1.0 - self.params.hard_stop_pct / 100.0)),
                SignalType::Sell => Some(current_price * (1.0 + self.params.hard_stop_pct / 100.0)),
                _ => None,
            }
        } else {
            // MA288 stop: based on MA value
            match signal_type {
                SignalType::Buy => Some(fast_ma * 0.98),  // 2% below MA
                SignalType::Sell => Some(fast_ma * 1.02),  // 2% above MA
                _ => None,
            }
        };

        // No fixed take profit - use trailing stop
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
            "entry_timeframe": self.params.entry_timeframe,
            "kline_count": klines.len(),
            "stop_mode": format!("{:?}", self.params.stop_mode),
            "hard_stop_pct": self.params.hard_stop_pct,
            "take_profit_mode": format!("{:?}", self.params.take_profit_mode),
            "trailing_activate_pct": self.params.trailing_activate_pct,
            "trailing_callback_pct": self.params.trailing_callback_pct,
            "realized_vol_threshold": self.params.realized_vol_threshold,
            "realized_vol_48": calculate_realized_vol_48(klines).unwrap_or(0.0),
            "use_30m_expanding": self.params.use_30m_expanding,
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
