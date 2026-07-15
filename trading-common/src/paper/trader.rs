// paper/trader.rs
// Paper Trader - 模拟交易器
// 复用 backtest::Portfolio，添加实时价格驱动和挂单管理

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::str::FromStr;
use std::sync::Arc;
use tokio::sync::RwLock;
use uuid::Uuid;

use crate::backtest::portfolio::{Portfolio, PositionSide as PortfolioPositionSide};
use crate::data::types::TradeSide;

// ===== 配置 =====

/// Paper Trader 配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaperTraderConfig {
    /// 关联的策略实例 ID（可选）
    pub instance_id: Option<Uuid>,
    /// 策略类型名称
    pub strategy_type: Option<String>,
    /// 策略显示名称
    pub display_name: Option<String>,
    /// 初始资金 (USDT)
    pub initial_capital: Decimal,
    /// 手续费率 (如 0.001 = 0.1%)
    pub commission_rate: Decimal,
    /// 滑点百分比 (如 0.0001 = 0.01%)
    pub slippage_pct: Decimal,
    /// 监控的交易对
    pub symbols: Vec<String>,
}

impl Default for PaperTraderConfig {
    fn default() -> Self {
        Self {
            instance_id: None,
            strategy_type: None,
            display_name: None,
            initial_capital: Decimal::from(10000),
            commission_rate: Decimal::from_str("0.001").unwrap_or(Decimal::ZERO),
            slippage_pct: Decimal::from_str("0.0001").unwrap_or(Decimal::ZERO),
            symbols: vec!["BTCUSDT".to_string(), "ETHUSDT".to_string()],
        }
    }
}

// ===== 订单类型 =====

/// 模拟订单类型
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum PaperOrderType {
    /// 市价单 - 立即成交
    Market,
    /// 限价单 - 到价成交
    Limit,
    /// 止损单 - 触发后市价成交
    StopLoss,
    /// 止盈单 - 触发后市价成交
    TakeProfit,
}

/// 模拟订单状态
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum PaperOrderStatus {
    /// 待处理 (限价/止损/止盈)
    Pending,
    /// 已成交
    Filled,
    /// 已取消
    Canceled,
    /// 被拒绝 (资金不足等)
    Rejected,
}

/// 模拟订单
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaperOrder {
    /// 订单 ID
    pub order_id: String,
    /// 交易对
    pub symbol: String,
    /// 买入/卖出
    pub side: TradeSide,
    /// 订单类型
    pub order_type: PaperOrderType,
    /// 数量
    pub quantity: Decimal,
    /// 限价/止损价 (市价单为 None)
    pub price: Option<Decimal>,
    /// 状态
    pub status: PaperOrderStatus,
    /// 成交价格
    pub filled_price: Option<Decimal>,
    /// 手续费
    pub commission: Decimal,
    /// 创建时间
    pub created_at: DateTime<Utc>,
    /// 成交时间
    pub filled_at: Option<DateTime<Utc>>,
    /// 拒绝原因
    pub reject_reason: Option<String>,
}

// ===== 状态快照 =====

/// 持仓快照 (用于前端展示)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaperPosition {
    pub symbol: String,
    pub side: String,       // "Long" / "Short"
    pub quantity: Decimal,
    pub avg_price: Decimal,
    pub current_price: Decimal,
    pub market_value: Decimal,
    pub unrealized_pnl: Decimal,
    pub unrealized_pnl_pct: Decimal,
}

