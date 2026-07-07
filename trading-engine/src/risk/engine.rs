// risk/engine.rs
// 风控引擎实现

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{debug, info, warn};

use super::config::RiskConfig;
use crate::exchange::types::{AccountInfo, OrderRequest};
use trading_common::data::types::TickData;

/// 风控决策
#[derive(Debug, Clone)]
pub enum RiskDecision {
    /// 允许执行
    Allow,
    /// 拒绝执行，附带原因
    Reject(String),
    /// 允许但修改数量
    Modify(Decimal),
}

impl RiskDecision {
    /// 是否接受订单
    pub fn is_accepted(&self) -> bool {
        match self {
            RiskDecision::Allow => true,
            RiskDecision::Modify(_) => true,
            RiskDecision::Reject(_) => false,
        }
    }

    /// 获取修改后的数量（如果有）
    pub fn modified_quantity(&self) -> Option<Decimal> {
        match self {
            RiskDecision::Modify(qty) => Some(*qty),
            _ => None,
        }
    }
}

/// 风控状态
#[derive(Debug, Clone)]
pub struct RiskState {
    /// 日盈亏
    pub daily_pnl: Decimal,
    /// 峰值权益
    pub peak_equity: Decimal,
    /// 当前权益
    pub current_equity: Decimal,
    /// 持仓信息
    pub positions: HashMap<String, PositionSnapshot>,
    /// 最后交易时间
    pub last_trade_time: HashMap<String, DateTime<Utc>>,
    /// 熔断结束时间
    pub circuit_breaker_until: Option<DateTime<Utc>>,
    /// 日交易次数
    pub daily_trade_count: u32,
    /// 最近价格历史 (用于黑天鹅检测)
    pub price_history: HashMap<String, Vec<(DateTime<Utc>, Decimal)>>,
}

/// 持仓快照
#[derive(Debug, Clone)]
pub struct PositionSnapshot {
    pub symbol: String,
    pub quantity: Decimal,
    pub avg_entry_price: Decimal,
    pub current_price: Decimal,
    pub unrealized_pnl: Decimal,
}

/// 风控引擎
pub struct RiskEngine {
    config: RiskConfig,
    state: Arc<Mutex<RiskState>>,
}

impl RiskEngine {
    /// 创建新的风控引擎
    pub fn new(config: RiskConfig) -> Self {
        let initial_state = RiskState {
            daily_pnl: Decimal::ZERO,
            peak_equity: Decimal::ZERO,
            current_equity: Decimal::ZERO,
            positions: HashMap::new(),
            last_trade_time: HashMap::new(),
            circuit_breaker_until: None,
            daily_trade_count: 0,
            price_history: HashMap::new(),
        };

        Self {
            config,
            state: Arc::new(Mutex::new(initial_state)),
        }
    }

    /// 核心方法：检查订单是否允许执行
    pub async fn check_order(
        &self,
        order: &OrderRequest,
        account: &AccountInfo,
    ) -> Result<RiskDecision, RiskError> {
        let state = self.state.lock().await;

        // 1. 熔断检查
        if let Some(until) = state.circuit_breaker_until {
            if Utc::now() < until {
                return Ok(RiskDecision::Reject(format!(
                    "Circuit breaker active until {}",
                    until
                )));
            }
        }

        // 2. 日亏损限制
        if state.daily_pnl < -self.config.max_daily_loss {
            return Ok(RiskDecision::Reject(format!(
                "Daily loss limit reached: {} / {}",
                state.daily_pnl, self.config.max_daily_loss
            )));
        }

        // 3. 最大回撤检查
        if state.peak_equity > Decimal::ZERO {
            let drawdown = (state.peak_equity - state.current_equity) / state.peak_equity;
            if drawdown > self.config.max_drawdown_pct {
                return Ok(RiskDecision::Reject(format!(
                    "Max drawdown breached: {}% / {}%",
                    drawdown * Decimal::from(100),
                    self.config.max_drawdown_pct * Decimal::from(100)
                )));
            }
        }

        // 4. 单笔仓位大小检查
        let order_value = order.quantity * order.price.unwrap_or(Decimal::ZERO);
        if order_value > self.config.max_position_size {
            return Ok(RiskDecision::Reject(format!(
                "Order value {} exceeds max position size {}",
                order_value, self.config.max_position_size
            )));
        }

        // 5. 单笔下单量检查
        if order.quantity > self.config.max_order_size {
            return Ok(RiskDecision::Reject(format!(
                "Order quantity {} exceeds max order size {}",
                order.quantity, self.config.max_order_size
            )));
        }

        // 6. 总曝光度检查
        let total_exposure: Decimal = state
            .positions
            .values()
            .map(|p| p.quantity * p.current_price)
            .sum();
        let new_exposure = total_exposure + order_value;
        let max_exposure = account.total_equity * self.config.max_exposure_pct;
        if new_exposure > max_exposure {
            return Ok(RiskDecision::Reject(format!(
                "Total exposure {} would exceed limit {}",
                new_exposure, max_exposure
            )));
        }

        // 7. 黑天鹅检测
        if self.detect_black_swan(&state, &order.symbol).await {
            return Ok(RiskDecision::Reject(
                "Black swan detected, trading halted".to_string(),
            ));
        }

        // 8. Kelly 仓位调整
        let adjusted_quantity = self.calculate_kelly_position(order, &state);
        if adjusted_quantity < order.quantity {
            return Ok(RiskDecision::Modify(adjusted_quantity));
        }

        Ok(RiskDecision::Allow)
    }

