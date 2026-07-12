// backtest/multi_timeframe_engine.rs
// 多时间框架回测引擎：逐 1m bar 喂入聚合器，当高时间框架 K 线完成时触发策略分析

use crate::backtest::metrics::BacktestMetrics;
use crate::backtest::portfolio::{Portfolio, PositionSide};
use crate::backtest::strategy::{EntryDirection, MultiTimeframeStrategy};
use crate::data::aggregator::KlineAggregator;
use crate::data::types::OHLCData;
use rust_decimal::Decimal;

use super::engine::BacktestConfig;
use super::engine::BacktestResult;

/// 多时间框架回测引擎
pub struct MultiTimeframeBacktestEngine {
    portfolio: Portfolio,
    strategy: Box<dyn MultiTimeframeStrategy>,
    aggregator: KlineAggregator,
    config: BacktestConfig,
    symbol: String,
    /// 每笔交易使用的 1m K 线数量（仓位大小 = capital * position_ratio / price）
    position_ratio: Decimal,
}

impl MultiTimeframeBacktestEngine {
    /// 创建新的多时间框架回测引擎
    pub fn new(
        strategy: Box<dyn MultiTimeframeStrategy>,
        config: BacktestConfig,
        symbol: String,
    ) -> Result<Self, String> {
        let mut strategy = strategy;
        strategy.reset();
        strategy.initialize(config.strategy_params.clone())?;

        let portfolio =
            Portfolio::new(config.initial_capital).with_commission_rate(config.commission_rate);

        Ok(Self {
            portfolio,
            strategy,
            aggregator: KlineAggregator::new(),
            config,
            symbol,
            position_ratio: Decimal::from(90) / Decimal::from(100), // 默认 90% 仓位
        })
    }

    /// 设置仓位比例 (0.0 - 1.0)
    pub fn with_position_ratio(mut self, ratio: Decimal) -> Self {
        self.position_ratio = ratio;
        self
    }

    /// 运行多时间框架回测
    ///
    /// 核心逻辑：
    /// 1. 逐根喂入 1m K 线到聚合器
    /// 2. 检查策略所需时间框架是否有足够数据
    /// 3. 调用策略分析，根据信号执行交易
    pub fn run(&mut self, klines_1m: Vec<OHLCData>) -> BacktestResult {
        println!("Starting multi-timeframe backtest...");
        println!("Strategy: {}", self.strategy.name());
        println!("Symbol: {}", self.symbol);
        println!("Initial capital: ${}", self.portfolio.initial_capital);
        println!("Data points: {} 1m candles", klines_1m.len());
        println!(
            "Commission rate: {}%",
            self.config.commission_rate * Decimal::from(100)
        );
        println!(
            "Required timeframes: {:?}",
            self.strategy
                .required_timeframes()
                .iter()
                .map(|tf| tf.as_str())
                .collect::<Vec<_>>()
        );
        println!("{}", "=".repeat(60));

        let total = klines_1m.len();
        let mut processed = 0;
        let mut last_progress = 0;
        let mut analysis_count = 0;

        for kline in &klines_1m {
            // 1. 喂入聚合器
            self.aggregator.update(kline.clone());

            // 2. 更新当前价格
            self.portfolio.update_price(&self.symbol, kline.close);

            // 3. 检查是否所有必需时间框架都有足够数据
            if !self.has_sufficient_data() {
                processed += 1;
                continue;
            }

            // 4. 获取多时间框架数据快照
            let all_klines = self.aggregator.get_all_timeframes();

            // 5. 调用策略分析
            let analysis = self.strategy.analyze(&all_klines);
            analysis_count += 1;

            // 6. 根据信号执行交易
            let current_price = kline.close;

            match self.portfolio.get_position_side(&self.symbol) {
                None => {
                    // 无持仓，检查是否应该入场
                    if self.strategy.should_enter(&analysis) {
                        if let Some(direction) = analysis.entry_direction {
                            match direction {
                                EntryDirection::Long => {
                                    self.open_long(current_price);
                                }
                                EntryDirection::Short => {
                                    self.open_short(current_price);
                                }
                            }
                        }
                    }
                }
                Some(PositionSide::Long) => {
                    // 持有多仓，检查是否应该平仓
                    if self.strategy.should_exit(&analysis, true) {
                        self.close_long(current_price);
                    }
                }
                Some(PositionSide::Short) => {
                    // 持有空仓，检查是否应该平仓
                    if self.strategy.should_exit(&analysis, false) {
                        self.close_short(current_price);
                    }
                }
            }

            processed += 1;

            // 进度显示
            let progress = (processed * 100) / total;
            if progress != last_progress && progress % 10 == 0 {
                let current_value = self.portfolio.total_value();
                let current_pnl = self.portfolio.total_pnl();
                println!(
                    "Progress: {}% ({}/{}) | Portfolio Value: ${} | P&L: ${} | Analyses: {}",
                    progress, processed, total, current_value, current_pnl, analysis_count
                );
                last_progress = progress;
            }
        }

        println!("\n{}", "=".repeat(60));

        // 计算结果
        self.build_result(klines_1m.len(), analysis_count)
    }

