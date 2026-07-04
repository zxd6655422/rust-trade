// data/parquet_store.rs
// Parquet 文件存储管理
// 用于历史 K线数据的高效存储和读取

use chrono::{DateTime, Datelike, TimeZone, Utc};
use polars::prelude::*;
use rust_decimal::Decimal;
use std::fs;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use tracing::{info, warn};

use super::types::{DataError, DataResult, OHLCData, Timeframe};

/// Parquet 存储配置
#[derive(Debug, Clone)]
pub struct ParquetStoreConfig {
    /// 基础路径
    pub base_path: PathBuf,
    /// 每个文件的最大行数
    pub max_rows_per_file: usize,
}

impl Default for ParquetStoreConfig {
    fn default() -> Self {
        Self {
            base_path: PathBuf::from("data/parquet"),
            max_rows_per_file: 500_000,
        }
    }
}

/// Parquet 存储管理器
pub struct ParquetStore {
    config: ParquetStoreConfig,
}

impl ParquetStore {
    /// 创建新的 Parquet 存储
    pub fn new(config: ParquetStoreConfig) -> Self {
        Self { config }
    }

    /// 使用默认配置创建
    pub fn with_base_path(base_path: impl Into<PathBuf>) -> Self {
        Self {
            config: ParquetStoreConfig {
                base_path: base_path.into(),
                ..Default::default()
            },
        }
    }

    /// 获取 symbol 的存储目录
    fn symbol_dir(&self, symbol: &str) -> PathBuf {
        self.config.base_path.join(symbol)
    }

    /// 获取 Parquet 文件路径 (按月分区)
    fn file_path(&self, symbol: &str, year: i32, month: u32) -> PathBuf {
        self.symbol_dir(symbol).join(format!("{:04}-{:02}.parquet", year, month))
    }

