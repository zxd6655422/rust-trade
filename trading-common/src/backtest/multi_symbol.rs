// backtest/multi_symbol.rs
// 多交易对回测编排器：批量运行多个 symbol，汇总统计

use rust_decimal::Decimal;
use std::collections::HashMap;

use crate::backtest::engine::BacktestConfig;
use crate::backtest::engine::BacktestResult;
use crate::backtest::market_state::{MarketStateAnalyzer, MarketStateReport};
use crate::backtest::multi_timeframe_engine::MultiTimeframeBacktestEngine;
use crate::backtest::strategy::MultiTimeframeStrategy;
use crate::data::types::OHLCData;

// =================================================================
// 结果类型
// =================================================================

/// 单个交易对的回测结果
#[derive(Debug)]
pub struct SymbolBacktestResult {
    pub symbol: String,
    pub result: BacktestResult,
    pub market_state: MarketStateReport,
}

/// 多交易对回测总结果
#[derive(Debug)]
pub struct MultiSymbolBacktestResult {
    pub results: Vec<SymbolBacktestResult>,
    pub total_symbols: usize,
    pub profitable_symbols: usize,
    pub losing_symbols: usize,
    pub avg_return_pct: Decimal,
    pub avg_sharpe: Decimal,
    pub avg_win_rate: Decimal,
    pub avg_max_drawdown: Decimal,
    pub total_trades: usize,
    pub best_symbol: String,
    pub best_return_pct: Decimal,
    pub worst_symbol: String,
    pub worst_return_pct: Decimal,
    pub cross_symbol_correlation: Decimal,
}

// =================================================================
// 引擎
// =================================================================

/// 多交易对回测编排器
pub struct MultiSymbolBacktestEngine;

impl MultiSymbolBacktestEngine {
    /// 运行多交易对回测（并发版本）
    ///
    /// - `symbol_data`: 每个交易对的 1m K 线数据
    /// - 对每个 symbol 运行独立的 MultiTimeframeBacktestEngine
    /// - 汇总所有结果
    pub fn run(
        strategy_factory: impl Fn() -> Box<dyn MultiTimeframeStrategy> + Send + Sync + 'static,
        config: &BacktestConfig,
        symbol_data: &HashMap<String, Vec<OHLCData>>,
        market_state_window: usize,
    ) -> Result<MultiSymbolBacktestResult, String> {
        if symbol_data.is_empty() {
            return Err("No symbol data provided".to_string());
        }

        println!("Starting multi-symbol backtest...");
        println!("Symbols: {:?}", symbol_data.keys().collect::<Vec<_>>());
        println!("{}", "=".repeat(60));

        // 串行执行（回测是 CPU 密集型，并发反而可能更慢）
        let mut results = Vec::new();

        for (symbol, klines) in symbol_data {
            if klines.is_empty() {
                println!("Skipping {} (no data)", symbol);
                continue;
            }

            println!("Backtesting {} ({} candles)...", symbol, klines.len());

            // 运行回测
            let mut engine = MultiTimeframeBacktestEngine::new(
                strategy_factory(),
                config.clone(),
                symbol.clone(),
            )?;
            let result = engine.run(klines.clone());

            // 分析市场状态
            let market_state = MarketStateAnalyzer::analyze(klines, market_state_window);

            println!(
                "  {}: return={:.2}%, sharpe={:.2}, trades={}, quality={:.0}/100",
                symbol,
                result.return_percentage,
                result.sharpe_ratio,
                result.total_trades,
                market_state.data_quality_score
            );

            results.push(SymbolBacktestResult {
                symbol: symbol.clone(),
                result,
                market_state,
            });
        }

        println!("{}", "=".repeat(60));

        if results.is_empty() {
            return Err("No valid backtest results".to_string());
        }

        let aggregate = Self::aggregate_results(results);

        println!("Multi-symbol backtest complete:");
        println!("  Profitable: {}/{}", aggregate.profitable_symbols, aggregate.total_symbols);
        println!("  Avg return: {:.2}%", aggregate.avg_return_pct);
        println!("  Avg Sharpe: {:.2}", aggregate.avg_sharpe);
        println!("  Best: {} ({:.2}%)", aggregate.best_symbol, aggregate.best_return_pct);
        println!("  Worst: {} ({:.2}%)", aggregate.worst_symbol, aggregate.worst_return_pct);

        Ok(aggregate)
    }

