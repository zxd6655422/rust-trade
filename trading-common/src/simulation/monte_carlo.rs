// simulation/monte_carlo.rs
// 蒙特卡洛模拟模块

use rust_decimal::Decimal;
use rust_decimal::prelude::*;

use super::brownian::{GeometricBrownianMotion, PathStatistics};
use crate::pricing::options::{OptionContract, OptionType};

/// 蒙特卡洛模拟器
#[derive(Debug, Clone)]
pub struct MonteCarloSimulator {
    /// 模拟次数
    pub n_simulations: usize,
    /// 时间步数
    pub n_steps: usize,
}

impl MonteCarloSimulator {
    /// 创建新的蒙特卡洛模拟器
    pub fn new(n_simulations: usize, n_steps: usize) -> Self {
        Self {
            n_simulations,
            n_steps,
        }
    }

    /// 蒙特卡洛期权定价
    ///
    /// 使用几何布朗运动模拟标的价格路径，计算期权的期望收益
    pub fn price_option(
        &self,
        option: &OptionContract,
        spot: Decimal,
        rate: Decimal,
        volatility: Decimal,
    ) -> Decimal {
        let spot_f64 = spot.to_f64().unwrap_or(0.0);
        let rate_f64 = rate.to_f64().unwrap_or(0.0);
        let vol_f64 = volatility.to_f64().unwrap_or(0.0);
        let time_f64 = option.time_to_expiry().to_f64().unwrap_or(0.0);
        let strike_f64 = option.strike.to_f64().unwrap_or(0.0);

        if spot_f64 <= 0.0 || vol_f64 <= 0.0 || time_f64 <= 0.0 {
            return Decimal::ZERO;
        }

        let dt = time_f64 / self.n_steps as f64;
        let gbm = GeometricBrownianMotion::new(rate_f64, vol_f64);

        let mut total_payoff = 0.0;

        for i in 0..self.n_simulations {
            let path = gbm.generate(spot_f64, self.n_steps, dt, i as u64);
            let final_price = *path.last().unwrap();

            let payoff = match option.option_type {
                OptionType::Call => (final_price - strike_f64).max(0.0),
                OptionType::Put => (strike_f64 - final_price).max(0.0),
            };

            total_payoff += payoff;
        }

        let avg_payoff = total_payoff / self.n_simulations as f64;
        let discount = (-rate_f64 * time_f64).exp();
        let price = avg_payoff * discount;

        Decimal::from_f64(price).unwrap_or(Decimal::ZERO)
    }

    /// 蒙特卡洛模拟价格路径
    pub fn simulate_paths(
        &self,
        initial_price: f64,
        drift: f64,
        volatility: f64,
        time_years: f64,
        seed: u64,
    ) -> Vec<Vec<f64>> {
        let dt = time_years / self.n_steps as f64;
        let gbm = GeometricBrownianMotion::new(drift, volatility);
        gbm.generate_paths(initial_price, self.n_steps, dt, self.n_simulations, seed)
    }

    /// 计算价格分布统计
    pub fn price_distribution(
        &self,
        initial_price: f64,
        drift: f64,
        volatility: f64,
        time_years: f64,
        seed: u64,
    ) -> PriceDistribution {
        let paths = self.simulate_paths(initial_price, drift, volatility, time_years, seed);
        let stats: Vec<PathStatistics> = paths.iter().map(|p| PathStatistics::from_path(p)).collect();

        let finals: Vec<f64> = stats.iter().map(|s| s.final_value).collect();
        let mean = finals.iter().sum::<f64>() / finals.len() as f64;
        let variance = finals.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / finals.len() as f64;
        let std_dev = variance.sqrt();

        // 计算分位数
        let mut sorted_finals = finals.clone();
        sorted_finals.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let percentile_5 = sorted_finals[(self.n_simulations as f64 * 0.05) as usize];
        let percentile_25 = sorted_finals[(self.n_simulations as f64 * 0.25) as usize];
        let percentile_50 = sorted_finals[(self.n_simulations as f64 * 0.50) as usize];
        let percentile_75 = sorted_finals[(self.n_simulations as f64 * 0.75) as usize];
        let percentile_95 = sorted_finals[(self.n_simulations as f64 * 0.95) as usize];

        // 计算达到特定价格的概率
        let prob_above_initial = finals.iter().filter(|&&f| f > initial_price).count() as f64
            / self.n_simulations as f64;
        let prob_above_10pct = finals
            .iter()
            .filter(|&&f| f > initial_price * 1.1)
            .count() as f64
            / self.n_simulations as f64;
        let prob_below_10pct = finals
            .iter()
            .filter(|&&f| f < initial_price * 0.9)
            .count() as f64
            / self.n_simulations as f64;

        // 计算最大回撤统计
        let max_drawdowns: Vec<f64> = stats.iter().map(|s| s.max_drawdown).collect();
        let avg_max_drawdown = max_drawdowns.iter().sum::<f64>() / max_drawdowns.len() as f64;
        let max_drawdown_95 = {
            let mut sorted_dd = max_drawdowns.clone();
            sorted_dd.sort_by(|a, b| a.partial_cmp(b).unwrap());
            sorted_dd[(self.n_simulations as f64 * 0.95) as usize]
        };

        PriceDistribution {
            initial_price,
            mean,
            std_dev,
            min: *sorted_finals.first().unwrap(),
            max: *sorted_finals.last().unwrap(),
            percentile_5,
            percentile_25,
            percentile_50,
            percentile_75,
            percentile_95,
            prob_above_initial,
            prob_above_10pct,
            prob_below_10pct,
            avg_max_drawdown,
            max_drawdown_95,
        }
    }
}

