use redis::Commands;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use tracing::{debug, warn};

use trading_common::data::types::{OHLCData, Timeframe};

// =================================================================
// Cache Size Configuration
// =================================================================
// Based on quantitative trading cycle analysis:
// - 1m: 2 weeks (real-time aggregation source)
// - 5m/15m/30m: 1 month (short-term strategies)
// - 1h/2h/4h: 6 months (half bull/bear cycle)
// - 1d/3d: 5 years (full bull/bear cycle)
// - 1w: all available data (macro analysis)

/// 1m K线缓存数量：14天 × 1440 = 20160
const KLINE_1M_CACHE_SIZE: usize = 20160;

/// 5m K线缓存数量：30天 × 288 = 8640
const KLINE_5M_CACHE_SIZE: usize = 8640;

/// 15m K线缓存数量：30天 × 96 = 2880
const KLINE_15M_CACHE_SIZE: usize = 2880;

/// 30m K线缓存数量：30天 × 48 = 1440
const KLINE_30M_CACHE_SIZE: usize = 1440;

/// 1h K线缓存数量：180天 × 24 = 4320
const KLINE_1H_CACHE_SIZE: usize = 4320;

/// 2h K线缓存数量：180天 × 12 = 2160
const KLINE_2H_CACHE_SIZE: usize = 2160;

/// 4h K线缓存数量：180天 × 6 = 1080
const KLINE_4H_CACHE_SIZE: usize = 1080;

/// 1d K线缓存数量：5年 × 365 = 1825
const KLINE_1D_CACHE_SIZE: usize = 1825;

/// 3d K线缓存数量：5年 × 122 = 610
const KLINE_3D_CACHE_SIZE: usize = 610;

/// 1w K线缓存数量：尽量多，设为500（约10年）
const KLINE_1W_CACHE_SIZE: usize = 500;

/// 按需聚合框架缓存大小（不存数据库，只在Redis中）
const KLINE_3M_CACHE_SIZE: usize = 2880;   // 6天
const KLINE_6H_CACHE_SIZE: usize = 720;    // 180天
const KLINE_8H_CACHE_SIZE: usize = 540;    // 180天
const KLINE_12H_CACHE_SIZE: usize = 365;   // 180天
const KLINE_45M_CACHE_SIZE: usize = 1920;  // 60天

/// Redis 缓存 TTL（秒）
const KLINE_TTL_SHORT: usize = 3600;       // 1小时（1m）
const KLINE_TTL_MEDIUM: usize = 86400;     // 1天（分钟级）
const KLINE_TTL_LONG: usize = 604800;      // 7天（小时级及以上）

// =================================================================
// Data structures
// =================================================================

/// ZSET member 格式：timestamp 作为 score，kline JSON 作为 member
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KlineZsetMember {
    pub ts: i64,
    pub o: f64,
    pub h: f64,
    pub l: f64,
    pub c: f64,
    pub v: f64,
    #[serde(default)]
    pub tc: u64,  // trade_count（旧数据中缺失时默认为0）
}

// =================================================================
// Helper functions
// =================================================================

/// Convert Decimal to f64 (lossy but acceptable for caching)
fn decimal_to_f64(d: Decimal) -> f64 {
    d.to_string().parse::<f64>().unwrap_or(0.0)
}

