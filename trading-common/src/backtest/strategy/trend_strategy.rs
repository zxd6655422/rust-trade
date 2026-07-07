// backtest/strategy/trend_strategy.rs
// 多时间框架趋势策略实现

use rust_decimal::Decimal;
use std::collections::HashMap;

use super::multi_timeframe::{
    calculate_ema, calculate_macd, calculate_rsi, EntryDirection, MultiTimeframeAnalysis,
    MultiTimeframeStrategy, TrendAnalysis, TrendDirection,
};
use crate::data::types::{OHLCData, Timeframe};

/// 多时间框架趋势策略
pub struct TrendStrategy {
    // EMA 参数
    ema_fast_period: usize,
    ema_slow_period: usize,

    // MACD 参数
    macd_fast_period: usize,
    macd_slow_period: usize,
    macd_signal_period: usize,

    // RSI 参数
    rsi_period: usize,
    rsi_oversold: Decimal,
    rsi_overbought: Decimal,

    // 入场阈值
    min_confidence: Decimal,

    // 内部状态
    last_analysis: Option<MultiTimeframeAnalysis>,
    is_long_position: bool,
}

impl TrendStrategy {
    /// 创建新的趋势策略
    pub fn new() -> Self {
        Self {
            ema_fast_period: 20,
            ema_slow_period: 50,
            macd_fast_period: 12,
            macd_slow_period: 26,
            macd_signal_period: 9,
            rsi_period: 14,
            rsi_oversold: Decimal::from(30),
            rsi_overbought: Decimal::from(70),
            min_confidence: Decimal::from(60) / Decimal::from(100), // 60%
            last_analysis: None,
            is_long_position: false,
        }
    }

    /// 分析 4h 时间框架（判断大趋势）
    fn analyze_4h(&self, klines: &[OHLCData]) -> TrendAnalysis {
        if klines.len() < self.ema_slow_period {
            return TrendAnalysis::neutral(Decimal::ZERO, "Insufficient 4h data");
        }

        let prices: Vec<Decimal> = klines.iter().map(|k| k.close).collect();
        let ema_fast = calculate_ema(&prices, self.ema_fast_period);
        let ema_slow = calculate_ema(&prices, self.ema_slow_period);

        let fast = ema_fast.last().unwrap();
        let slow = ema_slow.last().unwrap();

        let diff_percent = ((fast - slow) / slow * Decimal::from(100)).abs();

        if fast > slow {
            let confidence = (diff_percent / Decimal::from(5)).min(Decimal::from(1));
            TrendAnalysis::bullish(
                confidence,
                &format!("4h EMA{} > EMA{} by {:.2}%", self.ema_fast_period, self.ema_slow_period, diff_percent),
            )
        } else if fast < slow {
            let confidence = (diff_percent / Decimal::from(5)).min(Decimal::from(1));
            TrendAnalysis::bearish(
                confidence,
                &format!("4h EMA{} < EMA{} by {:.2}%", self.ema_fast_period, self.ema_slow_period, diff_percent),
            )
        } else {
            TrendAnalysis::neutral(Decimal::from(50) / Decimal::from(100), "4h EMA crossover neutral")
        }
    }

    /// 分析 1h 时间框架（确认趋势）
    fn analyze_1h(&self, klines: &[OHLCData]) -> TrendAnalysis {
        if klines.len() < self.macd_slow_period + self.macd_signal_period {
            return TrendAnalysis::neutral(Decimal::ZERO, "Insufficient 1h data");
        }

        let prices: Vec<Decimal> = klines.iter().map(|k| k.close).collect();
        let (macd_line, signal_line, histogram) = calculate_macd(
            &prices,
            self.macd_fast_period,
            self.macd_slow_period,
            self.macd_signal_period,
        );

        let macd = macd_line.last().unwrap();
        let signal = signal_line.last().unwrap();
        let hist = histogram.last().unwrap();
        let prev_hist = if histogram.len() > 1 {
            histogram[histogram.len() - 2]
        } else {
            Decimal::ZERO
        };

        // MACD 金叉
        if macd > signal && *hist > Decimal::ZERO && prev_hist <= Decimal::ZERO {
            TrendAnalysis::bullish(
                Decimal::from(80) / Decimal::from(100),
                "1h MACD golden cross",
            )
        }
        // MACD 死叉
        else if macd < signal && *hist < Decimal::ZERO && prev_hist >= Decimal::ZERO {
            TrendAnalysis::bearish(
                Decimal::from(80) / Decimal::from(100),
                "1h MACD death cross",
            )
        }
        // MACD 柱状图为正
        else if *hist > Decimal::ZERO {
            TrendAnalysis::bullish(
                Decimal::from(60) / Decimal::from(100),
                "1h MACD histogram positive",
            )
        }
        // MACD 柱状图为负
        else if *hist < Decimal::ZERO {
            TrendAnalysis::bearish(
                Decimal::from(60) / Decimal::from(100),
                "1h MACD histogram negative",
            )
        } else {
            TrendAnalysis::neutral(Decimal::from(50) / Decimal::from(100), "1h MACD neutral")
        }
    }

