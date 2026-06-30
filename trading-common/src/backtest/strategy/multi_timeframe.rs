// backtest/strategy/multi_timeframe.rs
// 多时间框架策略 trait 定义

use crate::data::types::{OHLCData, Timeframe};
use rust_decimal::Decimal;
use std::collections::HashMap;

/// 趋势方向
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrendDirection {
    /// 看涨
    Bullish,
    /// 看跌
    Bearish,
    /// 中性/震荡
    Neutral,
}

impl TrendDirection {
    /// 是否是看涨方向
    pub fn is_bullish(&self) -> bool {
        matches!(self, TrendDirection::Bullish)
    }

    /// 是否是看跌方向
    pub fn is_bearish(&self) -> bool {
        matches!(self, TrendDirection::Bearish)
    }

    /// 是否是中性
    pub fn is_neutral(&self) -> bool {
        matches!(self, TrendDirection::Neutral)
    }
}

/// 趋势分析结果
#[derive(Debug, Clone)]
pub struct TrendAnalysis {
    /// 趋势方向
    pub direction: TrendDirection,
    /// 置信度 (0.0 - 1.0)
    pub confidence: Decimal,
    /// 分析描述
    pub description: String,
}

impl TrendAnalysis {
    pub fn new(direction: TrendDirection, confidence: Decimal, description: String) -> Self {
        Self {
            direction,
            confidence,
            description,
        }
    }

    /// 创建看涨分析
    pub fn bullish(confidence: Decimal, description: &str) -> Self {
        Self::new(TrendDirection::Bullish, confidence, description.to_string())
    }

    /// 创建看跌分析
    pub fn bearish(confidence: Decimal, description: &str) -> Self {
        Self::new(TrendDirection::Bearish, confidence, description.to_string())
    }

    /// 创建中性分析
    pub fn neutral(confidence: Decimal, description: &str) -> Self {
        Self::new(TrendDirection::Neutral, confidence, description.to_string())
    }
}

/// 多时间框架分析结果
#[derive(Debug, Clone)]
pub struct MultiTimeframeAnalysis {
    /// 各时间框架的分析结果
    pub timeframe_analyses: HashMap<Timeframe, TrendAnalysis>,
    /// 综合趋势方向
    pub overall_direction: TrendDirection,
    /// 综合置信度
    pub overall_confidence: Decimal,
    /// 是否可以入场
    pub entry_allowed: bool,
    /// 入场方向 (Buy/Sell)
    pub entry_direction: Option<EntryDirection>,
}

/// 入场方向
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryDirection {
    /// 做多
    Long,
    /// 做空
    Short,
}

/// 多时间框架策略 trait
pub trait MultiTimeframeStrategy: Send + Sync {
    /// 策略名称
    fn name(&self) -> &str;

    /// 策略描述
    fn description(&self) -> &str;

    /// 需要的时间框架列表（按从大到小排列）
    fn required_timeframes(&self) -> Vec<Timeframe>;

    /// 初始化策略参数
    fn initialize(&mut self, params: HashMap<String, String>) -> Result<(), String>;

    /// 分析多时间框架数据
    fn analyze(&mut self, klines: &HashMap<Timeframe, Vec<OHLCData>>) -> MultiTimeframeAnalysis;

    /// 检查是否应该入场
    fn should_enter(&self, analysis: &MultiTimeframeAnalysis) -> bool;

    /// 检查是否应该出场
    fn should_exit(&self, analysis: &MultiTimeframeAnalysis, is_long: bool) -> bool;

    /// 重置策略状态
    fn reset(&mut self);
}

/// 辅助函数：计算 EMA (指数移动平均)
pub fn calculate_ema(prices: &[Decimal], period: usize) -> Vec<Decimal> {
    if prices.is_empty() || period == 0 {
        return Vec::new();
    }

    let multiplier = Decimal::from(2) / Decimal::from(period + 1);
    let mut ema = Vec::with_capacity(prices.len());

    // 第一个值使用 SMA
    let first_sma: Decimal = prices[..period.min(prices.len())].iter().sum::<Decimal>()
        / Decimal::from(period.min(prices.len()));
    ema.push(first_sma);

    // 后续值使用 EMA 公式
    for i in 1..prices.len() {
        let value = prices[i] * multiplier + ema[i - 1] * (Decimal::from(1) - multiplier);
        ema.push(value);
    }

    ema
}

