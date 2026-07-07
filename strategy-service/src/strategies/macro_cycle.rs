use async_trait::async_trait;
use serde::{Deserialize, Serialize};

use super::{Signal, SignalType, Strategy};
use crate::indicators;
use crate::redis_reader::{KlineData, MarketData, Timeframe};

/// 大周期分析策略参数
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MacroCycleParams {
    /// 主分析周期：1w（周K）或 3d（3日K）
    pub primary_timeframe: String,
    /// 辅助分析周期
    pub secondary_timeframe: String,
    /// 均线周期列表
    pub ma_periods: Vec<usize>,
    /// 接近历史高点/低点的阈值（百分比）
    pub proximity_threshold: f64,
    /// 趋势确认所需的 ADX 阈值
    pub adx_threshold: f64,
    /// 历史高点回看周期数
    pub lookback_periods: usize,
}

/// 大周期分析策略
///
/// 分析逻辑：
/// 1. 识别历史高点/低点（支撑/阻力位）
/// 2. 判断当前价格相对于历史位置
/// 3. 结合均线和 ADX 确认趋势
/// 4. 生成买入/卖出信号
pub struct MacroCycleStrategy {
    params: MacroCycleParams,
}

impl MacroCycleStrategy {
    /// 查找历史最高点
    fn find_highest_high(&self, klines: &[KlineData], lookback: usize) -> Option<f64> {
        if klines.is_empty() {
            return None;
        }
        let start = if klines.len() > lookback {
            klines.len() - lookback
        } else {
            0
        };
        klines[start..].iter().map(|k| k.high).reduce(f64::max)
    }

    /// 查找历史最低点
    fn find_lowest_low(&self, klines: &[KlineData], lookback: usize) -> Option<f64> {
        if klines.is_empty() {
            return None;
        }
        let start = if klines.len() > lookback {
            klines.len() - lookback
        } else {
            0
        };
        klines[start..].iter().map(|k| k.low).reduce(f64::min)
    }

    /// 计算价格相对于历史位置的百分比
    fn price_position(&self, current: f64, high: f64, low: f64) -> f64 {
        if high == low {
            return 0.5;
        }
        (current - low) / (high - low)
    }

    /// 识别支撑/阻力位
    fn find_support_resistance(&self, klines: &[KlineData]) -> Vec<(f64, String)> {
        let mut levels: Vec<(f64, String)> = Vec::new();

        if klines.len() < 20 {
            return levels;
        }

        // 使用滑动窗口识别局部高点和低点
        let window = 10;
        for i in window..klines.len() - window {
            let slice = &klines[i - window..i + window + 1];
            let current_high = klines[i].high;
            let current_low = klines[i].low;

            // 检查是否是局部高点
            let is_local_high = slice.iter().all(|k| k.high <= current_high);
            if is_local_high {
                levels.push((current_high, "resistance".to_string()));
            }

            // 检查是否是局部低点
            let is_local_low = slice.iter().all(|k| k.low >= current_low);
            if is_local_low {
                levels.push((current_low, "support".to_string()));
            }
        }

        // 去重并排序
        levels.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());
        levels.dedup_by(|a, b| (a.0 - b.0).abs() < 0.001);

        levels
    }
}

#[async_trait]
impl Strategy for MacroCycleStrategy {
    fn name(&self) -> &str {
        "macro_cycle"
    }

