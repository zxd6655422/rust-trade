use anyhow::Result;
use redis::aio::ConnectionManager;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use crate::config::RedisConfig;

/// 市场数据（包含 K 线和当前价格）
/// 用于策略分析
#[derive(Debug, Clone)]
pub struct MarketData {
    pub klines: Vec<KlineData>,
    pub current_price: f64,
    pub symbol: String,
    pub timeframe: Timeframe,
}

/// 多时间框架市场数据
#[derive(Debug, Clone)]
pub struct MultiTimeframeData {
    /// 主时间框架数据（最低级别）
    pub primary: MarketData,
    /// 辅助时间框架数据
    pub secondary: Option<MarketData>,
    /// 更高时间框架数据
    pub higher: Option<MarketData>,
    /// 所有时间框架数据（按级别排序）
    pub all: Vec<MarketData>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KlineData {
    pub timestamp: i64,
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
    pub volume: f64,
}

/// ZSET member 格式（与 trading-core 一致）
#[derive(Debug, Clone, Serialize, Deserialize)]
struct KlineZsetMember {
    ts: i64,
    o: f64,
    h: f64,
    l: f64,
    c: f64,
    v: f64,
}

impl From<KlineZsetMember> for KlineData {
    fn from(m: KlineZsetMember) -> Self {
        KlineData {
            timestamp: m.ts,
            open: m.o,
            high: m.h,
            low: m.l,
            close: m.c,
            volume: m.v,
        }
    }
}

/// 时间框架枚举
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Timeframe {
    OneMinute,
    ThreeMinutes,
    FiveMinutes,
    FifteenMinutes,
    ThirtyMinutes,
    FortyFiveMinutes,
    OneHour,
    TwoHour,
    FourHour,
    SixHour,
    EightHour,
    TwelveHour,
    OneDay,
    ThreeDay,
    OneWeek,
}

impl Timeframe {
    pub fn as_str(&self) -> &'static str {
        match self {
            Timeframe::OneMinute => "1m",
            Timeframe::ThreeMinutes => "3m",
            Timeframe::FiveMinutes => "5m",
            Timeframe::FifteenMinutes => "15m",
            Timeframe::ThirtyMinutes => "30m",
            Timeframe::FortyFiveMinutes => "45m",
            Timeframe::OneHour => "1h",
            Timeframe::TwoHour => "2h",
            Timeframe::FourHour => "4h",
            Timeframe::SixHour => "6h",
            Timeframe::EightHour => "8h",
            Timeframe::TwelveHour => "12h",
            Timeframe::OneDay => "1d",
            Timeframe::ThreeDay => "3d",
            Timeframe::OneWeek => "1w",
        }
    }

    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "1m" => Some(Timeframe::OneMinute),
            "3m" => Some(Timeframe::ThreeMinutes),
            "5m" => Some(Timeframe::FiveMinutes),
            "15m" => Some(Timeframe::FifteenMinutes),
            "30m" => Some(Timeframe::ThirtyMinutes),
            "45m" => Some(Timeframe::FortyFiveMinutes),
            "1h" => Some(Timeframe::OneHour),
            "2h" => Some(Timeframe::TwoHour),
            "4h" => Some(Timeframe::FourHour),
            "6h" => Some(Timeframe::SixHour),
            "8h" => Some(Timeframe::EightHour),
            "12h" => Some(Timeframe::TwelveHour),
            "1d" => Some(Timeframe::OneDay),
            "3d" => Some(Timeframe::ThreeDay),
            "1w" => Some(Timeframe::OneWeek),
            _ => None,
        }
    }

    /// 获取该时间框架需要的最小 K 线数量（用于指标预热）
    pub fn min_warmup_bars(&self) -> usize {
        match self {
            Timeframe::OneMinute => 500,
            Timeframe::ThreeMinutes => 300,
            Timeframe::FiveMinutes => 500,
            Timeframe::FifteenMinutes => 300,
            Timeframe::ThirtyMinutes => 200,
            Timeframe::FortyFiveMinutes => 150,
            Timeframe::OneHour => 200,
            Timeframe::TwoHour => 150,
            Timeframe::FourHour => 150,
            Timeframe::SixHour => 100,
            Timeframe::EightHour => 100,
            Timeframe::TwelveHour => 80,
            Timeframe::OneDay => 100,
            Timeframe::ThreeDay => 50,
            Timeframe::OneWeek => 50,
        }
    }

    /// 获取时间框架的级别（用于排序）
    pub fn level(&self) -> u8 {
        match self {
            Timeframe::OneMinute => 1,
            Timeframe::ThreeMinutes => 2,
            Timeframe::FiveMinutes => 3,
            Timeframe::FifteenMinutes => 4,
            Timeframe::ThirtyMinutes => 5,
            Timeframe::FortyFiveMinutes => 6,
            Timeframe::OneHour => 7,
            Timeframe::TwoHour => 8,
            Timeframe::FourHour => 9,
            Timeframe::SixHour => 10,
            Timeframe::EightHour => 11,
            Timeframe::TwelveHour => 12,
            Timeframe::OneDay => 13,
            Timeframe::ThreeDay => 14,
            Timeframe::OneWeek => 15,
        }
    }
}

