// backtest/walk_forward.rs
// 滚动前进回测引擎 + 样本外测试 + 过拟合检测

use chrono::{DateTime, Utc};
use rust_decimal::prelude::ToPrimitive;
use rust_decimal::Decimal;
use std::str::FromStr;

use crate::backtest::engine::BacktestConfig;
use crate::backtest::engine::BacktestResult;
use crate::backtest::multi_timeframe_engine::MultiTimeframeBacktestEngine;
use crate::backtest::strategy::MultiTimeframeStrategy;
use crate::data::types::OHLCData;

// =================================================================
// 配置
// =================================================================

/// 滚动前进测试配置
#[derive(Debug, Clone)]
pub struct WalkForwardConfig {
    /// 训练窗口大小（1m K 线数量），默认 43200 (30天)
    pub train_candles: usize,
    /// 测试窗口大小（1m K 线数量），默认 10080 (7天)
    pub test_candles: usize,
    /// 滚动步长（1m K 线数量），默认 10080 (7天)
    pub step_candles: usize,
    /// 过拟合阈值（训练/测试 Sharpe 差异比率），超过此值判定为过拟合
    pub overfit_threshold: Decimal,
}

impl Default for WalkForwardConfig {
    fn default() -> Self {
        Self {
            train_candles: 43200,  // 30 天
            test_candles: 10080,   // 7 天
            step_candles: 10080,   // 7 天
            overfit_threshold: Decimal::from_str("0.5").unwrap(),
        }
    }
}

impl WalkForwardConfig {
    pub fn with_train_candles(mut self, n: usize) -> Self {
        self.train_candles = n;
        self
    }

    pub fn with_test_candles(mut self, n: usize) -> Self {
        self.test_candles = n;
        self
    }

    pub fn with_step_candles(mut self, n: usize) -> Self {
        self.step_candles = n;
        self
    }

    pub fn with_overfit_threshold(mut self, t: Decimal) -> Self {
        self.overfit_threshold = t;
        self
    }
}

/// 样本外测试配置（简化版：单次 train/test 划分）
#[derive(Debug, Clone)]
pub struct OutOfSampleConfig {
    /// 训练集比例 (0.0 - 1.0)，默认 0.7
    pub train_ratio: Decimal,
}

impl Default for OutOfSampleConfig {
    fn default() -> Self {
        Self {
            train_ratio: Decimal::from_str("0.7").unwrap(),
        }
    }
}

// =================================================================
// 结果类型
// =================================================================

/// 单轮滚动前进测试结果
#[derive(Debug)]
pub struct WalkForwardRound {
    /// 轮次编号
    pub round: usize,
    /// 训练集起始时间
    pub train_start: DateTime<Utc>,
    /// 训练集结束时间
    pub train_end: DateTime<Utc>,
    /// 测试集起始时间
    pub test_start: DateTime<Utc>,
    /// 测试集结束时间
    pub test_end: DateTime<Utc>,
    /// 训练集回测结果
    pub train_result: BacktestResult,
    /// 测试集回测结果
    pub test_result: BacktestResult,
    /// 过拟合比率 = (train_sharpe - test_sharpe) / max(train_sharpe, 0.01)
    pub overfit_ratio: Decimal,
}

/// 滚动前进测试总结果
#[derive(Debug)]
pub struct WalkForwardResult {
    /// 各轮汇总指标（不含完整 BacktestResult，避免生命周期问题）
    pub round_summaries: Vec<WalkForwardRoundSummary>,
    /// 测试集累计收益率
    pub overall_test_return_pct: Decimal,
    /// 测试集平均 Sharpe
    pub overall_test_sharpe: Decimal,
    /// 测试集平均最大回撤
    pub overall_test_max_drawdown: Decimal,
    /// 测试集平均胜率
    pub overall_test_win_rate: Decimal,
    /// 平均过拟合比率
    pub avg_overfit_ratio: Decimal,
    /// 是否过拟合
    pub is_overfit: bool,
    /// 总轮次
    pub total_rounds: usize,
    /// 盈利轮次
    pub profitable_rounds: usize,
}

