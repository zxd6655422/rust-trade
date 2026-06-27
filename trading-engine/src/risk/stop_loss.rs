// risk/stop_loss.rs
// 止损止盈管理

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{info, warn};

use crate::exchange::types::{OrderRequest, OrderSide, OrderType, TimeInForce};

/// 止损止盈配置
#[derive(Debug, Clone)]
pub struct StopLossConfig {
    /// 默认止损百分比 (0.02 = 2%)
    pub default_stop_loss_pct: Decimal,
    /// 默认止盈百分比 (0.04 = 4%)
    pub default_take_profit_pct: Decimal,
    /// 是否启用追踪止损
    pub enable_trailing_stop: bool,
    /// 追踪止损回撤百分比
    pub trailing_stop_pct: Decimal,
}

impl Default for StopLossConfig {
    fn default() -> Self {
        Self {
            default_stop_loss_pct: Decimal::from(2) / Decimal::from(100),
            default_take_profit_pct: Decimal::from(4) / Decimal::from(100),
            enable_trailing_stop: false,
            trailing_stop_pct: Decimal::from(1) / Decimal::from(100),
        }
    }
}

/// 止损止盈订单信息
#[derive(Debug, Clone)]
pub struct StopOrder {
    /// 原始持仓的交易对
    pub symbol: String,
    /// 持仓方向 (多/空)
    pub side: OrderSide,
    /// 持仓数量
    pub quantity: Decimal,
    /// 入场价格
    pub entry_price: Decimal,
    /// 止损价格
    pub stop_loss_price: Decimal,
    /// 止盈价格
    pub take_profit_price: Decimal,
    /// 最高价 (用于追踪止损)
    pub highest_price: Decimal,
    /// 最低价 (用于追踪止损)
    pub lowest_price: Decimal,
    /// 创建时间
    pub created_at: DateTime<Utc>,
    /// 是否已触发
    pub triggered: bool,
}

/// 止损止盈管理器
pub struct StopLossManager {
    config: StopLossConfig,
    /// 活动的止损止盈订单
    stop_orders: Arc<Mutex<HashMap<String, StopOrder>>>,
}