/// Paper Trader 状态快照
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaperTraderStatus {
    /// 是否运行中
    pub running: bool,
    /// 关联的策略实例 ID
    pub instance_id: Option<Uuid>,
    /// 策略类型名称
    pub strategy_type: Option<String>,
    /// 策略显示名称
    pub display_name: Option<String>,
    /// 初始资金
    pub initial_capital: Decimal,
    /// 可用余额
    pub cash: Decimal,
    /// 总资产 (现金 + 持仓市值)
    pub total_value: Decimal,
    /// 总盈亏
    pub total_pnl: Decimal,
    /// 总盈亏百分比
    pub total_pnl_pct: Decimal,
    /// 已实现盈亏
    pub realized_pnl: Decimal,
    /// 未实现盈亏
    pub unrealized_pnl: Decimal,
    /// 总手续费
    pub total_commission: Decimal,
    /// 总交易次数
    pub total_trades: usize,
    /// 胜率
    pub win_rate: Decimal,
    /// 持仓列表
    pub positions: Vec<PaperPosition>,
    /// 待处理订单数
    pub pending_orders: usize,
    /// 最新价格
    pub latest_prices: HashMap<String, Decimal>,
    /// 启动时间
    pub started_at: Option<DateTime<Utc>>,
}

// ===== Paper Trader =====

/// Paper Trader - 模拟交易器
pub struct PaperTrader {
    /// 配置
    config: PaperTraderConfig,
    /// 虚拟投资组合
    portfolio: Portfolio,
    /// 待处理订单 (限价/止损/止盈)
    pending_orders: Vec<PaperOrder>,
    /// 已完成订单历史
    filled_orders: Vec<PaperOrder>,
    /// 最新价格
    latest_prices: HashMap<String, Decimal>,
    /// 是否运行中
    running: bool,
    /// 启动时间
    started_at: Option<DateTime<Utc>>,
    /// 订单 ID 计数器
    order_counter: u64,
}

impl PaperTrader {
    /// 创建新的 Paper Trader
    pub fn new(config: PaperTraderConfig) -> Self {
        let portfolio = Portfolio::new(config.initial_capital)
            .with_commission_rate(config.commission_rate);

        Self {
            config,
            portfolio,
            pending_orders: Vec::new(),
            filled_orders: Vec::new(),
            latest_prices: HashMap::new(),
            running: false,
            started_at: None,
            order_counter: 0,
        }
    }

    /// 启动模拟交易
    pub fn start(&mut self) {
        self.running = true;
        self.started_at = Some(Utc::now());
    }

    /// 停止模拟交易
    pub fn stop(&mut self) {
        self.running = false;
    }

    /// 是否运行中
    pub fn is_running(&self) -> bool {
        self.running
    }

    /// 获取关联的策略实例 ID
    pub fn instance_id(&self) -> Option<Uuid> {
        self.config.instance_id
    }

    /// 获取策略类型名称
    pub fn strategy_type(&self) -> Option<&str> {
        self.config.strategy_type.as_deref()
    }

    /// 获取策略显示名称
    pub fn display_name(&self) -> Option<&str> {
        self.config.display_name.as_deref()
    }

    /// 生成下一个订单 ID
    fn next_order_id(&mut self) -> String {
        self.order_counter += 1;
        format!("PAPER-{:06}", self.order_counter)
    }

    /// 应用滑点
    fn apply_slippage(&self, price: Decimal, side: &TradeSide) -> Decimal {
        let slippage = price * self.config.slippage_pct;
        match side {
            TradeSide::Buy => price + slippage,   // 买入滑点向上
            TradeSide::Sell => price - slippage,   // 卖出滑点向下
        }
    }

    // ===== 价格更新 =====

    /// 更新价格 - 驱动挂单触发和持仓估值
    pub fn update_price(&mut self, symbol: &str, price: Decimal) {
        self.latest_prices.insert(symbol.to_string(), price);
        self.portfolio.update_price(symbol, price);

        // 检查挂单是否触发
        self.check_pending_orders(symbol, price);
    }

    /// 批量更新价格
    pub fn update_prices(&mut self, prices: HashMap<String, Decimal>) {
        for (symbol, price) in prices {
            self.update_price(&symbol, price);
        }
    }

