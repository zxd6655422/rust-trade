use crate::backtest::metrics::BacktestMetrics;
use crate::data::types::TradeSide;
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use std::collections::HashMap;
use std::str::FromStr;

/// 持仓方向
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PositionSide {
    /// 多头
    Long,
    /// 空头
    Short,
}

#[derive(Debug, Clone)]
pub struct Position {
    pub symbol: String,
    pub quantity: Decimal,
    pub avg_price: Decimal,
    pub market_value: Decimal,
    pub unrealized_pnl: Decimal,
    pub side: PositionSide,
}

#[derive(Debug, Clone)]
pub struct Trade {
    pub symbol: String,
    pub side: TradeSide,
    pub quantity: Decimal,
    pub price: Decimal,
    pub timestamp: DateTime<Utc>,
    pub realized_pnl: Option<Decimal>,
    pub commission: Decimal,
}

pub struct Portfolio {
    pub initial_capital: Decimal,
    pub cash: Decimal,
    pub positions: HashMap<String, Position>,
    pub trades: Vec<Trade>,
    pub current_prices: HashMap<String, Decimal>,
    pub commission_rate: Decimal, // e.g., 0.001 for 0.1%
    pub slippage_pct: Decimal,    // e.g., 0.0001 for 0.01%
    pub total_slippage_cost: Decimal,
}

impl Portfolio {
    pub fn new(initial_capital: Decimal) -> Self {
        Self {
            initial_capital,
            cash: initial_capital,
            positions: HashMap::new(),
            trades: Vec::new(),
            current_prices: HashMap::new(),
            commission_rate: Decimal::from_str("0.001").unwrap_or(Decimal::ZERO), // 0.1% default
            slippage_pct: Decimal::from_str("0.0001").unwrap_or(Decimal::ZERO), // 0.01% default
            total_slippage_cost: Decimal::ZERO,
        }
    }

    pub fn with_commission_rate(mut self, rate: Decimal) -> Self {
        self.commission_rate = rate;
        self
    }

    pub fn with_slippage_pct(mut self, slippage_pct: Decimal) -> Self {
        self.slippage_pct = slippage_pct;
        self
    }

    /// 应用滑点：买入时价格上浮，卖出时价格下浮
    /// 返回 (调整后价格, 每单位滑点成本)
    fn apply_slippage(&self, price: Decimal, side: &TradeSide) -> Decimal {
        if self.slippage_pct.is_zero() {
            return price;
        }
        let slippage = price * self.slippage_pct;
        match side {
            TradeSide::Buy => price + slippage,
            TradeSide::Sell => price - slippage,
        }
    }

    pub fn update_price(&mut self, symbol: &str, price: Decimal) {
        self.current_prices.insert(symbol.to_string(), price);

        // Update position market value and unrealized PnL
        if let Some(position) = self.positions.get_mut(symbol) {
            match position.side {
                PositionSide::Long => {
                    position.market_value = position.quantity * price;
                    position.unrealized_pnl = (price - position.avg_price) * position.quantity;
                }
                PositionSide::Short => {
                    // 空头市值 = 数量 * 开仓均价 - 数量 * 当前价 = 数量 * (均价 - 现价)
                    position.market_value = position.quantity * (position.avg_price * Decimal::from(2) - price);
                    position.unrealized_pnl = (position.avg_price - price) * position.quantity;
                }
            }
        }
    }

    pub fn execute_buy(
        &mut self,
        symbol: String,
        quantity: Decimal,
        price: Decimal,
    ) -> Result<(), String> {
        let original_price = price;
        let price = self.apply_slippage(price, &TradeSide::Buy);
        self.total_slippage_cost += (price - original_price) * quantity;
        let cost = quantity * price;
        let commission = cost * self.commission_rate;
        let total_cost = cost + commission;

        if total_cost > self.cash {
            return Err(format!(
                "Insufficient funds: need ${}, available ${}",
                total_cost, self.cash
            ));
        }

        self.cash -= total_cost;

        match self.positions.get_mut(&symbol) {
            Some(position) => {
                let total_quantity = position.quantity + quantity;
                let total_cost = position.quantity * position.avg_price + cost;
                position.avg_price = total_cost / total_quantity;
                position.quantity = total_quantity;
                position.market_value = total_quantity * price;
                position.unrealized_pnl = (price - position.avg_price) * total_quantity;
            }
            None => {
                self.positions.insert(
                    symbol.clone(),
                    Position {
                        symbol: symbol.clone(),
                        quantity,
                        avg_price: price,
                        market_value: quantity * price,
                        unrealized_pnl: Decimal::ZERO,
                        side: PositionSide::Long,
                    },
                );
            }
        }

        self.trades.push(Trade {
            symbol,
            side: TradeSide::Buy,
            quantity,
            price,
            timestamp: Utc::now(),
            realized_pnl: None,
            commission,
        });

        Ok(())
    }

