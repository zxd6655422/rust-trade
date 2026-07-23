//! MA Trend Pullback Strategy - Backtest version
//!
//! Wraps the analysis strategy for use in the backtest engine.
//! Maintains a rolling window of OHLC bars and triggers signals
//! when MA crossover conditions are met.
//!
//! Supports multiple take profit modes:
//! - Trailing: activate + callback from peak profit
//! - Ma48: MA48 crossover confirmation
//! - Bb: Bollinger Band position
//! - None: only stop loss

use super::base::{Signal, Strategy};
use crate::data::types::{OHLCData, Timeframe, TickData};
use rust_decimal::prelude::ToPrimitive;
use rust_decimal::Decimal;
use std::collections::{HashMap, VecDeque};

/// Stop loss mode
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StopMode {
    /// Fixed percentage stop loss
    Fixed,
    /// Stop when price crosses MA288
    Ma288,
}

/// Take profit mode
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TakeProfitMode {
    /// Trailing stop (activate + callback)
    Trailing,
    /// MA48 crossover confirmation
    Ma48,
    /// Bollinger Band position
    Bb,
    /// No take profit (only stop loss)
    None,
}

/// Backtest-compatible MA Trend Pullback strategy
pub struct MATrendPullbackBacktestStrategy {
    // Parameters
    fast_ma_period: usize,
    slow_ma_period: usize,
    stop_mode: StopMode,
    fixed_stop_pct: f64,
    take_profit_mode: TakeProfitMode,
    trailing_activate_pct: f64,
    trailing_callback_pct: f64,
    ma48_tp_bars: usize,
    bb_tp_pct: f64,

    // State
    bars: VecDeque<Bar>,
    position: Position,
    max_profit_pct: f64,
    ma48_cross_count: usize,
    last_signal: Option<Signal>,
}

#[derive(Debug, Clone)]
struct Bar {
    open: f64,
    high: f64,
    low: f64,
    close: f64,
    volume: f64,
}

#[derive(Debug, Clone, PartialEq)]
enum Position {
    None,
    Long { entry_price: f64 },
    Short { entry_price: f64 },
}

impl MATrendPullbackBacktestStrategy {
    pub fn new() -> Self {
        Self {
            fast_ma_period: 288,
            slow_ma_period: 488,
            stop_mode: StopMode::Ma288,
            fixed_stop_pct: 2.0,
            take_profit_mode: TakeProfitMode::Trailing,
            trailing_activate_pct: 5.0,
            trailing_callback_pct: 5.0,
            ma48_tp_bars: 3,
            bb_tp_pct: 90.0,
            bars: VecDeque::new(),
            position: Position::None,
            max_profit_pct: 0.0,
            ma48_cross_count: 0,
            last_signal: None,
        }
    }

    /// Calculate SMA from the last N bars
    fn calculate_sma(&self, period: usize) -> Option<f64> {
        if self.bars.len() < period {
            return None;
        }
        let sum: f64 = self.bars.iter().rev().take(period).map(|b| b.close).sum();
        Some(sum / period as f64)
    }

    /// Get the previous bar's SMA (for crossover detection)
    fn calculate_sma_prev(&self, period: usize) -> Option<f64> {
        if self.bars.len() < period + 1 {
            return None;
        }
        let sum: f64 = self.bars.iter().rev().skip(1).take(period).map(|b| b.close).sum();
        Some(sum / period as f64)
    }

    /// Calculate current PnL percentage
    fn current_pnl_pct(&self, current_price: f64) -> f64 {
        match &self.position {
            Position::Long { entry_price } => {
                (current_price - entry_price) / entry_price * 100.0
            }
            Position::Short { entry_price } => {
                (entry_price - current_price) / entry_price * 100.0
            }
            Position::None => 0.0,
        }
    }

    /// Calculate Bollinger Band position (0-100%)
    fn calculate_bb_position(&self, period: usize) -> Option<f64> {
        if self.bars.len() < period {
            return None;
        }

        let closes: Vec<f64> = self.bars.iter().rev().take(period).map(|b| b.close).collect();
        let mean: f64 = closes.iter().sum::<f64>() / period as f64;
        let variance: f64 = closes.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / period as f64;
        let std = variance.sqrt();

        let upper = mean + 2.0 * std;
        let lower = mean - 2.0 * std;
        let current_price = *closes.first()?;

        if upper - lower > 0.0 {
            Some((current_price - lower) / (upper - lower) * 100.0)
        } else {
            Some(50.0)
        }
    }