    /// 检查挂单触发
    fn check_pending_orders(&mut self, symbol: &str, current_price: Decimal) {
        let mut triggered_indices = Vec::new();

        for (i, order) in self.pending_orders.iter().enumerate() {
            if order.symbol != symbol || order.price.is_none() {
                continue;
            }

            let trigger_price = order.price.unwrap();
            let should_trigger = match order.order_type {
                PaperOrderType::Limit => {
                    // 限价买单: 当前价 <= 限价; 限价卖单: 当前价 >= 限价
                    match order.side {
                        TradeSide::Buy => current_price <= trigger_price,
                        TradeSide::Sell => current_price >= trigger_price,
                    }
                }
                PaperOrderType::StopLoss => {
                    // 止损买单 (空头止损): 当前价 >= 止损价; 止损卖单 (多头止损): 当前价 <= 止损价
                    match order.side {
                        TradeSide::Buy => current_price >= trigger_price,
                        TradeSide::Sell => current_price <= trigger_price,
                    }
                }
                PaperOrderType::TakeProfit => {
                    // 止盈买单: 当前价 <= 止盈价; 止盈卖单: 当前价 >= 止盈价
                    match order.side {
                        TradeSide::Buy => current_price <= trigger_price,
                        TradeSide::Sell => current_price >= trigger_price,
                    }
                }
                PaperOrderType::Market => false, // 市价单不会在挂单中
            };

            if should_trigger {
                triggered_indices.push(i);
            }
        }

        // 从后往前移除触发的挂单并执行
        for &i in triggered_indices.iter().rev() {
            let mut order = self.pending_orders.remove(i);
            let fill_price = self.apply_slippage(current_price, &order.side);
            self.execute_fill(&mut order, fill_price);
            self.filled_orders.push(order);
        }
    }

    // ===== 下单接口 =====

    /// 市价下单
    pub fn place_market_order(
        &mut self,
        symbol: &str,
        side: TradeSide,
        quantity: Decimal,
    ) -> Result<PaperOrder, String> {
        if !self.running {
            return Err("Paper Trader is not running".to_string());
        }

        let current_price = self.latest_prices
            .get(symbol)
            .copied()
            .ok_or_else(|| format!("No price data for {}", symbol))?;

        let fill_price = self.apply_slippage(current_price, &side);

        let mut order = PaperOrder {
            order_id: self.next_order_id(),
            symbol: symbol.to_string(),
            side: side.clone(),
            order_type: PaperOrderType::Market,
            quantity,
            price: None,
            status: PaperOrderStatus::Pending,
            filled_price: None,
            commission: Decimal::ZERO,
            created_at: Utc::now(),
            filled_at: None,
            reject_reason: None,
        };

        self.execute_fill(&mut order, fill_price);
        let filled_order = order.clone();
        self.filled_orders.push(order);

        Ok(filled_order)
    }

    /// 限价下单
    pub fn place_limit_order(
        &mut self,
        symbol: &str,
        side: TradeSide,
        quantity: Decimal,
        price: Decimal,
    ) -> Result<PaperOrder, String> {
        if !self.running {
            return Err("Paper Trader is not running".to_string());
        }

        let order = PaperOrder {
            order_id: self.next_order_id(),
            symbol: symbol.to_string(),
            side,
            order_type: PaperOrderType::Limit,
            quantity,
            price: Some(price),
            status: PaperOrderStatus::Pending,
            filled_price: None,
            commission: Decimal::ZERO,
            created_at: Utc::now(),
            filled_at: None,
            reject_reason: None,
        };

        let order_clone = order.clone();
        self.pending_orders.push(order);

        Ok(order_clone)
    }

    /// 止损单
    pub fn place_stop_loss_order(
        &mut self,
        symbol: &str,
        side: TradeSide,
        quantity: Decimal,
        stop_price: Decimal,
    ) -> Result<PaperOrder, String> {
        if !self.running {
            return Err("Paper Trader is not running".to_string());
        }

        let order = PaperOrder {
            order_id: self.next_order_id(),
            symbol: symbol.to_string(),
            side,
            order_type: PaperOrderType::StopLoss,
            quantity,
            price: Some(stop_price),
            status: PaperOrderStatus::Pending,
            filled_price: None,
            commission: Decimal::ZERO,
            created_at: Utc::now(),
            filled_at: None,
            reject_reason: None,
        };

        let order_clone = order.clone();
        self.pending_orders.push(order);

        Ok(order_clone)
    }

