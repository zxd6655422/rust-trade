// order/manager.rs
// 订单管理器

use rust_decimal::Decimal;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{error, info, warn};

use crate::exchange::traits::Exchange;
use crate::exchange::types::*;
use crate::risk::{RiskDecision, RiskEngine, RiskAction, StopLossConfig, StopLossManager, StopAction};
use trading_common::backtest::strategy::Signal;

/// 将数量截断到 step_size 精度
fn round_to_step_size(quantity: Decimal, step_size: Decimal) -> Decimal {
    if step_size <= Decimal::ZERO {
        return quantity;
    }
    let remainder = quantity % step_size;
    quantity - remainder
}

/// 订单管理器
pub struct OrderManager {
    exchange: Arc<dyn Exchange>,
    risk_engine: Arc<RiskEngine>,
    stop_loss_manager: Arc<StopLossManager>,
    active_orders: Arc<Mutex<HashMap<String, OrderInfo>>>,
    /// 交易所标识（用于日志和记录）
    exchange_id: String,
    /// 交易模式: "spot" / "futures"
    market_type: String,
    /// 默认杠杆倍数
    leverage: u32,
    /// 默认保证金模式
    margin_type: MarginType,
}

impl OrderManager {
    /// 创建新的订单管理器
    pub fn new(
        exchange: Arc<dyn Exchange>,
        risk_engine: Arc<RiskEngine>,
        stop_loss_config: StopLossConfig,
    ) -> Self {
        Self {
            exchange,
            risk_engine,
            stop_loss_manager: Arc::new(StopLossManager::new(stop_loss_config)),
            active_orders: Arc::new(Mutex::new(HashMap::new())),
            exchange_id: "unknown".to_string(),
            market_type: "futures".to_string(),
            leverage: 10,  // 默认 10x 杠杆
            margin_type: MarginType::Isolated,  // 默认逐仓
        }
    }

    /// 创建带标识的订单管理器
    pub fn with_identity(
        exchange: Arc<dyn Exchange>,
        risk_engine: Arc<RiskEngine>,
        stop_loss_config: StopLossConfig,
        exchange_id: String,
        market_type: String,
        leverage: u32,
    ) -> Self {
        Self {
            exchange,
            risk_engine,
            stop_loss_manager: Arc::new(StopLossManager::new(stop_loss_config)),
            active_orders: Arc::new(Mutex::new(HashMap::new())),
            exchange_id,
            market_type,
            leverage,
            margin_type: MarginType::Isolated,
        }
    }

    /// 获取交易所标识
    pub fn exchange_id(&self) -> &str {
        &self.exchange_id
    }

    /// 获取交易模式
    pub fn market_type(&self) -> &str {
        &self.market_type
    }

    /// 设置杠杆倍数
    pub fn set_leverage(&mut self, leverage: u32) {
        self.leverage = leverage;
    }

    /// 设置保证金模式
    pub fn set_margin_type(&mut self, margin_type: MarginType) {
        self.margin_type = margin_type;
    }

    /// 获取止损止盈管理器
    pub fn stop_loss_manager(&self) -> &Arc<StopLossManager> {
        &self.stop_loss_manager
    }