pub async fn create_connection(config: &RedisConfig) -> Result<ConnectionManager> {
    let client = redis::Client::open(config.url.as_str())?;
    let manager = ConnectionManager::new(client).await?;
    Ok(manager)
}

/// 从 Redis ZSET 获取 K 线数据
///
/// Key 格式：kline:{symbol}:{timeframe}
/// 返回最新的 limit 根 K 线（按时间从旧到新排序）
pub async fn get_klines(
    conn: &mut ConnectionManager,
    symbol: &str,
    timeframe: &Timeframe,
    limit: usize,
) -> Result<Vec<KlineData>> {
    let key = format!("kline:{}:{}", symbol, timeframe.as_str());

    // ZREVRANGE 获取最新的 limit 条记录（score 从大到小）
    let kline_jsons: Vec<String> = redis::cmd("ZREVRANGE")
        .arg(&key)
        .arg(0)
        .arg((limit - 1) as isize)
        .query_async(conn)
        .await?;

    // 解析并反转顺序（从旧到新）
    let mut klines: Vec<KlineData> = kline_jsons
        .iter()
        .filter_map(|json| {
            let member: KlineZsetMember = serde_json::from_str(json).ok()?;
            Some(member.into())
        })
        .collect();

    klines.reverse(); // 变为从旧到新
    Ok(klines)
}

/// 获取指定时间范围内的 K 线
pub async fn get_klines_in_range(
    conn: &mut ConnectionManager,
    symbol: &str,
    timeframe: &Timeframe,
    start_ts: i64,
    end_ts: i64,
) -> Result<Vec<KlineData>> {
    let key = format!("kline:{}:{}", symbol, timeframe.as_str());

    // ZRANGEBYSCORE 获取指定范围的记录
    let kline_jsons: Vec<String> = redis::cmd("ZRANGEBYSCORE")
        .arg(&key)
        .arg(start_ts)
        .arg(end_ts)
        .query_async(conn)
        .await?;

    let klines: Vec<KlineData> = kline_jsons
        .iter()
        .filter_map(|json| {
            let member: KlineZsetMember = serde_json::from_str(json).ok()?;
            Some(member.into())
        })
        .collect();

    Ok(klines)
}

