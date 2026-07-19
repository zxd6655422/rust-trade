// storage/risk_config_repository.rs
// 风控参数配置仓储层
//
// 从 PostgreSQL risk_config 表加载风控参数，支持运行时热更新。
// 启动时加载一次到内存缓存，之后由 RiskEngine 定期调用 reload() 刷新。

use rust_decimal::Decimal;
use sqlx::PgPool;
use tokio::sync::RwLock;
use tracing::{info, warn};

use crate::risk::config::RiskConfig;

/// 风控参数配置仓储
pub struct RiskConfigRepository {
    pool: PgPool,
    cache: RwLock<RiskConfig>,
}

impl RiskConfigRepository {
    /// 创建新的风控参数仓储
    ///
    /// 启动时从 DB 加载一次，失败则使用默认值兜底
    pub async fn new(pool: PgPool, fallback: RiskConfig) -> Self {
        let config = match Self::load_from_db(&pool).await {
            Ok(config) => {
                info!("✅ Risk config loaded from database");
                config
            }
            Err(e) => {
                warn!(
                    "⚠️ Failed to load risk config from DB: {}, using fallback from config.toml",
                    e
                );
                fallback
            }
        };

        Self {
            pool,
            cache: RwLock::new(config),
        }
    }

    /// 获取当前缓存的风控配置（clone 返回，不持有锁）
    pub async fn get_config(&self) -> RiskConfig {
        self.cache.read().await.clone()
    }

    /// 从 DB 重新加载风控配置到缓存
    pub async fn reload(&self) -> Result<(), sqlx::Error> {
        let config = Self::load_from_db(&self.pool).await?;
        *self.cache.write().await = config;
        Ok(())
    }

    /// 更新单个风控参数到 DB 并刷新缓存
    pub async fn update(&self, key: &str, value: Decimal) -> Result<(), sqlx::Error> {
        sqlx::query(
            r#"UPDATE risk_config SET value = $1, updated_at = NOW() WHERE key = $2"#,
        )
        .bind(value)
        .bind(key)
        .execute(&self.pool)
        .await?;

        // 刷新缓存
        if let Err(e) = self.reload().await {
            warn!("Failed to reload risk config after update: {}", e);
        }

        info!("Risk config updated: {} = {}", key, value);
        Ok(())
    }

    /// 从 DB 加载所有风控参数并构建 RiskConfig
    async fn load_from_db(pool: &PgPool) -> Result<RiskConfig, sqlx::Error> {
        let rows: Vec<(String, Decimal)> =
            sqlx::query_as(r#"SELECT key, value FROM risk_config"#)
                .fetch_all(pool)
                .await?;

        let map: std::collections::HashMap<String, Decimal> = rows.into_iter().collect();

        let get = |key: &str| -> Decimal {
            map.get(key).copied().unwrap_or(Decimal::ZERO)
        };

        let get_u64 = |key: &str| -> u64 {
            map.get(key)
                .and_then(|d| d.to_string().parse::<u64>().ok())
                .unwrap_or(0)
        };

        let get_u32 = |key: &str| -> u32 {
            map.get(key)
                .and_then(|d| d.to_string().parse::<u32>().ok())
                .unwrap_or(0)
        };

        Ok(RiskConfig {
            max_position_pct: get("max_position_pct"),
            stop_loss_pct: get("stop_loss_pct"),
            take_profit_pct: get("take_profit_pct"),
            risk_per_trade_pct: get("risk_per_trade_pct"),
            max_daily_loss: get("max_daily_loss"),
            max_drawdown_pct: get("max_drawdown_pct"),
            max_exposure_pct: get("max_exposure_pct"),
            kelly_fraction: get("kelly_fraction"),
            volatility_lookback: get_u32("volatility_lookback"),
            volatility_target: get("volatility_target"),
            black_swan_threshold: get("black_swan_threshold"),
            circuit_breaker_cooldown: get_u64("circuit_breaker_cooldown"),
            daily_reset_hour: get_u32("daily_reset_hour"),
        })
    }
}
