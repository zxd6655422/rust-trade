// pricing/greeks.rs
// Greeks 计算模块 - 期权风险指标

use rust_decimal::Decimal;
use rust_decimal::prelude::*;
use statrs::distribution::{Continuous, ContinuousCDF, Normal};

use super::options::{BlackScholes, OptionType};

/// Greeks 期权风险指标
#[derive(Debug, Clone)]
pub struct Greeks {
    /// Delta: 期权价格对标的资产价格的敏感度
    pub delta: Decimal,
    /// Gamma: Delta 对标的资产价格的敏感度
    pub gamma: Decimal,
    /// Theta: 期权价格对时间的敏感度（每天）
    pub theta: Decimal,
    /// Vega: 期权价格对波动率的敏感度（每 1% 波动率变化）
    pub vega: Decimal,
    /// Rho: 期权价格对利率的敏感度（每 1% 利率变化）
    pub rho: Decimal,
}

impl std::fmt::Display for Greeks {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Delta: {:.4} | Gamma: {:.4} | Theta: {:.4} | Vega: {:.4} | Rho: {:.4}",
            self.delta, self.gamma, self.theta, self.vega, self.rho
        )
    }
}

/// Greeks 计算器
pub struct GreeksCalculator {
    bs: BlackScholes,
}

impl GreeksCalculator {
    /// 创建新的 Greeks 计算器
    pub fn new(bs: BlackScholes) -> Self {
        Self { bs }
    }

    /// 标准正态分布累积分布函数
    fn norm_cdf(&self, x: f64) -> f64 {
        let normal = Normal::new(0.0, 1.0).unwrap();
        normal.cdf(x)
    }

    /// 标准正态分布概率密度函数
    fn norm_pdf(&self, x: f64) -> f64 {
        let normal = Normal::new(0.0, 1.0).unwrap();
        normal.pdf(x)
    }

    /// 计算 d1
    fn d1(&self) -> f64 {
        let spot = self.bs.spot.to_f64().unwrap_or(0.0);
        let strike = self.bs.strike.to_f64().unwrap_or(0.0);
        let rate = self.bs.rate.to_f64().unwrap_or(0.0);
        let vol = self.bs.volatility.to_f64().unwrap_or(0.0);
        let time = self.bs.time.to_f64().unwrap_or(0.0);

        if spot <= 0.0 || strike <= 0.0 || vol <= 0.0 || time <= 0.0 {
            return 0.0;
        }

        ((spot / strike).ln() + (rate + vol * vol / 2.0) * time) / (vol * time.sqrt())
    }

    /// 计算 d2
    fn d2(&self) -> f64 {
        let vol = self.bs.volatility.to_f64().unwrap_or(0.0);
        let time = self.bs.time.to_f64().unwrap_or(0.0);
        self.d1() - vol * time.sqrt()
    }

    /// 计算 Delta
    ///
    /// Delta = N(d1) for Call
    /// Delta = N(d1) - 1 for Put
    pub fn delta(&self, option_type: OptionType) -> Decimal {
        let d1 = self.d1();
        let delta = match option_type {
            OptionType::Call => self.norm_cdf(d1),
            OptionType::Put => self.norm_cdf(d1) - 1.0,
        };
        Decimal::from_f64(delta).unwrap_or(Decimal::ZERO)
    }

    /// 计算 Gamma
    ///
    /// Gamma = N'(d1) / (S * σ * √T)
    pub fn gamma(&self) -> Decimal {
        let spot = self.bs.spot.to_f64().unwrap_or(0.0);
        let vol = self.bs.volatility.to_f64().unwrap_or(0.0);
        let time = self.bs.time.to_f64().unwrap_or(0.0);
        let d1 = self.d1();

        if spot <= 0.0 || vol <= 0.0 || time <= 0.0 {
            return Decimal::ZERO;
        }

        let gamma = self.norm_pdf(d1) / (spot * vol * time.sqrt());
        Decimal::from_f64(gamma).unwrap_or(Decimal::ZERO)
    }

    /// 计算 Theta（每天）
    ///
    /// Theta_Call = -(S * N'(d1) * σ) / (2√T) - rKe^(-rT)N(d2)
    /// Theta_Put = -(S * N'(d1) * σ) / (2√T) + rKe^(-rT)N(-d2)
    pub fn theta(&self, option_type: OptionType) -> Decimal {
        let spot = self.bs.spot.to_f64().unwrap_or(0.0);
        let strike = self.bs.strike.to_f64().unwrap_or(0.0);
        let rate = self.bs.rate.to_f64().unwrap_or(0.0);
        let vol = self.bs.volatility.to_f64().unwrap_or(0.0);
        let time = self.bs.time.to_f64().unwrap_or(0.0);
        let d1 = self.d1();
        let d2 = self.d2();

        if spot <= 0.0 || vol <= 0.0 || time <= 0.0 {
            return Decimal::ZERO;
        }

        let common = -(spot * self.norm_pdf(d1) * vol) / (2.0 * time.sqrt());
        let theta = match option_type {
            OptionType::Call => common - rate * strike * (-rate * time).exp() * self.norm_cdf(d2),
            OptionType::Put => common + rate * strike * (-rate * time).exp() * self.norm_cdf(-d2),
        };

        // 转换为每天（除以 365）
        let theta_per_day = theta / 365.0;
        Decimal::from_f64(theta_per_day).unwrap_or(Decimal::ZERO)
    }