    /// 止盈单
    pub fn place_take_profit_order(
        &mut self,
        symbol: &str,
        side: TradeSide,
        quantity: Decimal,
        take_profit_price: Decimal,
    ) -> Result<PaperOrder, String> {
        if !self.running {
            return Err("Paper Trader is not running".to_string());
        }

        let order = PaperOrder {
            order_id: self.next_order_id(),
            symbol: symbol.to_string(),
            side,
            order_type: PaperOrderType::TakeProfit,
            quantity,
            price: Some(take_profit_price),
            status: PaperOrderStatus::Pending,
            filled_price: None,
            commission: Decimal::ZERO,
            created_at: Utc::now(),
            filled_at: None,
            reject_reason: None,
        };

        let order_clone = order.clone();
        self.pending_orders.push(order);

        Ok(order_clone)
    }

    /// 取消挂单
    pub fn cancel_order(&mut self, order_id: &str) -> Result<(), String> {
        if let Some(pos) = self.pending_orders.iter().position(|o| o.order_id == order_id) {
            let mut order = self.pending_orders.remove(pos);
            order.status = PaperOrderStatus::Canceled;
            self.filled_orders.push(order);
            Ok(())
        } else {
            Err(format!("Order {} not found or already filled", order_id))
        }
    }

    /// 执行成交
    fn execute_fill(&mut self, order: &mut PaperOrder, fill_price: Decimal) {
        let result = match order.side {
            TradeSide::Buy => {
                // 买入: 判断是开多还是平空
                let has_short = self.portfolio.positions
                    .get(&order.symbol)
                    .map_or(false, |p| p.side == PortfolioPositionSide::Short);

                if has_short {
                    // 平空仓
                    let short_qty = self.portfolio.positions
                        .get(&order.symbol)
                        .map(|p| p.quantity)
                        .unwrap_or(Decimal::ZERO);
                    let close_qty = order.quantity.min(short_qty);
                    let mut result = Ok(());
                    if close_qty > Decimal::ZERO {
                        result = self.portfolio.execute_short_close(
                            order.symbol.clone(),
                            close_qty,
                            fill_price,
                        );
                    }
                    // 如果还有剩余，开多仓
                    let remaining = order.quantity - close_qty;
                    if remaining > Decimal::ZERO && result.is_ok() {
                        result = self.portfolio.execute_buy(
                            order.symbol.clone(),
                            remaining,
                            fill_price,
                        );
                    }
                    result
                } else {
                    // 开多仓
                    self.portfolio.execute_buy(
                        order.symbol.clone(),
                        order.quantity,
                        fill_price,
                    )
                }
            }
            TradeSide::Sell => {
                // 卖出: 判断是平多还是开空
                let has_long = self.portfolio.positions
                    .get(&order.symbol)
                    .map_or(false, |p| p.side == PortfolioPositionSide::Long && p.quantity > Decimal::ZERO);

                if has_long {
                    // 平多仓
                    let long_qty = self.portfolio.positions
                        .get(&order.symbol)
                        .map(|p| p.quantity)
                        .unwrap_or(Decimal::ZERO);
                    let close_qty = order.quantity.min(long_qty);
                    let mut result = Ok(());
                    if close_qty > Decimal::ZERO {
                        result = self.portfolio.execute_sell(
                            order.symbol.clone(),
                            close_qty,
                            fill_price,
                        );
                    }
                    // 如果还有剩余，开空仓
                    let remaining = order.quantity - close_qty;
                    if remaining > Decimal::ZERO && result.is_ok() {
                        result = self.portfolio.execute_short_open(
                            order.symbol.clone(),
                            remaining,
                            fill_price,
                        );
                    }
                    result
                } else {
                    // 开空仓
                    self.portfolio.execute_short_open(
                        order.symbol.clone(),
                        order.quantity,
                        fill_price,
                    )
                }
            }
        };

        match result {
            Ok(()) => {
                order.status = PaperOrderStatus::Filled;
                order.filled_price = Some(fill_price);
                order.commission = order.quantity * fill_price * self.config.commission_rate;
                order.filled_at = Some(Utc::now());
            }
            Err(e) => {
                order.status = PaperOrderStatus::Rejected;
                order.reject_reason = Some(e);
            }
        }
    }

