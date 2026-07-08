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
    /// 需要回填和存储的时间框架列表
    #[serde(default = "default_stored_timeframes")]
    pub stored_timeframes: Vec<String>,
    /// 是否启用多时间框架回填
    #[serde(default)]
    pub multi_tf_backfill_enabled: bool,
    /// 多时间框架回填间隔（小时），增量更新用
    #[serde(default = "default_multi_tf_backfill_interval_hours")]
    pub multi_tf_backfill_interval_hours: u64,
}

fn default_poll_interval() -> u64 {
    10
}

fn default_backfill_start_date() -> String {
    "2020-01-01".to_string()
}

fn default_stored_timeframes() -> Vec<String> {
    vec![
        "1m".to_string(),
        "5m".to_string(),
        "15m".to_string(),
        "30m".to_string(),
        "1h".to_string(),
        "2h".to_string(),
        "4h".to_string(),
        "1d".to_string(),
        "3d".to_string(),
        "1w".to_string(),
    ]
}

fn default_multi_tf_backfill_interval_hours() -> u64 {
    6 // 每6小时执行一次增量回填
}

impl Default for CollectorConfig {
    fn default() -> Self {
        Self {
            mode: CollectorMode::default(),
            enable_tick: false,
            poll_interval_secs: 10,
            backfill_enabled: false,
            backfill_start_date: "2020-01-01".to_string(),
            stored_timeframes: default_stored_timeframes(),
            multi_tf_backfill_enabled: false,
            multi_tf_backfill_interval_hours: 6,
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

#[derive(Debug, Deserialize, Clone)]
pub struct StrategyConfig {
    /// 分析间隔（秒），默认 300
    #[serde(default = "default_strategy_interval")]
    pub interval_secs: u64,
    /// 信号过期时间（小时），默认 24
    #[serde(default = "default_signal_max_age")]
    pub signal_max_age_hours: i64,
    /// 确认阈值（收益率%），默认 0.5
    #[serde(default = "default_confirm_threshold")]
    pub confirm_threshold_pct: f64,
    /// 止损阈值（收益率%），默认 -2.0
    #[serde(default = "default_stop_loss")]
    pub stop_loss_pct: f64,
    /// 止盈阈值（收益率%），默认 3.0
    #[serde(default = "default_take_profit")]
    pub take_profit_pct: f64,
}

fn default_strategy_interval() -> u64 { 300 }
fn default_signal_max_age() -> i64 { 24 }
fn default_confirm_threshold() -> f64 { 0.5 }
fn default_stop_loss() -> f64 { -2.0 }
fn default_take_profit() -> f64 { 3.0 }

impl Default for StrategyConfig {
    fn default() -> Self {
        Self {
            interval_secs: 300,
            signal_max_age_hours: 24,
            confirm_threshold_pct: 0.5,
            stop_loss_pct: -2.0,
            take_profit_pct: 3.0,
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct Settings {
    pub database: Database,
    pub cache: Cache,
    pub symbols: Vec<String>,
    /// 只有合约的交易对（使用 /fapi/v1/klines 而非 /api/v3/klines）
    #[serde(default)]
    pub futures_symbols: Vec<String>,
    pub paper_trading: PaperTrading,
    /// 数据采集配置
    #[serde(default)]
    pub collector: CollectorConfig,
    /// 策略配置
    #[serde(default)]
    pub strategy: StrategyConfig,
    /// 日志等级: trace, debug, info, warn, error
    #[serde(default = "default_log_level")]
    pub log_level: String,
}

fn default_log_level() -> String {
    "info".to_string()
}

impl Settings {
    pub fn new() -> Result<Self, ConfigError> {
        let run_mode = std::env::var("RUN_MODE").unwrap_or_else(|_| "development".into());

        // 配置文件加载优先级（类似 Spring Boot 约定）：
        // 1. 可执行文件同级目录 config/{run_mode}.toml（外部配置，优先）
        // 2. 当前工作目录 config/{run_mode}.toml
        // 3. 上级目录 config/{run_mode}.toml（开发时使用）
        let config_path = Self::find_config_path(&run_mode);

        println!("📋 Loading config: {}", config_path);

        let mut builder = Config::builder()
            .add_source(File::with_name(&config_path).required(true));

        // 环境变量可以覆盖敏感配置
        if let Ok(database_url) = std::env::var("DATABASE_URL") {
            builder = builder.set_override("database.url", database_url)?;
        }

        if let Ok(redis_url) = std::env::var("REDIS_URL") {
            builder = builder.set_override("cache.redis.url", redis_url)?;
        }

        let s = builder.build()?;
        s.try_deserialize()
    }

    /// 查找配置文件路径（支持外部配置覆盖内部配置）
    fn find_config_path(run_mode: &str) -> String {
        // 优先级 1: 可执行文件同级目录（打包部署时使用）
        if let Ok(exe_path) = std::env::current_exe() {
            if let Some(exe_dir) = exe_path.parent() {
                let config_dir = exe_dir.join("config");
                let config_file = config_dir.join(format!("{}.toml", run_mode));
                if config_file.exists() {
                    println!("✅ Using config from exe dir: {:?}", config_file);
                    // 返回绝对路径，config crate 会正确处理
                    return config_file.to_string_lossy().trim_end_matches(".toml").to_string();
                }
            }
        }

        // 优先级 2: 当前工作目录
        let cwd_config = std::env::current_dir()
            .unwrap_or_default()
            .join("config")
            .join(format!("{}.toml", run_mode));
        if cwd_config.exists() {
            println!("✅ Using config from cwd: {:?}", cwd_config);
            return cwd_config.to_string_lossy().trim_end_matches(".toml").to_string();
        }

        // 优先级 3: 上级目录（开发时使用）
        if let Ok(cwd) = std::env::current_dir() {
            if let Some(parent) = cwd.parent() {
                let parent_config = parent.join("config").join(format!("{}.toml", run_mode));
                if parent_config.exists() {
                    println!("✅ Using config from parent dir: {:?}", parent_config);
                    return parent_config.to_string_lossy().trim_end_matches(".toml").to_string();
                }
            }
        }

        // 默认：让 config crate 报错
        println!("⚠️ Config file not found, using default path");
        format!("config/{}", run_mode)
    }
}