    pub fn execute_sell(
        &mut self,
        symbol: String,
        quantity: Decimal,
        price: Decimal,
    ) -> Result<(), String> {
        let original_price = price;
        let price = self.apply_slippage(price, &TradeSide::Sell);
        self.total_slippage_cost += (original_price - price) * quantity;
        let position = self
            .positions
            .get_mut(&symbol)
            .ok_or("No position to sell")?;

        if quantity > position.quantity {
            return Err(format!(
                "Insufficient position: need {}, available {}",
                quantity, position.quantity
            ));
        }

        let proceeds = quantity * price;
        let commission = proceeds * self.commission_rate;
        let net_proceeds = proceeds - commission;

        self.cash += net_proceeds;

        // Calculate realized PnL
        let realized_pnl = (price - position.avg_price) * quantity - commission;

        position.quantity -= quantity;
        if position.quantity == Decimal::ZERO {
            self.positions.remove(&symbol);
        } else {
            position.market_value = position.quantity * price;
            position.unrealized_pnl = (price - position.avg_price) * position.quantity;
        }

        self.trades.push(Trade {
            symbol,
            side: TradeSide::Sell,
            quantity,
            price,
            timestamp: Utc::now(),
            realized_pnl: Some(realized_pnl),
            commission,
        });

        Ok(())
    }

    /// 开空仓：借入并卖出，获得 proceeds - commission
    pub fn execute_short_open(
        &mut self,
        symbol: String,
        quantity: Decimal,
        price: Decimal,
    ) -> Result<(), String> {
        let original_price = price;
        let price = self.apply_slippage(price, &TradeSide::Sell); // 开空 = 卖出
        self.total_slippage_cost += (original_price - price) * quantity;
        let proceeds = quantity * price;
        let commission = proceeds * self.commission_rate;

        // 开空获得 proceeds，扣除手续费
        self.cash += proceeds - commission;

        match self.positions.get_mut(&symbol) {
            Some(pos) if pos.side == PositionSide::Short => {
                let total_quantity = pos.quantity + quantity;
                let total_cost = pos.quantity * pos.avg_price + proceeds;
                pos.avg_price = total_cost / total_quantity;
                pos.quantity = total_quantity;
                pos.unrealized_pnl = (pos.avg_price - price) * total_quantity;
            }
            _ => {
                self.positions.insert(
                    symbol.clone(),
                    Position {
                        symbol: symbol.clone(),
                        quantity,
                        avg_price: price,
                        market_value: Decimal::ZERO,
                        unrealized_pnl: Decimal::ZERO,
                        side: PositionSide::Short,
                    },
                );
            }
        }

        self.trades.push(Trade {
            symbol,
            side: TradeSide::Sell,
            quantity,
            price,
            timestamp: Utc::now(),
            realized_pnl: None,
            commission,
        });

        Ok(())
    }

    /// 平空仓：买入归还，支付 cost + commission
    pub fn execute_short_close(
        &mut self,
        symbol: String,
        quantity: Decimal,
        price: Decimal,
    ) -> Result<(), String> {
        let original_price = price;
        let price = self.apply_slippage(price, &TradeSide::Buy); // 平空 = 买入
        self.total_slippage_cost += (price - original_price) * quantity;
        let position = self
            .positions
            .get_mut(&symbol)
            .ok_or("No short position to close")?;

        if position.side != PositionSide::Short {
            return Err("Position is not a short".to_string());
        }

        if quantity > position.quantity {
            return Err(format!(
                "Insufficient short position: need {}, available {}",
                quantity, position.quantity
            ));
        }

        let cost = quantity * price;
        let commission = cost * self.commission_rate;

        // 平空需要支付 cost + commission
        self.cash -= cost + commission;

        // 盈亏 = 开仓 proceeds - 平仓 cost - 所有手续费
        let open_proceeds = quantity * position.avg_price;
        let open_commission = open_proceeds * self.commission_rate;
        let realized_pnl = open_proceeds - cost - open_commission - commission;

        position.quantity -= quantity;
        if position.quantity == Decimal::ZERO {
            self.positions.remove(&symbol);
        } else {
            position.unrealized_pnl = (position.avg_price - price) * position.quantity;
        }

        self.trades.push(Trade {
            symbol,
            side: TradeSide::Buy,
            quantity,
            price,
            timestamp: Utc::now(),
            realized_pnl: Some(realized_pnl),
            commission,
        });

        Ok(())
    }