    /// 更新市场数据
    pub async fn update_market_data(&self, tick: &TickData) {
        let mut state = self.state.lock().await;

        // 更新价格历史
        let history = state
            .price_history
            .entry(tick.symbol.clone())
            .or_insert_with(Vec::new);
        history.push((tick.timestamp, tick.price));

        // 保持历史记录在合理范围内
        if history.len() > 1000 {
            history.drain(0..500);
        }

        // 更新持仓的当前价格
        if let Some(position) = state.positions.get_mut(&tick.symbol) {
            position.current_price = tick.price;
            position.unrealized_pnl =
                (tick.price - position.avg_entry_price) * position.quantity;
        }

        // 更新当前权益
        let total_unrealized: Decimal = state
            .positions
            .values()
            .map(|p| p.unrealized_pnl)
            .sum();
        state.current_equity = state.daily_pnl + total_unrealized;

        // 更新峰值权益
        if state.current_equity > state.peak_equity {
            state.peak_equity = state.current_equity;
        }
    }

    /// 记录交易结果
    pub async fn record_trade_result(
        &self,
        symbol: &str,
        side: &str,
        quantity: Decimal,
        price: Decimal,
    ) {
        let mut state = self.state.lock().await;

        // 更新日盈亏 (简化计算)
        // 实际应该从成交记录计算
        state.daily_trade_count += 1;
        state
            .last_trade_time
            .insert(symbol.to_string(), Utc::now());

        // 更新持仓
        if side == "BUY" {
            let position = state
                .positions
                .entry(symbol.to_string())
                .or_insert_with(|| PositionSnapshot {
                    symbol: symbol.to_string(),
                    quantity: Decimal::ZERO,
                    avg_entry_price: Decimal::ZERO,
                    current_price: price,
                    unrealized_pnl: Decimal::ZERO,
                });

            let total_cost = position.quantity * position.avg_entry_price + quantity * price;
            position.quantity += quantity;
            if position.quantity > Decimal::ZERO {
                position.avg_entry_price = total_cost / position.quantity;
            }
        } else if side == "SELL" {
            if let Some(position) = state.positions.get_mut(symbol) {
                let realized_pnl = (price - position.avg_entry_price) * quantity.min(position.quantity);
                let new_quantity = position.quantity - quantity;
                position.quantity = new_quantity;
                if position.quantity <= Decimal::ZERO {
                    state.positions.remove(symbol);
                }
                state.daily_pnl += realized_pnl;
            }
        }

        debug!(
            "Trade recorded: {} {} {} @ {} | Daily PnL: {}",
            symbol, side, quantity, price, state.daily_pnl
        );
    }

    /// 获取风控状态
    pub async fn get_status(&self) -> RiskStatus {
        let state = self.state.lock().await;
        RiskStatus {
            daily_pnl: state.daily_pnl,
            peak_equity: state.peak_equity,
            current_equity: state.current_equity,
            daily_trade_count: state.daily_trade_count,
            is_circuit_breaker_active: state
                .circuit_breaker_until
                .map(|until| Utc::now() < until)
                .unwrap_or(false),
            position_count: state.positions.len(),
        }
    }

