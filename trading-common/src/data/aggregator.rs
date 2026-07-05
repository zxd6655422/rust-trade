// data/aggregator.rs
// K线聚合器：将 1m K线聚合为其他时间框架

use std::collections::HashMap;

use super::types::{OHLCData, Timeframe};

/// K线聚合器
pub struct KlineAggregator {
    /// 缓存的 1m K线数据
    klines_1m: Vec<OHLCData>,
    /// 聚结后的多时间框架数据
    aggregated: HashMap<Timeframe, Vec<OHLCData>>,
    /// 每个时间框架的当前聚合状态
    current_candles: HashMap<Timeframe, Option<OHLCData>>,
}

impl KlineAggregator {
    /// 创建新的聚合器
    pub fn new() -> Self {
        let mut current_candles = HashMap::new();
        // 初始化所有时间框架的当前 K线为空
        current_candles.insert(Timeframe::FiveMinutes, None);
        current_candles.insert(Timeframe::FifteenMinutes, None);
        current_candles.insert(Timeframe::ThirtyMinutes, None);
        current_candles.insert(Timeframe::OneHour, None);
        current_candles.insert(Timeframe::FourHours, None);
        current_candles.insert(Timeframe::OneDay, None);

        Self {
            klines_1m: Vec::new(),
            aggregated: HashMap::new(),
            current_candles,
        }
    }

    /// 添加新的 1m K线并更新所有时间框架
    pub fn update(&mut self, kline_1m: OHLCData) {
        // 存储 1m K线
        self.klines_1m.push(kline_1m.clone());

        // 更新所有时间框架
        self.update_timeframe(Timeframe::FiveMinutes, &kline_1m);
        self.update_timeframe(Timeframe::FifteenMinutes, &kline_1m);
        self.update_timeframe(Timeframe::ThirtyMinutes, &kline_1m);
        self.update_timeframe(Timeframe::OneHour, &kline_1m);
        self.update_timeframe(Timeframe::FourHours, &kline_1m);
        self.update_timeframe(Timeframe::OneDay, &kline_1m);
    }

    /// 更新指定时间框架
    fn update_timeframe(&mut self, timeframe: Timeframe, kline_1m: &OHLCData) {
        let window_start = timeframe.align_timestamp(kline_1m.timestamp);

        let current = self.current_candles.get_mut(&timeframe).unwrap();

        match current {
            Some(ref mut candle) => {
                // 检查是否需要开始新的 K线
                if candle.timestamp != window_start {
                    // 保存完成的 K线
                    self.aggregated
                        .entry(timeframe)
                        .or_insert_with(Vec::new)
                        .push(candle.clone());

                    // 开始新的 K线
                    *candle = OHLCData::new(
                        window_start,
                        kline_1m.symbol.clone(),
                        timeframe,
                        kline_1m.open,
                        kline_1m.high,
                        kline_1m.low,
                        kline_1m.close,
                        kline_1m.volume,
                        kline_1m.trade_count,
                    );
                } else {
                    // 更新当前 K线
                    candle.high = candle.high.max(kline_1m.high);
                    candle.low = candle.low.min(kline_1m.low);
                    candle.close = kline_1m.close;
                    candle.volume += kline_1m.volume;
                    candle.trade_count += kline_1m.trade_count;
                }
            }
            None => {
                // 开始新的 K线
                *current = Some(OHLCData::new(
                    window_start,
                    kline_1m.symbol.clone(),
                    timeframe,
                    kline_1m.open,
                    kline_1m.high,
                    kline_1m.low,
                    kline_1m.close,
                    kline_1m.volume,
                    kline_1m.trade_count,
                ));
            }
        }
    }

    /// 获取指定时间框架的最新 N 根 K线
    pub fn get_klines(&self, timeframe: Timeframe, count: usize) -> Vec<OHLCData> {
        // 1m 数据直接从 klines_1m 返回
        if timeframe == Timeframe::OneMinute {
            return self.get_klines_1m(count);
        }

        let mut result = Vec::new();

        // 从已完成的 K线中获取
        if let Some(klines) = self.aggregated.get(&timeframe) {
            result.extend_from_slice(klines);
        }

        // 始终追加当前正在形成的 K线（包含最新数据）
        if let Some(Some(ref candle)) = self.current_candles.get(&timeframe) {
            result.push(candle.clone());
        }

        // 取最后 count 根（最新的）
        if result.len() > count {
            let start = result.len() - count;
            result = result[start..].to_vec();
        }

        result
    }