    /// 汇总各 symbol 结果
    fn aggregate_results(results: Vec<SymbolBacktestResult>) -> MultiSymbolBacktestResult {
        let total = results.len();
        let n = Decimal::from(total);

        let profitable = results.iter().filter(|r| r.result.is_profitable()).count();

        let avg_return: Decimal =
            results.iter().map(|r| r.result.return_percentage).sum::<Decimal>() / n;

        let avg_sharpe: Decimal =
            results.iter().map(|r| r.result.sharpe_ratio).sum::<Decimal>() / n;

        let avg_win_rate: Decimal =
            results.iter().map(|r| r.result.win_rate).sum::<Decimal>() / n;

        let avg_max_drawdown: Decimal =
            results.iter().map(|r| r.result.max_drawdown).sum::<Decimal>() / n;

        let total_trades: usize = results.iter().map(|r| r.result.total_trades).sum();

        // 最佳/最差 symbol
        let mut best_symbol = String::new();
        let mut best_return = Decimal::MIN;
        let mut worst_symbol = String::new();
        let mut worst_return = Decimal::MAX;
        let mut positive_count = 0;

        for r in &results {
            let ret = r.result.return_percentage;
            if ret > best_return {
                best_return = ret;
                best_symbol = r.symbol.clone();
            }
            if ret < worst_return {
                worst_return = ret;
                worst_symbol = r.symbol.clone();
            }
            if ret > Decimal::ZERO {
                positive_count += 1;
            }
        }

        let negative_count = total - positive_count;
        let correlation = if total > 0 {
            let dominant = positive_count.max(negative_count);
            Decimal::from(dominant) / n
        } else {
            Decimal::ZERO
        };

        MultiSymbolBacktestResult {
            results,
            total_symbols: total,
            profitable_symbols: profitable,
            losing_symbols: total - profitable,
            avg_return_pct: avg_return,
            avg_sharpe,
            avg_win_rate,
            avg_max_drawdown,
            total_trades,
            best_symbol,
            best_return_pct: best_return,
            worst_symbol,
            worst_return_pct: worst_return,
            cross_symbol_correlation: correlation,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backtest::strategy::create_multi_timeframe_strategy;
    use chrono::Utc;
    use crate::data::types::Timeframe;

    fn create_test_klines(count: usize, trend: &str) -> Vec<OHLCData> {
        let mut klines = Vec::new();
        let base_time = Utc::now();

        for i in 0..count {
            let price = match trend {
                "up" => Decimal::from(50000 + i as i64 * 10),
                _ => Decimal::from(50000),
            };

            klines.push(OHLCData::new(
                base_time + chrono::Duration::minutes(i as i64),
                "BTCUSDT".to_string(),
                Timeframe::OneMinute,
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
    fn test_multi_symbol_empty() {
        let config = BacktestConfig::new(Decimal::from(10000));
        let symbol_data = HashMap::new();

        let result = MultiSymbolBacktestEngine::run(
            || create_multi_timeframe_strategy("trend").unwrap(),
            &config,
            &symbol_data,
            50,
        );

        assert!(result.is_err());
    }

    #[test]
    fn test_multi_symbol_single() {
        let config = BacktestConfig::new(Decimal::from(10000));
        let mut symbol_data = HashMap::new();
        symbol_data.insert("BTCUSDT".to_string(), create_test_klines(500, "up"));

        let result = MultiSymbolBacktestEngine::run(
            || create_multi_timeframe_strategy("trend").unwrap(),
            &config,
            &symbol_data,
            50,
        );

        assert!(result.is_ok());
        let r = result.unwrap();
        assert_eq!(r.total_symbols, 1);
    }
}
