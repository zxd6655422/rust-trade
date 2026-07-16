// service/account_poller.rs
//
// 账户信息轮询服务
// 独立于自动交易，即使不开自动交易也能采集账户信息
// 用交易所返回的 uid 标识用户，支持跨 API Key 的历史数据连贯

use std::collections::HashMap;
use std::sync::Arc;
use tokio::time::{interval, Duration};
use tracing::{error, info, warn};

use crate::config::ExchangeInstanceConfig;
use crate::exchange::ExchangeFactory;
use trading_common::data::account_repository::AccountRepository;
use trading_common::data::account_types::AccountProvider as AccountProviderTrait;

/// 账户轮询配置
#[derive(Debug, Clone)]
pub struct AccountPollerConfig {
    /// 轮询间隔（秒），默认 60
    pub poll_interval_secs: u64,
}

impl Default for AccountPollerConfig {
    fn default() -> Self {
        Self {
            poll_interval_secs: 60,
        }
    }
}

/// 账户信息轮询服务
///
/// 功能：
/// 1. 定时从交易所获取账户快照、资产余额、持仓信息
/// 2. 用交易所返回的 uid 标识用户（Binance: uid, OKX: uid）
/// 3. 保存到数据库，供 trading-core API 查询
/// 4. 独立运行，不依赖自动交易开关
pub struct AccountPoller {
    configs: Vec<ExchangeInstanceConfig>,
    account_repo: Arc<AccountRepository>,
    config: AccountPollerConfig,
    /// 缓存的 uid (instance_id -> uid)
    uid_cache: Arc<tokio::sync::RwLock<HashMap<String, String>>>,
}

impl AccountPoller {
    pub fn new(
        configs: Vec<ExchangeInstanceConfig>,
        account_repo: Arc<AccountRepository>,
        config: AccountPollerConfig,
    ) -> Self {
        Self {
            configs,
            account_repo,
            config,
            uid_cache: Arc::new(tokio::sync::RwLock::new(HashMap::new())),
        }
    }

    /// 启动账户轮询服务（无限循环）
    pub async fn start(&self) {
        info!("📊 Account Poller starting (interval: {}s)", self.config.poll_interval_secs);

        // 筛选有 API Key 的配置
        let valid_configs: Vec<&ExchangeInstanceConfig> = self.configs
            .iter()
            .filter(|c| {
                let has_key = c.api_key().is_ok();
                if !has_key {
                    warn!("⚠️ Skipping {} - API key not configured", c.id);
                }
                has_key
            })
            .collect();

        if valid_configs.is_empty() {
            warn!("⚠️ No exchange configs with API keys found. Account poller idle.");
            // 仍然启动循环，等待配置变化
        }

        info!("📋 Account Poller monitoring {} exchanges", valid_configs.len());

        let mut poll_timer = interval(Duration::from_secs(self.config.poll_interval_secs));

        loop {
            poll_timer.tick().await;

            for config in &valid_configs {
                if let Err(e) = self.poll_exchange(config).await {
                    error!("❌ Failed to poll account for {}: {}", config.id, e);
                }
            }
        }
    }

    /// 轮询单个交易所的账户信息
    async fn poll_exchange(&self, config: &ExchangeInstanceConfig) -> Result<(), String> {
        let api_key = config.api_key()?;
        let api_secret = config.api_secret()?;
        let passphrase = config.passphrase();

        // 创建交易所适配器
        let exchange = ExchangeFactory::create(
            &config.exchange_id,
            config.testnet,
            &api_key,
            &api_secret,
            passphrase.as_deref(),
        ).map_err(|e| format!("Failed to create exchange: {}", e))?;

        // 获取 AccountProvider 引用
        let account_provider = exchange.as_account_provider();

        // 获取 uid（优先使用缓存）
        let uid = match self.get_or_fetch_uid(config, account_provider).await {
            Ok(uid) => uid,
            Err(e) => {
                error!("❌ Failed to get uid for {}: {}", config.id, e);
                error!("   请检查:");
                error!("   1. API Key 是否正确配置");
                error!("   2. API Key 是否有读取权限");
                error!("   3. 服务器 IP 是否在 API Key 白名单中");
                error!("   4. 是否使用了正确的网络 (testnet/mainnet)");
                return Err(e);
            }
        };

        // 获取账户快照
        match account_provider.get_account_snapshot(&config.market_type).await {
            Ok(mut snapshot) => {
                snapshot.uid = Some(uid.clone());
                if let Err(e) = self.account_repo.save_snapshot(&snapshot, &uid).await {
                    error!("Failed to save snapshot for {}: {}", config.id, e);
                }
            }
            Err(e) => {
                error!("❌ Failed to get account snapshot for {}: {}", config.id, e);
                self.log_api_error_hint(config, &e);
            }
        }

        // 获取资产余额
        match account_provider.get_asset_balances(&config.market_type).await {
            Ok(mut balances) => {
                for b in &mut balances {
                    b.uid = Some(uid.clone());
                }
                if let Err(e) = self.account_repo.save_asset_balances(&balances, &uid).await {
                    error!("Failed to save balances for {}: {}", config.id, e);
                }
            }
            Err(e) => {
                error!("❌ Failed to get asset balances for {}: {}", config.id, e);
                self.log_api_error_hint(config, &e);
            }
        }

        // 获取持仓（主要是合约）
        tracing::info!("[AccountPoller] {} market_type='{}'", config.id, config.market_type);
        if config.market_type == "futures" || config.market_type == "swap" {
            tracing::info!("[AccountPoller] 开始获取 {} 持仓...", config.id);
            match account_provider.get_positions().await {
                Ok(mut positions) => {
                    tracing::info!("[AccountPoller] {} 获取到 {} 个持仓", config.id, positions.len());
                    for p in &mut positions {
                        p.uid = Some(uid.clone());
                    }
                    if positions.is_empty() {
                        tracing::warn!("[AccountPoller] {} 持仓为空!", config.id);
                    }
                    match self.account_repo.save_positions(&positions, &uid).await {
                        Ok(_) => tracing::info!("[AccountPoller] {} 持仓保存成功", config.id),
                        Err(e) => error!("❌ [AccountPoller] {} 持仓保存失败: {}", config.id, e),
                    }
                }
                Err(e) => {
                    error!("❌ [AccountPoller] {} 获取持仓失败: {}", config.id, e);
                    self.log_api_error_hint(config, &e);
                }
            }
        } else {
            tracing::info!("[AccountPoller] {} 跳过持仓 (market_type={})", config.id, config.market_type);
        }

        info!("✅ Account poll completed for {} (uid={})", config.id, uid);
        Ok(())
    }

