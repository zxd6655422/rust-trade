// config.rs
// 交易引擎配置

use config::{Config, ConfigError, File};
use serde::Deserialize;

use crate::risk::RiskConfig;

/// 数据库配置
#[derive(Debug, Deserialize, Clone)]
pub struct DatabaseConfig {
    pub url: String,
    pub max_connections: u32,
    pub min_connections: u32,
    pub max_lifetime: u64,
}

/// 缓存配置
#[derive(Debug, Deserialize, Clone)]
pub struct CacheConfig {
    pub url: String,
    pub ttl_seconds: u64,
    pub max_ticks_per_symbol: usize,
}

/// 交易所配置（单个，兼容旧配置）
#[derive(Debug, Deserialize, Clone)]
pub struct ExchangeConfig {
    pub id: String,
    pub testnet: bool,
}

/// 交易所实例配置（支持多交易所多模式）
///
/// 每个实例代表一个 交易所+交易模式 的组合，
/// 如 binance-futures、binance-spot、okx-futures
///
/// API Key 从 .env 环境变量读取，交易对由策略服务控制
#[derive(Debug, Deserialize, Clone)]
pub struct ExchangeInstanceConfig {
    /// 实例唯一标识，如 "binance-futures"
    pub id: String,
    /// 交易所 ID，传给 ExchangeFactory，如 "binance", "binance-spot", "okx"
    pub exchange_id: String,
    /// 交易模式: "spot" / "futures"
    pub market_type: String,
    /// 是否测试网
    #[serde(default)]
    pub testnet: bool,
    /// 是否启用
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// 杠杆倍数（仅合约有效）
    #[serde(default = "default_leverage")]
    pub leverage: u32,
}

fn default_true() -> bool { true }
fn default_leverage() -> u32 { 10 }

/// 数据源类型
#[derive(Debug, Deserialize, Clone, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum DataSourceType {
    /// 逐笔成交 (高频，资源消耗大)
    Trades,
    /// 行情快照 (中频)
    Tickers,
    /// K线推送 (低频，资源消耗最小)
    Candle1m,
}

impl Default for DataSourceType {
    fn default() -> Self {
        DataSourceType::Trades
    }
}

impl std::fmt::Display for DataSourceType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            DataSourceType::Trades => write!(f, "trades"),
            DataSourceType::Tickers => write!(f, "tickers"),
            DataSourceType::Candle1m => write!(f, "candle1m"),
        }
    }
}

/// 交易配置
#[derive(Debug, Deserialize, Clone)]
pub struct TradingConfig {
    pub mode: String, // testnet / live
    pub strategy: String,
    pub symbols: Vec<String>,
    pub poll_interval_ms: u64,
    /// 数据源类型: trades / tickers / candle1m (默认 trades)
    #[serde(default)]
    pub data_source: DataSourceType,
}

/// 应用设置
#[derive(Debug, Deserialize, Clone)]
pub struct Settings {
    pub database: DatabaseConfig,
    pub cache: CacheConfig,
    /// 单交易所配置（兼容旧配置）
    pub exchange: ExchangeConfig,
    pub trading: TradingConfig,
    pub risk_control: RiskConfig,
}

impl Settings {
    /// 从配置文件加载设置
    pub fn new() -> Result<Self, ConfigError> {
        let run_mode = std::env::var("RUN_MODE").unwrap_or_else(|_| "development".into());

        // 配置文件加载优先级（类似 Spring Boot 约定）：
        // 1. 可执行文件同级目录 config/engine-{run_mode}.toml（外部配置，优先）
        // 2. 当前工作目录 config/engine-{run_mode}.toml
        // 3. 上级目录 config/engine-{run_mode}.toml（开发时使用）
        let config_path = Self::find_config_path(&run_mode);

        println!("📋 Loading config: {}", config_path);

        let mut builder = Config::builder()
            .add_source(File::with_name(&config_path).required(true));

        // 从环境变量覆盖数据库配置
        if let Ok(database_url) = std::env::var("DATABASE_URL") {
            builder = builder.set_override("database.url", database_url)?;
        }

        // 从环境变量覆盖 Redis 配置
        if let Ok(redis_url) = std::env::var("REDIS_URL") {
            builder = builder.set_override("cache.url", redis_url)?;
        }

        // 从环境变量覆盖 API Key (不存储在配置文件中)
        // API Key 应该通过环境变量传入
        if std::env::var("BINANCE_API_KEY").is_ok() {
            tracing::info!("Binance API key loaded from environment");
        }

        let s = builder.build()?;
        s.try_deserialize()
    }

