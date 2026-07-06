use anyhow::Result;
use redis::{aio::ConnectionManager, RedisResult};
use serde::{Deserialize, Serialize};

use crate::config::RedisConfig;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KlineData {
    pub timestamp: i64,
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
    pub volume: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MaData {
    pub ma7: f64,
    pub ma25: f64,
    pub ma99: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MacdData {
    pub macd: f64,
    pub signal: f64,
    pub hist: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BollingerData {
    pub upper: f64,
    pub middle: f64,
    pub lower: f64,
    pub bandwidth: f64,
    pub percent_b: f64,
}

#[derive(Debug, Clone)]
pub struct MarketData {
    pub klines: Vec<KlineData>,
    pub ma: Option<MaData>,
    pub rsi: Option<f64>,
    pub macd: Option<MacdData>,
    pub bollinger: Option<BollingerData>,
    pub current_price: f64,
}

pub async fn create_connection(config: &RedisConfig) -> Result<ConnectionManager> {
    let client = redis::Client::open(config.url.as_str())?;
    let manager = ConnectionManager::new(client).await?;
    Ok(manager)
}

/// 获取 K 线数据
pub async fn get_klines(
    conn: &mut ConnectionManager,
    symbol: &str,
    limit: usize,
) -> Result<Vec<KlineData>> {
    let key = format!("kline:{}:1m", symbol);
    let data: RedisResult<String> = redis::cmd("GET")
        .arg(&key)
        .query_async(conn)
        .await;

    match data {
        Ok(json) => {
            let klines: Vec<KlineData> = serde_json::from_str(&json)?;
            // 返回最新的 limit 根 K 线
            let start = if klines.len() > limit {
                klines.len() - limit
            } else {
                0
            };
            Ok(klines[start..].to_vec())
        }
        Err(_) => Ok(vec![]),
    }
}

/// 获取 MA 数据
pub async fn get_ma(
    conn: &mut ConnectionManager,
    symbol: &str,
) -> Result<Option<MaData>> {
    let key = format!("indicator:{}:ma", symbol);
    let data: RedisResult<String> = redis::cmd("GET")
        .arg(&key)
        .query_async(conn)
        .await;

    match data {
        Ok(json) => {
            let ma: MaData = serde_json::from_str(&json)?;
            Ok(Some(ma))
        }
        Err(_) => Ok(None),
    }
}

/// 获取 RSI 数据
pub async fn get_rsi(
    conn: &mut ConnectionManager,
    symbol: &str,
) -> Result<Option<f64>> {
    let key = format!("indicator:{}:rsi", symbol);
    let data: RedisResult<f64> = redis::cmd("GET")
        .arg(&key)
        .query_async(conn)
        .await;

    match data {
        Ok(value) => Ok(Some(value)),
        Err(_) => Ok(None),
    }
}

/// 获取 MACD 数据
pub async fn get_macd(
    conn: &mut ConnectionManager,
    symbol: &str,
) -> Result<Option<MacdData>> {
    let key = format!("indicator:{}:macd", symbol);
    let data: RedisResult<String> = redis::cmd("GET")
        .arg(&key)
        .query_async(conn)
        .await;

    match data {
        Ok(json) => {
            let macd: MacdData = serde_json::from_str(&json)?;
            Ok(Some(macd))
        }
        Err(_) => Ok(None),
    }
}

/// 获取当前价格
pub async fn get_current_price(
    conn: &mut ConnectionManager,
    symbol: &str,
) -> Result<f64> {
    let key = format!("kline:{}:1m", symbol);
    let data: RedisResult<String> = redis::cmd("GET")
        .arg(&key)
        .query_async(conn)
        .await;

    match data {
        Ok(json) => {
            let klines: Vec<KlineData> = serde_json::from_str(&json)?;
            if let Some(last) = klines.last() {
                Ok(last.close)
            } else {
                Ok(0.0)
            }
        }
        Err(_) => Ok(0.0),
    }
}

/// 获取完整的市场数据
pub async fn get_market_data(
    conn: &mut ConnectionManager,
    symbol: &str,
) -> Result<MarketData> {
    let klines = get_klines(conn, symbol, 100).await?;
    let ma = get_ma(conn, symbol).await?;
    let rsi = get_rsi(conn, symbol).await?;
    let macd = get_macd(conn, symbol).await?;

    let current_price = klines.last().map(|k| k.close).unwrap_or(0.0);

    // 计算布林带（如果有足够的 K 线数据）
    let bollinger = if klines.len() >= 20 {
        Some(calculate_bollinger(&klines, 20, 2.0))
    } else {
        None
    };

    Ok(MarketData {
        klines,
        ma,
        rsi,
        macd,
        bollinger,
        current_price,
    })
}

/// 计算布林带
fn calculate_bollinger(klines: &[KlineData], period: usize, std_dev: f64) -> BollingerData {
    if klines.len() < period {
        return BollingerData {
            upper: 0.0,
            middle: 0.0,
            lower: 0.0,
            bandwidth: 0.0,
            percent_b: 0.0,
        };
    }

    let closes: Vec<f64> = klines.iter().map(|k| k.close).collect();
    let recent = &closes[closes.len() - period..];

    // 计算中轨（简单移动平均）
    let middle: f64 = recent.iter().sum::<f64>() / period as f64;

    // 计算标准差
    let variance: f64 = recent.iter()
        .map(|x| (x - middle).powi(2))
        .sum::<f64>() / period as f64;
    let std = variance.sqrt();

    let upper = middle + std_dev * std;
    let lower = middle - std_dev * std;

    // 计算带宽
    let bandwidth = if middle > 0.0 {
        (upper - lower) / middle
    } else {
        0.0
    };

    // 计算 %B
    let current_price = *closes.last().unwrap_or(&0.0);
    let percent_b = if upper - lower > 0.0 {
        (current_price - lower) / (upper - lower)
    } else {
        0.5
    };

    BollingerData {
        upper,
        middle,
        lower,
        bandwidth,
        percent_b,
    }
}
