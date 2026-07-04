// data/polars_repository.rs
// Polars 高性能查询层
// 用于历史数据的快速查询和技术指标计算

use chrono::{DateTime, Duration, Utc};
use polars::prelude::*;
use std::path::PathBuf;
use tracing::info;

use super::parquet_store::{ParquetStore, ParquetStoreConfig, ParquetStats};
use super::types::{DataResult, OHLCData, Timeframe};

/// Polars 查询配置
#[derive(Debug, Clone)]
pub struct PolarsRepositoryConfig {
    /// Parquet 存储路径
    pub parquet_path: PathBuf,
    /// 热数据截止天数 (超过此天数从 Parquet 读取)
    pub hot_data_days: i64,
}

impl Default for PolarsRepositoryConfig {
    fn default() -> Self {
        Self {
            parquet_path: PathBuf::from("data/parquet"),
            hot_data_days: 7,
        }
    }
}

/// Polars 高性能数据仓库
pub struct PolarsRepository {
    store: ParquetStore,
    config: PolarsRepositoryConfig,
}

impl PolarsRepository {
    /// 创建新的 Polars 仓库
    pub fn new(config: PolarsRepositoryConfig) -> Self {
        let store = ParquetStore::with_base_path(&config.parquet_path);
        Self { store, config }
    }

    /// 获取热数据截止时间
    pub fn hot_data_cutoff(&self) -> DateTime<Utc> {
        Utc::now() - Duration::days(self.config.hot_data_days)
    }

    /// 检查是否应该使用 Polars (冷数据)
    pub fn should_use_polars(&self, start: DateTime<Utc>) -> bool {
        start < self.hot_data_cutoff()
    }

