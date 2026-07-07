// K线聚合器单元测试

#[cfg(test)]
mod tests {
    use crate::data::aggregator::KlineAggregator;
    use crate::data::types::{OHLCData, Timeframe};

    /// 创建测试用 1m K线
    fn create_kline_1m(timestamp_mins: i64, open: &str, high: &str, low: &str, close: &str, volume: &str) -> OHLCData {
        use chrono::{DateTime, TimeZone, Utc};
        let timestamp = Utc.timestamp_opt(timestamp_mins * 60, 0).unwrap();
        OHLCData::new(
            timestamp,
            "BTCUSDT".to_string(),
            Timeframe::OneMinute,
            rust_decimal::Decimal::from_str(open).unwrap(),
            rust_decimal::Decimal::from_str(high).unwrap(),
            rust_decimal::Decimal::from_str(low).unwrap(),
            rust_decimal::Decimal::from_str(close).unwrap(),
            rust_decimal::Decimal::from_str(volume).unwrap(),
            10,
        )
    }

    use std::str::FromStr;

    // ========== 基础功能测试 ==========

    #[test]
    fn test_aggregator_initialization() {
        let aggregator = KlineAggregator::new();
        assert_eq!(aggregator.count(Timeframe::FiveMinutes), 0);
        assert_eq!(aggregator.count(Timeframe::OneHour), 0);
        assert!(aggregator.get_klines_1m(10).is_empty());
    }

    #[test]
    fn test_aggregator_default() {
        let aggregator = KlineAggregator::default();
        assert_eq!(aggregator.count(Timeframe::FiveMinutes), 0);
    }

    // ========== 1m K线存储测试 ==========

    #[test]
    fn test_store_1m_klines() {
        let mut aggregator = KlineAggregator::new();

        // 添加 10 根 1m K线
        for i in 0..10 {
            let kline = create_kline_1m(i, "50000", "50100", "49900", "50050", "100");
            aggregator.update(kline);
        }

        // 验证 1m 数据
        let klines_1m = aggregator.get_klines_1m(100);
        assert_eq!(klines_1m.len(), 10);
    }

    #[test]
    fn test_get_1m_klines_limit() {
        let mut aggregator = KlineAggregator::new();

        // 添加 100 根 1m K线
        for i in 0..100 {
            let kline = create_kline_1m(i, "50000", "50100", "49900", "50050", "100");
            aggregator.update(kline);
        }

        // 获取最新 10 根
        let klines_1m = aggregator.get_klines_1m(10);
        assert_eq!(klines_1m.len(), 10);
    }

    // ========== 5 分钟聚合测试 ==========

    #[test]
    fn test_5m_aggregation_basic() {
        let mut aggregator = KlineAggregator::new();

        // 添加 5 根 1m K线（同一 5 分钟窗口）
        for i in 0..5 {
            let kline = create_kline_1m(i, "50000", "50100", "49900", "50050", "100");
            aggregator.update(kline);
        }

        // 5 分钟窗口应该有一根正在进行的 K线
        let klines_5m = aggregator.get_klines(Timeframe::FiveMinutes, 10);
        assert_eq!(klines_5m.len(), 1);

        // 验证聚合结果
        let candle = &klines_5m[0];
        assert_eq!(candle.open, rust_decimal::Decimal::from(50000));
        assert_eq!(candle.high, rust_decimal::Decimal::from(50100));
        assert_eq!(candle.low, rust_decimal::Decimal::from(49900));
        assert_eq!(candle.close, rust_decimal::Decimal::from(50050));
        assert_eq!(candle.volume, rust_decimal::Decimal::from(500));
    }