/// 单轮汇总指标
#[derive(Debug, Clone)]
pub struct WalkForwardRoundSummary {
    pub round: usize,
    pub train_start: DateTime<Utc>,
    pub train_end: DateTime<Utc>,
    pub test_start: DateTime<Utc>,
    pub test_end: DateTime<Utc>,
    pub train_return_pct: Decimal,
    pub train_sharpe: Decimal,
    pub train_trades: usize,
    pub test_return_pct: Decimal,
    pub test_sharpe: Decimal,
    pub test_trades: usize,
    pub test_win_rate: Decimal,
    pub test_max_drawdown: Decimal,
    pub overfit_ratio: Decimal,
}

/// 样本外测试结果
#[derive(Debug)]
pub struct OutOfSampleResult {
    /// 训练集回测结果
    pub train_result: BacktestResult,
    /// 测试集回测结果
    pub test_result: BacktestResult,
    /// 过拟合比率
    pub overfit_ratio: Decimal,
    /// 是否过拟合
    pub is_overfit: bool,
    /// 训练集 Sharpe
    pub train_sharpe: Decimal,
    /// 测试集 Sharpe
    pub test_sharpe: Decimal,
}

// =================================================================
// 引擎
// =================================================================

/// 滚动前进回测引擎
pub struct WalkForwardEngine;

impl WalkForwardEngine {
    /// 运行滚动前进测试
    pub fn run(
        strategy_factory: impl Fn() -> Box<dyn MultiTimeframeStrategy> + Sync,
        config: &BacktestConfig,
        wf_config: &WalkForwardConfig,
        klines_1m: &[OHLCData],
        symbol: &str,
    ) -> Result<WalkForwardResult, String> {
        let total = klines_1m.len();
        let window_size = wf_config.train_candles + wf_config.test_candles;

        if total < window_size {
            return Err(format!(
                "Insufficient data: need {} candles, have {}",
                window_size, total
            ));
        }

        let mut rounds = Vec::new();
        let mut round_num = 0;
        let mut start = 0;

        println!("Starting Walk-Forward Analysis...");
        println!("Total candles: {}", total);
        println!(
            "Window: train={}, test={}, step={}",
            wf_config.train_candles, wf_config.test_candles, wf_config.step_candles
        );
        println!("{}", "=".repeat(60));

        while start + window_size <= total {
            let train_start = start;
            let train_end = start + wf_config.train_candles;
            let test_start = train_end;
            let test_end = train_end + wf_config.test_candles;

            let train_data = &klines_1m[train_start..train_end];
            let test_data = &klines_1m[test_start..test_end];

            round_num += 1;
            println!(
                "Round {}: train [{}..{}], test [{}..{}]",
                round_num, train_start, train_end, test_start, test_end
            );

            // 训练集回测
            let mut train_engine = MultiTimeframeBacktestEngine::new(
                strategy_factory(),
                config.clone(),
                symbol.to_string(),
            )?;
            let train_result = train_engine.run(train_data.to_vec());

            // 测试集回测
            let mut test_engine = MultiTimeframeBacktestEngine::new(
                strategy_factory(),
                config.clone(),
                symbol.to_string(),
            )?;
            let test_result = test_engine.run(test_data.to_vec());

            // 计算过拟合比率
            let overfit_ratio = Self::calculate_overfit_ratio(
                train_result.sharpe_ratio,
                test_result.sharpe_ratio,
            );

            println!(
                "  Train: return={:.2}%, sharpe={:.2}, trades={}",
                train_result.return_percentage,
                train_result.sharpe_ratio,
                train_result.total_trades
            );
            println!(
                "  Test:  return={:.2}%, sharpe={:.2}, trades={}, overfit_ratio={:.2}",
                test_result.return_percentage,
                test_result.sharpe_ratio,
                test_result.total_trades,
                overfit_ratio
            );

            rounds.push(WalkForwardRound {
                round: round_num,
                train_start: train_data.first().unwrap().timestamp,
                train_end: train_data.last().unwrap().timestamp,
                test_start: test_data.first().unwrap().timestamp,
                test_end: test_data.last().unwrap().timestamp,
                train_result,
                test_result,
                overfit_ratio,
            });

            start += wf_config.step_candles;
        }

        println!("{}", "=".repeat(60));

        // 汇总
        let result = Self::aggregate_rounds(rounds, wf_config.overfit_threshold);

        println!("Walk-Forward Analysis Complete");
        println!(
            "Overall test return: {:.2}%",
            result.overall_test_return_pct
        );
        println!("Overall test Sharpe: {:.2}", result.overall_test_sharpe);
        println!(
            "Avg overfit ratio: {:.2} ({})",
            result.avg_overfit_ratio,
            if result.is_overfit {
                "OVERFIT"
            } else {
                "OK"
            }
        );
        println!(
            "Profitable rounds: {}/{}",
            result.profitable_rounds, result.total_rounds
        );

        Ok(result)
    }

