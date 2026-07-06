use anyhow::Result;

#[derive(Debug, Clone)]
pub struct AppConfig {
    pub server: ServerConfig,
    pub database: DatabaseConfig,
    pub redis: RedisConfig,
    pub engine: EngineConfig,
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
        };

        Ok(app_config)
    }
}