    /// 确保目录存在
    fn ensure_dir(&self, path: &Path) -> DataResult<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| {
                DataError::InvalidFormat(format!("Failed to create directory: {}", e))
            })?;
        }
        Ok(())
    }

    /// 导出 K线数据到 Parquet
    pub fn export_klines(&self, symbol: &str, klines: &[OHLCData]) -> DataResult<usize> {
        if klines.is_empty() {
            return Ok(0);
        }

        // 按月分组
        let mut groups: std::collections::HashMap<(i32, u32), Vec<&OHLCData>> =
            std::collections::HashMap::new();

        for kline in klines {
            let key = (kline.timestamp.year(), kline.timestamp.month());
            groups.entry(key).or_default().push(kline);
        }

        let mut total_exported = 0;

        for ((year, month), group) in groups {
            let path = self.file_path(symbol, year, month);
            self.ensure_dir(&path)?;

            // 构建 DataFrame
            let df = self.klines_to_dataframe(&group)?;

            // 追加或创建文件
            if path.exists() {
                // 读取现有数据，合并后写入
                let existing = self.read_parquet(&path)?;
                let combined = concat_df(&[existing, df], true)?;
                self.write_parquet(&combined, &path)?;
            } else {
                self.write_parquet(&df, &path)?;
            }

            total_exported += group.len();
            info!("Exported {} klines to {:?}", group.len(), path);
        }

        Ok(total_exported)
    }

    /// 读取 Parquet 文件
    fn read_parquet(&self, path: &Path) -> DataResult<DataFrame> {
        let file = std::fs::File::open(path).map_err(|e| {
            DataError::InvalidFormat(format!("Failed to open parquet file: {}", e))
        })?;

        ParquetReader::new(file).finish().map_err(|e| {
            DataError::InvalidFormat(format!("Failed to read parquet: {}", e))
        })
    }

    /// 写入 Parquet 文件
    fn write_parquet(&self, df: &DataFrame, path: &Path) -> DataResult<()> {
        let file = std::fs::File::create(path).map_err(|e| {
            DataError::InvalidFormat(format!("Failed to create parquet file: {}", e))
        })?;

        ParquetWriter::new(file)
            .finish(&mut df.clone())
            .map_err(|e| DataError::InvalidFormat(format!("Failed to write parquet: {}", e)))?;

        Ok(())
    }

    /// 将 K线数据转换为 DataFrame
    fn klines_to_dataframe(&self, klines: &[&OHLCData]) -> DataResult<DataFrame> {
        let timestamps: Vec<i64> = klines
            .iter()
            .map(|k| k.timestamp.timestamp_millis())
            .collect();

        let symbols: Vec<&str> = klines.iter().map(|k| k.symbol.as_str()).collect();

        let opens: Vec<f64> = klines
            .iter()
            .map(|k| k.open.to_string().parse::<f64>().unwrap_or(0.0))
            .collect();

        let highs: Vec<f64> = klines
            .iter()
            .map(|k| k.high.to_string().parse::<f64>().unwrap_or(0.0))
            .collect();

        let lows: Vec<f64> = klines
            .iter()
            .map(|k| k.low.to_string().parse::<f64>().unwrap_or(0.0))
            .collect();

        let closes: Vec<f64> = klines
            .iter()
            .map(|k| k.close.to_string().parse::<f64>().unwrap_or(0.0))
            .collect();

        let volumes: Vec<f64> = klines
            .iter()
            .map(|k| k.volume.to_string().parse::<f64>().unwrap_or(0.0))
            .collect();

        let trade_counts: Vec<u64> = klines.iter().map(|k| k.trade_count as u64).collect();

        let df = df!(
            "timestamp" => timestamps,
            "symbol" => symbols,
            "open" => opens,
            "high" => highs,
            "low" => lows,
            "close" => closes,
            "volume" => volumes,
            "trade_count" => trade_counts
        )
        .map_err(|e| DataError::InvalidFormat(format!("Failed to create DataFrame: {}", e)))?;

        Ok(df)
    }

    /// 从 DataFrame 转换为 K线数据
    fn dataframe_to_klines(&self, df: &DataFrame) -> DataResult<Vec<OHLCData>> {
        let timestamps = df
            .column("timestamp")
            .map_err(|e| DataError::InvalidFormat(e.to_string()))?
            .i64()
            .map_err(|e| DataError::InvalidFormat(e.to_string()))?;

        let symbols = df
            .column("symbol")
            .map_err(|e| DataError::InvalidFormat(e.to_string()))?
            .utf8()
            .map_err(|e| DataError::InvalidFormat(e.to_string()))?;

        let opens = df
            .column("open")
            .map_err(|e| DataError::InvalidFormat(e.to_string()))?
            .f64()
            .map_err(|e| DataError::InvalidFormat(e.to_string()))?;

        let highs = df
            .column("high")
            .map_err(|e| DataError::InvalidFormat(e.to_string()))?
            .f64()
            .map_err(|e| DataError::InvalidFormat(e.to_string()))?;

        let lows = df
            .column("low")
            .map_err(|e| DataError::InvalidFormat(e.to_string()))?
            .f64()
            .map_err(|e| DataError::InvalidFormat(e.to_string()))?;

        let closes = df
            .column("close")
            .map_err(|e| DataError::InvalidFormat(e.to_string()))?
            .f64()
            .map_err(|e| DataError::InvalidFormat(e.to_string()))?;

        let volumes = df
            .column("volume")
            .map_err(|e| DataError::InvalidFormat(e.to_string()))?
            .f64()
            .map_err(|e| DataError::InvalidFormat(e.to_string()))?;

        let trade_counts = df
            .column("trade_count")
            .map_err(|e| DataError::InvalidFormat(e.to_string()))?
            .u64()
            .map_err(|e| DataError::InvalidFormat(e.to_string()))?;

        let mut klines = Vec::with_capacity(df.height());

        for i in 0..df.height() {
            let ts_millis = timestamps.get(i).unwrap_or(0);
            let timestamp = Utc
                .timestamp_millis_opt(ts_millis)
                .single()
                .unwrap_or_default();

            klines.push(OHLCData {
                timestamp,
                symbol: symbols.get(i).unwrap_or("").to_string(),
                timeframe: Timeframe::OneMinute,
                open: Decimal::from_str(&opens.get(i).unwrap_or(0.0).to_string())
                    .unwrap_or(Decimal::ZERO),
                high: Decimal::from_str(&highs.get(i).unwrap_or(0.0).to_string())
                    .unwrap_or(Decimal::ZERO),
                low: Decimal::from_str(&lows.get(i).unwrap_or(0.0).to_string())
                    .unwrap_or(Decimal::ZERO),
                close: Decimal::from_str(&closes.get(i).unwrap_or(0.0).to_string())
                    .unwrap_or(Decimal::ZERO),
                volume: Decimal::from_str(&volumes.get(i).unwrap_or(0.0).to_string())
                    .unwrap_or(Decimal::ZERO),
                trade_count: trade_counts.get(i).unwrap_or(0u64),
            });
        }

        Ok(klines)
    }

    /// 读取指定时间范围的 K线数据
    pub fn read_klines(
        &self,
        symbol: &str,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> DataResult<Vec<OHLCData>> {
        let dir = self.symbol_dir(symbol);

        if !dir.exists() {
            return Ok(Vec::new());
        }

        let mut all_klines = Vec::new();

        // 遍历目录中的 Parquet 文件
        let entries = fs::read_dir(&dir).map_err(|e| {
            DataError::InvalidFormat(format!("Failed to read directory: {}", e))
        })?;

        for entry in entries {
            let entry = entry.map_err(|e| DataError::InvalidFormat(e.to_string()))?;
            let path = entry.path();

            if path.extension().and_then(|e| e.to_str()) != Some("parquet") {
                continue;
            }

            // 读取文件
            let df = self.read_parquet(&path)?;

            // 过滤时间范围
            let filtered = df
                .lazy()
                .filter(
                    col("timestamp")
                        .gt(start.timestamp_millis())
                        .and(col("timestamp").lt(end.timestamp_millis())),
                )
                .collect()
                .map_err(|e| DataError::InvalidFormat(e.to_string()))?;

            let klines = self.dataframe_to_klines(&filtered)?;
            all_klines.extend(klines);
        }

        // 按时间排序
        all_klines.sort_by(|a, b| a.timestamp.cmp(&b.timestamp));

        Ok(all_klines)
    }

    /// 获取 symbol 的数据统计
    pub fn get_stats(&self, symbol: &str) -> DataResult<ParquetStats> {
        let dir = self.symbol_dir(symbol);

        if !dir.exists() {
            return Ok(ParquetStats {
                symbol: symbol.to_string(),
                total_records: 0,
                files: 0,
                earliest_time: None,
                latest_time: None,
                total_size_bytes: 0,
            });
        }

        let mut total_records = 0;
        let mut files = 0;
        let mut earliest_time: Option<DateTime<Utc>> = None;
        let mut latest_time: Option<DateTime<Utc>> = None;
        let mut total_size_bytes: u64 = 0;

        let entries = fs::read_dir(&dir).map_err(|e| {
            DataError::InvalidFormat(format!("Failed to read directory: {}", e))
        })?;

        for entry in entries {
            let entry = entry.map_err(|e| DataError::InvalidFormat(e.to_string()))?;
            let path = entry.path();

            if path.extension().and_then(|e| e.to_str()) != Some("parquet") {
                continue;
            }

            let metadata = fs::metadata(&path).map_err(|e| {
                DataError::InvalidFormat(format!("Failed to read metadata: {}", e))
            })?;

            total_size_bytes += metadata.len();
            files += 1;

            // 读取文件获取记录数和时间范围
            if let Ok(df) = self.read_parquet(&path) {
                total_records += df.height();

                if let Ok(ts_col) = df.column("timestamp").and_then(|c| c.i64()) {
                    if let Some(min_ts) = ts_col.min() {
                        let dt = Utc.timestamp_millis_opt(min_ts).single();
                        if let Some(dt) = dt {
                            earliest_time = Some(
                                earliest_time.map_or(dt, |e| if dt < e { dt } else { e }),
                            );
                        }
                    }
                    if let Some(max_ts) = ts_col.max() {
                        let dt = Utc.timestamp_millis_opt(max_ts).single();
                        if let Some(dt) = dt {
                            latest_time = Some(
                                latest_time.map_or(dt, |l| if dt > l { dt } else { l }),
                            );
                        }
                    }
                }
            }
        }

        Ok(ParquetStats {
            symbol: symbol.to_string(),
            total_records,
            files,
            earliest_time,
            latest_time,
            total_size_bytes,
        })
    }

    /// 列出所有可用的 symbol
    pub fn list_symbols(&self) -> DataResult<Vec<String>> {
        let base = &self.config.base_path;

        if !base.exists() {
            return Ok(Vec::new());
        }

        let mut symbols = Vec::new();

        let entries = fs::read_dir(base).map_err(|e| {
            DataError::InvalidFormat(format!("Failed to read directory: {}", e))
        })?;

        for entry in entries {
            let entry = entry.map_err(|e| DataError::InvalidFormat(e.to_string()))?;
            let path = entry.path();

            if path.is_dir() {
                if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                    symbols.push(name.to_string());
                }
            }
        }

        Ok(symbols)
    }
}