    /// 手动触发熔断
    pub async fn trigger_circuit_breaker(&self, reason: &str) {
        let mut state = self.state.lock().await;
        let until = Utc::now() + chrono::Duration::seconds(self.config.circuit_breaker_cooldown as i64);
        state.circuit_breaker_until = Some(until);
        warn!(
            "Circuit breaker triggered: {} | Will resume at: {}",
            reason, until
        );
    }

    /// 重置日统计
    pub async fn reset_daily_stats(&self) {
        let mut state = self.state.lock().await;
        state.daily_pnl = Decimal::ZERO;
        state.daily_trade_count = 0;
        info!("Daily risk stats reset");
    }

    /// 检测黑天鹅事件
    async fn detect_black_swan(&self, state: &RiskState, symbol: &str) -> bool {
        if let Some(history) = state.price_history.get(symbol) {
            if history.len() < 2 {
                return false;
            }

            // 检查最近 N 个 tick 的价格波动
            let recent_count = 10.min(history.len());
            let recent_prices: Vec<Decimal> = history
                .iter()
                .rev()
                .take(recent_count)
                .map(|(_, price)| *price)
                .collect();

            if recent_prices.len() < 2 {
                return false;
            }

            // 计算最大价格变动
            let min_price = recent_prices.iter().cloned().min().unwrap_or(Decimal::ZERO);
            let max_price = recent_prices.iter().cloned().max().unwrap_or(Decimal::ZERO);

            if min_price > Decimal::ZERO {
                let change = (max_price - min_price) / min_price;
                if change > self.config.black_swan_threshold {
                    warn!(
                        "Black swan detected for {}: price changed {}% in {} ticks",
                        symbol,
                        change * Decimal::from(100),
                        recent_count
                    );
                    return true;
                }
            }
        }

        false
    }

    /// Kelly 公式仓位计算
    fn calculate_kelly_position(&self, order: &OrderRequest, state: &RiskState) -> Decimal {
        // 简化版本：基于历史胜率和盈亏比
        // 实际应该从交易历史中计算

        // 默认使用保守的 1/4 Kelly
        let base_kelly = self.config.kelly_fraction;

        // 波动率调整
        if let Some(history) = state.price_history.get(&order.symbol) {
            if history.len() >= 2 {
                let prices: Vec<Decimal> = history.iter().map(|(_, p)| *p).collect();
                let volatility = self.calculate_volatility(&prices);

                // 波动率越高，仓位越小
                if volatility > self.config.volatility_target {
                    let adjustment = self.config.volatility_target / volatility;
                    return order.quantity * base_kelly * adjustment;
                }
            }
        }

        order.quantity * base_kelly
    }

    /// 计算波动率
    fn calculate_volatility(&self, prices: &[Decimal]) -> Decimal {
        if prices.len() < 2 {
            return Decimal::ZERO;
        }

        // 计算收益率
        let returns: Vec<Decimal> = prices
            .windows(2)
            .map(|w| {
                if w[0] > Decimal::ZERO {
                    (w[1] - w[0]) / w[0]
                } else {
                    Decimal::ZERO
                }
            })
            .collect();

        if returns.is_empty() {
            return Decimal::ZERO;
        }

        // 计算标准差
        let mean = returns.iter().sum::<Decimal>() / Decimal::from(returns.len());
        let variance = returns
            .iter()
            .map(|r| (*r - mean) * (*r - mean))
            .sum::<Decimal>()
            / Decimal::from(returns.len());

        // 近似平方根
        self.approximate_sqrt(variance)
    }

    /// 近似平方根 (牛顿法)
    fn approximate_sqrt(&self, value: Decimal) -> Decimal {
        if value <= Decimal::ZERO {
            return Decimal::ZERO;
        }

        let mut x = value / Decimal::from(2);
        for _ in 0..10 {
            x = (x + value / x) / Decimal::from(2);
        }
        x
    }
}

/// 风控状态摘要
#[derive(Debug, Clone)]
pub struct RiskStatus {
    pub daily_pnl: Decimal,
    pub peak_equity: Decimal,
    pub current_equity: Decimal,
    pub daily_trade_count: u32,
    pub is_circuit_breaker_active: bool,
    pub position_count: usize,
}

/// 风控错误
#[derive(Debug, thiserror::Error)]
pub enum RiskError {
    #[error("Risk check failed: {0}")]
    CheckFailed(String),

    #[error("Invalid configuration: {0}")]
    InvalidConfig(String),
}
