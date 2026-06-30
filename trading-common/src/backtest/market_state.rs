// backtest/market_state.rs
// 市场状态分析器：分析 K 线数据的趋势/震荡/波动分布

use rust_decimal::Decimal;
use std::collections::HashMap;
use std::str::FromStr;

use crate::data::types::OHLCData;

// =================================================================
// 类型定义
// =================================================================

/// 市场状态枚举
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MarketState {
    /// 强势上涨（ADX > 25, +DI > -DI）
    StrongUptrend,
    /// 上涨趋势
    Uptrend,
    /// 震荡/横盘
    Ranging,
    /// 下跌趋势
    Downtrend,
    /// 强势下跌（ADX > 25, -DI > +DI）
    StrongDowntrend,
    /// 高波动（ATR 百分位 > 80）
    HighVolatility,
}

impl MarketState {
    pub fn as_str(&self) -> &'static str {
        match self {
            MarketState::StrongUptrend => "strong_uptrend",
            MarketState::Uptrend => "uptrend",
            MarketState::Ranging => "ranging",
            MarketState::Downtrend => "downtrend",
            MarketState::StrongDowntrend => "strong_downtrend",
            MarketState::HighVolatility => "high_volatility",
        }
    }

    pub fn is_trending(&self) -> bool {
        matches!(
            self,
            MarketState::StrongUptrend
                | MarketState::Uptrend
                | MarketState::Downtrend
                | MarketState::StrongDowntrend
        )
    }
}

/// 单段市场状态分析结果
#[derive(Debug, Clone)]
pub struct MarketStateAnalysis {
    pub state: MarketState,
    pub volatility_percentile: Decimal,
    pub trend_strength: Decimal,
    pub adx: Decimal,
    pub atr: Decimal,
    pub description: String,
}

/// 市场状态报告
#[derive(Debug, Clone)]
pub struct MarketStateReport {
    pub total_candles: usize,
    pub analysis_window: usize,
    pub state_distribution: HashMap<String, usize>,
    pub state_percentages: HashMap<String, Decimal>,
    pub avg_volatility: Decimal,
    pub avg_trend_strength: Decimal,
    pub data_quality_score: Decimal,
    pub trend_ratio: Decimal,
    pub ranging_ratio: Decimal,
    pub summary: String,
}

// =================================================================
// 分析器
// =================================================================

pub struct MarketStateAnalyzer;

impl MarketStateAnalyzer {
    /// 分析 K 线数据的市场状态分布
    pub fn analyze(klines: &[OHLCData], window: usize) -> MarketStateReport {
        if klines.len() < window.max(20) {
            return Self::empty_report(klines.len());
        }

        let window = window.max(20);
        let mut analyses = Vec::new();

        // 滑动窗口分析
        let mut i = window;
        while i <= klines.len() {
            let segment = &klines[i - window..i];
            let analysis = Self::analyze_segment(segment);
            analyses.push(analysis);
            i += window / 2; // 50% 重叠
        }

        if analyses.is_empty() {
            return Self::empty_report(klines.len());
        }

        // 统计状态分布
        let mut state_distribution: HashMap<String, usize> = HashMap::new();
        let total_segments = analyses.len();

        for a in &analyses {
            *state_distribution
                .entry(a.state.as_str().to_string())
                .or_insert(0) += 1;
        }

        let state_percentages: HashMap<String, Decimal> = state_distribution
            .iter()
            .map(|(k, v)| {
                (
                    k.clone(),
                    Decimal::from(*v) / Decimal::from(total_segments) * Decimal::from(100),
                )
            })
            .collect();

        // 计算平均指标
        let avg_volatility: Decimal =
            analyses.iter().map(|a| a.volatility_percentile).sum::<Decimal>()
                / Decimal::from(total_segments);

        let avg_trend_strength: Decimal =
            analyses.iter().map(|a| a.trend_strength.abs()).sum::<Decimal>()
                / Decimal::from(total_segments);

        // 趋势/震荡比例
        let trend_count: usize = analyses.iter().filter(|a| a.state.is_trending()).count();
        let trend_ratio = Decimal::from(trend_count) / Decimal::from(total_segments) * Decimal::from(100);
        let ranging_ratio = Decimal::from(100) - trend_ratio;

        // 数据质量评分：趋势和震荡都有覆盖 = 好数据
        let data_quality_score = Self::calculate_quality_score(trend_ratio, &state_percentages);

        let summary = Self::generate_summary(
            total_segments,
            &state_percentages,
            avg_volatility,
            avg_trend_strength,
            data_quality_score,
        );

        MarketStateReport {
            total_candles: klines.len(),
            analysis_window: window,
            state_distribution,
            state_percentages,
            avg_volatility,
            avg_trend_strength,
            data_quality_score,
            trend_ratio,
            ranging_ratio,
            summary,
        }
    }