    /// 执行交易信号
    pub async fn execute_signal(&self, signal: Signal) -> Result<OrderResult, OrderError> {
        // 1. 获取账户信息
        let account = self
            .exchange
            .get_account()
            .await
            .map_err(|e| OrderError::ExchangeError(e.to_string()))?;

        // 2. 构建订单请求
        let mut order_request = self.build_order_request(&signal, &account).await?;

        // 2.5 数量精度校验
        match self.exchange.get_symbol_precision(&order_request.symbol).await {
            Ok(precision) => {
                let original_qty = order_request.quantity;
                order_request.quantity = round_to_step_size(order_request.quantity, precision.step_size);
                if order_request.quantity != original_qty {
                    info!(
                        "Quantity rounded for {}: {} -> {} (step_size={})",
                        order_request.symbol, original_qty, order_request.quantity, precision.step_size
                    );
                }
                // 检查最小数量
                if order_request.quantity < precision.min_quantity {
                    return Err(OrderError::InsufficientBalance(format!(
                        "Order quantity {} below minimum {} for {}",
                        order_request.quantity, precision.min_quantity, order_request.symbol
                    )));
                }
            }
            Err(e) => {
                warn!("Failed to get symbol precision for {}: {}", order_request.symbol, e);
                // 继续执行，不阻断
            }
        }

        // 3. 设置杠杆和保证金模式（仅合约）
        if self.leverage > 0 {
            if let Err(e) = self.exchange.set_leverage(&order_request.symbol, self.leverage).await {
                warn!("Failed to set leverage for {}: {}", order_request.symbol, e);
                // 不阻断交易，使用当前杠杆设置
            }
        }
        if let Err(e) = self.exchange.set_margin_type(&order_request.symbol, self.margin_type.clone()).await {
            warn!("Failed to set margin type for {}: {}", order_request.symbol, e);
            // 不阻断交易，使用当前保证金模式
        }

        // 4. 风控检查
        match self
            .risk_engine
            .check_order(&order_request, &account)
            .await
            .map_err(|e| OrderError::RiskError(e.to_string()))?
        {
            RiskDecision::Allow => {
                info!(
                    "Risk check passed for {} {} {}",
                    order_request.symbol, order_request.side, order_request.quantity
                );
            }
            RiskDecision::Reject(reason) => {
                warn!("Order rejected by risk engine: {}", reason);
                return Err(OrderError::RiskRejected(reason));
            }
            RiskDecision::Modify(quantity) => {
                info!(
                    "Risk engine modified order quantity: {} -> {}",
                    order_request.quantity, quantity
                );
                let mut modified_request = order_request.clone();
                modified_request.quantity = quantity;
                return self.place_and_track_order(modified_request).await;
            }
        }

        // 5. 下单
        self.place_and_track_order(order_request).await
    }

    /// 下单并跟踪
    async fn place_and_track_order(
        &self,
        order: OrderRequest,
    ) -> Result<OrderResult, OrderError> {
        let result = self
            .exchange
            .place_order(order.clone())
            .await
            .map_err(|e| OrderError::ExchangeError(e.to_string()))?;

        // 记录到活动订单
        let order_info = OrderInfo {
            order_id: result.order_id.clone(),
            client_order_id: result.client_order_id.clone(),
            symbol: result.symbol.clone(),
            side: result.side.clone(),
            order_type: result.order_type.clone(),
            status: result.status.clone(),
            quantity: result.quantity,
            filled_quantity: result.filled_quantity,
            remaining_quantity: result.quantity - result.filled_quantity,
            price: result.price,
            stop_price: None,
            time_in_force: TimeInForce::Gtc,
            created_at: result.created_at,
            updated_at: result.updated_at,
        };

        let mut active_orders = self.active_orders.lock().await;
        active_orders.insert(result.order_id.clone(), order_info);

        info!(
            "Order placed: {} {} {} @ {:?} | ID: {}",
            result.symbol, result.side, result.quantity, result.price, result.order_id
        );

        Ok(result)
    }

