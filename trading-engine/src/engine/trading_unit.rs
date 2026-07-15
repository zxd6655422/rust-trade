// engine/trading_unit.rs
// 交易单元：一个交易所+交易模式的独立交易实例
//
// 每个 TradingUnit 拥有独立的：
// - Exchange 适配器
// - OrderManager（订单执行）
// - StopLossManager（止损止盈）
// - PortfolioManager（持仓管理）
//
// 共享 RiskEngine（跨所有 TradingUnit 聚合风控）

use std::sync::Arc;
use tracing::info;

use crate::config::ExchangeInstanceConfig;
use crate::exchange::traits::Exchange;
use crate::exchange::ExchangeFactory;
use crate::order::OrderManager;
use crate::portfolio::PortfolioManager;
use crate::risk::{RiskEngine, StopLossConfig, StopLossManager};
use crate::storage::{OrderRepository, PositionRepository, RedisCache, StopOrderRepository};

/// 一个交易所+交易模式的独立交易单元
///
/// 交易对由策略服务通过信号控制，TradingUnit 不预设交易对
pub struct TradingUnit {
    /// 实例唯一标识，如 "binance-futures"
    pub id: String,
    /// 交易所 ID，如 "binance"
    pub exchange_id: String,
    /// 交易模式: "spot" / "futures"
    pub market_type: String,
    /// 交易所适配器
    pub exchange: Arc<dyn Exchange>,
    /// 订单管理器
    pub order_manager: Arc<OrderManager>,
    /// 止损止盈管理器
    pub stop_loss_manager: Arc<StopLossManager>,
    /// 持仓管理器
    pub portfolio_manager: Arc<PortfolioManager>,
    /// 杠杆倍数
    pub leverage: u32,
    /// 是否启用
    pub enabled: bool,
}

impl TradingUnit {
    /// 从配置创建交易单元
    pub fn from_config(
        config: &ExchangeInstanceConfig,
        risk_engine: Arc<RiskEngine>,
        position_repo: Arc<PositionRepository>,
        cache: Arc<RedisCache>,
        stop_order_repo: Option<Arc<StopOrderRepository>>,
        order_repo: Option<Arc<OrderRepository>>,
    ) -> Result<Self, String> {
        // 从环境变量获取 API Key
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
        ).map_err(|e| format!("Failed to create exchange {}: {}", config.id, e))?;

        let exchange: Arc<dyn Exchange> = Arc::from(exchange);

        // 创建止损止盈配置
        // 注意：这里使用默认值，实际应从 RiskConfig 获取
        let stop_loss_config = StopLossConfig::default();

        // 创建订单管理器
        let mut order_manager = OrderManager::with_identity(
            exchange.clone(),
            risk_engine.clone(),
            stop_loss_config.clone(),
            config.id.clone(),
            config.market_type.clone(),
            config.leverage,
        );

        // 设置止损止盈仓储（启用 DB 持久化）
        if let Some(repo) = stop_order_repo {
            order_manager.set_stop_order_repo(repo);
        }

        // 设置订单仓储（启用订单持久化）
        if let Some(repo) = order_repo {
            order_manager.set_order_repo(repo);
        }

        let order_manager = Arc::new(order_manager);

        // 从 OrderManager 获取 StopLossManager
        let stop_loss_manager = order_manager.stop_loss_manager().clone();

        // 创建持仓管理器
        let portfolio_manager = Arc::new(PortfolioManager::new(
            exchange.clone(),
            position_repo.clone(),
            cache.clone(),
            risk_engine.clone(),
        ));

        info!(
            "✅ TradingUnit created: {} ({} {}, leverage={}x)",
            config.id, config.exchange_id, config.market_type, config.leverage
        );

        Ok(Self {
            id: config.id.clone(),
            exchange_id: config.exchange_id.clone(),
            market_type: config.market_type.clone(),
            exchange,
            order_manager,
            stop_loss_manager,
            portfolio_manager,
            leverage: config.leverage,
            enabled: config.enabled,
        })
    }
}