    /// Check stop loss condition
    fn check_stop_loss(&self, current_price: f64) -> bool {
        match &self.position {
            Position::None => false,
            Position::Long { .. } => match self.stop_mode {
                StopMode::Fixed => {
                    let pnl = self.current_pnl_pct(current_price);
                    pnl < -self.fixed_stop_pct
                }
                StopMode::Ma288 => {
                    let fast_ma = self.calculate_sma(self.fast_ma_period);
                    let prev_fast_ma = self.calculate_sma_prev(self.fast_ma_period);
                    if let (Some(ma), Some(prev_ma)) = (fast_ma, prev_fast_ma) {
                        let prev_price = self.bars.back().map(|b| b.close).unwrap_or(current_price);
                        prev_price > prev_ma && current_price < ma
                    } else {
                        false
                    }
                }
            },
            Position::Short { .. } => match self.stop_mode {
                StopMode::Fixed => {
                    let pnl = self.current_pnl_pct(current_price);
                    pnl < -self.fixed_stop_pct
                }
                StopMode::Ma288 => {
                    let fast_ma = self.calculate_sma(self.fast_ma_period);
                    let prev_fast_ma = self.calculate_sma_prev(self.fast_ma_period);
                    if let (Some(ma), Some(prev_ma)) = (fast_ma, prev_fast_ma) {
                        let prev_price = self.bars.back().map(|b| b.close).unwrap_or(current_price);
                        prev_price < prev_ma && current_price > ma
                    } else {
                        false
                    }
                }
            },
        }
    }

    /// Check take profit condition
    fn check_take_profit(&self, current_price: f64) -> bool {
        match &self.position {
            Position::None => false,
            Position::Long { .. } | Position::Short { .. } => {
                let current_pnl = self.current_pnl_pct(current_price);

                match self.take_profit_mode {
                    TakeProfitMode::Trailing => {
                        if self.max_profit_pct < self.trailing_activate_pct {
                            return false;
                        }
                        let drawdown = self.max_profit_pct - current_pnl;
                        drawdown >= self.trailing_callback_pct
                    }
                    TakeProfitMode::Ma48 => {
                        // MA48 crossover check is handled in process_bar
                        self.ma48_cross_count >= self.ma48_tp_bars
                    }
                    TakeProfitMode::Bb => {
                        if let Some(bb_pos) = self.calculate_bb_position(100) {
                            match &self.position {
                                Position::Long { .. } => bb_pos >= self.bb_tp_pct,
                                Position::Short { .. } => bb_pos <= (100.0 - self.bb_tp_pct),
                                _ => false,
                            }
                        } else {
                            false
                        }
                    }
                    TakeProfitMode::None => false,
                }
            }
        }
    }

    /// Check trend reversal
    fn check_trend_reversal(&self, current_price: f64) -> bool {
        if let (Some(fast_ma), Some(slow_ma)) = (
            self.calculate_sma(self.fast_ma_period),
            self.calculate_sma(self.slow_ma_period),
        ) {
            match &self.position {
                Position::Long { .. } => fast_ma < slow_ma,
                Position::Short { .. } => fast_ma > slow_ma,
                Position::None => false,
            }
        } else {
            false
        }
    }