    /// 处理订单状态更新
    pub async fn handle_order_update(&self, update: OrderUpdate) {
        let mut active_orders = self.active_orders.lock().await;

        if let Some(order) = active_orders.get_mut(&update.order_id) {
            order.status = update.status.clone();
            order.filled_quantity = update.filled_quantity;
            order.remaining_quantity = order.quantity - update.filled_quantity;

            match update.status {
                OrderStatus::Filled => {
                    info!(
                        "Order filled: {} {} @ {:?}",
                        update.symbol, update.filled_quantity, update.avg_price
                    );

                    // 更新风控状态
                    self.risk_engine
                        .record_trade_result(
                            &update.symbol,
                            &update.side.to_string(),
                            update.filled_quantity,
                            update.avg_price.unwrap_or_default(),
                        )
                        .await;

                    // 自动创建止损止盈订单
                    let entry_price = update.avg_price.unwrap_or_default();
                    if entry_price > Decimal::ZERO {
                        self.stop_loss_manager
                            .create_stop_order(
                                &update.symbol,
                                update.side.clone(),
                                update.filled_quantity,
                                entry_price,
                                None, // 使用默认止损
                                None, // 使用默认止盈
                            )
                            .await;
                    }

                    // 从活动订单中移除
                    active_orders.remove(&update.order_id);
                }
                OrderStatus::Canceled | OrderStatus::Rejected => {
                    warn!(
                        "Order {} for {}: {}",
                        update.order_id, update.symbol, update.status
                    );
                    active_orders.remove(&update.order_id);
                }
                OrderStatus::PartiallyFilled => {
                    info!(
                        "Order partially filled: {} {} / {}",
                        update.symbol, update.filled_quantity, update.quantity
                    );
                }
                _ => {}
            }
        } else {
            warn!("Received update for unknown order: {}", update.order_id);
        }
    }

    /// 检查止损止盈
    pub async fn check_stop_orders(&self, symbol: &str, current_price: Decimal) -> Option<StopAction> {
        self.stop_loss_manager.check_price(symbol, current_price).await
    }

    /// 执行止损止盈动作
    pub async fn execute_stop_action(&self, action: StopAction) -> Result<OrderResult, OrderError> {
        let order_request = action.to_order_request();
        info!(
            "Executing stop action: {} {} {}",
            order_request.symbol, order_request.side, order_request.quantity
        );

        // 移除止损止盈订单
        self.stop_loss_manager.remove_stop_order(&order_request.symbol).await;

        // 执行平仓订单
        self.place_and_track_order(order_request).await
    }

    /// 获取活动订单
    pub async fn get_active_orders(&self) -> Vec<OrderInfo> {
        let active_orders = self.active_orders.lock().await;
        active_orders.values().cloned().collect()
    }

    /// 取消所有订单
    pub async fn cancel_all_orders(&self) -> Result<(), OrderError> {
        // 获取所有活动订单的 symbol，逐个取消
        let symbols: Vec<String> = {
            let active_orders = self.active_orders.lock().await;
            active_orders.values().map(|o| o.symbol.clone()).collect()
        };

        // 去重后逐个取消
        let mut cancelled_symbols = std::collections::HashSet::new();
        for symbol in &symbols {
            if cancelled_symbols.contains(symbol) {
                continue;
            }
            cancelled_symbols.insert(symbol.clone());
            if let Err(e) = self.exchange.cancel_all_orders(Some(symbol)).await {
                warn!("Failed to cancel orders for {}: {}", symbol, e);
            }
        }

        // 清空本地活动订单记录
        let mut active_orders = self.active_orders.lock().await;
        active_orders.clear();

        info!("All orders cancelled ({} symbols)", cancelled_symbols.len());
        Ok(())
    }

    /// 紧急停止
    pub async fn emergency_stop(&self) -> Result<(), OrderError> {
        warn!("Emergency stop triggered!");

        // 1. 取消所有订单
        self.cancel_all_orders().await?;

        // 2. 触发风控熔断
        self.risk_engine
            .trigger_circuit_breaker("Emergency stop")
            .await;

        Ok(())
    }

