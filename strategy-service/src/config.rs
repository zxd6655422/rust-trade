use anyhow::Result;
use serde::Deserialize;

#[derive(Debug, Deserialize, Clone)]
pub struct AppConfig {
    pub server: ServerConfig,
    pub database: DatabaseConfig,
    pub redis: RedisConfig,
    pub engine: EngineConfig,
}

#[derive(Debug, Deserialize, Clone)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
}

#[derive(Debug, Deserialize, Clone)]
pub struct DatabaseConfig {
    pub url: String,
    pub max_connections: u32,
}

#[derive(Debug, Deserialize, Clone)]
pub struct RedisConfig {
    pub url: String,
}

#[derive(Debug, Deserialize, Clone)]
pub struct EngineConfig {
    pub poll_interval_secs: u64,
}

impl AppConfig {
    pub fn load() -> Result<Self> {
        dotenv::dotenv().ok();

        let config = config::Config::builder()
            .add_source(config::Environment::default().separator("__"))
            .build()?;

        let app_config = AppConfig {
            server: ServerConfig {
                host: config.get_string("SERVER_HOST").unwrap_or_else(|_| "0.0.0.0".to_string()),
                port: config.get_int("SERVER_PORT").unwrap_or(8082) as u16,
            },
            database: DatabaseConfig {
                url: config.get_string("DATABASE_URL")?,
                max_connections: config.get_int("DATABASE_MAX_CONNECTIONS").unwrap_or(10) as u32,
            },
            redis: RedisConfig {
                url: config.get_string("REDIS_URL")?,
            },
            engine: EngineConfig {
                poll_interval_secs: config.get_int("ENGINE_POLL_INTERVAL_SECS").unwrap_or(5) as u64,
            },
        };

        Ok(app_config)
    }
}