    async fn analyze(&self, data: &MarketData) -> Option<Signal> {
        let current_price = data.current_price;

        // 计算均线
        let multi_ma = indicators::calculate_multi_ma(&data.klines, &self.params.ma_periods);

        // 获取均线值
        let ma_values: Vec<(usize, f64)> = multi_ma.values.clone();
        let ma_short = ma_values.first().map(|(_, v)| *v)?;
        let ma_long = ma_values.last().map(|(_, v)| *v)?;

        // 计算 ADX
        let adx = indicators::calculate_adx(&data.klines, 14).map(|r| r.adx).unwrap_or(0.0);

        // 查找历史高低点
        let lookback = self.params.lookback_periods.min(data.klines.len());
        let historical_high = self.find_highest_high(&data.klines, lookback)?;
        let historical_low = self.find_lowest_low(&data.klines, lookback)?;

        // 计算价格位置
        let position = self.price_position(current_price, historical_high, historical_low);

        // 识别支撑/阻力位
        let levels = self.find_support_resistance(&data.klines);

        // 计算距离历史高点/低点的百分比
        let distance_to_high = (current_price - historical_high) / historical_high;
        let distance_to_low = (current_price - historical_low) / historical_low;

        // 趋势判断
        let is_uptrend = ma_short > ma_long && adx > self.params.adx_threshold;
        let is_downtrend = ma_short < ma_long && adx > self.params.adx_threshold;

        // 接近关键位置
        let near_high = distance_to_high.abs() < self.params.proximity_threshold / 100.0;
        let near_low = distance_to_low.abs() < self.params.proximity_threshold / 100.0;

        // 计算 ATR 用于止损
        let atr = indicators::calculate_atr(&data.klines, 14)
            .map(|r| r.value)
            .unwrap_or(current_price * 0.02);

        // 生成信号
        let signal = if near_high && is_downtrend {
            // 接近历史高点且趋势向下 → 卖出
            let stop_loss = current_price + 2.0 * atr;
            let take_profit = current_price - 3.0 * atr;
            let signal_strength = (1.0 - position) * 0.8;

            Some(Signal {
                signal_type: SignalType::Sell,
                signal_strength,
                entry_price: current_price,
                stop_loss: Some(stop_loss),
                take_profit: Some(take_profit),
                confidence: 0.75,
                reason: format!(
                    "接近历史阻力位 {:.2}（距离高点 {:.1}%），趋势向下，ADX={:.1}",
                    historical_high,
                    distance_to_high * 100.0,
                    adx,
                ),
                market_context: serde_json::json!({
                    "current_price": current_price,
                    "historical_high": historical_high,
                    "historical_low": historical_low,
                    "distance_to_high_pct": distance_to_high * 100.0,
                    "distance_to_low_pct": distance_to_low * 100.0,
                    "price_position": position,
                    "adx": adx,
                    "atr": atr,
                    "ma_short": ma_short,
                    "ma_long": ma_long,
                    "is_uptrend": is_uptrend,
                    "is_downtrend": is_downtrend,
                    "support_resistance_levels": levels.len(),
                    "timeframe": data.timeframe.as_str(),
                }),
            })
        } else if near_low && is_uptrend {
            // 接近历史低点且趋势向上 → 买入
            let stop_loss = current_price - 2.0 * atr;
            let take_profit = current_price + 3.0 * atr;
            let signal_strength = position * 0.8;

            Some(Signal {
                signal_type: SignalType::Buy,
                signal_strength,
                entry_price: current_price,
                stop_loss: Some(stop_loss),
                take_profit: Some(take_profit),
                confidence: 0.75,
                reason: format!(
                    "接近历史支撑位 {:.2}（距离低点 {:.1}%），趋势向上，ADX={:.1}",
                    historical_low,
                    distance_to_low.abs() * 100.0,
                    adx,
                ),
                market_context: serde_json::json!({
                    "current_price": current_price,
                    "historical_high": historical_high,
                    "historical_low": historical_low,
                    "distance_to_high_pct": distance_to_high * 100.0,
                    "distance_to_low_pct": distance_to_low * 100.0,
                    "price_position": position,
                    "adx": adx,
                    "atr": atr,
                    "ma_short": ma_short,
                    "ma_long": ma_long,
                    "is_uptrend": is_uptrend,
                    "is_downtrend": is_downtrend,
                    "support_resistance_levels": levels.len(),
                    "timeframe": data.timeframe.as_str(),
                }),
            })
        } else if is_uptrend && position < 0.3 {
            // 趋势向上且价格在低位区域 → 买入
            let stop_loss = current_price - 2.0 * atr;
            let take_profit = current_price + 3.0 * atr;
            let signal_strength = (1.0 - position) * 0.6;

            Some(Signal {
                signal_type: SignalType::Buy,
                signal_strength,
                entry_price: current_price,
                stop_loss: Some(stop_loss),
                take_profit: Some(take_profit),
                confidence: 0.65,
                reason: format!(
                    "上升趋势中价格处于低位区域（位置 {:.1}%），ADX={:.1}",
                    position * 100.0,
                    adx,
                ),
                market_context: serde_json::json!({
                    "current_price": current_price,
                    "historical_high": historical_high,
                    "historical_low": historical_low,
                    "price_position": position,
                    "adx": adx,
                    "atr": atr,
                    "timeframe": data.timeframe.as_str(),
                }),
            })
        } else if is_downtrend && position > 0.7 {
            // 趋势向下且价格在高位区域 → 卖出
            let stop_loss = current_price + 2.0 * atr;
            let take_profit = current_price - 3.0 * atr;
            let signal_strength = position * 0.6;

            Some(Signal {
                signal_type: SignalType::Sell,
                signal_strength,
                entry_price: current_price,
                stop_loss: Some(stop_loss),
                take_profit: Some(take_profit),
                confidence: 0.65,
                reason: format!(
                    "下降趋势中价格处于高位区域（位置 {:.1}%），ADX={:.1}",
                    position * 100.0,
                    adx,
                ),
                market_context: serde_json::json!({
                    "current_price": current_price,
                    "historical_high": historical_high,
                    "historical_low": historical_low,
                    "price_position": position,
                    "adx": adx,
                    "atr": atr,
                    "timeframe": data.timeframe.as_str(),
                }),
            })
        } else {
            None
        };

        signal
    }

    fn from_params(params: &serde_json::Value) -> anyhow::Result<Self> {
        let params: MacroCycleParams = serde_json::from_value(params.clone())?;
        Ok(Self { params })
    }
}
