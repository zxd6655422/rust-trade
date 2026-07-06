use redis::Commands;
use rust_decimal::Decimal;
use serde::Serialize;
use tracing::debug;

use trading_common::data::types::OHLCData;

// =================================================================
// Data structures — must match strategy-service/src/redis_reader.rs
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

#[derive(Debug, Clone, Serialize)]
struct MaData {
    ma7: f64,
    ma25: f64,
    ma99: f64,
}

#[derive(Debug, Clone, Serialize)]
struct MacdData {
    macd: f64,
    signal: f64,
    hist: f64,
}

// =================================================================
// Indicator calculations
// =================================================================

/// Simple Moving Average over the last `period` values.
/// Returns 0.0 if not enough data.
fn calculate_sma(closes: &[f64], period: usize) -> f64 {
    if closes.len() < period || period == 0 {
        return 0.0;
    }
    let start = closes.len() - period;
    closes[start..].iter().sum::<f64>() / period as f64
}

/// RSI using Wilder's smoothing method.
/// Period = 14 by default. Returns 50.0 if not enough data.
fn calculate_rsi(closes: &[f64], period: usize) -> f64 {
    if closes.len() < period + 1 || period == 0 {
        return 50.0;
    }

    // Calculate price changes
    let changes: Vec<f64> = closes
        .windows(2)
        .map(|w| w[1] - w[0])
        .collect();

    if changes.len() < period {
        return 50.0;
    }

    // Initial average gain/loss over first `period` changes
    let mut avg_gain: f64 = changes[..period].iter().filter(|&&c| c > 0.0).sum::<f64>() / period as f64;
    let mut avg_loss: f64 = changes[..period].iter().filter(|&&c| c < 0.0).map(|c| c.abs()).sum::<f64>() / period as f64;

    // Wilder's smoothing for remaining changes
    for &change in &changes[period..] {
        let gain = if change > 0.0 { change } else { 0.0 };
        let loss = if change < 0.0 { change.abs() } else { 0.0 };
        avg_gain = (avg_gain * (period as f64 - 1.0) + gain) / period as f64;
        avg_loss = (avg_loss * (period as f64 - 1.0) + loss) / period as f64;
    }

    if avg_loss == 0.0 {
        return 100.0;
    }

    let rs = avg_gain / avg_loss;
    100.0 - (100.0 / (1.0 + rs))
}

/// EMA helper for MACD calculation.
fn ema(values: &[f64], period: usize) -> Vec<f64> {
    if values.is_empty() || period == 0 {
        return vec![];
    }

    let alpha = 2.0 / (period as f64 + 1.0);
    let mut result = Vec::with_capacity(values.len());

    // First value: SMA of first `period` values
    if values.len() >= period {
        let first: f64 = values[..period].iter().sum::<f64>() / period as f64;
        result.push(first);
        // Subsequent values: EMA
        for &val in &values[period..] {
            let prev = *result.last().unwrap();
            result.push(alpha * val + (1.0 - alpha) * prev);
        }
    }

    result
}

/// MACD(12, 26, 9) — returns (macd_line, signal_line, histogram).
/// Uses the last value of each series.
fn calculate_macd(closes: &[f64]) -> MacdData {
    if closes.len() < 26 {
        return MacdData {
            macd: 0.0,
            signal: 0.0,
            hist: 0.0,
        };
    }

    let ema12 = ema(closes, 12);
    let ema26 = ema(closes, 26);

    // MACD line = EMA12 - EMA26 (aligned from the shorter series start)
    let offset = ema12.len().saturating_sub(ema26.len());
    let macd_line: Vec<f64> = ema26
        .iter()
        .enumerate()
        .map(|(i, &v)| ema12[offset + i] - v)
        .collect();

    if macd_line.is_empty() {
        return MacdData {
            macd: 0.0,
            signal: 0.0,
            hist: 0.0,
        };
    }

    // Signal line = EMA(9) of MACD line
    let signal_line = ema(&macd_line, 9);

    let macd_val = *macd_line.last().unwrap();
    let signal_val = signal_line.last().copied().unwrap_or(0.0);
    let hist_val = macd_val - signal_val;

    MacdData {
        macd: macd_val,
        signal: signal_val,
        hist: hist_val,
    }
}

// =================================================================
// Redis write
// =================================================================

/// Convert Decimal to f64 (lossy but acceptable for caching).
fn decimal_to_f64(d: Decimal) -> f64 {
    d.to_string().parse::<f64>().unwrap_or(0.0)
}

/// Write kline data and computed indicators to Redis.
///
/// Keys written (matching strategy-service expectations):
/// - `kline:{symbol}:1m` — latest 100 klines as JSON, TTL 300s
/// - `indicator:{symbol}:ma` — MA7/MA25/MA99, TTL 300s
/// - `indicator:{symbol}:rsi` — RSI(14), TTL 300s
/// - `indicator:{symbol}:macd` — MACD(12,26,9), TTL 300s
pub fn write_market_data(redis_url: &str, symbol: &str, ohlc_list: &[OHLCData]) -> anyhow::Result<()> {
    if ohlc_list.is_empty() {
        return Ok(());
    }

    let client = redis::Client::open(redis_url)?;
    let mut conn = client.get_connection()?;

    let ttl: usize = 300; // 5 minutes

    // 1. Write kline:{symbol}:1m — latest 100 klines
    let kline_data: Vec<KlineData> = ohlc_list
        .iter()
        .rev()
        .take(100)
        .map(|k| KlineData {
            timestamp: k.timestamp.timestamp_millis(),
            open: decimal_to_f64(k.open),
            high: decimal_to_f64(k.high),
            low: decimal_to_f64(k.low),
            close: decimal_to_f64(k.close),
            volume: decimal_to_f64(k.volume),
        })
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect();

    let kline_json = serde_json::to_string(&kline_data)?;
    let kline_key = format!("kline:{}:1m", symbol);
    conn.set_ex::<_, _, ()>(&kline_key, &kline_json, ttl)?;

    // 2. Extract close prices for indicator calculation
    let closes: Vec<f64> = ohlc_list.iter().map(|k| decimal_to_f64(k.close)).collect();

    // 3. Write indicator:{symbol}:ma
    let ma = MaData {
        ma7: calculate_sma(&closes, 7),
        ma25: calculate_sma(&closes, 25),
        ma99: calculate_sma(&closes, 99),
    };
    let ma_json = serde_json::to_string(&ma)?;
    let ma_key = format!("indicator:{}:ma", symbol);
    conn.set_ex::<_, _, ()>(&ma_key, &ma_json, ttl)?;

    // 4. Write indicator:{symbol}:rsi
    let rsi = calculate_rsi(&closes, 14);
    let rsi_key = format!("indicator:{}:rsi", symbol);
    conn.set_ex::<_, _, ()>(&rsi_key, rsi, ttl)?;

    // 5. Write indicator:{symbol}:macd
    let macd = calculate_macd(&closes);
    let macd_json = serde_json::to_string(&macd)?;
    let macd_key = format!("indicator:{}:macd", symbol);
    conn.set_ex::<_, _, ()>(&macd_key, &macd_json, ttl)?;

    debug!(
        "[{}] Redis 写入完成: kline({} 条), MA({:.2}/{:.2}/{:.2}), RSI({:.2}), MACD({:.4}/{:.4}/{:.4})",
        symbol,
        kline_data.len(),
        ma.ma7, ma.ma25, ma.ma99,
        rsi,
        macd.macd, macd.signal, macd.hist,
    );

    Ok(())
}