    /// 输出 API 错误的排查提示
    fn log_api_error_hint(&self, config: &ExchangeInstanceConfig, error: &trading_common::data::types::DataError) {
        let error_msg = error.to_string().to_lowercase();

        if error_msg.contains("-2015") || error_msg.contains("permission") || error_msg.contains("权限") {
            error!("   💡 提示: API Key 权限不足");
            error!("      请在 {} 后台检查 API Key 权限设置", config.exchange_id);
            error!("      只读权限即可查询账户信息，无需交易权限");
            if config.exchange_id == "binance" || config.exchange_id == "binance-spot" {
                error!("      Binance: API 管理 → 编辑 API Key → 勾选「读取」权限");
            } else if config.exchange_id == "okx" || config.exchange_id == "okx-spot" {
                error!("      OKX: API 管理 → 编辑 API Key → 权限选择「读取」");
            }
        } else if error_msg.contains("-2016") || error_msg.contains("ip") {
            error!("   💡 提示: IP 白名单问题");
            error!("      请将当前服务器 IP 添加到 API Key 白名单");
            error!("      或者在交易所后台将 IP 白名单设置为空（不限制 IP）");
        } else if error_msg.contains("-2014") || error_msg.contains("authentication") || error_msg.contains("invalid") {
            error!("   💡 提示: API Key 配置问题");
            let env_prefix = config.id.to_uppercase().replace("-", "_");
            error!("      请检查环境变量 {}_API_KEY 和 {}_API_SECRET", env_prefix, env_prefix);
        } else if error_msg.contains("-1021") || error_msg.contains("timestamp") {
            error!("   💡 提示: 时间戳问题");
            error!("      请检查服务器时间是否准确，或增大 recvWindow");
        } else if error_msg.contains("network") || error_msg.contains("timeout") || error_msg.contains("连接") {
            error!("   💡 提示: 网络连接问题");
            error!("      请检查服务器是否能访问交易所 API");
            if config.testnet {
                error!("      当前使用测试网，请确认测试网地址可访问");
            }
        }
    }

    /// 获取或缓存 uid
    ///
    /// 首次调用时从交易所获取 uid，后续使用缓存
    /// Binance Spot: 从 /api/v3/account 响应中提取 uid
    /// Binance Futures: 与 Spot 共用 uid（同一 Binance 账户）
    /// OKX: 从 /api/v5/account/config 提取 uid
    async fn get_or_fetch_uid(
        &self,
        config: &ExchangeInstanceConfig,
        exchange: &dyn AccountProviderTrait,
    ) -> Result<String, String> {
        // 检查缓存
        {
            let cache = self.uid_cache.read().await;
            if let Some(uid) = cache.get(&config.id) {
                return Ok(uid.clone());
            }
        }

        // 首次获取 uid
        let snapshot = exchange.get_account_snapshot(&config.market_type)
            .await
            .map_err(|e| format!("Failed to get uid: {}", e))?;

        let uid = snapshot.uid.unwrap_or_else(|| {
            // 如果没有 uid，使用 instance_id 作为降级方案
            warn!("No uid returned for {}, using instance_id as fallback", config.id);
            config.id.clone()
        });

        // 缓存 uid
        {
            let mut cache = self.uid_cache.write().await;
            cache.insert(config.id.clone(), uid.clone());
        }

        info!("🔑 Got uid={} for {}", uid, config.id);
        Ok(uid)
    }
}
