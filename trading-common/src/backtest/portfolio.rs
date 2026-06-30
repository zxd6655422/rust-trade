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
        }
    }

    pub fn with_commission_rate(mut self, rate: Decimal) -> Self {
        self.commission_rate = rate;
        self
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
}