/// 获取时间框架对应的缓存大小
pub fn get_cache_size(tf: &Timeframe) -> usize {
    match tf {
        Timeframe::OneMinute => KLINE_1M_CACHE_SIZE,
        Timeframe::ThreeMinutes => KLINE_3M_CACHE_SIZE,
        Timeframe::FiveMinutes => KLINE_5M_CACHE_SIZE,
        Timeframe::FifteenMinutes => KLINE_15M_CACHE_SIZE,
        Timeframe::ThirtyMinutes => KLINE_30M_CACHE_SIZE,
        Timeframe::FortyFiveMinutes => KLINE_45M_CACHE_SIZE,
        Timeframe::OneHour => KLINE_1H_CACHE_SIZE,
        Timeframe::TwoHour => KLINE_2H_CACHE_SIZE,
        Timeframe::FourHour => KLINE_4H_CACHE_SIZE,
        Timeframe::SixHour => KLINE_6H_CACHE_SIZE,
        Timeframe::EightHour => KLINE_8H_CACHE_SIZE,
        Timeframe::TwelveHour => KLINE_12H_CACHE_SIZE,
        Timeframe::OneDay => KLINE_1D_CACHE_SIZE,
        Timeframe::ThreeDay => KLINE_3D_CACHE_SIZE,
        Timeframe::OneWeek => KLINE_1W_CACHE_SIZE,
    }
}

/// 获取时间框架的 Redis key 后缀（直接委托给 Timeframe::as_str）
pub fn timeframe_key_suffix(tf: &Timeframe) -> &'static str {
    tf.as_str()
}

/// 获取时间框架的 TTL
pub fn timeframe_ttl(tf: &Timeframe) -> usize {
    match tf {
        Timeframe::OneMinute => KLINE_TTL_SHORT,
        Timeframe::ThreeMinutes
        | Timeframe::FiveMinutes
        | Timeframe::FifteenMinutes
        | Timeframe::ThirtyMinutes
        | Timeframe::FortyFiveMinutes => KLINE_TTL_MEDIUM,
        _ => KLINE_TTL_LONG,
    }
}

/// 获取所有需要存储的时间框架（写入数据库的）
pub fn get_stored_timeframes() -> Vec<Timeframe> {
    vec![
        Timeframe::OneMinute,
        Timeframe::FiveMinutes,
        Timeframe::FifteenMinutes,
        Timeframe::ThirtyMinutes,
        Timeframe::OneHour,
        Timeframe::TwoHour,
        Timeframe::FourHour,
        Timeframe::OneDay,
        Timeframe::ThreeDay,
        Timeframe::OneWeek,
    ]
}

/// 获取所有按需聚合的时间框架（不存数据库，只在 Redis 中聚合）
pub fn get_on_demand_timeframes() -> Vec<Timeframe> {
    vec![
        Timeframe::ThreeMinutes,
        Timeframe::FortyFiveMinutes,
        Timeframe::SixHour,
        Timeframe::EightHour,
        Timeframe::TwelveHour,
    ]
}

// =================================================================
// Kline aggregation (for on-demand timeframes)
// =================================================================

/// 从 1m K 线聚合生成指定时间框架的 K 线
///
/// 用于按需聚合的框架：3m, 45m, 6h, 8h, 12h
pub fn aggregate_klines(klines_1m: &[OHLCData], target_tf: &Timeframe) -> Vec<OHLCData> {
    if *target_tf == Timeframe::OneMinute {
        return klines_1m.to_vec();
    }

    let interval_secs = target_tf.as_duration().num_seconds();
    if interval_secs <= 0 {
        return Vec::new();
    }

    let mut aggregated: Vec<OHLCData> = Vec::new();
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
            if !current_window.is_empty() {
                if let Some(agg) = aggregate_window(&current_window, target_tf, interval_secs) {
                    aggregated.push(agg);
                }
            }
            current_window.clear();
            window_start = Some(bucket);
            current_window.push(kline);
        }
    }

    if !current_window.is_empty() {
        if let Some(agg) = aggregate_window(&current_window, target_tf, interval_secs) {
            aggregated.push(agg);
        }
    }

    aggregated
}