    /// 查找配置文件路径（支持外部配置覆盖内部配置）
    fn find_config_path(run_mode: &str) -> String {
        let config_name = format!("engine-{}", run_mode);

        // 优先级 1: 可执行文件同级目录（打包部署时使用）
        if let Ok(exe_path) = std::env::current_exe() {
            if let Some(exe_dir) = exe_path.parent() {
                let config_dir = exe_dir.join("config");
                let config_file = config_dir.join(format!("{}.toml", config_name));
                if config_file.exists() {
                    println!("✅ Using config from exe dir: {:?}", config_file);
                    return config_file.to_string_lossy().trim_end_matches(".toml").to_string();
                }
            }
        }

        // 优先级 2: 当前工作目录
        let cwd_config = std::env::current_dir()
            .unwrap_or_default()
            .join("config")
            .join(format!("{}.toml", config_name));
        if cwd_config.exists() {
            println!("✅ Using config from cwd: {:?}", cwd_config);
            return cwd_config.to_string_lossy().trim_end_matches(".toml").to_string();
        }

        // 优先级 3: 上级目录（开发时使用）
        if let Ok(cwd) = std::env::current_dir() {
            if let Some(parent) = cwd.parent() {
                let parent_config = parent.join("config").join(format!("{}.toml", config_name));
                if parent_config.exists() {
                    println!("✅ Using config from parent dir: {:?}", parent_config);
                    return parent_config.to_string_lossy().trim_end_matches(".toml").to_string();
                }
            }
        }

        // 默认：让 config crate 报错
        println!("⚠️ Config file not found, using default path");
        format!("config/{}", config_name)
    }

    /// 检查是否为测试网模式
    pub fn is_testnet(&self) -> bool {
        self.exchange.testnet || self.trading.mode == "testnet"
    }

    /// 获取交易所 ID
    pub fn exchange_id(&self) -> &str {
        &self.exchange.id
    }
}

impl ExchangeInstanceConfig {
    /// 从环境变量获取 API Key
    /// 环境变量名：{ID 大写，- 替换为 _}_API_KEY
    /// 例如 binance-futures → BINANCE_FUTURES_API_KEY
    pub fn api_key(&self) -> Result<String, String> {
        let env_name = format!("{}_API_KEY", self.id.to_uppercase().replace("-", "_"));
        std::env::var(&env_name).map_err(|_| format!("{} not set", env_name))
    }

    /// 从环境变量获取 API Secret
    pub fn api_secret(&self) -> Result<String, String> {
        let env_name = format!("{}_API_SECRET", self.id.to_uppercase().replace("-", "_"));
        std::env::var(&env_name).map_err(|_| format!("{} not set", env_name))
    }

    /// 从环境变量获取 Passphrase（OKX 需要，Binance 不需要）
    pub fn passphrase(&self) -> Option<String> {
        let env_name = format!("{}_PASSPHRASE", self.id.to_uppercase().replace("-", "_"));
        std::env::var(env_name).ok()
    }
}

/// 确定环境文件名
fn determine_env_filename() -> String {
    let run_mode = std::env::var("RUN_MODE").unwrap_or_else(|_| "development".into());
    format!(".env.{}", run_mode)
}

/// 加载环境变量（支持 config/ 目录）
pub fn load_env() {
    let run_mode = std::env::var("RUN_MODE").unwrap_or_else(|_| {
        if cfg!(debug_assertions) { "development".into() } else { "production".into() }
    });

    let env_filename = format!(".env.{}", run_mode);

    // 优先从 config/ 目录加载
    let config_env_path = format!("config/{}", env_filename);
    if std::path::Path::new(&config_env_path).exists() {
        if let Ok(_) = dotenvy::from_filename(&config_env_path) {
            tracing::info!("✅ Loaded env from: {}", config_env_path);
            return;
        }
    }

    // 回退到当前目录
    if let Err(_) = dotenvy::from_filename(&env_filename) {
        tracing::warn!("⚠️ {} not found, trying .env", env_filename);
        if let Err(_) = dotenvy::dotenv() {
            tracing::warn!("⚠️ No .env file found");
        }
    }
}
