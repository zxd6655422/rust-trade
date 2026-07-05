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

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal::Decimal;
    use std::str::FromStr;

    fn create_manager() -> StopLossManager {
        StopLossManager::new(StopLossConfig::default())
    }

    fn create_manager_with_trailing() -> StopLossManager {
        StopLossManager::new(StopLossConfig {
            default_stop_loss_pct: Decimal::from_str("0.02").unwrap(),
            default_take_profit_pct: Decimal::from_str("0.04").unwrap(),
            enable_trailing_stop: true,
            trailing_stop_pct: Decimal::from_str("0.01").unwrap(),
        })
    }

    // ========== 创建止损止盈订单测试 ==========

    #[tokio::test]
    async fn test_create_stop_order_buy() {
        let manager = create_manager();
        let entry_price = Decimal::from(50000);

        let order = manager
            .create_stop_order("BTCUSDT", OrderSide::Buy, Decimal::from_str("0.1").unwrap(), entry_price, None, None)
            .await;

        assert_eq!(order.symbol, "BTCUSDT");
        assert_eq!(order.side, OrderSide::Buy);
        // 止损 = 50000 * (1 - 0.02) = 49000
        assert_eq!(order.stop_loss_price, Decimal::from(49000));
        // 止盈 = 50000 * (1 + 0.04) = 52000
        assert_eq!(order.take_profit_price, Decimal::from(52000));
        assert!(!order.triggered);
    }

    #[tokio::test]
    async fn test_create_stop_order_sell() {
        let manager = create_manager();
        let entry_price = Decimal::from(3000);

        let order = manager
            .create_stop_order("ETHUSDT", OrderSide::Sell, Decimal::from(10), entry_price, None, None)
            .await;

        assert_eq!(order.symbol, "ETHUSDT");
        assert_eq!(order.side, OrderSide::Sell);
        // 止损 = 3000 * (1 + 0.02) = 3060
        assert_eq!(order.stop_loss_price, Decimal::from(3060));
        // 止盈 = 3000 * (1 - 0.04) = 2880
        assert_eq!(order.take_profit_price, Decimal::from(2880));
    }

    #[tokio::test]
    async fn test_create_stop_order_custom_prices() {
        let manager = create_manager();

        let order = manager
            .create_stop_order(
                "BTCUSDT",
                OrderSide::Buy,
                Decimal::from_str("0.1").unwrap(),
                Decimal::from(50000),
                Some(Decimal::from(48000)), // 自定义止损
                Some(Decimal::from(55000)), // 自定义止盈
            )
            .await;

        assert_eq!(order.stop_loss_price, Decimal::from(48000));
        assert_eq!(order.take_profit_price, Decimal::from(55000));
    }

    // ========== 止损触发测试 ==========

    #[tokio::test]
    async fn test_stop_loss_triggered_buy() {
        let manager = create_manager();
        manager
            .create_stop_order("BTCUSDT", OrderSide::Buy, Decimal::from_str("0.1").unwrap(), Decimal::from(50000), None, None)
            .await;

        // 价格跌到止损线以下
        let action = manager.check_price("BTCUSDT", Decimal::from(48900)).await;
        assert!(action.is_some());
        match action.unwrap() {
            StopAction::StopLoss { symbol, side, price, .. } => {
                assert_eq!(symbol, "BTCUSDT");
                assert_eq!(side, OrderSide::Buy);
                assert_eq!(price, Decimal::from(48900));
            }
            _ => panic!("Expected StopLoss"),
        }
    }

    #[tokio::test]
    async fn test_stop_loss_not_triggered_buy() {
        let manager = create_manager();
        manager
            .create_stop_order("BTCUSDT", OrderSide::Buy, Decimal::from_str("0.1").unwrap(), Decimal::from(50000), None, None)
            .await;

        // 价格在止损线以上
        let action = manager.check_price("BTCUSDT", Decimal::from(49500)).await;
        assert!(action.is_none());
    }

    // ========== 止盈触发测试 ==========

    #[tokio::test]
    async fn test_take_profit_triggered_buy() {
        let manager = create_manager();
        manager
            .create_stop_order("BTCUSDT", OrderSide::Buy, Decimal::from_str("0.1").unwrap(), Decimal::from(50000), None, None)
            .await;

        // 价格涨到止盈线以上
        let action = manager.check_price("BTCUSDT", Decimal::from(52500)).await;
        assert!(action.is_some());
        match action.unwrap() {
            StopAction::TakeProfit { symbol, .. } => {
                assert_eq!(symbol, "BTCUSDT");
            }
            _ => panic!("Expected TakeProfit"),
        }
    }

    #[tokio::test]
    async fn test_stop_loss_sell() {
        let manager = create_manager();
        manager
            .create_stop_order("ETHUSDT", OrderSide::Sell, Decimal::from(10), Decimal::from(3000), None, None)
            .await;

        // 空头止损：价格涨到止损线以上
        let action = manager.check_price("ETHUSDT", Decimal::from(3070)).await;
        assert!(action.is_some());
        match action.unwrap() {
            StopAction::StopLoss { .. } => {}
            _ => panic!("Expected StopLoss"),
        }
    }

    #[tokio::test]
    async fn test_take_profit_sell() {
        let manager = create_manager();
        manager
            .create_stop_order("ETHUSDT", OrderSide::Sell, Decimal::from(10), Decimal::from(3000), None, None)
            .await;

        // 空头止盈：价格跌到止盈线以下
        let action = manager.check_price("ETHUSDT", Decimal::from(2870)).await;
        assert!(action.is_some());
        match action.unwrap() {
            StopAction::TakeProfit { .. } => {}
            _ => panic!("Expected TakeProfit"),
        }
    }

    // ========== 追踪止损测试 ==========

    #[tokio::test]
    async fn test_trailing_stop_buy() {
        // 使用更高的止盈百分比，避免在测试追踪止损前就触发止盈
        let manager = StopLossManager::new(StopLossConfig {
            default_stop_loss_pct: Decimal::from_str("0.02").unwrap(),
            default_take_profit_pct: Decimal::from_str("0.08").unwrap(), // 8% 止盈
            enable_trailing_stop: true,
            trailing_stop_pct: Decimal::from_str("0.01").unwrap(),
        });
        // entry=50000, SL=49000, TP=54000
        manager
            .create_stop_order("BTCUSDT", OrderSide::Buy, Decimal::from_str("0.1").unwrap(), Decimal::from(50000), None, None)
            .await;

        // 价格上涨到 53000（低于 TP=54000，不会触发止盈）
        let action1 = manager.check_price("BTCUSDT", Decimal::from(53000)).await;
        assert!(action1.is_none()); // 不触发

        // 追踪止损线 = 53000 * (1 - 0.01) = 52470
        // 价格回落到 52400 < 52470，应触发追踪止损
        let action2 = manager.check_price("BTCUSDT", Decimal::from(52400)).await;
        assert!(action2.is_some());
        match action2.unwrap() {
            StopAction::TrailingStop { .. } => {}
            _ => panic!("Expected TrailingStop"),
        }
    }

    // ========== 价格更新测试 ==========

    #[tokio::test]
    async fn test_highest_price_updated() {
        let manager = create_manager();
        manager
            .create_stop_order("BTCUSDT", OrderSide::Buy, Decimal::from_str("0.1").unwrap(), Decimal::from(50000), None, None)
            .await;

        manager.check_price("BTCUSDT", Decimal::from(51000)).await;
        manager.check_price("BTCUSDT", Decimal::from(50500)).await;

        let order = manager.get_stop_order("BTCUSDT").await.unwrap();
        assert_eq!(order.highest_price, Decimal::from(51000));
    }

    #[tokio::test]
    async fn test_lowest_price_updated() {
        let manager = create_manager();
        manager
            .create_stop_order("BTCUSDT", OrderSide::Buy, Decimal::from_str("0.1").unwrap(), Decimal::from(50000), None, None)
            .await;

        manager.check_price("BTCUSDT", Decimal::from(49500)).await;
        manager.check_price("BTCUSDT", Decimal::from(50000)).await;

        let order = manager.get_stop_order("BTCUSDT").await.unwrap();
        assert_eq!(order.lowest_price, Decimal::from(49500));
    }

    // ========== 触发后不再重复触发 ==========

    #[tokio::test]
    async fn test_no_double_trigger() {
        let manager = create_manager();
        manager
            .create_stop_order("BTCUSDT", OrderSide::Buy, Decimal::from_str("0.1").unwrap(), Decimal::from(50000), None, None)
            .await;

        // 第一次触发
        let action1 = manager.check_price("BTCUSDT", Decimal::from(48000)).await;
        assert!(action1.is_some());

        // 第二次不应再触发
        let action2 = manager.check_price("BTCUSDT", Decimal::from(47000)).await;
        assert!(action2.is_none());
    }

    // ========== 移除止损止盈 ==========

    #[tokio::test]
    async fn test_remove_stop_order() {
        let manager = create_manager();
        manager
            .create_stop_order("BTCUSDT", OrderSide::Buy, Decimal::from_str("0.1").unwrap(), Decimal::from(50000), None, None)
            .await;

        let removed = manager.remove_stop_order("BTCUSDT").await;
        assert!(removed.is_some());

        // 移除后不应有触发
        let action = manager.check_price("BTCUSDT", Decimal::from(40000)).await;
        assert!(action.is_none());
    }

    // ========== 更新止损止盈价格 ==========

    #[tokio::test]
    async fn test_update_stop_prices() {
        let manager = create_manager();
        manager
            .create_stop_order("BTCUSDT", OrderSide::Buy, Decimal::from_str("0.1").unwrap(), Decimal::from(50000), None, None)
            .await;

        let updated = manager.update_stop_prices(
            "BTCUSDT",
            Some(Decimal::from(48000)),
            Some(Decimal::from(55000)),
        ).await;
        assert!(updated);

        let order = manager.get_stop_order("BTCUSDT").await.unwrap();
        assert_eq!(order.stop_loss_price, Decimal::from(48000));
        assert_eq!(order.take_profit_price, Decimal::from(55000));
    }

    // ========== StopAction 测试 ==========

    #[test]
    fn test_stop_action_to_order_request() {
        let action = StopAction::StopLoss {
            symbol: "BTCUSDT".to_string(),
            side: OrderSide::Buy,
            quantity: Decimal::from_str("0.1").unwrap(),
            price: Decimal::from(48000),
        };

        let request = action.to_order_request();
        assert_eq!(request.symbol, "BTCUSDT");
        assert_eq!(request.side, OrderSide::Sell); // 止损买入 → 卖出平仓
        assert_eq!(request.order_type, OrderType::Market);
    }

    #[test]
    fn test_stop_action_close_side() {
        let action_buy = StopAction::TakeProfit {
            symbol: "BTCUSDT".to_string(),
            side: OrderSide::Buy,
            quantity: Decimal::ONE,
            price: Decimal::from(55000),
        };
        assert_eq!(action_buy.close_side(), OrderSide::Sell);

        let action_sell = StopAction::StopLoss {
            symbol: "ETHUSDT".to_string(),
            side: OrderSide::Sell,
            quantity: Decimal::ONE,
            price: Decimal::from(3200),
        };
        assert_eq!(action_sell.close_side(), OrderSide::Buy);
    }

    // ========== 活动订单查询 ==========

    #[tokio::test]
    async fn test_get_active_stop_orders() {
        let manager = create_manager();
        manager.create_stop_order("BTCUSDT", OrderSide::Buy, Decimal::from_str("0.1").unwrap(), Decimal::from(50000), None, None).await;
        manager.create_stop_order("ETHUSDT", OrderSide::Buy, Decimal::from(10), Decimal::from(3000), None, None).await;

        let active = manager.get_active_stop_orders().await;
        assert_eq!(active.len(), 2);
    }

    #[tokio::test]
    async fn test_clear_triggered_orders() {
        let manager = create_manager();
        manager.create_stop_order("BTCUSDT", OrderSide::Buy, Decimal::from_str("0.1").unwrap(), Decimal::from(50000), None, None).await;
        manager.create_stop_order("ETHUSDT", OrderSide::Buy, Decimal::from(10), Decimal::from(3000), None, None).await;

        // 触发一个
        manager.check_price("BTCUSDT", Decimal::from(40000)).await;

        manager.clear_triggered_orders().await;
        let active = manager.get_active_stop_orders().await;
        assert_eq!(active.len(), 1);
        assert_eq!(active[0].symbol, "ETHUSDT");
    }
}
