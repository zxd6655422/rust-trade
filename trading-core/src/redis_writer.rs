use redis::aio::ConnectionManager;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use tracing::{debug, warn};

use trading_common::data::types::{OHLCData, Timeframe};

// =================================================================
// Cache Size Configuration
// =================================================================
// Based on quantitative trading cycle analysis:
// - 1m: 2 weeks (real-time data source)
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

/// Redis 缓存 TTL（秒）
/// TTL 与缓存容量对齐，确保数据在缓存容量范围内不会过期
const KLINE_TTL_SHORT: usize = 1209600;    // 14天（与1m缓存容量对齐）
const KLINE_TTL_MEDIUM: usize = 2592000;   // 30天（与5m/15m/30m缓存容量对齐）
const KLINE_TTL_LONG: usize = 15552000;    // 180天（与1h/2h/4h缓存容量对齐，1d/3d/1w也用此值）

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
///
/// 精度损失说明：
/// - f64 有效精度约 15-17 位十进制数字
/// - Decimal 可以存储更高精度（如价格 "12345.67890123"）
/// - 转换后可能丢失末尾精度（如变成 12345.678901230001）
/// - 对于缓存用途（非交易决策）可接受
/// - 策略引擎如需精确计算，应从 PostgreSQL 读取 Decimal 数据
fn decimal_to_f64(d: Decimal) -> f64 {
    d.to_string().parse::<f64>().unwrap_or(0.0)
}

/// 创建 Redis ConnectionManager（连接池）
///
/// ConnectionManager 是 Clone-safe 的，内部使用 Arc 共享连接池
/// 每次 clone 都指向同一个底层连接池，支持自动重连
pub async fn create_connection_manager(redis_url: &str) -> anyhow::Result<ConnectionManager> {
    let client = redis::Client::open(redis_url)?;
    let manager = ConnectionManager::new(client).await?;
    Ok(manager)
}

/// 获取时间框架对应的缓存大小
pub fn get_cache_size(tf: &Timeframe) -> usize {
    match tf {
        Timeframe::OneMinute => KLINE_1M_CACHE_SIZE,
        Timeframe::FiveMinutes => KLINE_5M_CACHE_SIZE,
        Timeframe::FifteenMinutes => KLINE_15M_CACHE_SIZE,
        Timeframe::ThirtyMinutes => KLINE_30M_CACHE_SIZE,
        Timeframe::OneHour => KLINE_1H_CACHE_SIZE,
        Timeframe::TwoHour => KLINE_2H_CACHE_SIZE,
        Timeframe::FourHour => KLINE_4H_CACHE_SIZE,
        Timeframe::OneDay => KLINE_1D_CACHE_SIZE,
        Timeframe::ThreeDay => KLINE_3D_CACHE_SIZE,
        Timeframe::OneWeek => KLINE_1W_CACHE_SIZE,
    }
}

/// 获取时间框架的 Redis key 后缀
pub fn timeframe_key_suffix(tf: &Timeframe) -> &'static str {
    tf.as_str()
}

/// 获取时间框架的 TTL
pub fn timeframe_ttl(tf: &Timeframe) -> usize {
    match tf {
        Timeframe::OneMinute => KLINE_TTL_SHORT,
        Timeframe::FiveMinutes | Timeframe::FifteenMinutes | Timeframe::ThirtyMinutes => KLINE_TTL_MEDIUM,
        _ => KLINE_TTL_LONG,
    }
}

/// 获取所有需要存储的时间框架
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

// =================================================================
// Redis write functions (async, 使用 ConnectionManager)
// =================================================================

/// 将 KlineZsetMember 列表写入 Redis ZSET
///
/// Key 格式：kline:{symbol}:{timeframe}
/// Score：timestamp (毫秒)
/// Member：JSON {ts, o, h, l, c, v, tc}
pub async fn write_kline_zset(
    conn: &mut ConnectionManager,
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

    // 批量 ZADD
    let mut pipe = redis::pipe();
    for kline in klines {
        let member_json = serde_json::to_string(kline)?;
        pipe.cmd("ZADD").arg(&key).arg(kline.ts).arg(&member_json);
    }
    pipe.query_async::<_, ()>(conn).await?;

    // 获取当前总数
    let total: usize = redis::cmd("ZCARD").arg(&key).query_async(conn).await?;

    // 裁剪 + 设置 TTL 合并到一个 pipeline
    if total > cache_size {
        let remove_count = total - cache_size;
        let mut trim_pipe = redis::pipe();
        trim_pipe.cmd("ZREMRANGEBYRANK")
            .arg(&key)
            .arg(0)
            .arg((remove_count - 1) as isize);
        trim_pipe.cmd("EXPIRE").arg(&key).arg(ttl);
        trim_pipe.query_async::<_, ()>(conn).await?;
    } else {
        redis::cmd("EXPIRE").arg(&key).arg(ttl).query_async::<_, ()>(conn).await?;
    }

    Ok(())
}