    /// 执行风控动作（持仓风控触发时调用）
    pub async fn execute_risk_action(
        &self,
        action: crate::risk::RiskAction,
    ) -> Result<(), OrderError> {
        use crate::risk::RiskAction;

        match action {
            RiskAction::ForceClose {
                symbol,
                quantity,
                reason,
            } => {
                warn!(
                    "⚠️ Risk force close: {} {} — {}",
                    symbol, quantity, reason
                );

                // 从交易所获取实时持仓确定平仓方向
                let position = self.exchange.get_position(&symbol).await
                    .map_err(|e| OrderError::ExchangeError(e.to_string()))?;
                let close_side = match position.side {
                    crate::exchange::types::PositionSide::Long => OrderSide::Sell,
                    crate::exchange::types::PositionSide::Short => OrderSide::Buy,
                    _ => OrderSide::Sell, // 默认卖出
                };

                let order_request = OrderRequest {
                    symbol: symbol.clone(),
                    side: close_side,
                    order_type: OrderType::Market,
                    quantity,
                    price: None,
                    stop_price: None,
                    time_in_force: Some(TimeInForce::Ioc),
                    client_order_id: None,
                };

                match self.place_and_track_order(order_request).await {
                    Ok(result) => {
                        info!("Risk force close order placed: {}", result.order_id);
                        // 移除止损止盈
                        self.stop_loss_manager.remove_stop_order(&symbol).await;
                    }
                    Err(e) => {
                        error!("Risk force close failed for {}: {}", symbol, e);
                        return Err(e);
                    }
                }
            }
            RiskAction::ReducePosition {
                symbol,
                current_quantity,
                target_quantity,
                reason,
            } => {
                warn!(
                    "⚠️ Risk reduce position: {} {} → {} — {}",
                    symbol, current_quantity, target_quantity, reason
                );

                let reduce_qty = current_quantity - target_quantity;
                if reduce_qty <= Decimal::ZERO {
                    return Ok(());
                }

                // 从交易所获取实时持仓确定减仓方向
                let position = self.exchange.get_position(&symbol).await
                    .map_err(|e| OrderError::ExchangeError(e.to_string()))?;
                let close_side = match position.side {
                    crate::exchange::types::PositionSide::Long => OrderSide::Sell,
                    crate::exchange::types::PositionSide::Short => OrderSide::Buy,
                    _ => OrderSide::Sell,
                };

                let order_request = OrderRequest {
                    symbol: symbol.clone(),
                    side: close_side,
                    order_type: OrderType::Market,
                    quantity: reduce_qty,
                    price: None,
                    stop_price: None,
                    time_in_force: Some(TimeInForce::Ioc),
                    client_order_id: None,
                };

                match self.place_and_track_order(order_request).await {
                    Ok(result) => {
                        info!("Risk reduce order placed: {}", result.order_id);
                    }
                    Err(e) => {
                        error!("Risk reduce failed for {}: {}", symbol, e);
                        return Err(e);
                    }
                }
            }
            RiskAction::CloseAll { reason } => {
                warn!("⚠️ Risk close ALL positions — {}", reason);

                // 获取所有持仓并逐个平仓
                let positions = self.exchange.get_positions().await.unwrap_or_default();
                for pos in positions {
                    if pos.quantity <= Decimal::ZERO {
                        continue;
                    }
                    let close_side = match pos.side {
                        crate::exchange::types::PositionSide::Long => OrderSide::Sell,
                        crate::exchange::types::PositionSide::Short => OrderSide::Buy,
                        _ => OrderSide::Sell,
                    };

                    let order_request = OrderRequest {
                        symbol: pos.symbol.clone(),
                        side: close_side,
                        order_type: OrderType::Market,
                        quantity: pos.quantity,
                        price: None,
                        stop_price: None,
                        time_in_force: Some(TimeInForce::Ioc),
                        client_order_id: None,
                    };

                    match self.place_and_track_order(order_request).await {
                        Ok(result) => {
                            info!(
                                "Risk close all — {} order placed: {}",
                                pos.symbol, result.order_id
                            );
                        }
                        Err(e) => {
                            error!("Risk close all failed for {}: {}", pos.symbol, e);
                        }
                    }
                }

                // 清除所有止损止盈
                // （CloseAll 后持仓已清，止损止盈也应清除）

                // 触发熔断，暂停交易
                self.risk_engine
                    .trigger_circuit_breaker(&format!("CloseAll: {}", reason))
                    .await;
            }
        }

        Ok(())
    }

