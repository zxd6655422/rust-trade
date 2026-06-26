// order/manager.rs
// 订单管理器

use rust_decimal::Decimal;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{info, warn};

use crate::exchange::traits::Exchange;
use crate::exchange::types::*;
use crate::risk::{RiskDecision, RiskEngine};
use trading_common::backtest::strategy::Signal;

/// 订单管理器
pub struct OrderManager {
    exchange: Arc<dyn Exchange>,
    risk_engine: Arc<RiskEngine>,
    active_orders: Arc<Mutex<HashMap<String, OrderInfo>>>,
}

impl OrderManager {
    /// 创建新的订单管理器
    pub fn new(
        exchange: Arc<dyn Exchange>,
        risk_engine: Arc<RiskEngine>,
    ) -> Self {
        Self {
            exchange,
            risk_engine,
            active_orders: Arc::new(Mutex::new(HashMap::new())),
        }
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
        let order_request = self.build_order_request(&signal, &account)?;

        // 3. 风控检查
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
                // 创建修改后的订单
                let mut modified_request = order_request.clone();
                modified_request.quantity = quantity;
                return self.place_and_track_order(modified_request).await;
            }
        }

        // 4. 下单
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

    /// 获取活动订单
    pub async fn get_active_orders(&self) -> Vec<OrderInfo> {
        let active_orders = self.active_orders.lock().await;
        active_orders.values().cloned().collect()
    }

    /// 取消所有订单
    pub async fn cancel_all_orders(&self) -> Result<(), OrderError> {
        self.exchange
            .cancel_all_orders(None)
            .await
            .map_err(|e| OrderError::ExchangeError(e.to_string()))?;

        let mut active_orders = self.active_orders.lock().await;
        active_orders.clear();

        info!("All orders cancelled");
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

    /// 构建订单请求
    fn build_order_request(
        &self,
        signal: &Signal,
        account: &AccountInfo,
    ) -> Result<OrderRequest, OrderError> {
        match signal {
            Signal::Buy {
                symbol,
                quantity,
            } => {
                // 检查余额是否充足
                let usdt_balance = account
                    .balances
                    .iter()
                    .find(|b| b.asset == "USDT")
                    .map(|b| b.free)
                    .unwrap_or(Decimal::ZERO);

                // 估算订单价值 (需要当前价格)
                // 这里简化处理，使用配置的最大订单大小
                let estimated_value = quantity * rust_decimal::Decimal::from(50000); // 假设 BTC 价格
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
                    price: None,
                    stop_price: None,
                    time_in_force: Some(TimeInForce::Ioc),
                    client_order_id: None,
                })
            }
            Signal::Sell {
                symbol,
                quantity,
            } => {
                // 检查持仓是否充足
                let base_asset = symbol.replace("USDT", "").replace("BUSD", "");
                let balance = account
                    .balances
                    .iter()
                    .find(|b| b.asset == base_asset)
                    .map(|b| b.free)
                    .unwrap_or(Decimal::ZERO);

                if *quantity > balance {
                    return Err(OrderError::InsufficientPosition(format!(
                        "Required: {} {}, Available: {} {}",
                        quantity, base_asset, balance, base_asset
                    )));
                }

                Ok(OrderRequest {
                    symbol: symbol.clone(),
                    side: OrderSide::Sell,
                    order_type: OrderType::Market,
                    quantity: *quantity,
                    price: None,
                    stop_price: None,
                    time_in_force: Some(TimeInForce::Ioc),
                    client_order_id: None,
                })
            }
            Signal::Hold => Err(OrderError::InvalidSignal("Cannot execute Hold signal".to_string())),
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
