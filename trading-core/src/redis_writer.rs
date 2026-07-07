use redis::Commands;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use tracing::{debug, warn};

use trading_common::data::types::{OHLCData, Timeframe};

// =================================================================
// Constants
// =================================================================

/// 每个时间框架缓存的 K 线数量
/// 20000 根覆盖时间：
///   1m: 约 14 天
///   5m: 约 69 天
///   15m: 约 208 天
///   1h: 约 2.3 年（支持大周期分析）
///   4h: 约 9.1 年
///   1d: 约 54.8 年
///   1w: 约 384 年
const KLINE_CACHE_SIZE: usize = 20000;

/// Redis 缓存 TTL（秒）- 1m K 线
const KLINE_1M_TTL: usize = 600; // 10 分钟

/// Redis 缓存 TTL（秒）- 其他时间框架
const KLINE_TTL: usize = 3600; // 1 小时

// =================================================================
// Data structures
// =================================================================

#[derive(Debug, Clone, Serialize)]
struct KlineData {
    timestamp: i64,
    open: f64,
    high: f64,
    low: f64,
    close: f64,
    volume: f64,
}

/// ZSET member 格式：timestamp 作为 score，kline JSON 作为 member
#[derive(Debug, Clone, Serialize, Deserialize)]
struct KlineZsetMember {
    ts: i64,
    o: f64,
    h: f64,
    l: f64,
    c: f64,
    v: f64,
}

// =================================================================
// Helper functions
// =================================================================

/// Convert Decimal to f64 (lossy but acceptable for caching)
fn decimal_to_f64(d: Decimal) -> f64 {
    d.to_string().parse::<f64>().unwrap_or(0.0)
}

/// 获取时间框架对应的秒数
fn timeframe_seconds(tf: &Timeframe) -> i64 {
    match tf {
        Timeframe::OneMinute => 60,
        Timeframe::FiveMinutes => 300,
        Timeframe::FifteenMinutes => 900,
        Timeframe::ThirtyMinutes => 1800,
        Timeframe::OneHour => 3600,
        Timeframe::TwoHour => 7200,
        Timeframe::FourHour => 14400,
        Timeframe::OneDay => 86400,
        Timeframe::ThreeDay => 259200,
        Timeframe::OneWeek => 604800,
    }
}

/// 获取时间框架的 Redis key 后缀
fn timeframe_key_suffix(tf: &Timeframe) -> &'static str {
    match tf {
        Timeframe::OneMinute => "1m",
        Timeframe::FiveMinutes => "5m",
        Timeframe::FifteenMinutes => "15m",
        Timeframe::ThirtyMinutes => "30m",
        Timeframe::OneHour => "1h",
        Timeframe::TwoHour => "2h",
        Timeframe::FourHour => "4h",
        Timeframe::OneDay => "1d",
        Timeframe::ThreeDay => "3d",
        Timeframe::OneWeek => "1w",
    }
}

/// 获取时间框架的 TTL
fn timeframe_ttl(tf: &Timeframe) -> usize {
    match tf {
        Timeframe::OneMinute => KLINE_1M_TTL,
        Timeframe::OneDay | Timeframe::ThreeDay | Timeframe::OneWeek => 86400, // 1天
        _ => KLINE_TTL,
    }
}

// =================================================================
// Kline aggregation
// =================================================================

/// 从 1m K 线聚合生成指定时间框架的 K 线
fn aggregate_klines(klines_1m: &[OHLCData], target_tf: &Timeframe) -> Vec<OHLCData> {
    if target_tf == &Timeframe::OneMinute {
        return klines_1m.to_vec();
    }

    let interval_secs = timeframe_seconds(target_tf);
    let mut aggregated: Vec<OHLCData> = Vec::new();

    // 按时间窗口分组
    let mut current_window: Vec<&OHLCData> = Vec::new();
    let mut window_start: Option<i64> = None;

    for kline in klines_1m {
        let ts = kline.timestamp.timestamp();
        let bucket = (ts / interval_secs) * interval_secs;

        if window_start.is_none() {
            window_start = Some(bucket);
        }

        if Some(bucket) == window_start {
            current_window.push(kline);
        } else {
            // 聚合当前窗口
            if !current_window.is_empty() {
                if let Some(agg) = aggregate_window(&current_window, target_tf) {
                    aggregated.push(agg);
                }
            }
            // 开始新窗口
            current_window.clear();
            window_start = Some(bucket);
            current_window.push(kline);
        }
    }

    // 处理最后一个窗口
    if !current_window.is_empty() {
        if let Some(agg) = aggregate_window(&current_window, target_tf) {
            aggregated.push(agg);
        }
    }

    aggregated
}