/// 价格分布统计
#[derive(Debug, Clone)]
pub struct PriceDistribution {
    /// 初始价格
    pub initial_price: f64,
    /// 终值均值
    pub mean: f64,
    /// 终值标准差
    pub std_dev: f64,
    /// 最小终值
    pub min: f64,
    /// 最大终值
    pub max: f64,
    /// 5% 分位数
    pub percentile_5: f64,
    /// 25% 分位数
    pub percentile_25: f64,
    /// 50% 分位数（中位数）
    pub percentile_50: f64,
    /// 75% 分位数
    pub percentile_75: f64,
    /// 95% 分位数
    pub percentile_95: f64,
    /// 高于初始价格的概率
    pub prob_above_initial: f64,
    /// 涨幅超过 10% 的概率
    pub prob_above_10pct: f64,
    /// 跌幅超过 10% 的概率
    pub prob_below_10pct: f64,
    /// 平均最大回撤
    pub avg_max_drawdown: f64,
    /// 95% 分位最大回撤
    pub max_drawdown_95: f64,
}

impl std::fmt::Display for PriceDistribution {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "初始: {:.2} | 均值: {:.2} | 标准差: {:.2} | 范围: [{:.2}, {:.2}]\n\
             分位数: 5%={:.2}, 25%={:.2}, 50%={:.2}, 75%={:.2}, 95%={:.2}\n\
             概率: 上涨>{:.1}%, 涨>10%={:.1}%, 跌>10%={:.1}%\n\
             最大回撤: 平均={:.2}%, 95%={:.2}%",
            self.initial_price,
            self.mean,
            self.std_dev,
            self.min,
            self.max,
            self.percentile_5,
            self.percentile_25,
            self.percentile_50,
            self.percentile_75,
            self.percentile_95,
            self.prob_above_initial * 100.0,
            self.prob_above_10pct * 100.0,
            self.prob_below_10pct * 100.0,
            self.avg_max_drawdown * 100.0,
            self.max_drawdown_95 * 100.0,
        )
    }
}

/// 风险价值 (VaR) 计算
pub fn calculate_var(returns: &[f64], confidence_level: f64) -> f64 {
    if returns.is_empty() {
        return 0.0;
    }

    let mut sorted_returns = returns.to_vec();
    sorted_returns.sort_by(|a, b| a.partial_cmp(b).unwrap());

    let index = ((1.0 - confidence_level) * sorted_returns.len() as f64) as usize;
    if index < sorted_returns.len() {
        -sorted_returns[index]
    } else {
        0.0
    }
}

/// 条件风险价值 (CVaR) 计算
pub fn calculate_cvar(returns: &[f64], confidence_level: f64) -> f64 {
    if returns.is_empty() {
        return 0.0;
    }

    let mut sorted_returns = returns.to_vec();
    sorted_returns.sort_by(|a, b| a.partial_cmp(b).unwrap());

    let cutoff_index = ((1.0 - confidence_level) * sorted_returns.len() as f64) as usize;
    if cutoff_index == 0 {
        return 0.0;
    }

    let tail_returns: Vec<f64> = sorted_returns[..cutoff_index].to_vec();
    let avg_tail = tail_returns.iter().sum::<f64>() / tail_returns.len() as f64;
    -avg_tail
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pricing::options::{OptionContract, OptionType};
    use chrono::Utc;

    #[test]
    fn test_monte_carlo_option_pricing() {
        let mc = MonteCarloSimulator::new(10000, 252);

        let option = OptionContract::new(
            "BTC".to_string(),
            OptionType::Call,
            Decimal::from(100),
            Utc::now() + chrono::Duration::days(365),
            Decimal::ZERO,
        );

        let price = mc.price_option(
            &option,
            Decimal::from(100),                    // spot
            Decimal::from_f64(0.05).unwrap(),      // rate 5%
            Decimal::from_f64(0.20).unwrap(),      // volatility 20%
        );

        // ATM call with these parameters should be around 10-11
        assert!(price > Decimal::from(9));
        assert!(price < Decimal::from(12));
    }

    #[test]
    fn test_price_distribution() {
        let mc = MonteCarloSimulator::new(1000, 252);
        let dist = mc.price_distribution(100.0, 0.05, 0.2, 1.0, 42);

        assert!(dist.mean > 0.0);
        assert!(dist.std_dev > 0.0);
        assert!(dist.percentile_5 < dist.percentile_95);
        assert!(dist.prob_above_initial > 0.0);
        assert!(dist.prob_above_initial < 1.0);
    }

    #[test]
    fn test_var_calculation() {
        let returns = vec![-0.05, -0.03, -0.01, 0.01, 0.02, 0.03, 0.05];
        let var_95 = calculate_var(&returns, 0.95);
        assert!(var_95 > 0.0);
    }

    #[test]
    fn test_cvar_calculation() {
        let returns = vec![-0.05, -0.03, -0.01, 0.01, 0.02, 0.03, 0.05];
        let cvar_95 = calculate_cvar(&returns, 0.95);
        assert!(cvar_95 > 0.0);
    }
}