    /// 构建订单请求
    ///
    /// 根据 market_type 区分校验逻辑：
    /// - futures: 查询交易所合约持仓（get_position）
    /// - spot: 查询现货余额（account.balances）
    async fn build_order_request(
        &self,
        signal: &Signal,
        account: &AccountInfo,
    ) -> Result<OrderRequest, OrderError> {
        match signal {
            Signal::Buy {
                symbol,
                quantity,
                entry_price,
            } => {
                // 检查 USDT 余额是否充足
                let usdt_balance = account
                    .balances
                    .iter()
                    .find(|b| b.asset == "USDT")
                    .map(|b| b.free)
                    .unwrap_or(Decimal::ZERO);

                let estimated_value = quantity * entry_price;
                if estimated_value > usdt_balance {
                    return Err(OrderError::InsufficientBalance(format!(
                        "Required: {} USDT, Available: {} USDT",
                        estimated_value, usdt_balance
                    )));
                }

                Ok(OrderRequest {
                    symbol: symbol.clone(),
                    side: OrderSide::Buy,
                    order_type: OrderType::Market,
                    quantity: *quantity,
                    price: Some(*entry_price),
                    stop_price: None,
                    time_in_force: Some(TimeInForce::Ioc),
                    client_order_id: None,
                })
            }
            Signal::Sell {
                symbol,
                quantity,
                entry_price,
            } => {
                // 根据交易模式选择不同的持仓校验方式
                if self.market_type == "futures" {
                    // 合约模式：从交易所查询合约持仓
                    let position = self.exchange.get_position(symbol).await
                        .map_err(|e| OrderError::ExchangeError(
                            format!("Failed to get futures position for {}: {}", symbol, e)
                        ))?;

                    if position.quantity < *quantity {
                        return Err(OrderError::InsufficientPosition(format!(
                            "[futures] Sell {} {}: need {}, have {}",
                            symbol, quantity, quantity, position.quantity
                        )));
                    }
                } else {
                    // 现货模式：从账户余额查询 base asset
                    let base_asset = Self::extract_base_asset(symbol);
                    let balance = account
                        .balances
                        .iter()
                        .find(|b| b.asset == base_asset)
                        .map(|b| b.free)
                        .unwrap_or(Decimal::ZERO);

                    if *quantity > balance {
                        return Err(OrderError::InsufficientPosition(format!(
                            "[spot] Sell {} {}: need {} {}, have {} {}",
                            symbol, quantity, quantity, base_asset, balance, base_asset
                        )));
                    }
                }

                Ok(OrderRequest {
                    symbol: symbol.clone(),
                    side: OrderSide::Sell,
                    order_type: OrderType::Market,
                    quantity: *quantity,
                    price: Some(*entry_price),
                    stop_price: None,
                    time_in_force: Some(TimeInForce::Ioc),
                    client_order_id: None,
                })
            }
            Signal::Hold => Err(OrderError::InvalidSignal("Cannot execute Hold signal".to_string())),
        }
    }

    /// 从 symbol 提取 base asset（如 BTCUSDT → BTC）
    fn extract_base_asset(symbol: &str) -> &str {
        if symbol.ends_with("USDT") {
            symbol.strip_suffix("USDT").unwrap_or(symbol)
        } else if symbol.ends_with("BUSD") {
            symbol.strip_suffix("BUSD").unwrap_or(symbol)
        } else if symbol.ends_with("USDC") {
            symbol.strip_suffix("USDC").unwrap_or(symbol)
        } else if symbol.ends_with("-USDT") {
            // OKX 格式: BTC-USDT → BTC
            symbol.strip_suffix("-USDT").unwrap_or(symbol)
        } else {
            symbol
        }
    }
}

/// 订单错误
#[derive(Debug, thiserror::Error)]
pub enum OrderError {
    #[error("Exchange error: {0}")]
    ExchangeError(String),

    #[error("Risk rejected: {0}")]
    RiskRejected(String),

    #[error("Risk error: {0}")]
    RiskError(String),

    #[error("Insufficient balance: {0}")]
    InsufficientBalance(String),

    #[error("Insufficient position: {0}")]
    InsufficientPosition(String),

    #[error("Invalid signal: {0}")]
    InvalidSignal(String),