/// 聚合一个时间窗口内的 K 线
fn aggregate_window(klines: &[&OHLCData], target_tf: &Timeframe, interval_secs: i64) -> Option<OHLCData> {
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
pub fn write_kline_zset(
    conn: &mut redis::Connection,
    symbol: &str,
    timeframe: &Timeframe,
    klines: &[KlineZsetMember],
) -> anyhow::Result<()> {
    if klines.is_empty() {
        return Ok(());
    }

    let key = format!("kline:{}:{}", symbol, timeframe_key_suffix(timeframe));
    let ttl = timeframe_ttl(timeframe);
    let cache_size = get_cache_size(timeframe);

    // 写入新数据
    let mut pipe = redis::pipe();
    for kline in klines {
        let member_json = serde_json::to_string(kline)?;
        pipe.cmd("ZADD").arg(&key).arg(kline.ts).arg(&member_json);
    }
    pipe.execute(conn);

    // 裁剪到指定大小（保留 score 最大的）
    let total: usize = redis::cmd("ZCARD").arg(&key).query(conn)?;
    if total > cache_size {
        let remove_count = total - cache_size;
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

/// 写入单个时间框架的 OHLC 数据到 Redis
pub fn write_single_timeframe(
    conn: &mut redis::Connection,
    symbol: &str,
    timeframe: &Timeframe,
    ohlc_list: &[OHLCData],
) -> anyhow::Result<()> {
    let klines: Vec<KlineZsetMember> = ohlc_list
        .iter()
        .map(|k| KlineZsetMember {
            ts: k.timestamp.timestamp_millis(),
            o: decimal_to_f64(k.open),
            h: decimal_to_f64(k.high),
            l: decimal_to_f64(k.low),
            c: decimal_to_f64(k.close),
            v: decimal_to_f64(k.volume),
            tc: k.trade_count,
        })
        .collect();

    write_kline_zset(conn, symbol, timeframe, &klines)
}

/// 写入多时间框架数据到 Redis
///
/// 这是主要的写入接口，用于：
/// 1. 从数据库加载历史数据到缓存
/// 2. 实时数据写入
pub fn write_market_data_multi_tf(
    redis_url: &str,
    symbol: &str,
    data: &std::collections::HashMap<Timeframe, Vec<OHLCData>>,
) -> anyhow::Result<()> {
    if data.is_empty() {
        return Ok(());
    }

    let client = redis::Client::open(redis_url)?;
    let mut conn = client.get_connection()?;

    for (timeframe, ohlc_list) in data {
        write_single_timeframe(&mut conn, symbol, timeframe, ohlc_list)?;
    }

    debug!("[{}] Redis multi-TF write completed", symbol);
    Ok(())
}

/// 写入 1m K 线并按需聚合成其他框架
///
/// 保留向后兼容：
/// - 写入 1m K 线
/// - 按需聚合 3m/45m/6h/8h/12h 到 Redis（不存数据库）
/// - 高时间框架数据（5m/15m/30m/1h/2h/4h/1d/3d/1w）从数据库加载到 Redis
pub fn write_market_data(redis_url: &str, symbol: &str, ohlc_list: &[OHLCData]) -> anyhow::Result<()> {
    if ohlc_list.is_empty() {
        return Ok(());
    }

    let client = redis::Client::open(redis_url)?;
    let mut conn = client.get_connection()?;

    // 写入 1m K 线
    write_single_timeframe(&mut conn, symbol, &Timeframe::OneMinute, ohlc_list)?;

    // 按需聚合 3m, 45m, 6h, 8h, 12h 框架
    for tf in get_on_demand_timeframes() {
        let aggregated = aggregate_klines(ohlc_list, &tf);
        if !aggregated.is_empty() {
            write_single_timeframe(&mut conn, symbol, &tf, &aggregated)?;
        }
    }

    debug!(
        "[{}] Redis 写入完成: 1m({} 条), 已聚合按需框架(3m/45m/6h/8h/12h)",
        symbol,
        ohlc_list.len(),
    );

    Ok(())
}

// =================================================================
// Cache loading from database
// =================================================================

/// 从数据库加载 K 线数据到 Redis 缓存
///
/// 在以下场景调用：
/// 1. 系统启动时
/// 2. 定期刷新缓存
/// 3. 缓存 miss 时
pub async fn load_cache_from_db(
    redis_url: &str,
    symbol: &str,
    timeframe: &Timeframe,
    klines: &[OHLCData],
) -> anyhow::Result<usize> {
    if klines.is_empty() {
        return Ok(0);
    }

    let client = redis::Client::open(redis_url)?;
    let mut conn = client.get_connection()?;

    let count = klines.len();
    write_single_timeframe(&mut conn, symbol, timeframe, klines)?;

    debug!(
        "[{}] Loaded {} {} klines from database to Redis",
        symbol, count, timeframe_key_suffix(timeframe)
    );

    Ok(count)
}

/// 检查 Redis 缓存是否有足够的数据
pub fn has_sufficient_cache(
    redis_url: &str,
    symbol: &str,
    timeframe: &Timeframe,
    min_count: usize,
) -> anyhow::Result<bool> {
    let client = redis::Client::open(redis_url)?;
    let mut conn = client.get_connection()?;

    let key = format!("kline:{}:{}", symbol, timeframe_key_suffix(timeframe));
    let count: usize = redis::cmd("ZCARD").arg(&key).query(&mut conn)?;

    Ok(count >= min_count)
}

/// 获取 Redis 缓存中的 K 线数量
pub fn get_cache_count(
    redis_url: &str,
    symbol: &str,
    timeframe: &Timeframe,
) -> anyhow::Result<usize> {
    let client = redis::Client::open(redis_url)?;
    let mut conn = client.get_connection()?;

    let key = format!("kline:{}:{}", symbol, timeframe_key_suffix(timeframe));
    let count: usize = redis::cmd("ZCARD").arg(&key).query(&mut conn)?;

    Ok(count)
}

// =================================================================
// Legacy compatibility
// =================================================================

/// 获取当前价格（从最新 1m K 线）
pub fn get_current_price(redis_url: &str, symbol: &str) -> anyhow::Result<f64> {
    let client = redis::Client::open(redis_url)?;
    let mut conn = client.get_connection()?;

    let key = format!("kline:{}:1m", symbol);

    let kline_json: Option<String> = redis::cmd("ZREVRANGE")
        .arg(&key)
        .arg(0)
        .arg(0)
        .query(&mut conn)?;

    match kline_json {
        Some(json) => {
            let member: KlineZsetMember = serde_json::from_str(&json)?;
            Ok(member.c)
        }
        None => Ok(0.0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cache_sizes() {
        // 验证缓存大小配置
        assert_eq!(get_cache_size(&Timeframe::OneMinute), 20160);
        assert_eq!(get_cache_size(&Timeframe::ThreeMinutes), 2880);
        assert_eq!(get_cache_size(&Timeframe::FiveMinutes), 8640);
        assert_eq!(get_cache_size(&Timeframe::FortyFiveMinutes), 1920);
        assert_eq!(get_cache_size(&Timeframe::OneHour), 4320);
        assert_eq!(get_cache_size(&Timeframe::SixHour), 720);
        assert_eq!(get_cache_size(&Timeframe::OneDay), 1825);
    }

    #[test]
    fn test_on_demand_timeframes() {
        let on_demand = get_on_demand_timeframes();
        assert_eq!(on_demand.len(), 5);
        assert!(on_demand.contains(&Timeframe::ThreeMinutes));
        assert!(on_demand.contains(&Timeframe::FortyFiveMinutes));
        assert!(on_demand.contains(&Timeframe::SixHour));
        assert!(on_demand.contains(&Timeframe::EightHour));
        assert!(on_demand.contains(&Timeframe::TwelveHour));

        // 按需框架不应在存储列表中
        let stored = get_stored_timeframes();
        for tf in &on_demand {
            assert!(!stored.contains(tf), "{:?} should not be in stored timeframes", tf);
        }
    }
}