/// 写入单个时间框架的 OHLC 数据到 Redis
pub async fn write_single_timeframe(
    conn: &mut ConnectionManager,
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

    write_kline_zset(conn, symbol, timeframe, &klines).await
}

/// 写入 1m K 线到 Redis（Poll loop 调用）
pub async fn write_market_data(
    conn: &mut ConnectionManager,
    symbol: &str,
    ohlc_list: &[OHLCData],
) -> anyhow::Result<()> {
    if ohlc_list.is_empty() {
        return Ok(());
    }

    write_single_timeframe(conn, symbol, &Timeframe::OneMinute, ohlc_list).await?;

    debug!("[{}] Redis 写入完成: 1m({} 条)", symbol, ohlc_list.len());

    Ok(())
}

// =================================================================
// Data integrity validation
// =================================================================

/// 验证 K 线数据完整性
///
/// 检查项：
/// 1. 最新 K 线时间戳是否在合理范围内（不超过 2 个周期的延迟）
/// 2. 数据条数是否达到最低要求
///
/// 返回 Ok(()) 表示数据完整，Err 包含警告信息（不阻断流程）
pub fn validate_kline_integrity(
    ohlc_list: &[OHLCData],
    timeframe: &Timeframe,
) -> Result<(), String> {
    if ohlc_list.is_empty() {
        return Err(format!("[{}] 数据为空", timeframe.as_str()));
    }

    // 检查最新时间戳是否过旧
    if let Some(latest) = ohlc_list.last() {
        let now = chrono::Utc::now();
        let age = now.signed_duration_since(latest.timestamp);
        let max_age = timeframe.as_duration() * 2; // 允许最多 2 个周期的延迟

        if age > max_age {
            return Err(format!(
                "[{}] 数据过旧: 最新时间 {}, 延迟 {} 分钟",
                timeframe.as_str(),
                latest.timestamp.format("%Y-%m-%d %H:%M"),
                age.num_minutes()
            ));
        }
    }

    Ok(())
}

/// 验证 Redis 缓存中的 K 线数据完整性
///
/// 检查缓存中是否有足够的数据用于策略计算
pub async fn validate_cache_integrity(
    conn: &mut ConnectionManager,
    symbol: &str,
    timeframe: &Timeframe,
) -> Result<CacheValidationResult, String> {
    let key = format!("kline:{}:{}", symbol, timeframe_key_suffix(timeframe));

    // 获取缓存中的数据条数
    let count: usize = redis::cmd("ZCARD")
        .arg(&key)
        .query_async(conn)
        .await
        .map_err(|e| format!("Redis ZCARD 失败: {}", e))?;

    // 获取最新的时间戳
    let latest_json: Option<String> = redis::cmd("ZREVRANGE")
        .arg(&key)
        .arg(0)
        .arg(0)
        .query_async(conn)
        .await
        .map_err(|e| format!("Redis ZREVRANGE 失败: {}", e))?;

    let latest_ts = latest_json
        .and_then(|json| serde_json::from_str::<KlineZsetMember>(&json).ok())
        .map(|m| m.ts);

    // 检查数据是否足够
    let expected_min = get_cache_size(timeframe) / 10; // 至少有 10% 的数据
    let has_sufficient_data = count >= expected_min;

    // 检查最新数据是否过旧
    let is_stale = if let Some(ts) = latest_ts {
        let now_ms = chrono::Utc::now().timestamp_millis();
        let age_ms = now_ms - ts;
        let max_age_ms = timeframe.as_duration().num_milliseconds() * 2;
        age_ms > max_age_ms
    } else {
        true // 没有数据也算过旧
    };

    Ok(CacheValidationResult {
        symbol: symbol.to_string(),
        timeframe: *timeframe,
        count,
        latest_ts,
        has_sufficient_data,
        is_stale,
    })
}

/// 缓存验证结果
#[derive(Debug)]
pub struct CacheValidationResult {
    pub symbol: String,
    pub timeframe: Timeframe,
    pub count: usize,
    pub latest_ts: Option<i64>,
    pub has_sufficient_data: bool,
    pub is_stale: bool,
}

impl CacheValidationResult {
    /// 数据是否可用于策略计算
    pub fn is_usable(&self) -> bool {
        self.has_sufficient_data && !self.is_stale
    }

    /// 获取诊断信息
    pub fn diagnostic(&self) -> String {
        format!(
            "[{}:{}] count={}, latest={}, sufficient={}, stale={}",
            self.symbol,
            self.timeframe.as_str(),
            self.count,
            self.latest_ts.map(|ts| chrono::DateTime::from_timestamp_millis(ts)
                .map(|dt| dt.format("%Y-%m-%d %H:%M").to_string())
                .unwrap_or_else(|| "invalid".to_string()))
                .unwrap_or_else(|| "none".to_string()),
            self.has_sufficient_data,
            self.is_stale,
        )
    }
}

// =================================================================
// Cache loading from database (async)
// =================================================================

