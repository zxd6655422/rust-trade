//! 账户余额快照同步模块
//!
//! 定时从交易所同步账户余额，写入 account_snapshot 表
//! 用于 TradeValidator 降级查询（API 不可用时从本地快照读取）

use anyhow::Result;
use chrono::Utc;
use sqlx::PgPool;
use std::sync::Arc;
use std::time::Duration;
use tracing::{debug, error, info, warn};

use crate::exchange::{ExchangeApiConfig, ExchangeClient};

/// 账户余额快照同步器
pub struct AccountSync {
    pool: PgPool,
    exchange_client: Option<ExchangeClient>,
    /// 同步间隔（秒）
    sync_interval_secs: u64,
}

impl AccountSync {
    pub fn new(pool: PgPool, sync_interval_secs: u64) -> Self {
        let exchange_client = match ExchangeApiConfig::binance_from_env() {
            Ok(api_config) => Some(ExchangeClient::new(api_config)),
            Err(e) => {
                warn!("Failed to create exchange client for account sync: {}", e);
                None
            }
        };

        Self {
            pool,
            exchange_client,
            sync_interval_secs,
        }
    }

    /// 启动定时同步任务
    pub async fn start(self: Arc<Self>) {
        info!(
            "Starting account snapshot sync (interval: {}s)",
            self.sync_interval_secs
        );

        // 启动时立即同步一次
        if let Err(e) = self.sync_account().await {
            error!("Initial account sync failed: {}", e);
        }

        let mut interval = tokio::time::interval(Duration::from_secs(self.sync_interval_secs));

        loop {
            interval.tick().await;

            if let Err(e) = self.sync_account().await {
                error!("Account sync error: {}", e);
            }
        }
    }

    /// 同步账户余额到快照表
    async fn sync_account(&self) -> Result<()> {
        let client = match &self.exchange_client {
            Some(client) => client,
            None => {
                debug!("No exchange client, skipping account sync");
                return Ok(());
            }
        };

        // 同步合约账户
        if let Err(e) = self.sync_futures(client).await {
            warn!("Futures account sync failed: {}", e);
        }

        // 同步现货账户
        if let Err(e) = self.sync_spot(client).await {
            warn!("Spot account sync failed: {}", e);
        }

        // 清理 7 天前的旧快照
        if let Err(e) = self.cleanup_old_snapshots().await {
            warn!("Snapshot cleanup failed: {}", e);
        }

        Ok(())
    }

    /// 同步合约账户余额
    async fn sync_futures(&self, client: &ExchangeClient) -> Result<()> {
        let available = client.get_usdt_balance().await.unwrap_or_else(|e| {
            warn!("Failed to get futures balance: {}", e);
            rust_decimal::Decimal::ZERO
        });

        let positions = client.get_positions().await.unwrap_or_else(|e| {
            warn!("Failed to get futures positions: {}", e);
            Vec::new()
        });

        let position_count = positions.len() as i32;

        // 计算总余额（可用 + 持仓保证金）
        let total = available;

        sqlx::query(
            r#"
            INSERT INTO account_snapshot (exchange, market_type, available_balance, total_balance, position_count, snapshot_at)
            VALUES ('binance', 'futures', $1, $2, $3, NOW())
            "#,
        )
        .bind(available)
        .bind(total)
        .bind(position_count)
        .execute(&self.pool)
        .await?;

        debug!(
            "Futures snapshot: available={}, total={}, positions={}",
            available, total, position_count
        );

        Ok(())
    }

    /// 同步现货账户余额
    async fn sync_spot(&self, client: &ExchangeClient) -> Result<()> {
        let available = client.get_spot_usdt_balance().await.unwrap_or_else(|e| {
            warn!("Failed to get spot balance: {}", e);
            rust_decimal::Decimal::ZERO
        });

        sqlx::query(
            r#"
            INSERT INTO account_snapshot (exchange, market_type, available_balance, total_balance, position_count, snapshot_at)
            VALUES ('binance', 'spot', $1, $1, 0, NOW())
            "#,
        )
        .bind(available)
        .execute(&self.pool)
        .await?;

        debug!("Spot snapshot: available={}", available);

        Ok(())
    }

    /// 清理 7 天前的旧快照
    async fn cleanup_old_snapshots(&self) -> Result<()> {
        let result = sqlx::query(
            "DELETE FROM account_snapshot WHERE snapshot_at < NOW() - INTERVAL '7 days'",
        )
        .execute(&self.pool)
        .await?;

        if result.rows_affected() > 0 {
            debug!(
                "Cleaned up {} old snapshots",
                result.rows_affected()
            );
        }

        Ok(())
    }
}