/// 获取当前价格（最新 K 线的收盘价）
pub async fn get_current_price(
    conn: &mut ConnectionManager,
    symbol: &str,
) -> Result<f64> {
    let key = format!("kline:{}:1m", symbol);

    // ZREVRANGE 获取最新的一条记录
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

/// 获取 K 线数量
pub async fn get_kline_count(
    conn: &mut ConnectionManager,
    symbol: &str,
    timeframe: &Timeframe,
) -> Result<usize> {
    let key = format!("kline:{}:{}", symbol, timeframe.as_str());
    let count: usize = redis::cmd("ZCARD").arg(&key).query_async(conn).await?;
    Ok(count)
}

/// 检查是否有足够的 K 线数据
pub async fn has_sufficient_data(
    conn: &mut ConnectionManager,
    symbol: &str,
    timeframe: &Timeframe,
    min_count: usize,
) -> Result<bool> {
    let count = get_kline_count(conn, symbol, timeframe).await?;
    Ok(count >= min_count)
}

/// 获取多个时间框架的 K 线数据
pub async fn get_multi_timeframe_klines(
    conn: &mut ConnectionManager,
    symbol: &str,
    timeframes: &[Timeframe],
    limit: usize,
) -> Result<HashMap<String, Vec<KlineData>>> {
    let mut result = HashMap::new();

    for tf in timeframes {
        let klines = get_klines(conn, symbol, tf, limit).await?;
        result.insert(tf.as_str().to_string(), klines);
    }

    Ok(result)
}

/// 获取最新的 K 线（单根）
pub async fn get_latest_kline(
    conn: &mut ConnectionManager,
    symbol: &str,
    timeframe: &Timeframe,
) -> Result<Option<KlineData>> {
    let klines = get_klines(conn, symbol, timeframe, 1).await?;
    Ok(klines.into_iter().next())
}

/// 获取所有可用的交易对
pub async fn get_available_symbols(
    conn: &mut ConnectionManager,
) -> Result<Vec<String>> {
    // 使用 SCAN 查找所有 kline:*:1m 的 key
    let mut symbols = Vec::new();
    let mut cursor: u64 = 0;

    loop {
        let (new_cursor, keys): (u64, Vec<String>) = redis::cmd("SCAN")
            .arg(cursor)
            .arg("MATCH")
            .arg("kline:*:1m")
            .arg("COUNT")
            .arg(100)
            .query_async(conn)
        .await?;

        for key in keys {
            // 从 key 中提取 symbol：kline:{symbol}:1m
            if let Some(symbol) = key.split(':').nth(1) {
                symbols.push(symbol.to_string());
            }
        }

        cursor = new_cursor;
        if cursor == 0 {
            break;
        }
    }

    symbols.sort();
    symbols.dedup();
    Ok(symbols)
}

/// 获取市场数据（用于策略分析）
///
/// # 参数
/// - `conn`: Redis 连接
/// - `symbol`: 交易对
/// - `timeframe`: 时间框架
/// - `limit`: K 线数量
///
/// # 返回
/// MarketData 包含 K 线数据和当前价格
pub async fn get_market_data(
    conn: &mut ConnectionManager,
    symbol: &str,
    timeframe: &Timeframe,
    limit: usize,
) -> Result<MarketData> {
    let klines = get_klines(conn, symbol, timeframe, limit).await?;
    let current_price = klines.last().map(|k| k.close).unwrap_or(0.0);

    Ok(MarketData {
        klines,
        current_price,
        symbol: symbol.to_string(),
        timeframe: *timeframe,
    })
}

/// 获取默认时间框架的市场数据（1m，100 根）
///
/// 保持向后兼容
pub async fn get_market_data_default(
    conn: &mut ConnectionManager,
    symbol: &str,
) -> Result<MarketData> {
    get_market_data(conn, symbol, &Timeframe::OneMinute, 100).await
}

/// 获取多时间框架市场数据
///
/// # 参数
/// - `conn`: Redis 连接
/// - `symbol`: 交易对
/// - `timeframes`: 时间框架列表（按级别从低到高排序）
/// - `limit`: 每个时间框架的 K 线数量
///
/// # 返回
/// MultiTimeframeData 包含所有时间框架的数据
pub async fn get_multi_timeframe_data(
    conn: &mut ConnectionManager,
    symbol: &str,
    timeframes: &[Timeframe],
    limit: usize,
) -> Result<MultiTimeframeData> {
    let mut all_data: Vec<MarketData> = Vec::new();

    for tf in timeframes {
        let klines = get_klines(conn, symbol, tf, limit).await?;
        let current_price = klines.last().map(|k| k.close).unwrap_or(0.0);

        all_data.push(MarketData {
            klines,
            current_price,
            symbol: symbol.to_string(),
            timeframe: *tf,
        });
    }

    // 按时间框架级别排序
    all_data.sort_by_key(|d| d.timeframe.level());

    // 拆分为主/辅/高
    let primary = all_data.first().cloned();
    let secondary = if all_data.len() > 1 {
        Some(all_data[1].clone())
    } else {
        None
    };
    let higher = if all_data.len() > 2 {
        Some(all_data[2].clone())
    } else {
        None
    };

    Ok(MultiTimeframeData {
        primary: primary.unwrap_or_else(|| MarketData {
            klines: vec![],
            current_price: 0.0,
            symbol: symbol.to_string(),
            timeframe: Timeframe::OneMinute,
        }),
        secondary,
        higher,
        all: all_data,
    })
}

/// 获取策略所需的多时间框架数据
///
/// 根据策略类型自动选择时间框架
pub async fn get_strategy_data(
    conn: &mut ConnectionManager,
    symbol: &str,
    strategy_type: &str,
    params: &serde_json::Value,
) -> Result<MultiTimeframeData> {
    // 根据策略类型确定需要的时间框架
    let timeframes = get_strategy_timeframes(strategy_type, params);

    // 获取数据
    let limit = 500; // 默认每个时间框架500根
    get_multi_timeframe_data(conn, symbol, &timeframes, limit).await
}

/// 获取策略需要的时间框架列表
fn get_strategy_timeframes(strategy_type: &str, params: &serde_json::Value) -> Vec<Timeframe> {
    match strategy_type {
        "multi_tf" => {
            // 多时间框架策略：从参数中读取
            if let Some(tfs) = params.get("timeframes").and_then(|v| v.as_array()) {
                tfs.iter()
                    .filter_map(|v| v.as_str().and_then(Timeframe::from_str))
                    .collect()
            } else {
                // 默认：1h + 4h + 1d
                vec![Timeframe::OneHour, Timeframe::FourHour, Timeframe::OneDay]
            }
        }
        "macro_cycle" => {
            // 大周期策略：使用周K和日K
            vec![Timeframe::OneDay, Timeframe::ThreeDay, Timeframe::OneWeek]
        }
        "trend" => {
            // 趋势策略：1h + 4h
            vec![Timeframe::OneHour, Timeframe::FourHour]
        }
        _ => {
            // 其他策略：默认使用单一时间框架
            vec![Timeframe::OneMinute]
        }
    }
}