    /// 检查策略所需时间框架是否有足够数据
    fn has_sufficient_data(&self) -> bool {
        let required = self.strategy.required_timeframes();
        let min_candles = 50; // 至少需要 50 根已完成的 K 线

        for tf in &required {
            if self.aggregator.count(*tf) < min_candles {
                return false;
            }
        }
        true
    }

    /// 开多仓
    fn open_long(&mut self, price: Decimal) {
        let available = self.portfolio.cash * self.position_ratio;
        let quantity = available / price;

        if quantity <= Decimal::ZERO {
            return;
        }

        match self
            .portfolio
            .execute_buy(self.symbol.clone(), quantity, price)
        {
            Ok(_) => {
                println!(
                    "LONG {} {} @ ${} (confidence: {})",
                    self.symbol,
                    quantity,
                    price,
                    self.strategy
                        .required_timeframes()
                        .len()
                );
            }
            Err(e) => {
                println!("Long entry failed: {}", e);
            }
        }
    }

    /// 开空仓
    fn open_short(&mut self, price: Decimal) {
        let available = self.portfolio.cash * self.position_ratio;
        let quantity = available / price;

        if quantity <= Decimal::ZERO {
            return;
        }

        match self
            .portfolio
            .execute_short_open(self.symbol.clone(), quantity, price)
        {
            Ok(_) => {
                println!("SHORT {} {} @ ${}", self.symbol, quantity, price);
            }
            Err(e) => {
                println!("Short entry failed: {}", e);
            }
        }
    }

    /// 平多仓
    fn close_long(&mut self, price: Decimal) {
        if let Some(position) = self.portfolio.positions.get(&self.symbol) {
            let quantity = position.quantity;
            match self
                .portfolio
                .execute_sell(self.symbol.clone(), quantity, price)
            {
                Ok(_) => {
                    println!("CLOSE LONG {} {} @ ${}", self.symbol, quantity, price);
                }
                Err(e) => {
                    println!("Close long failed: {}", e);
                }
            }
        }
    }

    /// 平空仓
    fn close_short(&mut self, price: Decimal) {
        if let Some(position) = self.portfolio.positions.get(&self.symbol) {
            let quantity = position.quantity;
            match self
                .portfolio
                .execute_short_close(self.symbol.clone(), quantity, price)
            {
                Ok(_) => {
                    println!("CLOSE SHORT {} {} @ ${}", self.symbol, quantity, price);
                }
                Err(e) => {
                    println!("Close short failed: {}", e);
                }
            }
        }
    }