    pub fn total_value(&self) -> Decimal {
        let mut total = self.cash;

        for position in self.positions.values() {
            match position.side {
                PositionSide::Long => {
                    // 多头：市值 = 数量 * 当前价
                    total += position.market_value;
                }
                PositionSide::Short => {
                    // 空头：cash 已包含开仓 proceeds，加上未实现盈亏即可
                    total += position.unrealized_pnl;
                }
            }
        }

        total
    }

    pub fn total_realized_pnl(&self) -> Decimal {
        self.trades
            .iter()
            .filter_map(|trade| trade.realized_pnl)
            .sum()
    }

    pub fn total_unrealized_pnl(&self) -> Decimal {
        self.positions.values().map(|pos| pos.unrealized_pnl).sum()
    }

    pub fn total_pnl(&self) -> Decimal {
        self.total_realized_pnl() + self.total_unrealized_pnl()
    }

    pub fn total_commission(&self) -> Decimal {
        self.trades.iter().map(|trade| trade.commission).sum()
    }

    pub fn has_position(&self, symbol: &str) -> bool {
        self.positions.contains_key(symbol)
            && self.positions.get(symbol).unwrap().quantity > Decimal::ZERO
    }

    /// 是否有多头持仓
    pub fn has_long_position(&self, symbol: &str) -> bool {
        self.positions
            .get(symbol)
            .map_or(false, |p| p.side == PositionSide::Long && p.quantity > Decimal::ZERO)
    }

    /// 是否有空头持仓
    pub fn has_short_position(&self, symbol: &str) -> bool {
        self.positions
            .get(symbol)
            .map_or(false, |p| p.side == PositionSide::Short && p.quantity > Decimal::ZERO)
    }

    /// 获取持仓方向
    pub fn get_position_side(&self, symbol: &str) -> Option<PositionSide> {
        self.positions.get(symbol).and_then(|p| {
            if p.quantity > Decimal::ZERO {
                Some(p.side)
            } else {
                None
            }
        })
    }

    pub fn get_equity_curve(&self) -> Vec<Decimal> {
        let mut equity_curve = vec![self.initial_capital];
        let mut running_cash = self.initial_capital;
        let mut running_positions: HashMap<String, (Decimal, Decimal)> = HashMap::new(); // (quantity, avg_price)

        for trade in &self.trades {
            match trade.side {
                TradeSide::Buy => {
                    running_cash -= trade.quantity * trade.price + trade.commission;
                    let (curr_qty, curr_avg) = running_positions
                        .get(&trade.symbol)
                        .unwrap_or(&(Decimal::ZERO, Decimal::ZERO));
                    let new_qty = curr_qty + trade.quantity;
                    let new_avg = if new_qty > Decimal::ZERO {
                        (curr_qty * curr_avg + trade.quantity * trade.price) / new_qty
                    } else {
                        Decimal::ZERO
                    };
                    running_positions.insert(trade.symbol.clone(), (new_qty, new_avg));
                }
                TradeSide::Sell => {
                    running_cash += trade.quantity * trade.price - trade.commission;
                    if let Some((curr_qty, _)) = running_positions.get_mut(&trade.symbol) {
                        *curr_qty -= trade.quantity;
                        if *curr_qty <= Decimal::ZERO {
                            running_positions.remove(&trade.symbol);
                        }
                    }
                }
            }

            // Calculate current portfolio value
            let mut portfolio_value = running_cash;
            for (symbol, (quantity, _)) in &running_positions {
                if let Some(current_price) = self.current_prices.get(symbol) {
                    portfolio_value += quantity * current_price;
                }
            }
            equity_curve.push(portfolio_value);
        }

        equity_curve
    }

    // ===== 风险指标便捷方法 =====

    /// 计算收益率序列（基于权益曲线）
    pub fn returns(&self) -> Vec<Decimal> {
        let equity_curve = self.get_equity_curve();
        if equity_curve.len() < 2 {
            return Vec::new();
        }

        equity_curve
            .windows(2)
            .filter_map(|window| {
                let prev = window[0];
                let curr = window[1];
                if prev > Decimal::ZERO {
                    Some((curr - prev) / prev)
                } else {
                    None
                }
            })
            .collect()
    }

    /// 计算夏普比率
    ///
    /// Sharpe Ratio = (Mean Return - Risk Free Rate) / Standard Deviation
    pub fn sharpe_ratio(&self, risk_free_rate: Decimal) -> Decimal {
        let returns = self.returns();
        BacktestMetrics::calculate_sharpe_ratio(&returns, risk_free_rate)
    }

    /// 计算最大回撤
    ///
    /// Max Drawdown = Max((Peak - Trough) / Peak)
    pub fn max_drawdown(&self) -> Decimal {
        let equity_curve = self.get_equity_curve();
        BacktestMetrics::calculate_max_drawdown(&equity_curve)
    }

