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
//!
//! Supports5m diffusion filter (from13th analysis optimization):
//! - use_5m_expanding: only enter when5m dual MA is expanding
//! - min_angle_5m: minimum angle threshold
//! - entry_timeframe: "30m" or "5m" for entry signal detection

use super::base::{Signal, SignalIntent, Strategy};
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
    hard_stop_pct: f64,  // Hard stop loss percentage from entry price
    take_profit_mode: TakeProfitMode,
    trailing_activate_pct: f64,
    trailing_callback_pct: f64,
    // 30m diffusion filter parameters
    use_30m_expanding: bool,
    // 5m diffusion filter parameters
    use_5m_expanding: bool,
    min_angle_5m: f64,
    entry_timeframe: String,

    // State
    bars: VecDeque<Bar>,
    bars_5m: VecDeque<Bar>,  // 5m bars for diffusion filter
    position: Position,
    entry_price: f64,  // Track entry price for hard stop
    hard_stop_price: f64,  // Hard stop price level
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
            hard_stop_pct: 0.0,  // Disabled by default
            take_profit_mode: TakeProfitMode::Trailing,
            trailing_activate_pct: 5.0,
            trailing_callback_pct: 5.0,
            use_30m_expanding: false,
            use_5m_expanding: false,
            min_angle_5m: 0.0,
            entry_timeframe: "30m".to_string(),
            bars: VecDeque::new(),
            bars_5m: VecDeque::new(),
            position: Position::None,
            entry_price: 0.0,
            hard_stop_price: 0.0,
            max_profit_pct: 0.0,
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

    /// Calculate SMA from5m bars
    fn calculate_sma_5m(&self, period: usize) -> Option<f64> {
        if self.bars_5m.len() < period {
            return None;
        }
        let sum: f64 = self.bars_5m.iter().rev().take(period).map(|b| b.close).sum();
        Some(sum / period as f64)
    }

    /// Calculate previous SMA from5m bars
    fn calculate_sma_5m_prev(&self, period: usize) -> Option<f64> {
        if self.bars_5m.len() < period + 1 {
            return None;
        }
        let sum: f64 = self.bars_5m.iter().rev().skip(1).take(period).map(|b| b.close).sum();
        Some(sum / period as f64)
    }

    /// Check if5m dual MA is expanding
    fn is_5m_expanding(&self) -> Option<bool> {
        if self.bars_5m.len() < self.slow_ma_period + 5 {
            return None;
        }

        let fast_ma = self.calculate_sma_5m(self.fast_ma_period)?;
        let slow_ma = self.calculate_sma_5m(self.slow_ma_period)?;
        let current_spread = fast_ma - slow_ma;

        // Calculate spread5 bars ago
        let prev_bars: Vec<Bar> = self.bars_5m.iter().rev().skip(5).take(self.slow_ma_period + 5).cloned().collect();
        if prev_bars.len() < self.slow_ma_period {
            return None;
        }
        let prev_fast: f64 = prev_bars.iter().rev().take(self.fast_ma_period).map(|b| b.close).sum::<f64>() / self.fast_ma_period as f64;
        let prev_slow: f64 = prev_bars.iter().rev().take(self.slow_ma_period).map(|b| b.close).sum::<f64>() / self.slow_ma_period as f64;
        let prev_spread = prev_fast - prev_slow;

        Some(current_spread.abs() > prev_spread.abs())
    }

    /// Calculate approximate angle between dual MAs (in degrees)
    fn calculate_5m_angle(&self) -> Option<f64> {
        if self.bars_5m.len() < self.slow_ma_period + 5 {
            return None;
        }

        let fast_ma = self.calculate_sma_5m(self.fast_ma_period)?;
        let slow_ma = self.calculate_sma_5m(self.slow_ma_period)?;
        let current_spread = fast_ma - slow_ma;

        // Calculate spread5 bars ago
        let prev_bars: Vec<Bar> = self.bars_5m.iter().rev().skip(5).take(self.slow_ma_period + 5).cloned().collect();
        if prev_bars.len() < self.slow_ma_period {
            return None;
        }
        let prev_fast: f64 = prev_bars.iter().rev().take(self.fast_ma_period).map(|b| b.close).sum::<f64>() / self.fast_ma_period as f64;
        let prev_slow: f64 = prev_bars.iter().rev().take(self.slow_ma_period).map(|b| b.close).sum::<f64>() / self.slow_ma_period as f64;
        let prev_spread = prev_fast - prev_slow;

        let delta = current_spread - prev_spread;
        Some(delta.atan2(5.0) * (180.0 / std::f64::consts::PI))
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

    /// Check stop loss condition (hard stop has priority)
    fn check_stop_loss(&self, bar: &Bar) -> (bool, f64) {
        match &self.position {
            Position::None => (false, 0.0),
            Position::Long { .. } => {
                // 1. Hard stop (priority) - check against bar low
                if self.hard_stop_pct > 0.0 && bar.low <= self.hard_stop_price {
                    return (true, self.hard_stop_price);
                }
                // 2. MA288 stop - check against close
                match self.stop_mode {
                    StopMode::Fixed => {
                        let pnl = self.current_pnl_pct(bar.close);
                        if pnl < -self.fixed_stop_pct {
                            (true, bar.close)
                        } else {
                            (false, 0.0)
                        }
                    }
                    StopMode::Ma288 => {
                        let fast_ma = self.calculate_sma(self.fast_ma_period);
                        let prev_fast_ma = self.calculate_sma_prev(self.fast_ma_period);
                        if let (Some(ma), Some(prev_ma)) = (fast_ma, prev_fast_ma) {
                            let prev_price = self.bars.back().map(|b| b.close).unwrap_or(bar.close);
                            if prev_price > prev_ma && bar.close < ma {
                                (true, bar.close)
                            } else {
                                (false, 0.0)
                            }
                        } else {
                            (false, 0.0)
                        }
                    }
                }
            },
            Position::Short { .. } => {
                // 1. Hard stop (priority) - check against bar high
                if self.hard_stop_pct > 0.0 && bar.high >= self.hard_stop_price {
                    return (true, self.hard_stop_price);
                }
                // 2. MA288 stop - check against close
                match self.stop_mode {
                    StopMode::Fixed => {
                        let pnl = self.current_pnl_pct(bar.close);
                        if pnl < -self.fixed_stop_pct {
                            (true, bar.close)
                        } else {
                            (false, 0.0)
                        }
                    }
                    StopMode::Ma288 => {
                        let fast_ma = self.calculate_sma(self.fast_ma_period);
                        let prev_fast_ma = self.calculate_sma_prev(self.fast_ma_period);
                        if let (Some(ma), Some(prev_ma)) = (fast_ma, prev_fast_ma) {
                            let prev_price = self.bars.back().map(|b| b.close).unwrap_or(bar.close);
                            if prev_price < prev_ma && bar.close > ma {
                                (true, bar.close)
                            } else {
                                (false, 0.0)
                            }
                        } else {
                            (false, 0.0)
                        }
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
                    TakeProfitMode::None => false,
                }
            }
        }
    }

    /// Check trend reversal
    fn check_trend_reversal(&self, _current_price: f64) -> bool {
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
    fn process_bar(&mut self, bar: Bar, symbol: &str, price_decimal: Decimal, is_5m: bool) -> Signal {
        // Add bar to appropriate history
        if is_5m {
            self.bars_5m.push_back(bar.clone());
            // Keep reasonable history length for5m bars
            let max_5m_bars = self.slow_ma_period * 2;
            while self.bars_5m.len() > max_5m_bars {
                self.bars_5m.pop_front();
            }
            // For5m bars, we don't process entry/exit directly
            // Just store the data for diffusion filter
            return Signal::Hold;
        }

        // 30m bar processing
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

            // Check stop loss (with hard stop support)
            let (should_stop, _exit_price) = self.check_stop_loss(&bar);
            if should_stop {
                let signal = match &self.position {
                    Position::Long { .. } => Signal::Sell {
                        symbol: symbol.to_string(),
                        quantity: Decimal::from(100),
                        entry_price: price_decimal,
                        intent: SignalIntent::Exit,
                        stop_loss: None,
                    },
                    Position::Short { .. } => Signal::Buy {
                        symbol: symbol.to_string(),
                        quantity: Decimal::from(100),
                        entry_price: price_decimal,
                        intent: SignalIntent::Exit,
                        stop_loss: None,
                    },
                    Position::None => unreachable!(),
                };
                self.position = Position::None;
                self.entry_price = 0.0;
                self.hard_stop_price = 0.0;
                self.max_profit_pct = 0.0;
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
                        intent: SignalIntent::Exit,
                        stop_loss: None,
                    },
                    Position::Short { .. } => Signal::Buy {
                        symbol: symbol.to_string(),
                        quantity: Decimal::from(100),
                        entry_price: price_decimal,
                        intent: SignalIntent::Exit,
                        stop_loss: None,
                    },
                    Position::None => unreachable!(),
                };
                self.position = Position::None;
                self.max_profit_pct = 0.0;
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
                        intent: SignalIntent::Exit,
                        stop_loss: None,
                    },
                    Position::Short { .. } => Signal::Buy {
                        symbol: symbol.to_string(),
                        quantity: Decimal::from(100),
                        entry_price: price_decimal,
                        intent: SignalIntent::Exit,
                        stop_loss: None,
                    },
                    Position::None => unreachable!(),
                };
                self.position = Position::None;
                self.max_profit_pct = 0.0;
                self.last_signal = Some(signal.clone());
                return signal;
            }
        }

        // Check 30m diffusion filter (if enabled)
        if self.use_30m_expanding {
            if let Some(fast_ma) = self.calculate_sma(self.fast_ma_period) {
                if let Some(slow_ma) = self.calculate_sma(self.slow_ma_period) {
                    let current_spread = fast_ma - slow_ma;
                    // Get spread 5 bars ago
                    if self.bars.len() > self.slow_ma_period + 5 {
                        let prev_bars: Vec<Bar> = self.bars.iter().rev().skip(5).take(self.slow_ma_period + 5).cloned().collect();
                        if prev_bars.len() >= self.slow_ma_period {
                            let prev_fast: f64 = prev_bars.iter().rev().take(self.fast_ma_period).map(|b| b.close).sum::<f64>() / self.fast_ma_period as f64;
                            let prev_slow: f64 = prev_bars.iter().rev().take(self.slow_ma_period).map(|b| b.close).sum::<f64>() / self.slow_ma_period as f64;
                            let prev_spread = prev_fast - prev_slow;
                            if current_spread.abs() <= prev_spread.abs() {
                                return Signal::Hold; // 30m is converging, skip entry
                            }
                        }
                    }
                }
            }
        }

        // Check 5m diffusion filter (if enabled)
        if self.use_5m_expanding {
            if let Some(expanding) = self.is_5m_expanding() {
                if !expanding {
                    return Signal::Hold; // 5m is converging, skip entry
                }
            }

            if self.min_angle_5m > 0.0 {
                if let Some(angle) = self.calculate_5m_angle() {
                    if angle.abs() < self.min_angle_5m {
                        return Signal::Hold; // Angle too small, skip entry
                    }
                }
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
                    // Close short if exists (no reverse opening)
                    if matches!(self.position, Position::Short { .. }) {
                        let signal = Signal::Buy {
                            symbol: symbol.to_string(),
                            quantity: Decimal::from(100),
                            entry_price: price_decimal,
                            intent: SignalIntent::Exit,
                            stop_loss: None,
                        };
                        self.position = Position::None;
                        self.entry_price = 0.0;
                        self.hard_stop_price = 0.0;
                        self.max_profit_pct = 0.0;
                        
                        self.last_signal = Some(signal.clone());
                        // Don't open new position immediately, wait for next signal
                        return signal;
                    }

                    // Open long (only when no position)
                    if self.position == Position::None {
                        // Calculate stop loss price first
                        let stop_loss_price = if self.hard_stop_pct > 0.0 {
                            current_price * (1.0 - self.hard_stop_pct / 100.0)
                        } else {
                            fast_ma * 0.98  // MA288止损: MA下方2%
                        };
                        let signal = Signal::Buy {
                            symbol: symbol.to_string(),
                            quantity: Decimal::from(100),
                            entry_price: price_decimal,
                            intent: SignalIntent::Entry,
                            stop_loss: Some(Decimal::try_from(stop_loss_price).unwrap_or(Decimal::ZERO)),
                        };
                        self.position = Position::Long { entry_price: current_price };
                        self.entry_price = current_price;
                        self.hard_stop_price = stop_loss_price;
                        self.max_profit_pct = 0.0;
                        
                        self.last_signal = Some(signal.clone());
                        return signal;
                    }
                }
            }
            // Bearish trend: MA288 < MA488
            else if fast_ma < slow_ma {
                // Entry: price crosses below MA288
                if prev_price > prev_fast_ma && current_price < fast_ma {
                    // Close long if exists (no reverse opening)
                    if matches!(self.position, Position::Long { .. }) {
                        let signal = Signal::Sell {
                            symbol: symbol.to_string(),
                            quantity: Decimal::from(100),
                            entry_price: price_decimal,
                            intent: SignalIntent::Exit,
                            stop_loss: None,
                        };
                        self.position = Position::None;
                        self.entry_price = 0.0;
                        self.hard_stop_price = 0.0;
                        self.max_profit_pct = 0.0;
                        
                        self.last_signal = Some(signal.clone());
                        // Don't open new position immediately, wait for next signal
                        return signal;
                    }

                    // Open short (only when no position)
                    if self.position == Position::None {
                        // Calculate stop loss price first
                        let stop_loss_price = if self.hard_stop_pct > 0.0 {
                            current_price * (1.0 + self.hard_stop_pct / 100.0)
                        } else {
                            fast_ma * 1.02  // MA288止损: MA上方2%
                        };
                        let signal = Signal::Sell {
                            symbol: symbol.to_string(),
                            quantity: Decimal::from(100),
                            entry_price: price_decimal,
                            intent: SignalIntent::Entry,
                            stop_loss: Some(Decimal::try_from(stop_loss_price).unwrap_or(Decimal::ZERO)),
                        };
                        self.position = Position::Short { entry_price: current_price };
                        self.entry_price = current_price;
                        self.hard_stop_price = stop_loss_price;
                        self.max_profit_pct = 0.0;
                        
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
        if let Some(hard_stop) = params.get("hard_stop_pct") {
            self.hard_stop_pct = hard_stop.parse().map_err(|_| "Invalid hard_stop_pct")?;
        }
        if let Some(tp_mode) = params.get("take_profit_mode") {
            self.take_profit_mode = match tp_mode.as_str() {
                "trailing" => TakeProfitMode::Trailing,
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
        if let Some(use_30m) = params.get("use_30m_expanding") {
            self.use_30m_expanding = use_30m.parse().map_err(|_| "Invalid use_30m_expanding")?;
        }
        if let Some(use_expanding) = params.get("use_5m_expanding") {
            self.use_5m_expanding = use_expanding.parse().map_err(|_| "Invalid use_5m_expanding")?;
        }
        if let Some(min_angle) = params.get("min_angle_5m") {
            self.min_angle_5m = min_angle.parse().map_err(|_| "Invalid min_angle_5m")?;
        }
        if let Some(entry_tf) = params.get("entry_timeframe") {
            self.entry_timeframe = entry_tf.clone();
        }

        if self.fast_ma_period >= self.slow_ma_period {
            return Err("Fast MA period must be less than slow MA period".to_string());
        }

        println!(
            "MA Trend Pullback Strategy initialized: fast_ma={}, slow_ma={}, stop={:?}, hard_stop={}%, tp={:?}, act={}%, cb={}%, entry_tf={}, use_30m_expanding={}, use_5m_expanding={}, min_angle_5m={}",
            self.fast_ma_period, self.slow_ma_period, self.stop_mode, self.hard_stop_pct,
            self.take_profit_mode, self.trailing_activate_pct, self.trailing_callback_pct,
            self.entry_timeframe, self.use_30m_expanding, self.use_5m_expanding, self.min_angle_5m
        );
        Ok(())
    }

    fn reset(&mut self) {
        self.bars.clear();
        self.bars_5m.clear();
        self.position = Position::None;
        self.entry_price = 0.0;
        self.hard_stop_price = 0.0;
        self.max_profit_pct = 0.0;
        
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

        // Ticks are always treated as30m bars for now
        self.process_bar(bar, &tick.symbol, tick.price, false)
    }

    fn on_ohlc(&mut self, ohlc: &OHLCData) -> Signal {
        let bar = Bar {
            open: ohlc.open.to_f64().unwrap_or(0.0),
            high: ohlc.high.to_f64().unwrap_or(0.0),
            low: ohlc.low.to_f64().unwrap_or(0.0),
            close: ohlc.close.to_f64().unwrap_or(0.0),
            volume: ohlc.volume.to_f64().unwrap_or(0.0),
        };

        // Determine if this is a5m bar based on timeframe
        let is_5m = ohlc.timeframe == Timeframe::FiveMinutes;
        self.process_bar(bar, &ohlc.symbol, ohlc.close, is_5m)
    }

    fn supports_ohlc(&self) -> bool {
        true
    }

    fn preferred_timeframe(&self) -> Option<Timeframe> {
        Some(Timeframe::ThirtyMinutes)
    }
}