    /// 运行样本外测试（简化版：单次 70/30 划分）
    pub fn run_out_of_sample(
        strategy_factory: impl Fn() -> Box<dyn MultiTimeframeStrategy>,
        config: &BacktestConfig,
        os_config: &OutOfSampleConfig,
        klines_1m: &[OHLCData],
        symbol: &str,
    ) -> Result<OutOfSampleResult, String> {
        let total = klines_1m.len();
        if total < 1000 {
            return Err("Insufficient data: need at least 1000 candles".to_string());
        }

        let train_end = (Decimal::from(total) * os_config.train_ratio)
            .to_usize()
            .unwrap_or(total * 7 / 10);

        let train_data = &klines_1m[..train_end];
        let test_data = &klines_1m[train_end..];

        println!("Out-of-Sample Backtest...");
        println!(
            "Train: {} candles ({}..{}), Test: {} candles ({}..{})",
            train_data.len(),
            0,
            train_end,
            test_data.len(),
            train_end,
            total
        );
        println!("{}", "=".repeat(60));

        // 训练集回测
        let mut train_engine = MultiTimeframeBacktestEngine::new(
            strategy_factory(),
            config.clone(),
            symbol.to_string(),
        )?;
        let train_result = train_engine.run(train_data.to_vec());

        // 测试集回测
        let mut test_engine = MultiTimeframeBacktestEngine::new(
            strategy_factory(),
            config.clone(),
            symbol.to_string(),
        )?;
        let test_result = test_engine.run(test_data.to_vec());

        let train_sharpe = train_result.sharpe_ratio;
        let test_sharpe = test_result.sharpe_ratio;
        let overfit_ratio = Self::calculate_overfit_ratio(train_sharpe, test_sharpe);
        let is_overfit = overfit_ratio > Decimal::from_str("0.5").unwrap();

        println!("{}", "=".repeat(60));
        println!("Out-of-Sample Results:");
        println!(
            "  Train: return={:.2}%, sharpe={:.2}",
            train_result.return_percentage, train_sharpe
        );
        println!(
            "  Test:  return={:.2}%, sharpe={:.2}",
            test_result.return_percentage, test_sharpe
        );
        println!(
            "  Overfit ratio: {:.2} ({})",
            overfit_ratio,
            if is_overfit { "OVERFIT" } else { "OK" }
        );

        Ok(OutOfSampleResult {
            train_result,
            test_result,
            overfit_ratio,
            is_overfit,
            train_sharpe,
            test_sharpe,
        })
    }

    /// 计算过拟合比率
    fn calculate_overfit_ratio(train_sharpe: Decimal, test_sharpe: Decimal) -> Decimal {
        let denominator = train_sharpe.abs().max(Decimal::from_str("0.01").unwrap());
        let ratio = (train_sharpe - test_sharpe) / denominator;
        ratio.max(Decimal::ZERO) // 负值表示测试比训练好，不算过拟合
    }

