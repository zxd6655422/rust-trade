use crate::data::types::{OHLCData, TickData};
use rust_decimal::Decimal;
use std::collections::HashMap;

/// 信号意图
#[derive(Debug, Clone, PartialEq)]
pub enum SignalIntent {
    /// 开仓信号 (开新仓)
    Entry,
    /// 平仓信号 (止损/止盈/趋势反转，只平仓不开新仓)
    Exit,
    /// 反手信号 (平旧仓 + 开新仓)
    Reverse,
}

#[derive(Debug, Clone)]
pub enum Signal {
    Buy {
        symbol: String,
        quantity: Decimal,
        entry_price: Decimal,
        intent: SignalIntent,
        /// 策略计算的止损价（可选，None 则由执行层用默认百分比计算）
        stop_loss: Option<Decimal>,
    },
    Sell {
        symbol: String,
        quantity: Decimal,
        entry_price: Decimal,
        intent: SignalIntent,
        /// 策略计算的止损价（可选，None 则由执行层用默认百分比计算）
        stop_loss: Option<Decimal>,
    },
    Hold,
}

pub trait Strategy: Send + Sync {
    fn name(&self) -> &str;
    fn on_tick(&mut self, tick: &TickData) -> Signal;
    fn initialize(&mut self, params: HashMap<String, String>) -> Result<(), String>;

    /// Reset strategy state for new backtest
    fn reset(&mut self) {
        // Default implementation does nothing
        // Strategies can override if needed
    }

    fn on_ohlc(&mut self, _ohlc: &OHLCData) -> Signal {
        Signal::Hold
    }
    fn supports_ohlc(&self) -> bool {
        false
    }
    fn preferred_timeframe(&self) -> Option<crate::data::types::Timeframe> {
        None
    }
}