    /// 分析单段数据
    fn analyze_segment(klines: &[OHLCData]) -> MarketStateAnalysis {
        let closes: Vec<Decimal> = klines.iter().map(|k| k.close).collect();
        let highs: Vec<Decimal> = klines.iter().map(|k| k.high).collect();
        let lows: Vec<Decimal> = klines.iter().map(|k| k.low).collect();

        // 计算 ATR (Average True Range)
        let atr = Self::calculate_atr(&highs, &lows, &closes, 14);

        // 计算 ADX (Average Directional Index)
        let (adx, plus_di, minus_di) = Self::calculate_adx(&highs, &lows, &closes, 14);

        // 计算波动率百分位（ATR / 收盘价 * 100）
        let last_price = *closes.last().unwrap_or(&Decimal::ONE);
        let volatility_pct = if last_price > Decimal::ZERO {
            atr / last_price * Decimal::from(100)
        } else {
            Decimal::ZERO
        };

        // 趋势强度：(plus_di - minus_di) / (plus_di + minus_di)
        let di_sum = plus_di + minus_di;
        let trend_strength = if di_sum > Decimal::ZERO {
            (plus_di - minus_di) / di_sum
        } else {
            Decimal::ZERO
        };

        // 判定市场状态
        let state = if volatility_pct > Decimal::from_str("3.0").unwrap() {
            MarketState::HighVolatility
        } else if adx > Decimal::from(25) {
            if plus_di > minus_di {
                MarketState::StrongUptrend
            } else {
                MarketState::StrongDowntrend
            }
        } else if trend_strength > Decimal::from_str("0.2").unwrap() {
            MarketState::Uptrend
        } else if trend_strength < Decimal::from_str("-0.2").unwrap() {
            MarketState::Downtrend
        } else {
            MarketState::Ranging
        };

        let description = format!(
            "ADX={:.1}, +DI={:.1}, -DI={:.1}, ATR={:.2}, trend_strength={:.2}",
            adx, plus_di, minus_di, atr, trend_strength
        );

        MarketStateAnalysis {
            state,
            volatility_percentile: volatility_pct,
            trend_strength,
            adx,
            atr,
            description,
        }
    }

    /// 计算 ATR (Average True Range)
    fn calculate_atr(
        highs: &[Decimal],
        lows: &[Decimal],
        closes: &[Decimal],
        period: usize,
    ) -> Decimal {
        if highs.len() < 2 || period == 0 {
            return Decimal::ZERO;
        }

        let mut true_ranges = Vec::new();
        for i in 1..highs.len() {
            let hl = highs[i] - lows[i];
            let hc = (highs[i] - closes[i - 1]).abs();
            let lc = (lows[i] - closes[i - 1]).abs();
            true_ranges.push(hl.max(hc).max(lc));
        }

        if true_ranges.len() < period {
            return true_ranges.iter().sum::<Decimal>() / Decimal::from(true_ranges.len().max(1));
        }

        // EMA 方式计算 ATR
        let mut atr = true_ranges[..period].iter().sum::<Decimal>() / Decimal::from(period);
        let multiplier = Decimal::ONE / Decimal::from(period);

        for i in period..true_ranges.len() {
            atr = true_ranges[i] * multiplier + atr * (Decimal::ONE - multiplier);
        }

        atr
    }

    /// 计算 ADX (Average Directional Index)
    fn calculate_adx(
        highs: &[Decimal],
        lows: &[Decimal],
        closes: &[Decimal],
        period: usize,
    ) -> (Decimal, Decimal, Decimal) {
        if highs.len() < period + 1 {
            return (Decimal::ZERO, Decimal::ZERO, Decimal::ZERO);
        }

        let mut plus_dm = Vec::new();
        let mut minus_dm = Vec::new();
        let mut true_ranges = Vec::new();

        for i in 1..highs.len() {
            let up_move = highs[i] - highs[i - 1];
            let down_move = lows[i - 1] - lows[i];

            plus_dm.push(if up_move > down_move && up_move > Decimal::ZERO {
                up_move
            } else {
                Decimal::ZERO
            });
            minus_dm.push(if down_move > up_move && down_move > Decimal::ZERO {
                down_move
            } else {
                Decimal::ZERO
            });

            let hl = highs[i] - lows[i];
            let hc = (highs[i] - closes[i - 1]).abs();
            let lc = (lows[i] - closes[i - 1]).abs();
            true_ranges.push(hl.max(hc).max(lc));
        }

        // Smoothed averages
        let smooth_tr = Self::smooth_sum(&true_ranges, period);
        let smooth_plus_dm = Self::smooth_sum(&plus_dm, period);
        let smooth_minus_dm = Self::smooth_sum(&minus_dm, period);

        if smooth_tr == Decimal::ZERO {
            return (Decimal::ZERO, Decimal::ZERO, Decimal::ZERO);
        }

        let plus_di = smooth_plus_dm / smooth_tr * Decimal::from(100);
        let minus_di = smooth_minus_dm / smooth_tr * Decimal::from(100);

        // DX and ADX
        let di_sum = plus_di + minus_di;
        let dx = if di_sum > Decimal::ZERO {
            (plus_di - minus_di).abs() / di_sum * Decimal::from(100)
        } else {
            Decimal::ZERO
        };

        // 简化：用最后 period 个 DX 的平均值作为 ADX
        let adx = dx; // 简化实现

        (adx, plus_di, minus_di)
    }