    #[test]
    fn test_5m_aggregation_window_boundary() {
        let mut aggregator = KlineAggregator::new();

        // 添加跨越 5 分钟边界的 K线
        // 第一个窗口: 0-4 分钟
        for i in 0..5 {
            let kline = create_kline_1m(i, "50000", "50100", "49900", "50050", "100");
            aggregator.update(kline);
        }

        // 第二个窗口: 5-9 分钟
        for i in 5..10 {
            let kline = create_kline_1m(i, "50100", "50200", "50000", "50150", "120");
            aggregator.update(kline);
        }

        // 应该有 1 根已完成的 5m K线 + 1 根正在进行的
        let klines_5m = aggregator.get_klines(Timeframe::FiveMinutes, 10);
        assert_eq!(klines_5m.len(), 2);

        // 第一根应该是第一个窗口的聚合
        let first = &klines_5m[0];
        assert_eq!(first.open, rust_decimal::Decimal::from(50000));
        assert_eq!(first.close, rust_decimal::Decimal::from(50050));
    }

    // ========== 15 分钟聚合测试 ==========

    #[test]
    fn test_15m_aggregation() {
        let mut aggregator = KlineAggregator::new();

        // 添加 15 根 1m K线
        for i in 0..15 {
            let kline = create_kline_1m(i, "50000", "50100", "49900", "50050", "100");
            aggregator.update(kline);
        }

        // 15 分钟窗口应该有一根正在进行的 K线
        let klines_15m = aggregator.get_klines(Timeframe::FifteenMinutes, 10);
        assert_eq!(klines_15m.len(), 1);

        // 验证聚合
        let candle = &klines_15m[0];
        assert_eq!(candle.volume, rust_decimal::Decimal::from(1500));
    }

    // ========== 1 小时聚合测试 ==========

    #[test]
    fn test_1h_aggregation() {
        let mut aggregator = KlineAggregator::new();

        // 添加 60 根 1m K线
        for i in 0..60 {
            let kline = create_kline_1m(i, "50000", "50100", "49900", "50050", "100");
            aggregator.update(kline);
        }

        // 1 小时窗口应该有一根正在进行的 K线
        let klines_1h = aggregator.get_klines(Timeframe::OneHour, 10);
        assert_eq!(klines_1h.len(), 1);

        // 验证聚合
        let candle = &klines_1h[0];
        assert_eq!(candle.volume, rust_decimal::Decimal::from(6000));
    }

    // ========== 多时间框架测试 ==========

    #[test]
    fn test_multiple_timeframes() {
        let mut aggregator = KlineAggregator::new();

        // 添加 60 根 1m K线
        for i in 0..60 {
            let kline = create_kline_1m(i, "50000", "50100", "49900", "50050", "100");
            aggregator.update(kline);
        }

        // 获取所有时间框架
        let all_timeframes = aggregator.get_all_timeframes();

        // 应该有 7 个时间框架
        assert_eq!(all_timeframes.len(), 7);
        assert!(all_timeframes.contains_key(&Timeframe::OneMinute));
        assert!(all_timeframes.contains_key(&Timeframe::FiveMinutes));
        assert!(all_timeframes.contains_key(&Timeframe::FifteenMinutes));
        assert!(all_timeframes.contains_key(&Timeframe::ThirtyMinutes));
        assert!(all_timeframes.contains_key(&Timeframe::OneHour));
        assert!(all_timeframes.contains_key(&Timeframe::FourHour));
        assert!(all_timeframes.contains_key(&Timeframe::OneDay));
    }

    // ========== 高低价聚合测试 ==========

    #[test]
    fn test_high_low_aggregation() {
        let mut aggregator = KlineAggregator::new();

        // 添加 5 根 1m K线，有不同的高低点
        let klines = vec![
            create_kline_1m(0, "50000", "50200", "49800", "50100", "100"),
            create_kline_1m(1, "50100", "50300", "49900", "50200", "120"),
            create_kline_1m(2, "50200", "50400", "50000", "50300", "80"),
            create_kline_1m(3, "50300", "50500", "50100", "50400", "150"),
            create_kline_1m(4, "50400", "50600", "50200", "50500", "90"),
        ];

        for kline in klines {
            aggregator.update(kline);
        }

        let klines_5m = aggregator.get_klines(Timeframe::FiveMinutes, 10);
        let candle = &klines_5m[0];

        // 验证最高价和最低价
        assert_eq!(candle.high, rust_decimal::Decimal::from(50600));
        assert_eq!(candle.low, rust_decimal::Decimal::from(49800));
        assert_eq!(candle.open, rust_decimal::Decimal::from(50000));
        assert_eq!(candle.close, rust_decimal::Decimal::from(50500));
    }