    /// 获取所有时间框架的最新数据
    pub fn get_all_timeframes(&self) -> HashMap<Timeframe, Vec<OHLCData>> {
        let mut result = HashMap::new();

        // 对于每个时间框架，获取最新的一些 K线
        let timeframes = vec![
            Timeframe::OneMinute,
            Timeframe::FiveMinutes,
            Timeframe::FifteenMinutes,
            Timeframe::ThirtyMinutes,
            Timeframe::OneHour,
            Timeframe::FourHours,
            Timeframe::OneDay,
        ];

        for tf in timeframes {
            let count = match tf {
                Timeframe::OneMinute => 100,
                Timeframe::FiveMinutes => 50,
                Timeframe::FifteenMinutes => 50,
                Timeframe::ThirtyMinutes => 50,
                Timeframe::OneHour => 50,
                Timeframe::FourHours => 50,
                Timeframe::OneDay => 30,
                _ => 50,
            };
            result.insert(tf, self.get_klines(tf, count));
        }

        result
    }

    /// 获取 1m K线数据
    pub fn get_klines_1m(&self, count: usize) -> Vec<OHLCData> {
        let start = if self.klines_1m.len() > count {
            self.klines_1m.len() - count
        } else {
            0
        };
        self.klines_1m[start..].to_vec()
    }

    /// 清空所有数据
    pub fn clear(&mut self) {
        self.klines_1m.clear();
        self.aggregated.clear();
        for (_, v) in self.current_candles.iter_mut() {
            *v = None;
        }
    }

    /// 获取指定时间框架已完成的 K线数量
    pub fn count(&self, timeframe: Timeframe) -> usize {
        self.aggregated
            .get(&timeframe)
            .map(|v| v.len())
            .unwrap_or(0)
    }
}

impl Default for KlineAggregator {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{DateTime, Utc};
    use rust_decimal::Decimal;

    fn create_test_kline_1m(timestamp: DateTime<Utc>, close: Decimal) -> OHLCData {
        OHLCData::new(
            timestamp,
            "BTCUSDT".to_string(),
            Timeframe::OneMinute,
            close - Decimal::from(10),
            close + Decimal::from(10),
            close - Decimal::from(20),
            close,
            Decimal::from(100),
            10,
        )
    }

    #[test]
    fn test_aggregate_5m() {
        let mut aggregator = KlineAggregator::new();

        // 创建 5 根 1m K线，时间在同一个 5 分钟窗口内
        let base_time = Utc::now();
        let aligned = Timeframe::FiveMinutes.align_timestamp(base_time);

        for i in 0..5 {
            let timestamp = aligned + chrono::Duration::minutes(i);
            let kline = create_test_kline_1m(timestamp, Decimal::from(100 + i * 10));
            aggregator.update(kline);
        }

        // 应该有 1 根 5m K线（当前正在形成的）
        let klines_5m = aggregator.get_klines(Timeframe::FiveMinutes, 10);
        assert_eq!(klines_5m.len(), 1);

        // 验证 OHLC 值
        let candle = &klines_5m[0];
        assert_eq!(candle.open, Decimal::from(90)); // 100 - 10
        assert_eq!(candle.high, Decimal::from(150)); // 140 + 10
        assert_eq!(candle.low, Decimal::from(80)); // 100 - 20
        assert_eq!(candle.close, Decimal::from(140)); // 最后一根的 close
    }

    #[test]
    fn test_aggregate_multiple_windows() {
        let mut aggregator = KlineAggregator::new();

        // 创建跨越两个 5 分钟窗口的数据
        let base_time = Utc::now();
        let aligned = Timeframe::FiveMinutes.align_timestamp(base_time);

        // 第一个窗口：3 根 K线
        for i in 0..3 {
            let timestamp = aligned + chrono::Duration::minutes(i);
            let kline = create_test_kline_1m(timestamp, Decimal::from(100 + i * 10));
            aggregator.update(kline);
        }

        // 第二个窗口：2 根 K线
        for i in 0..2 {
            let timestamp = aligned + chrono::Duration::minutes(5 + i);
            let kline = create_test_kline_1m(timestamp, Decimal::from(200 + i * 10));
            aggregator.update(kline);
        }

        // 应该有 1 根已完成的 5m K线 + 1 根正在形成的
        let klines_5m = aggregator.get_klines(Timeframe::FiveMinutes, 10);
        assert_eq!(klines_5m.len(), 2);
    }
}