    /// Process a bar and return signal
    fn process_bar(&mut self, bar: Bar, symbol: &str, price_decimal: Decimal) -> Signal {
        // Add bar to history
        self.bars.push_back(bar.clone());

        // Keep reasonable history length
        let max_bars = self.slow_ma_period * 2;
        while self.bars.len() > max_bars {
            self.bars.pop_front();
        }

        let current_price = bar.close;

        // Check exit conditions if in position
        if self.position != Position::None {
            // Update max profit
            let current_pnl = self.current_pnl_pct(current_price);
            self.max_profit_pct = self.max_profit_pct.max(current_pnl);

            // Update MA48 cross count for MA48 take profit mode
            if self.take_profit_mode == TakeProfitMode::Ma48 {
                if let Some(ma48) = self.calculate_sma(48) {
                    let is_cross = match &self.position {
                        Position::Long { .. } => current_price < ma48,
                        Position::Short { .. } => current_price > ma48,
                        Position::None => false,
                    };
                    if is_cross {
                        self.ma48_cross_count += 1;
                    } else {
                        self.ma48_cross_count = 0;
                    }
                }
            }

            // Check stop loss
            if self.check_stop_loss(current_price) {
                let signal = match &self.position {
                    Position::Long { .. } => Signal::Sell {
                        symbol: symbol.to_string(),
                        quantity: Decimal::from(100),
                        entry_price: price_decimal,
                    },
                    Position::Short { .. } => Signal::Buy {
                        symbol: symbol.to_string(),
                        quantity: Decimal::from(100),
                        entry_price: price_decimal,
                    },
                    Position::None => unreachable!(),
                };
                self.position = Position::None;
                self.max_profit_pct = 0.0;
                self.ma48_cross_count = 0;
                self.last_signal = Some(signal.clone());
                return signal;
            }

            // Check take profit
            if self.check_take_profit(current_price) {
                let signal = match &self.position {
                    Position::Long { .. } => Signal::Sell {
                        symbol: symbol.to_string(),
                        quantity: Decimal::from(100),
                        entry_price: price_decimal,
                    },
                    Position::Short { .. } => Signal::Buy {
                        symbol: symbol.to_string(),
                        quantity: Decimal::from(100),
                        entry_price: price_decimal,
                    },
                    Position::None => unreachable!(),
                };
                self.position = Position::None;
                self.max_profit_pct = 0.0;
                self.ma48_cross_count = 0;
                self.last_signal = Some(signal.clone());
                return signal;
            }

            // Check trend reversal
            if self.check_trend_reversal(current_price) {
                let signal = match &self.position {
                    Position::Long { .. } => Signal::Sell {
                        symbol: symbol.to_string(),
                        quantity: Decimal::from(100),
                        entry_price: price_decimal,
                    },
                    Position::Short { .. } => Signal::Buy {
                        symbol: symbol.to_string(),
                        quantity: Decimal::from(100),
                        entry_price: price_decimal,
                    },
                    Position::None => unreachable!(),
                };
                self.position = Position::None;
                self.max_profit_pct = 0.0;
                self.ma48_cross_count = 0;
                self.last_signal = Some(signal.clone());
                return signal;
            }
        }

        // Check entry conditions
        if let (Some(fast_ma), Some(slow_ma), Some(prev_fast_ma)) = (
            self.calculate_sma(self.fast_ma_period),
            self.calculate_sma(self.slow_ma_period),
            self.calculate_sma_prev(self.fast_ma_period),
        ) {
            let prev_price = self.bars.iter().rev().nth(1).map(|b| b.close).unwrap_or(current_price);

            // Bullish trend: MA288 > MA488
            if fast_ma > slow_ma {
                // Entry: price crosses above MA288
                if prev_price < prev_fast_ma && current_price > fast_ma {
                    // Close short if exists
                    if matches!(self.position, Position::Short { .. }) {
                        let signal = Signal::Buy {
                            symbol: symbol.to_string(),
                            quantity: Decimal::from(100),
                            entry_price: price_decimal,
                        };
                        self.position = Position::Long { entry_price: current_price };
                        self.max_profit_pct = 0.0;
                        self.ma48_cross_count = 0;
                        self.last_signal = Some(signal.clone());
                        return signal;
                    }

                    // Open long
                    if self.position == Position::None {
                        let signal = Signal::Buy {
                            symbol: symbol.to_string(),
                            quantity: Decimal::from(100),
                            entry_price: price_decimal,
                        };
                        self.position = Position::Long { entry_price: current_price };
                        self.max_profit_pct = 0.0;
                        self.ma48_cross_count = 0;
                        self.last_signal = Some(signal.clone());
                        return signal;
                    }
                }
            }
            // Bearish trend: MA288 < MA488
            else if fast_ma < slow_ma {
                // Entry: price crosses below MA288
                if prev_price > prev_fast_ma && current_price < fast_ma {
                    // Close long if exists
                    if matches!(self.position, Position::Long { .. }) {
                        let signal = Signal::Sell {
                            symbol: symbol.to_string(),
                            quantity: Decimal::from(100),
                            entry_price: price_decimal,
                        };
                        self.position = Position::Short { entry_price: current_price };
                        self.max_profit_pct = 0.0;
                        self.ma48_cross_count = 0;
                        self.last_signal = Some(signal.clone());
                        return signal;
                    }

                    // Open short
                    if self.position == Position::None {
                        let signal = Signal::Sell {
                            symbol: symbol.to_string(),
                            quantity: Decimal::from(100),
                            entry_price: price_decimal,
                        };
                        self.position = Position::Short { entry_price: current_price };
                        self.max_profit_pct = 0.0;
                        self.ma48_cross_count = 0;
                        self.last_signal = Some(signal.clone());
                        return signal;
                    }
                }
            }
        }

        Signal::Hold
    }
}