impl StopLossManager {
    /// 创建新的止损止盈管理器
    pub fn new(config: StopLossConfig) -> Self {
        Self {
            config,
            stop_orders: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// 为持仓创建止损止盈订单
    pub async fn create_stop_order(
        &self,
        symbol: &str,
        side: OrderSide,
        quantity: Decimal,
        entry_price: Decimal,
        custom_stop_loss: Option<Decimal>,
        custom_take_profit: Option<Decimal>,
    ) -> StopOrder {
        // 计算止损价格
        let stop_loss_price = match custom_stop_loss {
            Some(price) => price,
            None => match side {
                OrderSide::Buy => entry_price * (Decimal::ONE - self.config.default_stop_loss_pct),
                OrderSide::Sell => {
                    entry_price * (Decimal::ONE + self.config.default_stop_loss_pct)
                }
            },
        };

        // 计算止盈价格
        let take_profit_price = match custom_take_profit {
            Some(price) => price,
            None => match side {
                OrderSide::Buy => {
                    entry_price * (Decimal::ONE + self.config.default_take_profit_pct)
                }
                OrderSide::Sell => {
                    entry_price * (Decimal::ONE - self.config.default_take_profit_pct)
                }
            },
        };

        let stop_order = StopOrder {
            symbol: symbol.to_string(),
            side: side.clone(),
            quantity,
            entry_price,
            stop_loss_price,
            take_profit_price,
            highest_price: entry_price,
            lowest_price: entry_price,
            created_at: Utc::now(),
            triggered: false,
        };

        // 存储止损止盈订单
        let mut stop_orders = self.stop_orders.lock().await;
        stop_orders.insert(symbol.to_string(), stop_order.clone());

        info!(
            "Stop order created for {}: SL={}, TP={}, Entry={}",
            symbol, stop_loss_price, take_profit_price, entry_price
        );

        stop_order
    }

    /// 检查价格是否触发止损止盈
    pub async fn check_price(
        &self,
        symbol: &str,
        current_price: Decimal,
    ) -> Option<StopAction> {
        let mut stop_orders = self.stop_orders.lock().await;

        if let Some(stop_order) = stop_orders.get_mut(symbol) {
            if stop_order.triggered {
                return None;
            }

            // 更新最高/最低价 (用于追踪止损)
            if current_price > stop_order.highest_price {
                stop_order.highest_price = current_price;
            }
            if current_price < stop_order.lowest_price {
                stop_order.lowest_price = current_price;
            }

            // 检查是否触发止损
            let should_stop_loss = match stop_order.side {
                OrderSide::Buy => current_price <= stop_order.stop_loss_price,
                OrderSide::Sell => current_price >= stop_order.stop_loss_price,
            };

            if should_stop_loss {
                stop_order.triggered = true;
                warn!(
                    "Stop loss triggered for {} at {} (entry: {}, stop: {})",
                    symbol, current_price, stop_order.entry_price, stop_order.stop_loss_price
                );
                return Some(StopAction::StopLoss {
                    symbol: symbol.to_string(),
                    side: stop_order.side.clone(),
                    quantity: stop_order.quantity,
                    price: current_price,
                });
            }

            // 检查是否触发止盈
            let should_take_profit = match stop_order.side {
                OrderSide::Buy => current_price >= stop_order.take_profit_price,
                OrderSide::Sell => current_price <= stop_order.take_profit_price,
            };

            if should_take_profit {
                stop_order.triggered = true;
                info!(
                    "Take profit triggered for {} at {} (entry: {}, tp: {})",
                    symbol, current_price, stop_order.entry_price, stop_order.take_profit_price
                );
                return Some(StopAction::TakeProfit {
                    symbol: symbol.to_string(),
                    side: stop_order.side.clone(),
                    quantity: stop_order.quantity,
                    price: current_price,
                });
            }

            // 检查追踪止损
            if self.config.enable_trailing_stop {
                let trailing_stop_price = match stop_order.side {
                    OrderSide::Buy => {
                        stop_order.highest_price * (Decimal::ONE - self.config.trailing_stop_pct)
                    }
                    OrderSide::Sell => {
                        stop_order.lowest_price * (Decimal::ONE + self.config.trailing_stop_pct)
                    }
                };

                let should_trailing_stop = match stop_order.side {
                    OrderSide::Buy => current_price <= trailing_stop_price,
                    OrderSide::Sell => current_price >= trailing_stop_price,
                };

                if should_trailing_stop {
                    stop_order.triggered = true;
                    info!(
                        "Trailing stop triggered for {} at {} (highest: {}, lowest: {})",
                        symbol, current_price, stop_order.highest_price, stop_order.lowest_price
                    );
                    return Some(StopAction::TrailingStop {
                        symbol: symbol.to_string(),
                        side: stop_order.side.clone(),
                        quantity: stop_order.quantity,
                        price: current_price,
                    });
                }
            }
        }

        None
    }

    /// 移除止损止盈订单
    pub async fn remove_stop_order(&self, symbol: &str) -> Option<StopOrder> {
        let mut stop_orders = self.stop_orders.lock().await;
        stop_orders.remove(symbol)
    }

    /// 更新止损止盈价格
    pub async fn update_stop_prices(
        &self,
        symbol: &str,
        new_stop_loss: Option<Decimal>,
        new_take_profit: Option<Decimal>,
    ) -> bool {
        let mut stop_orders = self.stop_orders.lock().await;

        if let Some(stop_order) = stop_orders.get_mut(symbol) {
            if let Some(sl) = new_stop_loss {
                stop_order.stop_loss_price = sl;
            }
            if let Some(tp) = new_take_profit {
                stop_order.take_profit_price = tp;
            }
            info!(
                "Stop prices updated for {}: SL={}, TP={}",
                symbol, stop_order.stop_loss_price, stop_order.take_profit_price
            );
            return true;
        }

        false
    }

    /// 获取所有活动的止损止盈订单
    pub async fn get_active_stop_orders(&self) -> Vec<StopOrder> {
        let stop_orders = self.stop_orders.lock().await;
        stop_orders.values().filter(|o| !o.triggered).cloned().collect()
    }

    /// 获取指定交易对的止损止盈订单
    pub async fn get_stop_order(&self, symbol: &str) -> Option<StopOrder> {
        let stop_orders = self.stop_orders.lock().await;
        stop_orders.get(symbol).cloned()
    }

    /// 清除所有已触发的止损止盈订单
    pub async fn clear_triggered_orders(&self) {
        let mut stop_orders = self.stop_orders.lock().await;
        stop_orders.retain(|_, o| !o.triggered);
    }
}

/// 止损止盈动作
#[derive(Debug, Clone)]
pub enum StopAction {
    /// 止损触发
    StopLoss {
        symbol: String,
        side: OrderSide,
        quantity: Decimal,
        price: Decimal,
    },
    /// 止盈触发
    TakeProfit {
        symbol: String,
        side: OrderSide,
        quantity: Decimal,
        price: Decimal,
    },
    /// 追踪止损触发
    TrailingStop {
        symbol: String,
        side: OrderSide,
        quantity: Decimal,
        price: Decimal,
    },
}

impl StopAction {
    /// 获取交易对
    pub fn symbol(&self) -> &str {
        match self {
            StopAction::StopLoss { symbol, .. } => symbol,
            StopAction::TakeProfit { symbol, .. } => symbol,
            StopAction::TrailingStop { symbol, .. } => symbol,
        }
    }

    /// 获取平仓方向 (止损止盈都是平仓操作)
    pub fn close_side(&self) -> OrderSide {
        match self {
            StopAction::StopLoss { side, .. } => match side {
                OrderSide::Buy => OrderSide::Sell,
                OrderSide::Sell => OrderSide::Buy,
            },
            StopAction::TakeProfit { side, .. } => match side {
                OrderSide::Buy => OrderSide::Sell,
                OrderSide::Sell => OrderSide::Buy,
            },
            StopAction::TrailingStop { side, .. } => match side {
                OrderSide::Buy => OrderSide::Sell,
                OrderSide::Sell => OrderSide::Buy,
            },
        }
    }

    /// 转换为订单请求
    pub fn to_order_request(&self) -> OrderRequest {
        let (symbol, side, quantity) = match self {
            StopAction::StopLoss {
                symbol,
                side,
                quantity,
                ..
            } => (symbol, side, quantity),
            StopAction::TakeProfit {
                symbol,
                side,
                quantity,
                ..
            } => (symbol, side, quantity),
            StopAction::TrailingStop {
                symbol,
                side,
                quantity,
                ..
            } => (symbol, side, quantity),
        };

        // 止损止盈使用市价单快速平仓
        let close_side = match side {
            OrderSide::Buy => OrderSide::Sell,
            OrderSide::Sell => OrderSide::Buy,
        };

        OrderRequest {
            symbol: symbol.clone(),
            side: close_side,
            order_type: OrderType::Market,
            quantity: *quantity,
            price: None,
            stop_price: None,
            time_in_force: Some(TimeInForce::Ioc),
            client_order_id: None,
        }
    }
}