    // ========== 清空测试 ==========

    #[test]
    fn test_clear() {
        let mut aggregator = KlineAggregator::new();

        // 添加一些数据
        for i in 0..10 {
            let kline = create_kline_1m(i, "50000", "50100", "49900", "50050", "100");
            aggregator.update(kline);
        }

        // 清空
        aggregator.clear();

        // 验证数据已清空
        assert_eq!(aggregator.count(Timeframe::FiveMinutes), 0);
        assert!(aggregator.get_klines_1m(10).is_empty());
    }

    // ========== 边界情况测试 ==========

    #[test]
    fn test_empty_aggregation() {
        let aggregator = KlineAggregator::new();

        // 获取不存在的时间框架数据
        let klines = aggregator.get_klines(Timeframe::FiveMinutes, 10);
        assert!(klines.is_empty());
    }

    #[test]
    fn test_single_kline() {
        let mut aggregator = KlineAggregator::new();

        // 添加 1 根 1m K线
        let kline = create_kline_1m(0, "50000", "50100", "49900", "50050", "100");
        aggregator.update(kline);

        // 应该有 1 根正在进行的 5m K线
        let klines_5m = aggregator.get_klines(Timeframe::FiveMinutes, 10);
        assert_eq!(klines_5m.len(), 1);
    }

    // ========== OHLCData 创建测试 ==========

    #[test]
    fn test_ohlc_data_creation() {
        use chrono::Utc;

        let kline = OHLCData::new(
            Utc::now(),
            "BTCUSDT".to_string(),
            Timeframe::OneMinute,
            rust_decimal::Decimal::from(50000),
            rust_decimal::Decimal::from(50100),
            rust_decimal::Decimal::from(49900),
            rust_decimal::Decimal::from(50050),
            rust_decimal::Decimal::from(100),
            10,
        );

        assert_eq!(kline.symbol, "BTCUSDT");
        assert_eq!(kline.timeframe, Timeframe::OneMinute);
        assert_eq!(kline.open, rust_decimal::Decimal::from(50000));
        assert_eq!(kline.high, rust_decimal::Decimal::from(50100));
        assert_eq!(kline.low, rust_decimal::Decimal::from(49900));
        assert_eq!(kline.close, rust_decimal::Decimal::from(50050));
        assert_eq!(kline.volume, rust_decimal::Decimal::from(100));
        assert_eq!(kline.trade_count, 10);
    }

    // ========== Timeframe 对齐测试 ==========

    #[test]
    fn test_timeframe_alignment() {
        use chrono::{TimeZone, Utc};

        // 测试 5 分钟对齐
        let timestamp = Utc.timestamp_opt(300, 0).unwrap(); // 5 分钟
        let aligned = Timeframe::FiveMinutes.align_timestamp(timestamp);
        assert_eq!(aligned, timestamp);

        // 测试非对齐时间戳
        let timestamp = Utc.timestamp_opt(330, 0).unwrap(); // 5 分钟 30 秒
        let aligned = Timeframe::FiveMinutes.align_timestamp(timestamp);
        assert_eq!(aligned, Utc.timestamp_opt(300, 0).unwrap());
    }

    // ========== 批量更新测试 ==========

    #[test]
    fn test_batch_update_performance() {
        let mut aggregator = KlineAggregator::new();

        // 添加 1000 根 1m K线
        for i in 0..1000 {
            let kline = create_kline_1m(i, "50000", "50100", "49900", "50050", "100");
            aggregator.update(kline);
        }

        // 验证数据完整性
        let klines_1m = aggregator.get_klines_1m(1000);
        assert_eq!(klines_1m.len(), 1000);

        // 验证 5 分钟聚合
        let klines_5m = aggregator.get_klines(Timeframe::FiveMinutes, 100);
        // 1000 / 5 = 200 根 5m K线
        assert!(klines_5m.len() > 0);
    }
}