/// 辅助函数：计算 SMA (简单移动平均)
pub fn calculate_sma(prices: &[Decimal], period: usize) -> Vec<Decimal> {
    if prices.is_empty() || period == 0 {
        return Vec::new();
    }

    let mut sma = Vec::with_capacity(prices.len());

    for i in 0..prices.len() {
        let start = if i >= period { i - period + 1 } else { 0 };
        let window = &prices[start..=i];
        let avg = window.iter().sum::<Decimal>() / Decimal::from(window.len());
        sma.push(avg);
    }

    sma
}

/// 辅助函数：计算 RSI
pub fn calculate_rsi(prices: &[Decimal], period: usize) -> Vec<Option<Decimal>> {
    if prices.len() < 2 {
        return vec![None; prices.len()];
    }

    let mut rsi = vec![None; prices.len()];
    let mut gains = Vec::new();
    let mut losses = Vec::new();

    // 计算价格变化
    for i in 1..prices.len() {
        let change = prices[i] - prices[i - 1];
        if change > Decimal::ZERO {
            gains.push(change);
            losses.push(Decimal::ZERO);
        } else {
            gains.push(Decimal::ZERO);
            losses.push(-change);
        }
    }

    // 计算 RSI
    for i in period..gains.len() {
        let avg_gain: Decimal = gains[i - period + 1..=i].iter().sum::<Decimal>()
            / Decimal::from(period);
        let avg_loss: Decimal = losses[i - period + 1..=i].iter().sum::<Decimal>()
            / Decimal::from(period);

        if avg_loss == Decimal::ZERO {
            rsi[i + 1] = Some(Decimal::from(100));
        } else {
            let rs = avg_gain / avg_loss;
            rsi[i + 1] = Some(Decimal::from(100) - (Decimal::from(100) / (Decimal::from(1) + rs)));
        }
    }

    rsi
}

/// 辅助函数：计算 MACD
pub fn calculate_macd(
    prices: &[Decimal],
    fast_period: usize,
    slow_period: usize,
    signal_period: usize,
) -> (Vec<Decimal>, Vec<Decimal>, Vec<Decimal>) {
    let fast_ema = calculate_ema(prices, fast_period);
    let slow_ema = calculate_ema(prices, slow_period);

    // MACD 线 = 快线 - 慢线
    let macd_line: Vec<Decimal> = fast_ema
        .iter()
        .zip(slow_ema.iter())
        .map(|(fast, slow)| fast - slow)
        .collect();

    // 信号线 = MACD 的 EMA
    let signal_line = calculate_ema(&macd_line, signal_period);

    // MACD 柱状图 = MACD 线 - 信号线
    let histogram: Vec<Decimal> = macd_line
        .iter()
        .zip(signal_line.iter())
        .map(|(macd, signal)| macd - signal)
        .collect();

    (macd_line, signal_line, histogram)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ema_calculation() {
        let prices = vec![
            Decimal::from(10),
            Decimal::from(11),
            Decimal::from(12),
            Decimal::from(11),
            Decimal::from(13),
        ];

        let ema = calculate_ema(&prices, 3);
        assert!(!ema.is_empty());
        // 第一个 EMA 值应该是前 3 个价格的 SMA
        let expected_first = (Decimal::from(10) + Decimal::from(11) + Decimal::from(12))
            / Decimal::from(3);
        assert_eq!(ema[0], expected_first);
    }

    #[test]
    fn test_rsi_calculation() {
        use std::str::FromStr;
        let prices: Vec<Decimal> = [
            "44", "44.34", "44.09", "43.61", "44.33", "44.83", "45.10", "45.42", "45.84",
            "46.08", "45.89", "46.03", "45.61", "46.28", "46.28", "46.00", "46.03", "46.41",
            "46.22", "45.64",
        ]
        .iter()
        .map(|s| Decimal::from_str(s).unwrap())
        .collect();

        let rsi = calculate_rsi(&prices, 14);
        // 最后一个 RSI 值应该存在
        assert!(rsi[rsi.len() - 1].is_some());
    }

    #[test]
    fn test_macd_calculation() {
        let prices = vec![
            Decimal::from(10),
            Decimal::from(11),
            Decimal::from(12),
            Decimal::from(11),
            Decimal::from(13),
            Decimal::from(14),
            Decimal::from(12),
            Decimal::from(15),
        ];

        let (macd, signal, histogram) = calculate_macd(&prices, 3, 5, 2);
        assert!(!macd.is_empty());
        assert!(!signal.is_empty());
        assert!(!histogram.is_empty());
    }
}
