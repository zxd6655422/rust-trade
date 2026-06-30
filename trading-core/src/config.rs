use config::{Config, ConfigError, File};
use serde::Deserialize;

/// 数据采集模式
#[derive(Debug, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum CollectorMode {
    /// 禁用数据采集（仅回测）
    Disabled,
    /// 采集 tick 数据（高频，资源消耗大）
    Tick,
    /// 采集 candle1m 数据（低频，资源消耗最小）
    Candle1m,
}

impl Default for CollectorMode {
    fn default() -> Self {
        CollectorMode::Candle1m
    }
}

impl std::fmt::Display for CollectorMode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CollectorMode::Disabled => write!(f, "disabled"),
            CollectorMode::Tick => write!(f, "tick"),
            CollectorMode::Candle1m => write!(f, "candle1m"),
        }
    }
}

/// 数据采集配置
#[derive(Debug, Deserialize, Clone)]
pub struct CollectorConfig {
    /// 采集模式: disabled / tick / candle1m
    #[serde(default)]
    pub mode: CollectorMode,
    /// 是否同时启用 tick 采集（用于高频数据）
    #[serde(default)]
    pub enable_tick: bool,
    /// 采集间隔（秒），仅对 candle1m 模式有效
    #[serde(default = "default_poll_interval")]
    pub poll_interval_secs: u64,
    /// 是否启用历史数据回填
    #[serde(default)]
    pub backfill_enabled: bool,
    /// 回填起始日期，格式 "YYYY-MM-DD"
    #[serde(default = "default_backfill_start_date")]
    pub backfill_start_date: String,
}

fn default_poll_interval() -> u64 {
    10
}

fn default_backfill_start_date() -> String {
    "2020-01-01".to_string()
}

impl Default for CollectorConfig {
    fn default() -> Self {
        Self {
            mode: CollectorMode::default(),
            enable_tick: false,
            poll_interval_secs: 10,
            backfill_enabled: false,
            backfill_start_date: "2020-01-01".to_string(),
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct Database {
    pub url: String,
    pub max_connections: u32,
    pub min_connections: u32,
    pub max_lifetime: u64,
}

#[derive(Debug, Deserialize)]
pub struct MemoryCache {
    pub max_ticks_per_symbol: usize,
    pub ttl_seconds: u64,
}

#[derive(Debug, Deserialize)]
pub struct RedisCache {
    pub url: String,
    pub ttl_seconds: u64,
    pub max_ticks_per_symbol: usize,
}

#[derive(Debug, Deserialize)]
pub struct Cache {
    pub memory: MemoryCache,
    pub redis: RedisCache,
}

#[derive(Debug, Deserialize)]
pub struct PaperTrading {
    pub enabled: bool,
    pub strategy: String,
    pub initial_capital: f64,
}

#[derive(Debug, Deserialize)]
pub struct Settings {
    pub database: Database,
    pub cache: Cache,
    pub symbols: Vec<String>,
    pub paper_trading: PaperTrading,
    /// 数据采集配置
    #[serde(default)]
    pub collector: CollectorConfig,
}

impl Settings {
    pub fn new() -> Result<Self, ConfigError> {
        let run_mode = std::env::var("RUN_MODE").unwrap_or_else(|_| "development".into());

        // 配置文件路径：优先从当前目录的 config/ 加载，否则从 ../config/ 加载
        let config_path = if std::path::Path::new("config").exists() {
            format!("config/{}", run_mode)
        } else {
            format!("../config/{}", run_mode)
        };

        let mut builder = Config::builder()
            .add_source(File::with_name(&config_path).required(true));

        if let Ok(database_url) = std::env::var("DATABASE_URL") {
            builder = builder.set_override("database.url", database_url)?;
        }

        if let Ok(redis_url) = std::env::var("REDIS_URL") {
            builder = builder.set_override("cache.redis.url", redis_url)?;
        }

        let s = builder.build()?;
        s.try_deserialize()
    }
}
