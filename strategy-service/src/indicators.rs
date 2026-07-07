//! 技术指标计算模块
//!
//! 支持动态参数，所有指标函数接收 K 线数据和参数，返回计算结果。
//! 用于 strategy-service 根据策略配置动态计算指标。

use serde::{Deserialize, Serialize};

use crate::redis_reader::KlineData;

// =================================================================
// 指标结果类型
// =================================================================

/// 移动平均线结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MaResult {
    pub value: f64,
    pub period: usize,
}

/// EMA 结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EmaResult {
    pub value: f64,
    pub period: usize,
}

/// RSI 结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RsiResult {
    pub value: f64,
    pub period: usize,
}

/// MACD 结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MacdResult {
    pub macd: f64,
    pub signal: f64,
    pub histogram: f64,
    pub fast_period: usize,
    pub slow_period: usize,
    pub signal_period: usize,
}

/// 布林带结果
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

/// ATR 结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AtrResult {
    pub value: f64,
    pub period: usize,
}

/// ADX 结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AdxResult {
    pub adx: f64,
    pub plus_di: f64,
    pub minus_di: f64,
    pub period: usize,
}

/// 多均线结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MultiMaResult {
    pub values: Vec<(usize, f64)>, // (period, value)
}

// =================================================================
// 辅助函数
// =================================================================

/// 从 K 线数据提取收盘价
fn extract_closes(klines: &[KlineData]) -> Vec<f64> {
    klines.iter().map(|k| k.close).collect()
}

/// 从 K 线数据提取最高价
fn extract_highs(klines: &[KlineData]) -> Vec<f64> {
    klines.iter().map(|k| k.high).collect()
}

/// 从 K 线数据提取最低价
fn extract_lows(klines: &[KlineData]) -> Vec<f64> {
    klines.iter().map(|k| k.low).collect()
}

// =================================================================
// 指标计算函数
// =================================================================

/// 简单移动平均线 (SMA)
///
/// # 参数
/// - `klines`: K 线数据
/// - `period`: 均线周期
///
/// # 返回
/// 最新一根 SMA 值，数据不足时返回 None
pub fn calculate_ma(klines: &[KlineData], period: usize) -> Option<MaResult> {
    if klines.len() < period || period == 0 {
        return None;
    }

    let closes = extract_closes(klines);
    let start = closes.len() - period;
    let sum: f64 = closes[start..].iter().sum();
    let value = sum / period as f64;

    Some(MaResult { value, period })
}

/// 计算多个周期的 SMA
///
/// # 参数
/// - `klines`: K 线数据
/// - `periods`: 均线周期列表，如 [7, 25, 99]
///
/// # 返回
/// 各周期的 SMA 结果，数据不足的周期跳过
pub fn calculate_multi_ma(klines: &[KlineData], periods: &[usize]) -> MultiMaResult {
    let values: Vec<(usize, f64)> = periods
        .iter()
        .filter_map(|&period| {
            calculate_ma(klines, period).map(|result| (period, result.value))
        })
        .collect();

    MultiMaResult { values }
}

/// 指数移动平均线 (EMA)
///
/// # 参数
/// - `klines`: K 线数据
/// - `period`: EMA 周期
///
/// # 返回
/// 最新一根 EMA 值
pub fn calculate_ema(klines: &[KlineData], period: usize) -> Option<EmaResult> {
    if klines.len() < period || period == 0 {
        return None;
    }

    let closes = extract_closes(klines);
    let alpha = 2.0 / (period as f64 + 1.0);

    // 第一个值：SMA
    let first_ema: f64 = closes[..period].iter().sum::<f64>() / period as f64;

    // 后续值：EMA 递推
    let mut ema = first_ema;
    for &price in &closes[period..] {
        ema = alpha * price + (1.0 - alpha) * ema;
    }

    Some(EmaResult { value: ema, period })
}