/// Parquet 数据统计
#[derive(Debug, Clone)]
pub struct ParquetStats {
    pub symbol: String,
    pub total_records: usize,
    pub files: usize,
    pub earliest_time: Option<DateTime<Utc>>,
    pub latest_time: Option<DateTime<Utc>>,
    pub total_size_bytes: u64,
}

/// 合并多个 DataFrame
fn concat_df(dfs: &[DataFrame], rechunk: bool) -> DataResult<DataFrame> {
    let mut all_height = 0;
    for df in dfs {
        all_height += df.height();
    }

    if all_height == 0 {
        return Ok(DataFrame::empty());
    }

    let first = dfs.first().ok_or_else(|| {
        DataError::InvalidFormat("No DataFrames to concatenate".to_string())
    })?;

    let mut combined = first.clone();

    for df in dfs.iter().skip(1) {
        combined = concat_df_hstack(&combined, df)?;
    }

    Ok(combined)
}

/// 水平合并 DataFrame
fn concat_df_hstack(a: &DataFrame, b: &DataFrame) -> DataResult<DataFrame> {
    let mut result = a.clone();

    for col_name in b.get_column_names() {
        if let Ok(col) = b.column(col_name) {
            result
                .with_column(col.clone())
                .map_err(|e| DataError::InvalidFormat(e.to_string()))?;
        }
    }

    Ok(result)
}