impl Strategy for MATrendPullbackBacktestStrategy {
    fn name(&self) -> &str {
        "MA Trend Pullback"
    }

    fn initialize(&mut self, params: HashMap<String, String>) -> Result<(), String> {
        if let Some(fast) = params.get("fast_ma_period") {
            self.fast_ma_period = fast.parse().map_err(|_| "Invalid fast_ma_period")?;
        }
        if let Some(slow) = params.get("slow_ma_period") {
            self.slow_ma_period = slow.parse().map_err(|_| "Invalid slow_ma_period")?;
        }
        if let Some(stop_mode) = params.get("stop_mode") {
            self.stop_mode = match stop_mode.as_str() {
                "fixed" => StopMode::Fixed,
                "ma288" => StopMode::Ma288,
                _ => return Err("Invalid stop_mode".to_string()),
            };
        }
        if let Some(fixed_stop) = params.get("fixed_stop_pct") {
            self.fixed_stop_pct = fixed_stop.parse().map_err(|_| "Invalid fixed_stop_pct")?;
        }
        if let Some(tp_mode) = params.get("take_profit_mode") {
            self.take_profit_mode = match tp_mode.as_str() {
                "trailing" => TakeProfitMode::Trailing,
                "ma48" => TakeProfitMode::Ma48,
                "bb" => TakeProfitMode::Bb,
                "none" => TakeProfitMode::None,
                _ => return Err("Invalid take_profit_mode".to_string()),
            };
        }
        if let Some(activate) = params.get("trailing_activate_pct") {
            self.trailing_activate_pct = activate.parse().map_err(|_| "Invalid trailing_activate_pct")?;
        }
        if let Some(callback) = params.get("trailing_callback_pct") {
            self.trailing_callback_pct = callback.parse().map_err(|_| "Invalid trailing_callback_pct")?;
        }
        if let Some(bars) = params.get("ma48_tp_bars") {
            self.ma48_tp_bars = bars.parse().map_err(|_| "Invalid ma48_tp_bars")?;
        }
        if let Some(bb_pct) = params.get("bb_tp_pct") {
            self.bb_tp_pct = bb_pct.parse().map_err(|_| "Invalid bb_tp_pct")?;
        }

        if self.fast_ma_period >= self.slow_ma_period {
            return Err("Fast MA period must be less than slow MA period".to_string());
        }

        println!(
            "MA Trend Pullback Strategy initialized: fast_ma={}, slow_ma={}, stop={:?}, tp={:?}",
            self.fast_ma_period, self.slow_ma_period, self.stop_mode, self.take_profit_mode
        );
        Ok(())
    }

    fn reset(&mut self) {
        self.bars.clear();
        self.position = Position::None;
        self.max_profit_pct = 0.0;
        self.ma48_cross_count = 0;
        self.last_signal = None;
    }

    fn on_tick(&mut self, tick: &TickData) -> Signal {
        let bar = Bar {
            open: tick.price.to_f64().unwrap_or(0.0),
            high: tick.price.to_f64().unwrap_or(0.0),
            low: tick.price.to_f64().unwrap_or(0.0),
            close: tick.price.to_f64().unwrap_or(0.0),
            volume: 0.0,
        };

        self.process_bar(bar, &tick.symbol, tick.price)
    }

    fn on_ohlc(&mut self, ohlc: &OHLCData) -> Signal {
        let bar = Bar {
            open: ohlc.open.to_f64().unwrap_or(0.0),
            high: ohlc.high.to_f64().unwrap_or(0.0),
            low: ohlc.low.to_f64().unwrap_or(0.0),
            close: ohlc.close.to_f64().unwrap_or(0.0),
            volume: ohlc.volume.to_f64().unwrap_or(0.0),
        };

        self.process_bar(bar, &ohlc.symbol, ohlc.close)
    }

    fn supports_ohlc(&self) -> bool {
        true
    }

    fn preferred_timeframe(&self) -> Option<Timeframe> {
        Some(Timeframe::ThirtyMinutes)
    }
}