    /// 计算 Vega（每 1% 波动率变化）
    ///
    /// Vega = S * √T * N'(d1)
    pub fn vega(&self) -> Decimal {
        let spot = self.bs.spot.to_f64().unwrap_or(0.0);
        let time = self.bs.time.to_f64().unwrap_or(0.0);
        let d1 = self.d1();

        if spot <= 0.0 || time <= 0.0 {
            return Decimal::ZERO;
        }

        let vega = spot * time.sqrt() * self.norm_pdf(d1);
        // 转换为每 1% 波动率变化
        let vega_per_1pct = vega / 100.0;
        Decimal::from_f64(vega_per_1pct).unwrap_or(Decimal::ZERO)
    }

    /// 计算 Rho（每 1% 利率变化）
    ///
    /// Rho_Call = KTe^(-rT)N(d2)
    /// Rho_Put = -KTe^(-rT)N(-d2)
    pub fn rho(&self, option_type: OptionType) -> Decimal {
        let strike = self.bs.strike.to_f64().unwrap_or(0.0);
        let rate = self.bs.rate.to_f64().unwrap_or(0.0);
        let time = self.bs.time.to_f64().unwrap_or(0.0);
        let d2 = self.d2();

        if strike <= 0.0 || time <= 0.0 {
            return Decimal::ZERO;
        }

        let rho = match option_type {
            OptionType::Call => strike * time * (-rate * time).exp() * self.norm_cdf(d2),
            OptionType::Put => -strike * time * (-rate * time).exp() * self.norm_cdf(-d2),
        };

        // 转换为每 1% 利率变化
        let rho_per_1pct = rho / 100.0;
        Decimal::from_f64(rho_per_1pct).unwrap_or(Decimal::ZERO)
    }

    /// 计算所有 Greeks
    pub fn calculate(&self, option_type: OptionType) -> Greeks {
        Greeks {
            delta: self.delta(option_type),
            gamma: self.gamma(),
            theta: self.theta(option_type),
            vega: self.vega(),
            rho: self.rho(option_type),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pricing::options::BlackScholes;

    fn create_test_bs() -> BlackScholes {
        BlackScholes::new(
            Decimal::from(100),  // spot
            Decimal::from(100),  // strike (ATM)
            Decimal::from_f64(0.05).unwrap(),  // rate 5%
            Decimal::from_f64(0.20).unwrap(),  // volatility 20%
            Decimal::from(1),    // 1 year
        )
    }

    #[test]
    fn test_delta_call() {
        let bs = create_test_bs();
        let calc = GreeksCalculator::new(bs);
        let delta = calc.delta(OptionType::Call);

        // ATM call delta should be around 0.6
        assert!(delta > Decimal::from_f64(0.5).unwrap());
        assert!(delta < Decimal::from_f64(0.7).unwrap());
    }

    #[test]
    fn test_delta_put() {
        let bs = create_test_bs();
        let calc = GreeksCalculator::new(bs);
        let delta = calc.delta(OptionType::Put);

        // ATM put delta should be around -0.4
        assert!(delta < Decimal::from_f64(-0.3).unwrap());
        assert!(delta > Decimal::from_f64(-0.5).unwrap());
    }

    #[test]
    fn test_gamma() {
        let bs = create_test_bs();
        let calc = GreeksCalculator::new(bs);
        let gamma = calc.gamma();

        // Gamma should be positive
        assert!(gamma > Decimal::ZERO);
    }

    #[test]
    fn test_vega() {
        let bs = create_test_bs();
        let calc = GreeksCalculator::new(bs);
        let vega = calc.vega();

        // Vega should be positive
        assert!(vega > Decimal::ZERO);
    }

    #[test]
    fn test_theta() {
        let bs = create_test_bs();
        let calc = GreeksCalculator::new(bs);
        let theta = calc.theta(OptionType::Call);

        // Theta should be negative (time decay)
        assert!(theta < Decimal::ZERO);
    }

    #[test]
    fn test_calculate_all() {
        let bs = create_test_bs();
        let calc = GreeksCalculator::new(bs);
        let greeks = calc.calculate(OptionType::Call);

        // Verify all Greeks are calculated
        assert!(greeks.delta > Decimal::ZERO);
        assert!(greeks.gamma > Decimal::ZERO);
        assert!(greeks.theta < Decimal::ZERO);
        assert!(greeks.vega > Decimal::ZERO);
    }
}