/// RSI (相对强弱指数)
///
/// 使用 Wilder 平滑方法
///
/// # 参数
/// - `klines`: K 线数据
/// - `period`: RSI 周期（默认 14）
///
/// # 返回
/// RSI 值 (0-100)，数据不足时返回 50.0
pub fn calculate_rsi(klines: &[KlineData], period: usize) -> Option<RsiResult> {
    if klines.len() < period + 1 || period == 0 {
        return None;
    }

    let closes = extract_closes(klines);

    // 计算价格变化
    let changes: Vec<f64> = closes
        .windows(2)
        .map(|w| w[1] - w[0])
        .collect();

    if changes.len() < period {
        return None;
    }

    // 初始平均涨跌幅
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

    // Wilder 平滑
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

/// MACD (指数平滑异同移动平均线)
///
/// # 参数
/// - `klines`: K 线数据
/// - `fast_period`: 快线周期（默认 12）
/// - `slow_period`: 慢线周期（默认 26）
/// - `signal_period`: 信号线周期（默认 9）
///
/// # 返回
/// MACD 线、信号线、柱状图
pub fn calculate_macd(
    klines: &[KlineData],
    fast_period: usize,
    slow_period: usize,
    signal_period: usize,
) -> Option<MacdResult> {
    if klines.len() < slow_period + signal_period {
        return None;
    }

    let closes = extract_closes(klines);

    // 计算快线 EMA
    let ema_fast = ema_series(&closes, fast_period);
    // 计算慢线 EMA
    let ema_slow = ema_series(&closes, slow_period);

    // 对齐长度
    let offset = ema_fast.len().saturating_sub(ema_slow.len());
    if ema_slow.is_empty() {
        return None;
    }

    // MACD 线 = 快线 EMA - 慢线 EMA
    let macd_line: Vec<f64> = ema_slow
        .iter()
        .enumerate()
        .map(|(i, &v)| ema_fast[offset + i] - v)
        .collect();

    if macd_line.len() < signal_period {
        return None;
    }

    // 信号线 = MACD 线的 EMA
    let signal_line = ema_series(&macd_line, signal_period);

    let macd_val = *macd_line.last().unwrap();
    let signal_val = signal_line.last().copied().unwrap_or(0.0);
    let hist_val = macd_val - signal_val;

    Some(MacdResult {
        macd: macd_val,
        signal: signal_val,
        histogram: hist_val,
        fast_period,
        slow_period,
        signal_period,
    })
}

/// EMA 序列计算（返回完整序列）
fn ema_series(values: &[f64], period: usize) -> Vec<f64> {
    if values.len() < period || period == 0 {
        return vec![];
    }

    let alpha = 2.0 / (period as f64 + 1.0);
    let mut result = Vec::with_capacity(values.len() - period + 1);

    // 第一个值：SMA
    let first: f64 = values[..period].iter().sum::<f64>() / period as f64;
    result.push(first);

    // 后续值：EMA
    for &val in &values[period..] {
        let prev = *result.last().unwrap();
        result.push(alpha * val + (1.0 - alpha) * prev);
    }

    result
}

/// 布林带 (Bollinger Bands)
///
/// # 参数
/// - `klines`: K 线数据
/// - `period`: 周期（默认 20）
/// - `std_dev`: 标准差倍数（默认 2.0）
///
/// # 返回
/// 上轨、中轨、下轨、带宽、%B
pub fn calculate_bollinger(
    klines: &[KlineData],
    period: usize,
    std_dev: f64,
) -> Option<BollingerResult> {
    if klines.len() < period || period == 0 {
        return None;
    }

    let closes = extract_closes(klines);
    let recent = &closes[closes.len() - period..];

    // 中轨 = SMA
    let middle: f64 = recent.iter().sum::<f64>() / period as f64;

    // 标准差
    let variance: f64 = recent
        .iter()
        .map(|x| (x - middle).powi(2))
        .sum::<f64>()
        / period as f64;
    let std = variance.sqrt();

    let upper = middle + std_dev * std;
    let lower = middle - std_dev * std;

    // 带宽 = (上轨 - 下轨) / 中轨
    let bandwidth = if middle > 0.0 {
        (upper - lower) / middle
    } else {
        0.0
    };

    // %B = (当前价 - 下轨) / (上轨 - 下轨)
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

/// ATR (平均真实波幅)
///
/// # 参数
/// - `klines`: K 线数据
/// - `period`: ATR 周期（默认 14）
///
/// # 返回
/// ATR 值
pub fn calculate_atr(klines: &[KlineData], period: usize) -> Option<AtrResult> {
    if klines.len() < period + 1 || period == 0 {
        return None;
    }

    let highs = extract_highs(klines);
    let lows = extract_lows(klines);
    let closes = extract_closes(klines);

    // 计算 True Range
    let mut tr_values: Vec<f64> = Vec::new();
    for i in 1..klines.len() {
        let high_low = highs[i] - lows[i];
        let high_prev_close = (highs[i] - closes[i - 1]).abs();
        let low_prev_close = (lows[i] - closes[i - 1]).abs();
        tr_values.push(high_low.max(high_prev_close).max(low_prev_close));
    }

    if tr_values.len() < period {
        return None;
    }

    // 第一个 ATR = 前 period 个 TR 的 SMA
    let first_atr: f64 = tr_values[..period].iter().sum::<f64>() / period as f64;

    // 后续 ATR = (前一个 ATR * (period - 1) + 当前 TR) / period
    let mut atr = first_atr;
    for &tr in &tr_values[period..] {
        atr = (atr * (period as f64 - 1.0) + tr) / period as f64;
    }

    Some(AtrResult { value: atr, period })
}

/// ADX (平均方向指数)
///
/// # 参数
/// - `klines`: K 线数据
/// - `period`: ADX 周期（默认 14）
///
/// # 返回
/// ADX、+DI、-DI
pub fn calculate_adx(klines: &[KlineData], period: usize) -> Option<AdxResult> {
    if klines.len() < period * 2 + 1 || period == 0 {
        return None;
    }

    let highs = extract_highs(klines);
    let lows = extract_lows(klines);
    let closes = extract_closes(klines);

    // 计算 +DM、-DM、TR
    let mut plus_dm: Vec<f64> = Vec::new();
    let mut minus_dm: Vec<f64> = Vec::new();
    let mut tr: Vec<f64> = Vec::new();

    for i in 1..klines.len() {
        let high_diff = highs[i] - highs[i - 1];
        let low_diff = lows[i - 1] - lows[i];

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

        let high_low = highs[i] - lows[i];
        let high_prev_close = (highs[i] - closes[i - 1]).abs();
        let low_prev_close = (lows[i] - closes[i - 1]).abs();
        let tr_val = high_low.max(high_prev_close).max(low_prev_close);

        plus_dm.push(pdm);
        minus_dm.push(mdm);
        tr.push(tr_val);
    }

    if tr.len() < period * 2 {
        return None;
    }

    // 平滑 +DM、-DM、TR
    let smooth_plus_dm = wilder_smooth(&plus_dm, period);
    let smooth_minus_dm = wilder_smooth(&minus_dm, period);
    let smooth_tr = wilder_smooth(&tr, period);

    if smooth_tr.is_empty() {
        return None;
    }

    // 计算 +DI、-DI
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

    // 计算 DX
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

    // 计算 ADX = DX 的 Wilder 平滑
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

/// Wilder 平滑方法
fn wilder_smooth(values: &[f64], period: usize) -> Vec<f64> {
    if values.len() < period {
        return vec![];
    }

    let mut result = Vec::with_capacity(values.len() - period + 1);

    // 第一个值：SMA
    let first: f64 = values[..period].iter().sum::<f64>() / period as f64;
    result.push(first);

    // 后续值：Wilder 平滑
    for &val in &values[period..] {
        let prev = *result.last().unwrap();
        result.push((prev * (period as f64 - 1.0) + val) / period as f64);
    }

    result
}

// =================================================================
// 测试
// =================================================================

#[cfg(test)]
mod tests {
    use super::*;

    fn create_test_klines(count: usize, base_price: f64) -> Vec<KlineData> {
        (0..count)
            .map(|i| {
                let price = base_price + (i as f64 * 0.5);
                KlineData {
                    timestamp: 1000000 + (i as i64 * 60000),
                    open: price - 0.1,
                    high: price + 0.5,
                    low: price - 0.5,
                    close: price,
                    volume: 1000.0 + i as f64,
                }
            })
            .collect()
    }

    #[test]
    fn test_ma() {
        let klines = create_test_klines(100, 100.0);
        let result = calculate_ma(&klines, 20);
        assert!(result.is_some());
        let result = result.unwrap();
        assert_eq!(result.period, 20);
        assert!(result.value > 0.0);
    }

    #[test]
    fn test_ema() {
        let klines = create_test_klines(100, 100.0);
        let result = calculate_ema(&klines, 20);
        assert!(result.is_some());
    }

    #[test]
    fn test_rsi() {
        let klines = create_test_klines(100, 100.0);
        let result = calculate_rsi(&klines, 14);
        assert!(result.is_some());
        let result = result.unwrap();
        assert!(result.value >= 0.0 && result.value <= 100.0);
    }

    #[test]
    fn test_macd() {
        let klines = create_test_klines(100, 100.0);
        let result = calculate_macd(&klines, 12, 26, 9);
        assert!(result.is_some());
    }

    #[test]
    fn test_bollinger() {
        let klines = create_test_klines(100, 100.0);
        let result = calculate_bollinger(&klines, 20, 2.0);
        assert!(result.is_some());
        let result = result.unwrap();
        assert!(result.upper > result.middle);
        assert!(result.middle > result.lower);
    }

    #[test]
    fn test_atr() {
        let klines = create_test_klines(100, 100.0);
        let result = calculate_atr(&klines, 14);
        assert!(result.is_some());
        let result = result.unwrap();
        assert!(result.value > 0.0);
    }

    #[test]
    fn test_adx() {
        let klines = create_test_klines(100, 100.0);
        let result = calculate_adx(&klines, 14);
        assert!(result.is_some());
        let result = result.unwrap();
        assert!(result.adx >= 0.0 && result.adx <= 100.0);
    }

    #[test]
    fn test_multi_ma() {
        let klines = create_test_klines(200, 100.0);
        let periods = vec![7, 25, 99];
        let result = calculate_multi_ma(&klines, &periods);
        assert_eq!(result.values.len(), 3);
    }
}