    #[error("Order not found: {0}")]
    OrderNotFound(String),

    #[error("Unknown error: {0}")]
    Unknown(String),
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::exchange::adapters::mock_exchange::{MockExchange, MockExchangeConfig};
    use crate::risk::{RiskConfig, StopLossConfig};
    use rust_decimal::Decimal;
    use std::str::FromStr;

    fn create_risk_config() -> RiskConfig {
        RiskConfig {
            max_position_size: Decimal::from(50000),
            max_order_size: Decimal::from(1),
            stop_loss_pct: Decimal::from_str("0.02").unwrap(),
            take_profit_pct: Decimal::from_str("0.04").unwrap(),
            max_daily_loss: Decimal::from(5000),
            max_drawdown_pct: Decimal::from_str("0.15").unwrap(),
            max_exposure_pct: Decimal::from_str("0.8").unwrap(),
            kelly_fraction: Decimal::from_str("0.25").unwrap(),
            volatility_lookback: 20,
            volatility_target: Decimal::from_str("0.15").unwrap(),
            circuit_breaker_cooldown: 3600,
            black_swan_threshold: Decimal::from_str("0.05").unwrap(),
        }
    }

    async fn setup_exchange_and_manager() -> (Arc<MockExchange>, OrderManager) {
        let config = MockExchangeConfig {
            initial_balance: Decimal::from(100000),
            ..Default::default()
        };
        let exchange = Arc::new(MockExchange::new(config).unwrap());
        exchange.set_price("BTCUSDT", Decimal::from(50000)).await;
        exchange.set_price("ETHUSDT", Decimal::from(3000)).await;

        let risk_engine = Arc::new(RiskEngine::new(create_risk_config()));
        let stop_config = StopLossConfig::default();
        let manager = OrderManager::new(exchange.clone(), risk_engine, stop_config);
        (exchange, manager)
    }

    // ========== 基础功能测试 ==========

    #[tokio::test]
    async fn test_order_manager_initialization() {
        let (_exchange, manager) = setup_exchange_and_manager().await;

        let orders = manager.get_active_orders().await;
        assert_eq!(orders.len(), 0);
    }

    #[tokio::test]
    async fn test_execute_buy_signal() {
        let (_exchange, manager) = setup_exchange_and_manager().await;

        let signal = Signal::Buy {
            symbol: "BTCUSDT".to_string(),
            quantity: Decimal::from_str("0.1").unwrap(),
            entry_price: Decimal::from(50000),
        };

        let result = manager.execute_signal(signal).await;
        assert!(result.is_ok());

        let order = result.unwrap();
        assert_eq!(order.symbol, "BTCUSDT");
        assert_eq!(order.side, OrderSide::Buy);
        assert_eq!(order.status, OrderStatus::Filled);
    }

    #[tokio::test]
    async fn test_execute_sell_signal() {
        let (_exchange, manager) = setup_exchange_and_manager().await;

        // 先买入 0.1 BTC
        let buy_signal = Signal::Buy {
            symbol: "BTCUSDT".to_string(),
            quantity: Decimal::from_str("0.1").unwrap(),
        };
        manager.execute_signal(buy_signal).await.unwrap();

        // 尝试卖出较少的数量（避免精度问题）
        let sell_signal = Signal::Sell {
            symbol: "BTCUSDT".to_string(),
            quantity: Decimal::from_str("0.02").unwrap(),
            entry_price: Decimal::from(50000),
        };
        let result = manager.execute_signal(sell_signal).await;
        assert!(result.is_ok(), "Sell signal failed: {:?}", result.err());
    }

    #[tokio::test]
    async fn test_execute_hold_signal_fails() {
        let (_exchange, manager) = setup_exchange_and_manager().await;

        let signal = Signal::Hold;
        let result = manager.execute_signal(signal).await;
        assert!(result.is_err());
    }

    // ========== 订单更新处理测试 ==========

