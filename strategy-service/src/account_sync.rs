//! 账户余额快照同步模块
//!
//! 定时从交易所同步账户余额，写入 account_snapshot 表
//! 使用统一的 AccountProvider 接口，支持 Binance / OKX

use anyhow::Result;
use sqlx::PgPool;
use std::sync::Arc;
use std::time::Duration;
use tracing::{debug, error, info, warn};

use trading_common::data::account_types::AccountProvider;
use trading_common::data::repository::TickDataRepository;

use crate::binance_account::BinanceAccountProvider;
use crate::exchange::ExchangeApiConfig;
use crate::okx_account::OkxAccountProvider;
use crate::okx_client::OkxConfig;

/// 账户余额快照同步器
pub struct AccountSync {
    repo: Arc<TickDataRepository>,
    /// Binance 账户提供者
    binance_provider: Option<Box<dyn AccountProvider>>,
    /// OKX 账户提供者
    okx_provider: Option<Box<dyn AccountProvider>>,
    /// 同步间隔（秒）
    sync_interval_secs: u64,
}

impl AccountSync {
    pub fn new(repo: Arc<TickDataRepository>, sync_interval_secs: u64) -> Self {
        // 初始化 Binance 提供者
        let binance_provider = match ExchangeApiConfig::binance_from_env() {
            Ok(api_config) => {
                let provider = BinanceAccountProvider::new(api_config);
                Some(Box::new(provider) as Box<dyn AccountProvider>)
            }
            Err(e) => {
                warn!("Binance API not configured: {}", e);
                None
            }
        };

        // 初始化 OKX 提供者
        let okx_provider = match OkxConfig::from_env() {
            Ok(okx_config) => {
                let provider = OkxAccountProvider::new(okx_config);
                Some(Box::new(provider) as Box<dyn AccountProvider>)
            }
            Err(e) => {
                warn!("OKX API not configured: {}", e);
                None
            }
        };

        Self {
            repo,
            binance_provider,
            okx_provider,
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
        // 同步 Binance 账户
        if let Some(provider) = &self.binance_provider {
            if let Err(e) = self.sync_exchange(provider, "binance").await {
                warn!("Binance account sync failed: {}", e);
            }
        }

        // 同步 OKX 账户
        if let Some(provider) = &self.okx_provider {
            if let Err(e) = self.sync_exchange(provider, "okx").await {
                warn!("OKX account sync failed: {}", e);
            }
        }

        // 清理 7 天前的旧快照
        if let Err(e) = self.cleanup_old_snapshots().await {
            warn!("Snapshot cleanup failed: {}", e);
        }

        Ok(())
    }

    /// 同步单个交易所的账户数据
    async fn sync_exchange(
        &self,
        provider: &Box<dyn AccountProvider>,
        exchange_name: &str,
    ) -> Result<()> {
        // 同步合约账户
        if let Err(e) = self.sync_futures(provider, exchange_name).await {
            warn!("[{}] Futures account sync failed: {}", exchange_name, e);
        }

        // 同步现货账户
        if let Err(e) = self.sync_spot(provider, exchange_name).await {
            warn!("[{}] Spot account sync failed: {}", exchange_name, e);
        }

        Ok(())
    }

    /// 同步合约账户余额
    async fn sync_futures(
        &self,
        provider: &Box<dyn AccountProvider>,
        exchange_name: &str,
    ) -> Result<()> {
        // OKX 使用 "swap"，Binance 使用 "futures"
        let market_type = if exchange_name == "okx" { "swap" } else { "futures" };

        // 获取账户快照
        let snapshot = provider.get_account_snapshot(market_type).await?;

        // 写入账户快照
        self.repo.insert_account_snapshot(
            &snapshot.exchange,
            &snapshot.market_type,
            snapshot.total_equity,
            snapshot.total_balance,
            snapshot.available_balance,
            snapshot.frozen_balance,
            snapshot.unrealized_pnl,
            snapshot.initial_margin,
            snapshot.maint_margin,
            snapshot.margin_ratio,
            snapshot.position_count,
            snapshot.raw_data.clone(),
        ).await?;

        // 获取资产余额
        let balances = provider.get_asset_balances(market_type).await?;
        for balance in &balances {
            self.repo.insert_asset_balance(
                &balance.exchange,
                &balance.market_type,
                &balance.asset,
                balance.total,
                balance.available,
                balance.frozen,
                balance.unrealized_pnl,
                balance.usd_value,
            ).await?;
        }

        // 获取持仓信息
        let positions = provider.get_positions().await?;
        for pos in &positions {
            self.repo.insert_position_snapshot(
                &pos.exchange,
                market_type,
                &pos.symbol,
                &pos.raw_symbol,
                pos.position_side.as_str(),
                pos.position_amt,
                pos.entry_price,
                pos.mark_price,
                pos.unrealized_pnl,
                pos.leverage as i32,
                pos.margin_type.as_str(),
                pos.initial_margin,
                pos.maint_margin,
                pos.liquidation_price,
                pos.notional,
                pos.break_even_price,
                pos.isolated_wallet,
                Some(pos.pnl_ratio()),
                pos.raw_data.clone(),
            ).await?;
        }

        debug!(
            "[{}] Futures snapshot: equity={}, balance={}, positions={}",
            exchange_name, snapshot.total_equity, snapshot.total_balance, snapshot.position_count
        );

        Ok(())
    }

    /// 同步现货账户余额
    async fn sync_spot(
        &self,
        provider: &Box<dyn AccountProvider>,
        exchange_name: &str,
    ) -> Result<()> {
        // 获取账户快照
        let snapshot = provider.get_account_snapshot("spot").await?;

        // 写入账户快照
        self.repo.insert_account_snapshot(
            &snapshot.exchange,
            &snapshot.market_type,
            snapshot.total_equity,
            snapshot.total_balance,
            snapshot.available_balance,
            snapshot.frozen_balance,
            snapshot.unrealized_pnl,
            snapshot.initial_margin,
            snapshot.maint_margin,
            snapshot.margin_ratio,
            snapshot.position_count,
            snapshot.raw_data.clone(),
        ).await?;

        // 获取资产余额
        let balances = provider.get_asset_balances("spot").await?;
        for balance in &balances {
            self.repo.insert_asset_balance(
                &balance.exchange,
                &balance.market_type,
                &balance.asset,
                balance.total,
                balance.available,
                balance.frozen,
                balance.unrealized_pnl,
                balance.usd_value,
            ).await?;
        }

        debug!(
            "[{}] Spot snapshot: equity={}, balance={}",
            exchange_name, snapshot.total_equity, snapshot.total_balance
        );

        Ok(())
    }

    /// 清理 7 天前的旧快照
    async fn cleanup_old_snapshots(&self) -> Result<()> {
        let count = self.repo.cleanup_old_account_snapshots().await?;

        if count > 0 {
            debug!("Cleaned up {} old snapshots", count);
        }

        Ok(())
    }
}
