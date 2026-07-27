use anyhow::Result;

#[derive(Debug, Clone)]
pub struct AppConfig {
    pub server: ServerConfig,
    pub database: DatabaseConfig,
    pub redis: RedisConfig,
    pub engine: EngineConfig,
    pub kline: KlineConfig,
    pub binance: BinanceConfig,
}

#[derive(Debug, Clone)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
}

#[derive(Debug, Clone)]
pub struct DatabaseConfig {
    pub url: String,
    pub max_connections: u32,
}

#[derive(Debug, Clone)]
pub struct RedisConfig {
    pub url: String,
}

#[derive(Debug, Clone)]
pub struct EngineConfig {
    pub poll_interval_secs: u64,
}

/// K线数据配置
#[derive(Debug, Clone)]
pub struct KlineConfig {
    /// 默认加载和保持的K线数量
    pub default_max_bars: usize,
    /// 是否启用从交易所补拉缺口
    pub enable_gap_fill: bool,
}

impl Default for KlineConfig {
    fn default() -> Self {
        Self {
            default_max_bars: 1000,
            enable_gap_fill: true,
        }
    }
}

/// Binance API 配置
#[derive(Debug, Clone)]
pub struct BinanceConfig {
    /// 市场类型: "futures" 或 "spot"
    pub market_type: String,
}

impl Default for BinanceConfig {
    fn default() -> Self {
        Self {
            market_type: "futures".to_string(),
        }
    }
}

fn get_env(key: &str) -> Result<String> {
    std::env::var(key).map_err(|_| anyhow::anyhow!("configuration property \"{}\" not found", key))
}

fn get_env_or(key: &str, default: &str) -> String {
    std::env::var(key).unwrap_or_else(|_| default.to_string())
}

impl AppConfig {
    pub fn load() -> Result<Self> {
        let app_config = AppConfig {
            server: ServerConfig {
                host: get_env_or("SERVER_HOST", "0.0.0.0"),
                port: get_env_or("SERVER_PORT", "8082").parse()?,
            },
            database: DatabaseConfig {
                url: get_env("DATABASE_URL")?,
                max_connections: get_env_or("DATABASE_MAX_CONNECTIONS", "10").parse()?,
            },
            redis: RedisConfig {
                url: get_env("REDIS_URL")?,
            },
            engine: EngineConfig {
                poll_interval_secs: get_env_or("ENGINE_POLL_INTERVAL_SECS", "5").parse()?,
            },
            kline: KlineConfig {
                default_max_bars: get_env_or("KLINE_DEFAULT_MAX_BARS", "1000").parse()?,
                enable_gap_fill: get_env_or("KLINE_ENABLE_GAP_FILL", "true").parse()?,
            },
            binance: BinanceConfig {
                market_type: get_env_or("BINANCE_MARKET_TYPE", "futures"),
            },
        };

        Ok(app_config)
    }
}