    /// 分析 15m 时间框架（寻找入场点）
    fn analyze_15m(&self, klines: &[OHLCData]) -> TrendAnalysis {
        if klines.len() < self.rsi_period + 1 {
            return TrendAnalysis::neutral(Decimal::ZERO, "Insufficient 15m data");
        }

        let prices: Vec<Decimal> = klines.iter().map(|k| k.close).collect();
        let rsi_values = calculate_rsi(&prices, self.rsi_period);

        let rsi = rsi_values.last().unwrap().unwrap_or(Decimal::from(50));

        if rsi < self.rsi_oversold {
            let confidence = ((self.rsi_oversold - rsi) / self.rsi_oversold).min(Decimal::from(1));
            TrendAnalysis::bullish(
                confidence,
                &format!("15m RSI oversold at {:.2}", rsi),
            )
        } else if rsi > self.rsi_overbought {
            let confidence = ((rsi - self.rsi_overbought) / (Decimal::from(100) - self.rsi_overbought)).min(Decimal::from(1));
            TrendAnalysis::bearish(
                confidence,
                &format!("15m RSI overbought at {:.2}", rsi),
            )
        } else {
            let mid = (self.rsi_oversold + self.rsi_overbought) / Decimal::from(2);
            let distance = (rsi - mid).abs();
            let max_distance = (self.rsi_overbought - self.rsi_oversold) / Decimal::from(2);
            let confidence = Decimal::from(1) - (distance / max_distance);

            TrendAnalysis::neutral(confidence, &format!("15m RSI neutral at {:.2}", rsi))
        }
    }

    /// 综合所有时间框架分析
    fn combine_analyses(
        &self,
        analysis_4h: &TrendAnalysis,
        analysis_1h: &TrendAnalysis,
        analysis_15m: &TrendAnalysis,
    ) -> MultiTimeframeAnalysis {
        let mut timeframe_analyses = HashMap::new();
        timeframe_analyses.insert(Timeframe::FourHour, analysis_4h.clone());
        timeframe_analyses.insert(Timeframe::OneHour, analysis_1h.clone());
        timeframe_analyses.insert(Timeframe::FifteenMinutes, analysis_15m.clone());

        // 权重：4h 最重要，1h 次之，15m 用于入场
        let weight_4h = Decimal::from(50) / Decimal::from(100);
        let weight_1h = Decimal::from(30) / Decimal::from(100);
        let weight_15m = Decimal::from(20) / Decimal::from(100);

        // 计算加权置信度
        let bullish_score = self.calculate_direction_score(analysis_4h, TrendDirection::Bullish) * weight_4h
            + self.calculate_direction_score(analysis_1h, TrendDirection::Bullish) * weight_1h
            + self.calculate_direction_score(analysis_15m, TrendDirection::Bullish) * weight_15m;

        let bearish_score = self.calculate_direction_score(analysis_4h, TrendDirection::Bearish) * weight_4h
            + self.calculate_direction_score(analysis_1h, TrendDirection::Bearish) * weight_1h
            + self.calculate_direction_score(analysis_15m, TrendDirection::Bearish) * weight_15m;

        let overall_direction = if bullish_score > bearish_score && bullish_score > self.min_confidence {
            TrendDirection::Bullish
        } else if bearish_score > bullish_score && bearish_score > self.min_confidence {
            TrendDirection::Bearish
        } else {
            TrendDirection::Neutral
        };

        let overall_confidence = bullish_score.max(bearish_score);

        // 判断是否可以入场
        let entry_allowed = overall_confidence >= self.min_confidence;
        let entry_direction = if entry_allowed {
            match overall_direction {
                TrendDirection::Bullish => Some(EntryDirection::Long),
                TrendDirection::Bearish => Some(EntryDirection::Short),
                TrendDirection::Neutral => None,
            }
        } else {
            None
        };

        MultiTimeframeAnalysis {
            timeframe_analyses,
            overall_direction,
            overall_confidence,
            entry_allowed,
            entry_direction,
        }
    }

    /// 计算某个方向的得分
    fn calculate_direction_score(&self, analysis: &TrendAnalysis, direction: TrendDirection) -> Decimal {
        if analysis.direction == direction {
            analysis.confidence
        } else {
            Decimal::ZERO
        }
    }
}

impl MultiTimeframeStrategy for TrendStrategy {
    fn name(&self) -> &str {
        "Multi-Timeframe Trend"
    }

    fn description(&self) -> &str {
        "Multi-timeframe trend strategy: 4h for trend direction, 1h for confirmation, 15m for entry"
    }

    fn required_timeframes(&self) -> Vec<Timeframe> {
        vec![
            Timeframe::FourHour,
            Timeframe::OneHour,
            Timeframe::FifteenMinutes,
        ]
    }