/// 聚合一个时间窗口内的 K 线
fn aggregate_window(klines: &[&OHLCData], target_tf: &Timeframe) -> Option<OHLCData> {
    if klines.is_empty() {
        return None;
    }

    let first = klines[0];
    let last = klines[klines.len() - 1];

    let open = first.open;
    let high = klines.iter().map(|k| k.high).max()?;
    let low = klines.iter().map(|k| k.low).min()?;
    let close = last.close;
    let volume = klines.iter().map(|k| k.volume).sum();
    let trade_count = klines.iter().map(|k| k.trade_count).sum();

    // 使用窗口开始时间作为 K 线时间
    let interval_secs = timeframe_seconds(target_tf);
    let ts = first.timestamp.timestamp();
    let window_start = (ts / interval_secs) * interval_secs;

    Some(OHLCData {
        timestamp: chrono::DateTime::from_timestamp(window_start, 0)?.with_timezone(&chrono::Utc),
        symbol: first.symbol.clone(),
        timeframe: *target_tf,
        open,
        high,
        low,
        close,
        volume,
        trade_count,
    })
}

// =================================================================
// Redis write functions
// =================================================================

/// 将 K 线数据写入 Redis（使用 ZSET）
///
/// Key 格式：kline:{symbol}:{timeframe}
/// Score：timestamp (毫秒)
/// Member：JSON {ts, o, h, l, c, v}
///
/// 每个时间框架保留最新 KLINE_CACHE_SIZE 根 K 线
fn write_kline_zset(
    conn: &mut redis::Connection,
    symbol: &str,
    timeframe: &Timeframe,
    klines: &[KlineData],
) -> anyhow::Result<()> {
    if klines.is_empty() {
        return Ok(());
    }

    let key = format!("kline:{}:{}", symbol, timeframe_key_suffix(timeframe));
    let ttl = timeframe_ttl(timeframe);

    // 先删除同时间戳的旧条目，再写入新数据，避免重复
    // ZSET member 是 JSON，同一时间戳可能有多个不同 JSON（close/volume 变化）
    let mut pipe = redis::pipe();
    for kline in klines {
        // 精确删除该时间戳的所有旧条目（score = timestamp 即时间戳范围）
        pipe.cmd("ZREMRANGEBYSCORE").arg(&key).arg(kline.timestamp).arg(kline.timestamp);
    }
    pipe.execute(conn);

    // 写入新数据
    let mut pipe = redis::pipe();
    for kline in klines {
        let member_json = serde_json::to_string(&KlineZsetMember {
            ts: kline.timestamp,
            o: kline.open,
            h: kline.high,
            l: kline.low,
            c: kline.close,
            v: kline.volume,
        })?;
        pipe.cmd("ZADD").arg(&key).arg(kline.timestamp).arg(&member_json);
    }
    pipe.execute(conn);

    // 裁剪到最新 N 根（保留 score 最大的）
    let total: usize = redis::cmd("ZCARD").arg(&key).query(conn)?;
    if total > KLINE_CACHE_SIZE {
        let remove_count = total - KLINE_CACHE_SIZE;
        // ZREMRANGEBYRANK 移除 score 最小的（索引 0 到 remove_count-1）
        redis::cmd("ZREMRANGEBYRANK")
            .arg(&key)
            .arg(0)
            .arg((remove_count - 1) as isize)
            .query::<()>(conn)?;
    }

    // 设置 TTL
    redis::cmd("EXPIRE").arg(&key).arg(ttl).query::<()>(conn)?;

    Ok(())
}