    /// 计算索提诺比率（只考虑下行风险）
    ///
    /// Sortino Ratio = (Mean Return - Risk Free Rate) / Downside Deviation
    pub fn sortino_ratio(&self, risk_free_rate: Decimal) -> Decimal {
        let returns = self.returns();
        BacktestMetrics::calculate_sortino_ratio(&returns, risk_free_rate, risk_free_rate)
    }

    /// 计算风险价值 (VaR)
    ///
    /// VaR = 在给定置信水平下的最大预期损失
    pub fn value_at_risk(&self, confidence_level: Decimal) -> Decimal {
        let returns = self.returns();
        BacktestMetrics::calculate_var(&returns, confidence_level)
    }

    /// 计算卡尔玛比率（年化收益 / 最大回撤）
    pub fn calmar_ratio(&self, annual_return: Decimal) -> Decimal {
        let max_dd = self.max_drawdown();
        BacktestMetrics::calculate_calmar_ratio(annual_return, max_dd)
    }

    /// 计算胜率
    pub fn win_rate(&self) -> Decimal {
        BacktestMetrics::calculate_win_rate(&self.trades)
    }

    /// 计算盈亏比（总盈利 / 总亏损）
    pub fn profit_factor(&self) -> Decimal {
        BacktestMetrics::calculate_profit_factor(&self.trades)
    }

    /// 计算平均交易时长（秒）
    pub fn average_trade_duration(&self) -> f64 {
        BacktestMetrics::calculate_average_trade_duration(&self.trades)
    }

    /// 计算年化收益率
    ///
    /// 基于初始资金和当前总价值计算年化收益（简化计算）
    pub fn annualized_return(&self) -> Decimal {
        if self.trades.is_empty() {
            return Decimal::ZERO;
        }

        let total_value = self.total_value();
        if self.initial_capital <= Decimal::ZERO {
            return Decimal::ZERO;
        }

        let total_return = (total_value - self.initial_capital) / self.initial_capital;

        // 计算交易天数
        let first_trade_time = self.trades.first().map(|t| t.timestamp);
        let last_trade_time = self.trades.last().map(|t| t.timestamp);

        match (first_trade_time, last_trade_time) {
            (Some(first), Some(last)) => {
                let days = (last - first).num_days() as f64;
                if days <= 0.0 {
                    return total_return;
                }
                // 简化年化: total_return * (365 / days)
                let years = Decimal::from_str(&(days / 365.0).to_string()).unwrap_or(Decimal::ONE);
                if years > Decimal::ZERO {
                    total_return / years
                } else {
                    total_return
                }
            }
            _ => Decimal::ZERO,
        }
    }

    /// 获取完整的风险指标摘要
    pub fn risk_summary(&self, risk_free_rate: Decimal) -> RiskSummary {
        RiskSummary {
            total_return: self.total_pnl() / self.initial_capital,
            annualized_return: self.annualized_return(),
            sharpe_ratio: self.sharpe_ratio(risk_free_rate),
            sortino_ratio: self.sortino_ratio(risk_free_rate),
            max_drawdown: self.max_drawdown(),
            win_rate: self.win_rate(),
            profit_factor: self.profit_factor(),
            total_trades: self.trades.len(),
            total_commission: self.total_commission(),
            average_trade_duration_secs: self.average_trade_duration(),
        }
    }
}

/// 风险指标摘要
#[derive(Debug, Clone)]
pub struct RiskSummary {
    /// 总收益率
    pub total_return: Decimal,
    /// 年化收益率
    pub annualized_return: Decimal,
    /// 夏普比率
    pub sharpe_ratio: Decimal,
    /// 索提诺比率
    pub sortino_ratio: Decimal,
    /// 最大回撤
    pub max_drawdown: Decimal,
    /// 胜率 (%)
    pub win_rate: Decimal,
    /// 盈亏比
    pub profit_factor: Decimal,
    /// 总交易次数
    pub total_trades: usize,
    /// 总手续费
    pub total_commission: Decimal,
    /// 平均交易时长（秒）
    pub average_trade_duration_secs: f64,
}

impl std::fmt::Display for RiskSummary {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "收益: {:.2}% | 年化: {:.2}% | 夏普: {:.2} | 索提诺: {:.2} | 最大回撤: {:.2}% | 胜率: {:.1}% | 盈亏比: {:.2} | 交易: {} | 手续费: {:.2}",
            self.total_return * Decimal::from(100),
            self.annualized_return * Decimal::from(100),
            self.sharpe_ratio,
            self.sortino_ratio,
            self.max_drawdown * Decimal::from(100),
            self.win_rate,
            self.profit_factor,
            self.total_trades,
            self.total_commission,
        )
    }
}