    /// 获取 K线数据 (高性能)
    pub fn get_klines(
        &self,
        symbol: &str,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> DataResult<Vec<OHLCData>> {
        let start_time = std::time::Instant::now();
        let klines = self.store.read_klines(symbol, start, end)?;
        let elapsed = start_time.elapsed();

        info!(
            "Polars read {} klines for {} in {:?}",
            klines.len(),
            symbol,
            elapsed
        );

        Ok(klines)
    }

    /// 获取 K线数据为 DataFrame (用于计算)
    pub fn get_klines_df(
        &self,
        symbol: &str,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> DataResult<DataFrame> {
        let klines = self.get_klines(symbol, start, end)?;
        self.klines_to_df(&klines)
    }

    /// 将 K线转换为 DataFrame
    fn klines_to_df(&self, klines: &[OHLCData]) -> DataResult<DataFrame> {
        let timestamps: Vec<i64> = klines.iter().map(|k| k.timestamp.timestamp_millis()).collect();
        let opens: Vec<f64> = klines
            .iter()
            .map(|k| k.open.to_string().parse().unwrap_or(0.0))
            .collect();
        let highs: Vec<f64> = klines
            .iter()
            .map(|k| k.high.to_string().parse().unwrap_or(0.0))
            .collect();
        let lows: Vec<f64> = klines
            .iter()
            .map(|k| k.low.to_string().parse().unwrap_or(0.0))
            .collect();
        let closes: Vec<f64> = klines
            .iter()
            .map(|k| k.close.to_string().parse().unwrap_or(0.0))
            .collect();
        let volumes: Vec<f64> = klines
            .iter()
            .map(|k| k.volume.to_string().parse().unwrap_or(0.0))
            .collect();

        let df = df!(
            "timestamp" => timestamps,
            "open" => opens,
            "high" => highs,
            "low" => lows,
            "close" => closes,
            "volume" => volumes
        )
        .map_err(|e| super::types::DataError::InvalidFormat(e.to_string()))?;

        Ok(df)
    }

    /// 计算 SMA (简单移动平均) - 手动实现
    pub fn calculate_sma(&self, df: &DataFrame, period: usize) -> DataResult<Vec<f64>> {
        let close = df
            .column("close")
            .map_err(|e| super::types::DataError::InvalidFormat(e.to_string()))?
            .f64()
            .map_err(|e| super::types::DataError::InvalidFormat(e.to_string()))?;

        let values: Vec<f64> = close.into_no_null_iter().collect();
        let len = values.len();

        if len < period || period == 0 {
            return Ok(vec![f64::NAN; len]);
        }

        let mut sma = vec![f64::NAN; len];

        for i in (period - 1)..len {
            let start = i + 1 - period;
            let sum: f64 = values[start..=i].iter().sum();
            sma[i] = sum / period as f64;
        }

        Ok(sma)
    }

    /// 计算 EMA (指数移动平均) - 手动实现
    pub fn calculate_ema(&self, df: &DataFrame, period: usize) -> DataResult<Vec<f64>> {
        let close = df
            .column("close")
            .map_err(|e| super::types::DataError::InvalidFormat(e.to_string()))?
            .f64()
            .map_err(|e| super::types::DataError::InvalidFormat(e.to_string()))?;

        let values: Vec<f64> = close.into_no_null_iter().collect();
        let len = values.len();
        let mut ema = vec![0.0; len];

        let alpha = 2.0 / (period as f64 + 1.0);

        for i in 0..len {
            if i == 0 {
                ema[i] = values[i];
            } else {
                ema[i] = values[i] * alpha + ema[i - 1] * (1.0 - alpha);
            }
        }

        Ok(ema)
    }

    /// 计算 RSI (相对强弱指数) - 手动实现
    pub fn calculate_rsi(&self, df: &DataFrame, period: usize) -> DataResult<Vec<f64>> {
        let close = df
            .column("close")
            .map_err(|e| super::types::DataError::InvalidFormat(e.to_string()))?
            .f64()
            .map_err(|e| super::types::DataError::InvalidFormat(e.to_string()))?;

        let values: Vec<f64> = close.into_no_null_iter().collect();
        let len = values.len();

        if len < 2 {
            return Ok(vec![50.0; len]);
        }

        let mut gains = Vec::with_capacity(len - 1);
        let mut losses = Vec::with_capacity(len - 1);

        for i in 1..len {
            let change = values[i] - values[i - 1];
            if change > 0.0 {
                gains.push(change);
                losses.push(0.0);
            } else {
                gains.push(0.0);
                losses.push(-change);
            }
        }

        let mut rsi = vec![50.0; len];

        if gains.len() < period {
            return Ok(rsi);
        }

        let mut avg_gain: f64 = gains[..period].iter().sum::<f64>() / period as f64;
        let mut avg_loss: f64 = losses[..period].iter().sum::<f64>() / period as f64;

        for i in period..gains.len() {
            avg_gain = (avg_gain * (period - 1) as f64 + gains[i]) / period as f64;
            avg_loss = (avg_loss * (period - 1) as f64 + losses[i]) / period as f64;

            let rs = if avg_loss == 0.0 {
                100.0
            } else {
                avg_gain / avg_loss
            };
            rsi[i + 1] = 100.0 - (100.0 / (1.0 + rs));
        }

        Ok(rsi)
    }

    /// 计算 MACD - 手动实现
    pub fn calculate_macd(
        &self,
        df: &DataFrame,
        fast: usize,
        slow: usize,
        signal: usize,
    ) -> DataResult<(Vec<f64>, Vec<f64>, Vec<f64>)> {
        let ema_fast = self.calculate_ema(df, fast)?;
        let ema_slow = self.calculate_ema(df, slow)?;

        let len = ema_fast.len();

        // MACD Line = EMA_fast - EMA_slow
        let macd_line: Vec<f64> = ema_fast
            .iter()
            .zip(ema_slow.iter())
            .map(|(f, s)| f - s)
            .collect();

        // Signal Line = EMA of MACD
        let mut signal_line = vec![0.0; len];
        let alpha = 2.0 / (signal as f64 + 1.0);
        for i in 0..len {
            if i == 0 {
                signal_line[i] = macd_line[i];
            } else {
                signal_line[i] = macd_line[i] * alpha + signal_line[i - 1] * (1.0 - alpha);
            }
        }

        // Histogram = MACD - Signal
        let histogram: Vec<f64> = macd_line
            .iter()
            .zip(signal_line.iter())
            .map(|(m, s)| m - s)
            .collect();

        Ok((macd_line, signal_line, histogram))
    }

    /// 计算布林带 - 手动实现
    pub fn calculate_bollinger_bands(
        &self,
        df: &DataFrame,
        period: usize,
        std_dev: f64,
    ) -> DataResult<(Vec<f64>, Vec<f64>, Vec<f64>)> {
        let close = df
            .column("close")
            .map_err(|e| super::types::DataError::InvalidFormat(e.to_string()))?
            .f64()
            .map_err(|e| super::types::DataError::InvalidFormat(e.to_string()))?;

        let values: Vec<f64> = close.into_no_null_iter().collect();
        let len = values.len();

        let mut upper = vec![f64::NAN; len];
        let mut middle = vec![f64::NAN; len];
        let mut lower = vec![f64::NAN; len];

        for i in (period - 1)..len {
            let window = &values[(i - period + 1)..=i];
            let mean = window.iter().sum::<f64>() / period as f64;
            let variance =
                window.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / period as f64;
            let std = variance.sqrt();

            middle[i] = mean;
            upper[i] = mean + std_dev * std;
            lower[i] = mean - std_dev * std;
        }

        Ok((upper, middle, lower))
    }

    /// 获取数据统计
    pub fn get_stats(&self, symbol: &str) -> DataResult<ParquetStats> {
        self.store.get_stats(symbol)
    }

    /// 列出所有可用的 symbol
    pub fn list_symbols(&self) -> DataResult<Vec<String>> {
        self.store.list_symbols()
    }

    /// 导出 PostgreSQL 数据到 Parquet
    pub fn export_klines(&self, symbol: &str, klines: &[OHLCData]) -> DataResult<usize> {
        self.store.export_klines(symbol, klines)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal::Decimal;
    use std::str::FromStr;

    fn create_test_klines(count: usize) -> Vec<OHLCData> {
        let mut klines = Vec::new();
        let base_time = Utc::now() - Duration::days(30);

        for i in 0..count {
            let timestamp = base_time + Duration::minutes(i as i64);
            let price = 50000.0 + (i as f64 * 10.0);

            klines.push(OHLCData {
                timestamp,
                symbol: "BTCUSDT".to_string(),
                timeframe: Timeframe::OneMinute,
                open: Decimal::from_str(&price.to_string()).unwrap(),
                high: Decimal::from_str(&(price + 100.0).to_string()).unwrap(),
                low: Decimal::from_str(&(price - 100.0).to_string()).unwrap(),
                close: Decimal::from_str(&(price + 50.0).to_string()).unwrap(),
                volume: Decimal::from(1000),
                trade_count: 100,
            });
        }

        klines
    }

    #[test]
    fn test_sma_calculation() {
        let klines = create_test_klines(100);
        let config = PolarsRepositoryConfig::default();
        let repo = PolarsRepository::new(config);

        let df = repo.klines_to_df(&klines).unwrap();
        let sma = repo.calculate_sma(&df, 20).unwrap();

        assert_eq!(sma.len(), 100);
        assert!(!sma[19].is_nan()); // SMA(20) 应该从第 19 个开始有值
    }

    #[test]
    fn test_rsi_calculation() {
        let klines = create_test_klines(100);
        let config = PolarsRepositoryConfig::default();
        let repo = PolarsRepository::new(config);

        let df = repo.klines_to_df(&klines).unwrap();
        let rsi = repo.calculate_rsi(&df, 14).unwrap();

        assert_eq!(rsi.len(), 100);
    }

    #[test]
    fn test_macd_calculation() {
        let klines = create_test_klines(100);
        let config = PolarsRepositoryConfig::default();
        let repo = PolarsRepository::new(config);

        let df = repo.klines_to_df(&klines).unwrap();
        let (macd, signal, histogram) = repo.calculate_macd(&df, 12, 26, 9).unwrap();

        assert_eq!(macd.len(), 100);
        assert_eq!(signal.len(), 100);
        assert_eq!(histogram.len(), 100);
    }
}