    /// 汇总各轮结果（消费 rounds，提取摘要）
    fn aggregate_rounds(
        rounds: Vec<WalkForwardRound>,
        overfit_threshold: Decimal,
    ) -> WalkForwardResult {
        if rounds.is_empty() {
            return WalkForwardResult {
                round_summaries: Vec::new(),
                overall_test_return_pct: Decimal::ZERO,
                overall_test_sharpe: Decimal::ZERO,
                overall_test_max_drawdown: Decimal::ZERO,
                overall_test_win_rate: Decimal::ZERO,
                avg_overfit_ratio: Decimal::ZERO,
                is_overfit: false,
                total_rounds: 0,
                profitable_rounds: 0,
            };
        }

        let n = Decimal::from(rounds.len());
        let mut round_summaries = Vec::with_capacity(rounds.len());

        // 测试集各轮收益率累乘
        let mut cumulative_return = Decimal::ONE;
        let mut profitable_rounds = 0;

        for round in rounds {
            let round_return = Decimal::ONE + round.test_result.return_percentage / Decimal::from(100);
            cumulative_return *= round_return;

            if round.test_result.is_profitable() {
                profitable_rounds += 1;
            }

            round_summaries.push(WalkForwardRoundSummary {
                round: round.round,
                train_start: round.train_start,
                train_end: round.train_end,
                test_start: round.test_start,
                test_end: round.test_end,
                train_return_pct: round.train_result.return_percentage,
                train_sharpe: round.train_result.sharpe_ratio,
                train_trades: round.train_result.total_trades,
                test_return_pct: round.test_result.return_percentage,
                test_sharpe: round.test_result.sharpe_ratio,
                test_trades: round.test_result.total_trades,
                test_win_rate: round.test_result.win_rate,
                test_max_drawdown: round.test_result.max_drawdown,
                overfit_ratio: round.overfit_ratio,
            });
        }

        let overall_test_return_pct = (cumulative_return - Decimal::ONE) * Decimal::from(100);

        let overall_test_sharpe: Decimal =
            round_summaries.iter().map(|r| r.test_sharpe).sum::<Decimal>() / n;

        let overall_test_max_drawdown: Decimal =
            round_summaries.iter().map(|r| r.test_max_drawdown).sum::<Decimal>() / n;

        let overall_test_win_rate: Decimal =
            round_summaries.iter().map(|r| r.test_win_rate).sum::<Decimal>() / n;

        let avg_overfit_ratio: Decimal =
            round_summaries.iter().map(|r| r.overfit_ratio).sum::<Decimal>() / n;

        let is_overfit = avg_overfit_ratio > overfit_threshold;

        WalkForwardResult {
            round_summaries,
            overall_test_return_pct,
            overall_test_sharpe,
            overall_test_max_drawdown,
            overall_test_win_rate,
            avg_overfit_ratio,
            is_overfit,
            total_rounds: n.to_usize().unwrap_or(0),
            profitable_rounds,
        }
    }
}

// =================================================================
// 需要为 WalkForwardRound 实现 Clone（因为 BacktestResult 不是 Clone）
// 通过重新运行的方式不实际，所以直接存储引用或移动所有权
// 这里选择不实现 Clone，让 API handler 直接使用结果
// =================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backtest::strategy::create_multi_timeframe_strategy;

    fn create_test_klines_1m(count: usize, start_price: Decimal, trend: &str) -> Vec<OHLCData> {
        let mut klines = Vec::new();
        let base_time = Utc::now();

        for i in 0..count {
            let price = match trend {
                "up" => start_price + Decimal::from(i as i64),
                "down" => start_price - Decimal::from(i as i64),
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
    fn test_walk_forward_insufficient_data() {
        let klines = create_test_klines_1m(100, Decimal::from(50000), "up");
        let config = BacktestConfig::new(Decimal::from(10000));
        let wf_config = WalkForwardConfig::default();

        let result = WalkForwardEngine::run(
            || create_multi_timeframe_strategy("trend").unwrap(),
            &config,
            &wf_config,
            &klines,
            "BTCUSDT",
        );

        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Insufficient data"));
    }

    #[test]
    fn test_out_of_sample_insufficient_data() {
        let klines = create_test_klines_1m(100, Decimal::from(50000), "up");
        let config = BacktestConfig::new(Decimal::from(10000));
        let os_config = OutOfSampleConfig::default();

        let result = WalkForwardEngine::run_out_of_sample(
            || create_multi_timeframe_strategy("trend").unwrap(),
            &config,
            &os_config,
            &klines,
            "BTCUSDT",
        );

        assert!(result.is_err());
    }

    #[test]
    fn test_overfit_ratio_calculation() {
        // 训练好，测试差 → 高过拟合
        let ratio = WalkForwardEngine::calculate_overfit_ratio(
            Decimal::from(2),
            Decimal::from(0),
        );
        assert_eq!(ratio, Decimal::from(1)); // (2-0)/2 = 1.0

        // 训练和测试一样好 → 无过拟合
        let ratio = WalkForwardEngine::calculate_overfit_ratio(
            Decimal::from(1),
            Decimal::from(1),
        );
        assert_eq!(ratio, Decimal::ZERO);

        // 测试比训练好 → 无过拟合
        let ratio = WalkForwardEngine::calculate_overfit_ratio(
            Decimal::from(1),
            Decimal::from(2),
        );
        assert_eq!(ratio, Decimal::ZERO);
    }
}