/// 写入 1m K 线并聚合生成其他时间框架
///
/// 调用时机：每次拉取新的 1m K 线后
/// 处理流程：
///   1. 写入 1m K 线到 Redis ZSET
///   2. 从 Redis 读取足够的 1m K 线用于聚合
///   3. 聚合生成 5m/15m/30m/1h/2h/4h K 线
///   4. 写入各时间框架到 Redis ZSET
pub fn write_market_data(redis_url: &str, symbol: &str, ohlc_list: &[OHLCData]) -> anyhow::Result<()> {
    if ohlc_list.is_empty() {
        return Ok(());
    }

    let client = redis::Client::open(redis_url)?;
    let mut conn = client.get_connection()?;

    // 1. 写入 1m K 线
    let klines_1m: Vec<KlineData> = ohlc_list
        .iter()
        .map(|k| KlineData {
            timestamp: k.timestamp.timestamp_millis(),
            open: decimal_to_f64(k.open),
            high: decimal_to_f64(k.high),
            low: decimal_to_f64(k.low),
            close: decimal_to_f64(k.close),
            volume: decimal_to_f64(k.volume),
        })
        .collect();

    write_kline_zset(&mut conn, symbol, &Timeframe::OneMinute, &klines_1m)?;

    // 2. 从 Redis 读取足够的 1m K 线用于聚合
    // 最大需要：4h = 240 根 1m K 线，但我们多读一些以确保完整性
    let read_limit = 1000; // 读取最新 1000 根 1m K 线
    let key_1m = format!("kline:{}:1m", symbol);

    let kline_jsons: Vec<String> = redis::cmd("ZREVRANGE")
        .arg(&key_1m)
        .arg(0)
        .arg((read_limit - 1) as isize)
        .query(&mut conn)?;

    let mut all_klines_1m: Vec<OHLCData> = kline_jsons
        .iter()
        .filter_map(|json| {
            let member: KlineZsetMember = serde_json::from_str(json).ok()?;
            Some(OHLCData {
                timestamp: chrono::DateTime::from_timestamp_millis(member.ts)?.with_timezone(&chrono::Utc),
                symbol: symbol.to_string(),
                timeframe: Timeframe::OneMinute,
                open: Decimal::from_f64_retain(member.o)?,
                high: Decimal::from_f64_retain(member.h)?,
                low: Decimal::from_f64_retain(member.l)?,
                close: Decimal::from_f64_retain(member.c)?,
                volume: Decimal::from_f64_retain(member.v)?,
                trade_count: 0,
            })
        })
        .collect();

    // 按时间排序（从旧到新）
    all_klines_1m.sort_by_key(|k| k.timestamp);

    // 3. 聚合并写入其他时间框架
    let timeframes = [
        Timeframe::FiveMinutes,
        Timeframe::FifteenMinutes,
        Timeframe::ThirtyMinutes,
        Timeframe::OneHour,
        Timeframe::TwoHour,
        Timeframe::FourHour,
        Timeframe::OneDay,
        Timeframe::ThreeDay,
        Timeframe::OneWeek,
    ];

    for tf in &timeframes {
        let aggregated = aggregate_klines(&all_klines_1m, tf);
        if !aggregated.is_empty() {
            let klines: Vec<KlineData> = aggregated
                .iter()
                .map(|k| KlineData {
                    timestamp: k.timestamp.timestamp_millis(),
                    open: decimal_to_f64(k.open),
                    high: decimal_to_f64(k.high),
                    low: decimal_to_f64(k.low),
                    close: decimal_to_f64(k.close),
                    volume: decimal_to_f64(k.volume),
                })
                .collect();

            write_kline_zset(&mut conn, symbol, tf, &klines)?;
        }
    }

    debug!(
        "[{}] Redis 写入完成: 1m({} 条), 已聚合 5m/15m/30m/1h/2h/4h/1d/3d/1w",
        symbol,
        klines_1m.len(),
    );

    Ok(())
}

// =================================================================
// Legacy compatibility - 保留旧接口但标记废弃
// =================================================================

/// 获取当前价格（从最新 1m K 线）
pub fn get_current_price(redis_url: &str, symbol: &str) -> anyhow::Result<f64> {
    let client = redis::Client::open(redis_url)?;
    let mut conn = client.get_connection()?;

    let key = format!("kline:{}:1m", symbol);

    // 获取 score 最大的 member（最新 K 线）
    let kline_json: Option<String> = redis::cmd("ZREVRANGE")
        .arg(&key)
        .arg(0)
        .arg(0)
        .query(&mut conn)?;

    match kline_json {
        Some(json) => {
            let member: KlineZsetMember = serde_json::from_str(&json)?;
            Ok(member.c) // 返回收盘价
        }
        None => Ok(0.0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_aggregate_klines() {
        // 创建测试数据：10 根 1m K 线
        let base_ts = chrono::DateTime::parse_from_str("2024-01-01 00:00:00 +0000", "%Y-%m-%d %H:%M:%S %z")
            .unwrap()
            .with_timezone(&chrono::Utc);

        let klines: Vec<OHLCData> = (0..10)
            .map(|i| OHLCData {
                timestamp: base_ts + chrono::Duration::minutes(i),
                symbol: "BTCUSDT".to_string(),
                timeframe: Timeframe::OneMinute,
                open: Decimal::from(100 + i),
                high: Decimal::from(105 + i),
                low: Decimal::from(95 + i),
                close: Decimal::from(102 + i),
                volume: Decimal::from(1000 + i * 100),
                trade_count: 10 + i as i32,
            })
            .collect();

        // 聚合为 5m K 线
        let aggregated = aggregate_klines(&klines, &Timeframe::FiveMinutes);

        // 应该有 2 根 5m K 线（0-4 分钟，5-9 分钟）
        assert_eq!(aggregated.len(), 2);

        // 第一根 5m K 线
        assert_eq!(aggregated[0].open, Decimal::from(100));
        assert_eq!(aggregated[0].high, Decimal::from(109)); // max(105,106,107,108,109)
        assert_eq!(aggregated[0].low, Decimal::from(95));   // min(95,96,97,98,99)
        assert_eq!(aggregated[0].close, Decimal::from(106));
    }
}