    /// 构建回测结果
    fn build_result(&self, _data_points: usize, analysis_count: usize) -> BacktestResult {
        let final_value = self.portfolio.total_value();
        let total_pnl = self.portfolio.total_pnl();
        let total_return_pct = if self.portfolio.initial_capital > Decimal::ZERO {
            (total_pnl / self.portfolio.initial_capital) * Decimal::from(100)
        } else {
            Decimal::ZERO
        };

        let equity_curve = self.portfolio.get_equity_curve();
        let returns = Self::calculate_returns(&equity_curve);

        let max_drawdown = BacktestMetrics::calculate_max_drawdown(&equity_curve);
        let sharpe_ratio = BacktestMetrics::calculate_sharpe_ratio(&returns, Decimal::ZERO);
        let volatility = BacktestMetrics::calculate_volatility(&returns);
        let win_rate = BacktestMetrics::calculate_win_rate(&self.portfolio.trades);
        let profit_factor = BacktestMetrics::calculate_profit_factor(&self.portfolio.trades);
        let avg_trade_duration =
            BacktestMetrics::calculate_average_trade_duration(&self.portfolio.trades);

        println!("Total analyses performed: {}", analysis_count);

        BacktestResult {
            initial_capital: self.portfolio.initial_capital,
            final_value,
            total_pnl,
            return_percentage: total_return_pct,
            total_trades: self.portfolio.trades.len(),
            winning_trades: self.count_winning_trades(),
            losing_trades: self.count_losing_trades(),
            max_drawdown,
            sharpe_ratio,
            volatility,
            win_rate,
            profit_factor,
            avg_trade_duration_seconds: avg_trade_duration,
            total_commission: self.portfolio.total_commission(),
            total_slippage_cost: self.portfolio.total_slippage_cost,
            positions: self.portfolio.positions.clone(),
            trades: self.portfolio.trades.clone(),
            equity_curve,
            strategy_name: self.strategy.name().to_string(),
        }
    }

    fn calculate_returns(equity_curve: &[Decimal]) -> Vec<Decimal> {
        if equity_curve.len() < 2 {
            return Vec::new();
        }

        equity_curve
            .windows(2)
            .map(|window| {
                if window[0] > Decimal::ZERO {
                    (window[1] - window[0]) / window[0]
                } else {
                    Decimal::ZERO
                }
            })
            .collect()
    }

    fn count_winning_trades(&self) -> usize {
        self.portfolio
            .trades
            .iter()
            .filter(|trade| trade.realized_pnl.map_or(false, |pnl| pnl > Decimal::ZERO))
            .count()
    }

    fn count_losing_trades(&self) -> usize {
        self.portfolio
            .trades
            .iter()
            .filter(|trade| trade.realized_pnl.map_or(false, |pnl| pnl < Decimal::ZERO))
            .count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backtest::strategy::create_multi_timeframe_strategy;
    use chrono::Utc;
    use rust_decimal::Decimal;

    fn create_test_klines_1m(count: usize, start_price: Decimal, trend: &str) -> Vec<OHLCData> {
        let mut klines = Vec::new();
        let base_time = Utc::now();

        for i in 0..count {
            let price = match trend {
                "up" => start_price + Decimal::from(i as i64 * 10),
                "down" => start_price - Decimal::from(i as i64 * 10),
                _ => start_price,
            };

            klines.push(OHLCData::new(
                base_time + chrono::Duration::minutes(i as i64),
                "BTCUSDT".to_string(),
                crate::data::types::Timeframe::OneMinute,
                price - Decimal::from(5),
                price + Decimal::from(10),
                price - Decimal::from(15),
                price,
                Decimal::from(100),
                10,
            ));
        }

        klines
    }

    #[test]
    fn test_multi_timeframe_backtest_engine_creation() {
        let strategy = create_multi_timeframe_strategy("trend").unwrap();
        let config = BacktestConfig::new(Decimal::from(10000));
        let engine =
            MultiTimeframeBacktestEngine::new(strategy, config, "BTCUSDT".to_string()).unwrap();

        assert_eq!(engine.symbol, "BTCUSDT");
        assert_eq!(engine.portfolio.initial_capital, Decimal::from(10000));
    }

    #[test]
    fn test_multi_timeframe_backtest_engine_run() {
        let strategy = create_multi_timeframe_strategy("trend").unwrap();
        let config = BacktestConfig::new(Decimal::from(10000));
        let mut engine =
            MultiTimeframeBacktestEngine::new(strategy, config, "BTCUSDT".to_string()).unwrap();

        // 需要足够多的数据让聚合器有足够的时间框架数据
        // 4h 需要至少 50 根，即 50 * 4 * 60 = 12000 根 1m K 线
        // 测试用少一点数据，验证不会 panic
        let klines = create_test_klines_1m(500, Decimal::from(50000), "up");
        let result = engine.run(klines);

        // 基本验证
        assert_eq!(result.initial_capital, Decimal::from(10000));
        assert_eq!(result.strategy_name, "Multi-Timeframe Trend");
    }
}