    // ===== 查询接口 =====

    /// 获取当前状态快照
    pub fn get_status(&self) -> PaperTraderStatus {
        let positions: Vec<PaperPosition> = self.portfolio.positions.iter().map(|(symbol, pos)| {
            let current_price = self.latest_prices.get(symbol).copied().unwrap_or(pos.avg_price);
            let pnl_pct = if pos.avg_price > Decimal::ZERO {
                match pos.side {
                    PortfolioPositionSide::Long => {
                        (current_price - pos.avg_price) / pos.avg_price * Decimal::from(100)
                    }
                    PortfolioPositionSide::Short => {
                        (pos.avg_price - current_price) / pos.avg_price * Decimal::from(100)
                    }
                }
            } else {
                Decimal::ZERO
            };

            PaperPosition {
                symbol: symbol.clone(),
                side: match pos.side {
                    PortfolioPositionSide::Long => "Long".to_string(),
                    PortfolioPositionSide::Short => "Short".to_string(),
                },
                quantity: pos.quantity,
                avg_price: pos.avg_price,
                current_price,
                market_value: pos.market_value,
                unrealized_pnl: pos.unrealized_pnl,
                unrealized_pnl_pct: pnl_pct,
            }
        }).collect();

        let total_value = self.portfolio.total_value();
        let total_pnl = self.portfolio.total_pnl();
        let total_pnl_pct = if self.portfolio.initial_capital > Decimal::ZERO {
            total_pnl / self.portfolio.initial_capital * Decimal::from(100)
        } else {
            Decimal::ZERO
        };

        // 计算胜率 (基于 portfolio 中有 realized_pnl 的交易)
        let completed_trades: Vec<_> = self.portfolio.trades.iter()
            .filter(|t| t.realized_pnl.is_some())
            .collect();
        let winning_trades = completed_trades.iter()
            .filter(|t| t.realized_pnl.unwrap_or(Decimal::ZERO) > Decimal::ZERO)
            .count();
        let total_completed = completed_trades.len();
        let win_rate = if total_completed > 0 {
            Decimal::from(winning_trades) / Decimal::from(total_completed) * Decimal::from(100)
        } else {
            Decimal::ZERO
        };

        PaperTraderStatus {
            running: self.running,
            instance_id: self.config.instance_id,
            strategy_type: self.config.strategy_type.clone(),
            display_name: self.config.display_name.clone(),
            initial_capital: self.portfolio.initial_capital,
            cash: self.portfolio.cash,
            total_value,
            total_pnl,
            total_pnl_pct,
            realized_pnl: self.portfolio.total_realized_pnl(),
            unrealized_pnl: self.portfolio.total_unrealized_pnl(),
            total_commission: self.portfolio.total_commission(),
            total_trades: self.filled_orders.len(),
            win_rate,
            positions,
            pending_orders: self.pending_orders.len(),
            latest_prices: self.latest_prices.clone(),
            started_at: self.started_at,
        }
    }

    /// 获取所有交易记录
    pub fn get_trades(&self) -> Vec<PaperOrder> {
        self.filled_orders.clone()
    }

    /// 获取挂单列表
    pub fn get_pending_orders(&self) -> Vec<PaperOrder> {
        self.pending_orders.clone()
    }

    /// 获取配置
    pub fn get_config(&self) -> &PaperTraderConfig {
        &self.config
    }

    /// 重置 (清空所有状态，保留配置)
    pub fn reset(&mut self) {
        self.portfolio = Portfolio::new(self.config.initial_capital)
            .with_commission_rate(self.config.commission_rate);
        self.pending_orders.clear();
        self.filled_orders.clear();
        self.latest_prices.clear();
        self.running = false;
        self.started_at = None;
        self.order_counter = 0;
    }
}

/// 共享的 Paper Trader 状态 (用于 Tauri 多线程访问)
pub type SharedPaperTrader = Arc<RwLock<PaperTrader>>;
