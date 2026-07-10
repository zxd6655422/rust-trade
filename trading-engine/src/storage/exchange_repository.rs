// storage/exchange_repository.rs
// 交易所配置仓储 - 从数据库加载交易所实例配置
//
// 配置内容：交易所类型、模式、启用状态、杠杆
// API Key 从 .env 环境变量读取
// 交易对由策略服务控制

use sqlx::{PgPool, Row};
use tracing::info;

use crate::config::ExchangeInstanceConfig;

/// 交易所配置仓储
pub struct ExchangeRepository {
    pool: PgPool,
}

impl ExchangeRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// 从数据库加载所有已启用的交易所配置
    pub async fn load_enabled(&self) -> Result<Vec<ExchangeInstanceConfig>, String> {
        let rows = sqlx::query(
            "SELECT id, exchange_id, market_type, testnet, enabled, leverage \
             FROM exchange_config WHERE enabled = true ORDER BY id"
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| format!("Failed to load exchange config: {}", e))?;

        let configs: Vec<ExchangeInstanceConfig> = rows.iter().map(|row| {
            ExchangeInstanceConfig {
                id: row.get("id"),
                exchange_id: row.get("exchange_id"),
                market_type: row.get("market_type"),
                testnet: row.get("testnet"),
                enabled: row.get("enabled"),
                leverage: row.get::<i32, _>("leverage") as u32,
            }
        }).collect();

        info!("Loaded {} exchange configs from database", configs.len());
        Ok(configs)
    }

    /// 更新交易所启用状态
    pub async fn set_enabled(&self, id: &str, enabled: bool) -> Result<(), String> {
        sqlx::query(
            "UPDATE exchange_config SET enabled = $1, updated_at = NOW() WHERE id = $2"
        )
        .bind(enabled)
        .bind(id)
        .execute(&self.pool)
        .await
        .map_err(|e| format!("Failed to update exchange config: {}", e))?;

        info!("Exchange {} enabled={}", id, enabled);
        Ok(())
    }

    /// 更新杠杆倍数
    pub async fn set_leverage(&self, id: &str, leverage: u32) -> Result<(), String> {
        sqlx::query(
            "UPDATE exchange_config SET leverage = $1, updated_at = NOW() WHERE id = $2"
        )
        .bind(leverage as i32)
        .bind(id)
        .execute(&self.pool)
        .await
        .map_err(|e| format!("Failed to update leverage: {}", e))?;

        info!("Exchange {} leverage={}", id, leverage);
        Ok(())
    }
}