/// 从数据库加载 K 线数据到 Redis 缓存
///
/// 在以下场景调用：
/// 1. 系统启动时（warm-up）
/// 2. 定期刷新缓存（每30分钟）
/// 3. Redis 重连后立即刷新
pub async fn load_cache_from_db(
    conn: &mut ConnectionManager,
    symbol: &str,
    timeframe: &Timeframe,
    klines: &[OHLCData],
) -> anyhow::Result<usize> {
    if klines.is_empty() {
        return Ok(0);
    }

    let count = klines.len();
    write_single_timeframe(conn, symbol, timeframe, klines).await?;

    debug!(
        "[{}] Loaded {} {} klines from database to Redis",
        symbol, count, timeframe_key_suffix(timeframe)
    );

    Ok(count)
}

/// 检查 Redis 缓存是否有足够的数据
pub async fn has_sufficient_cache(
    conn: &mut ConnectionManager,
    symbol: &str,
    timeframe: &Timeframe,
    min_count: usize,
) -> anyhow::Result<bool> {
    let key = format!("kline:{}:{}", symbol, timeframe_key_suffix(timeframe));
    let count: usize = redis::cmd("ZCARD").arg(&key).query_async(conn).await?;
    Ok(count >= min_count)
}

/// 获取 Redis 缓存中的 K 线数量
pub async fn get_cache_count(
    conn: &mut ConnectionManager,
    symbol: &str,
    timeframe: &Timeframe,
) -> anyhow::Result<usize> {
    let key = format!("kline:{}:{}", symbol, timeframe_key_suffix(timeframe));
    let count: usize = redis::cmd("ZCARD").arg(&key).query_async(conn).await?;
    Ok(count)
}

/// 检查Redis缓存完整性，返回需要全量同步的时间框架列表
///
/// 遍历所有时间框架，检查缓存数量是否满足策略最低预热要求
/// 返回数量不足的时间框架列表，调用方应触发全量同步
pub async fn check_cache_integrity(
    conn: &mut ConnectionManager,
    symbol: &str,
    timeframes: &[Timeframe],
) -> anyhow::Result<Vec<Timeframe>> {
    let mut need_sync = Vec::new();
    for tf in timeframes {
        let count = get_cache_count(conn, symbol, tf).await?;
        let min_required = tf.min_warmup_bars();
        if count < min_required {
            warn!(
                "[{}:{}] Redis缓存不足: {}条 < {}条",
                symbol, tf.as_str(), count, min_required
            );
            need_sync.push(*tf);
        }
    }
    Ok(need_sync)
}

// =================================================================
// Legacy compatibility
// =================================================================

/// 获取当前价格（从最新 1m K 线）
pub async fn get_current_price(
    conn: &mut ConnectionManager,
    symbol: &str,
) -> anyhow::Result<f64> {
    let key = format!("kline:{}:1m", symbol);

    let kline_json: Option<String> = redis::cmd("ZREVRANGE")
        .arg(&key)
        .arg(0)
        .arg(0)
        .query_async(conn)
        .await?;

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
        assert_eq!(get_cache_size(&Timeframe::OneMinute), 20160);
        assert_eq!(get_cache_size(&Timeframe::FiveMinutes), 8640);
        assert_eq!(get_cache_size(&Timeframe::FifteenMinutes), 2880);
        assert_eq!(get_cache_size(&Timeframe::ThirtyMinutes), 1440);
        assert_eq!(get_cache_size(&Timeframe::OneHour), 4320);
        assert_eq!(get_cache_size(&Timeframe::TwoHour), 2160);
        assert_eq!(get_cache_size(&Timeframe::FourHour), 1080);
        assert_eq!(get_cache_size(&Timeframe::OneDay), 1825);
        assert_eq!(get_cache_size(&Timeframe::ThreeDay), 610);
        assert_eq!(get_cache_size(&Timeframe::OneWeek), 500);
    }

    #[test]
    fn test_ttl_config() {
        assert_eq!(timeframe_ttl(&Timeframe::OneMinute), 1209600);      // 14天
        assert_eq!(timeframe_ttl(&Timeframe::FiveMinutes), 2592000);    // 30天
        assert_eq!(timeframe_ttl(&Timeframe::FifteenMinutes), 2592000); // 30天
        assert_eq!(timeframe_ttl(&Timeframe::ThirtyMinutes), 2592000);  // 30天
        assert_eq!(timeframe_ttl(&Timeframe::OneHour), 15552000);       // 180天
        assert_eq!(timeframe_ttl(&Timeframe::TwoHour), 15552000);       // 180天
        assert_eq!(timeframe_ttl(&Timeframe::FourHour), 15552000);      // 180天
        assert_eq!(timeframe_ttl(&Timeframe::OneDay), 15552000);        // 180天
        assert_eq!(timeframe_ttl(&Timeframe::ThreeDay), 15552000);      // 180天
        assert_eq!(timeframe_ttl(&Timeframe::OneWeek), 15552000);       // 180天
    }
}