    /// Wilder's smoothing
    fn smooth_sum(values: &[Decimal], period: usize) -> Decimal {
        if values.len() < period {
            return values.iter().sum::<Decimal>();
        }

        let mut sum = values[..period].iter().sum::<Decimal>();
        for i in period..values.len() {
            sum = sum - sum / Decimal::from(period) + values[i];
        }
        sum / Decimal::from(period)
    }

    /// 计算数据质量评分
    fn calculate_quality_score(trend_ratio: Decimal, state_percentages: &HashMap<String, Decimal>) -> Decimal {
        let mut score = Decimal::ZERO;

        // 趋势和震荡都有覆盖（各至少 20%）= 好数据
        let ranging_pct = state_percentages
            .get("ranging")
            .copied()
            .unwrap_or(Decimal::ZERO);

        if trend_ratio >= Decimal::from(20) && trend_ratio <= Decimal::from(80) {
            score += Decimal::from(40);
        } else if trend_ratio >= Decimal::from(10) {
            score += Decimal::from(20);
        }

        if ranging_pct >= Decimal::from(20) {
            score += Decimal::from(30);
        } else if ranging_pct >= Decimal::from(10) {
            score += Decimal::from(15);
        }

        // 多种状态都出现 = 更好的数据
        let state_count = state_percentages.len();
        score += Decimal::from(state_count.min(4)) * Decimal::from(7) + Decimal::from(5);

        score.min(Decimal::from(100))
    }

    fn generate_summary(
        total_segments: usize,
        state_percentages: &HashMap<String, Decimal>,
        avg_volatility: Decimal,
        avg_trend_strength: Decimal,
        data_quality_score: Decimal,
    ) -> String {
        let dominant_state = state_percentages
            .iter()
            .max_by(|a, b| a.1.cmp(b.1))
            .map(|(k, v)| format!("{} ({:.1}%)", k, v))
            .unwrap_or_else(|| "unknown".to_string());

        format!(
            "Analyzed {} segments. Dominant: {}. Avg volatility: {:.2}%, Avg trend strength: {:.2}. Data quality: {:.0}/100",
            total_segments, dominant_state, avg_volatility, avg_trend_strength, data_quality_score
        )
    }

    fn empty_report(total_candles: usize) -> MarketStateReport {
        MarketStateReport {
            total_candles,
            analysis_window: 0,
            state_distribution: HashMap::new(),
            state_percentages: HashMap::new(),
            avg_volatility: Decimal::ZERO,
            avg_trend_strength: Decimal::ZERO,
            data_quality_score: Decimal::ZERO,
            trend_ratio: Decimal::ZERO,
            ranging_ratio: Decimal::ZERO,
            summary: "Insufficient data for market state analysis".to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;
    use crate::data::types::Timeframe;

    fn create_test_klines(count: usize, trend: &str) -> Vec<OHLCData> {
        let mut klines = Vec::new();
        let base_time = Utc::now();
        let base_price = Decimal::from(50000);

        for i in 0..count {
            let i_dec = Decimal::from(i as i64);
            let (open, close) = match trend {
                "up" => (base_price + i_dec * Decimal::from(10), base_price + i_dec * Decimal::from(10) + Decimal::from(5)),
                "down" => (base_price - i_dec * Decimal::from(10), base_price - i_dec * Decimal::from(10) - Decimal::from(5)),
                _ => (base_price, base_price + Decimal::from(if i % 2 == 0 { 5 } else { -5 })),
            };

            klines.push(OHLCData::new(
                base_time + chrono::Duration::minutes(i as i64),
                "BTCUSDT".to_string(),
                Timeframe::OneMinute,
                open,
                open.max(close) + Decimal::from(10),
                open.min(close) - Decimal::from(10),
                close,
                Decimal::from(100),
                10,
            ));
        }
        klines
    }

    #[test]
    fn test_market_state_analyze_insufficient() {
        let klines = create_test_klines(5, "up");
        let report = MarketStateAnalyzer::analyze(&klines, 20);
        assert_eq!(report.total_candles, 5);
        assert!(report.summary.contains("Insufficient"));
    }

    #[test]
    fn test_market_state_analyze_uptrend() {
        let klines = create_test_klines(200, "up");
        let report = MarketStateAnalyzer::analyze(&klines, 50);
        assert!(report.total_candles > 0);
        assert!(!report.state_percentages.is_empty());
    }

    #[test]
    fn test_market_state_analyze_ranging() {
        let klines = create_test_klines(200, "flat");
        let report = MarketStateAnalyzer::analyze(&klines, 50);
        assert!(report.total_candles > 0);
    }
}