    fn initialize(&mut self, params: HashMap<String, String>) -> Result<(), String> {
        if let Some(ema_fast) = params.get("ema_fast") {
            self.ema_fast_period = ema_fast.parse().map_err(|_| "Invalid ema_fast")?;
        }
        if let Some(ema_slow) = params.get("ema_slow") {
            self.ema_slow_period = ema_slow.parse().map_err(|_| "Invalid ema_slow")?;
        }
        if let Some(rsi_period) = params.get("rsi_period") {
            self.rsi_period = rsi_period.parse().map_err(|_| "Invalid rsi_period")?;
        }
        if let Some(rsi_oversold) = params.get("rsi_oversold") {
            self.rsi_oversold = rsi_oversold.parse().map_err(|_| "Invalid rsi_oversold")?;
        }
        if let Some(rsi_overbought) = params.get("rsi_overbought") {
            self.rsi_overbought = rsi_overbought.parse().map_err(|_| "Invalid rsi_overbought")?;
        }
        if let Some(min_confidence) = params.get("min_confidence") {
            let conf: f64 = min_confidence.parse().map_err(|_| "Invalid min_confidence")?;
            self.min_confidence = Decimal::try_from(conf).map_err(|_| "Invalid min_confidence")?;
        }

        Ok(())
    }

    fn analyze(&mut self, klines: &HashMap<Timeframe, Vec<OHLCData>>) -> MultiTimeframeAnalysis {
        // 获取各时间框架数据
        let klines_4h = klines.get(&Timeframe::FourHour).cloned().unwrap_or_default();
        let klines_1h = klines.get(&Timeframe::OneHour).cloned().unwrap_or_default();
        let klines_15m = klines.get(&Timeframe::FifteenMinutes).cloned().unwrap_or_default();

        // 分析各时间框架
        let analysis_4h = self.analyze_4h(&klines_4h);
        let analysis_1h = self.analyze_1h(&klines_1h);
        let analysis_15m = self.analyze_15m(&klines_15m);

        // 综合分析
        let result = self.combine_analyses(&analysis_4h, &analysis_1h, &analysis_15m);

        // 保存分析结果
        self.last_analysis = Some(result.clone());

        result
    }

    fn should_enter(&self, analysis: &MultiTimeframeAnalysis) -> bool {
        analysis.entry_allowed && analysis.entry_direction.is_some()
    }

    fn should_exit(&self, analysis: &MultiTimeframeAnalysis, is_long: bool) -> bool {
        match analysis.overall_direction {
            TrendDirection::Bullish => !is_long, // 如果看涨但持有空单，应该平仓
            TrendDirection::Bearish => is_long,  // 如果看跌但持有多单，应该平仓
            TrendDirection::Neutral => false,    // 中性时不主动平仓
        }
    }

    fn reset(&mut self) {
        self.last_analysis = None;
        self.is_long_position = false;
    }
}

impl Default for TrendStrategy {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Utc;

    fn create_test_klines(count: usize, start_price: Decimal, trend: &str) -> Vec<OHLCData> {
        let mut klines = Vec::new();
        let base_time = Utc::now();

        for i in 0..count {
            let price = match trend {
                "up" => start_price + Decimal::from(i * 10),
                "down" => start_price - Decimal::from(i * 10),
                _ => start_price,
            };

            klines.push(OHLCData::new(
                base_time + chrono::Duration::hours(i as i64),
                "BTCUSDT".to_string(),
                Timeframe::OneHour,
                price - Decimal::from(5),
                price + Decimal::from(10),
                price - Decimal::from(15),
                price,
                Decimal::from(100),
                10,
            ));
        }

        klines
    }

    #[test]
    fn test_trend_strategy_analyze() {
        let mut strategy = TrendStrategy::new();

        let mut klines = HashMap::new();
        klines.insert(Timeframe::FourHour, create_test_klines(60, Decimal::from(1000), "up"));
        klines.insert(Timeframe::OneHour, create_test_klines(60, Decimal::from(1000), "up"));
        klines.insert(Timeframe::FifteenMinutes, create_test_klines(60, Decimal::from(1000), "up"));

        let analysis = strategy.analyze(&klines);

        // 上升趋势应该被识别为看涨
        assert!(analysis.overall_confidence > Decimal::ZERO);
    }

    #[test]
    fn test_trend_strategy_name() {
        let strategy = TrendStrategy::new();
        assert_eq!(strategy.name(), "Multi-Timeframe Trend");
    }

    #[test]
    fn test_required_timeframes() {
        let strategy = TrendStrategy::new();
        let timeframes = strategy.required_timeframes();
        assert_eq!(timeframes.len(), 3);
        assert!(timeframes.contains(&Timeframe::FourHour));
        assert!(timeframes.contains(&Timeframe::OneHour));
        assert!(timeframes.contains(&Timeframe::FifteenMinutes));
    }
}