    #[tokio::test]
    async fn test_handle_order_update_filled() {
        let (_exchange, manager) = setup_exchange_and_manager().await;

        // 执行买入
        let signal = Signal::Buy {
            symbol: "BTCUSDT".to_string(),
            quantity: Decimal::from_str("0.1").unwrap(),
            entry_price: Decimal::from(50000),
        };
        let result = manager.execute_signal(signal).await.unwrap();

        // 模拟订单更新
        let update = OrderUpdate {
            order_id: result.order_id.clone(),
            client_order_id: None,
            symbol: "BTCUSDT".to_string(),
            side: OrderSide::Buy,
            order_type: OrderType::Market,
            status: OrderStatus::Filled,
            quantity: Decimal::from_str("0.1").unwrap(),
            filled_quantity: Decimal::from_str("0.1").unwrap(),
            price: Some(Decimal::from(50000)),
            avg_price: Some(Decimal::from(50000)),
            commission: Some(Decimal::from(5)),
            commission_asset: Some("USDT".to_string()),
            timestamp: chrono::Utc::now(),
        };

        manager.handle_order_update(update).await;

        // 订单应该从活动列表中移除
        let active = manager.get_active_orders().await;
        assert_eq!(active.len(), 0);
    }

    // ========== 止损止盈测试 ==========

    #[tokio::test]
    async fn test_stop_loss_integration() {
        let (_exchange, manager) = setup_exchange_and_manager().await;

        // 执行买入
        let signal = Signal::Buy {
            symbol: "BTCUSDT".to_string(),
            quantity: Decimal::from_str("0.1").unwrap(),
            entry_price: Decimal::from(50000),
        };
        let result = manager.execute_signal(signal).await.unwrap();

        // 模拟订单成交，触发止损创建
        let update = OrderUpdate {
            order_id: result.order_id.clone(),
            client_order_id: None,
            symbol: "BTCUSDT".to_string(),
            side: OrderSide::Buy,
            order_type: OrderType::Market,
            status: OrderStatus::Filled,
            quantity: Decimal::from_str("0.1").unwrap(),
            filled_quantity: Decimal::from_str("0.1").unwrap(),
            price: Some(Decimal::from(50000)),
            avg_price: Some(Decimal::from(50000)),
            commission: Some(Decimal::from(5)),
            commission_asset: Some("USDT".to_string()),
            timestamp: chrono::Utc::now(),
        };
        manager.handle_order_update(update).await;

        // 检查止损是否创建
        let stop_order = manager.stop_loss_manager().get_stop_order("BTCUSDT").await;
        assert!(stop_order.is_some());
    }

    // ========== 取消所有订单测试 ==========

    #[tokio::test]
    async fn test_cancel_all_orders() {
        let (_exchange, manager) = setup_exchange_and_manager().await;

        let result = manager.cancel_all_orders().await;
        assert!(result.is_ok());
    }

    // ========== 紧急停止测试 ==========

    #[tokio::test]
    async fn test_emergency_stop() {
        let (_exchange, manager) = setup_exchange_and_manager().await;

        let result = manager.emergency_stop().await;
        assert!(result.is_ok());
    }

    // ========== 余额不足测试 ==========

    #[tokio::test]
    async fn test_insufficient_balance() {
        let config = MockExchangeConfig {
            initial_balance: Decimal::from(100), // 很少的余额
            ..Default::default()
        };
        let exchange = Arc::new(MockExchange::new(config).unwrap());
        exchange.set_price("BTCUSDT", Decimal::from(50000)).await;

        let risk_engine = Arc::new(RiskEngine::new(create_risk_config()));
        let stop_config = StopLossConfig::default();
        let manager = OrderManager::new(exchange, risk_engine, stop_config);

        // 尝试买入超过余额的订单
        let signal = Signal::Buy {
            symbol: "BTCUSDT".to_string(),
            quantity: Decimal::from(1), // 价值 50000，超过余额 100
            entry_price: Decimal::from(50000),
        };
        let result = manager.execute_signal(signal).await;
        assert!(result.is_err());
    }
}
