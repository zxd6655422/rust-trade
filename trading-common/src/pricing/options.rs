// pricing/options.rs
// 期权定价模块 - Black-Scholes 模型

use rust_decimal::Decimal;
use rust_decimal::prelude::*;
use statrs::distribution::{Continuous, ContinuousCDF, Normal};

/// 期权类型
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum OptionType {
    /// 看涨期权
    Call,
    /// 看跌期权
    Put,
}

/// Black-Scholes 期权定价模型
///
/// 用于计算欧式期权的理论价格
#[derive(Debug, Clone)]
pub struct BlackScholes {
    /// 标的资产当前价格
    pub spot: Decimal,
    /// 期权行权价格
    pub strike: Decimal,
    /// 无风险利率（年化）
    pub rate: Decimal,
    /// 波动率（年化）
    pub volatility: Decimal,
    /// 到期时间（年）
    pub time: Decimal,
}

impl BlackScholes {
    /// 创建新的 Black-Scholes 实例
    pub fn new(
        spot: Decimal,
        strike: Decimal,
        rate: Decimal,
        volatility: Decimal,
        time: Decimal,
    ) -> Self {
        Self {
            spot,
            strike,
            rate,
            volatility,
            time,
        }
    }

    /// 计算 d1 参数
    fn d1(&self) -> f64 {
        let spot = self.spot.to_f64().unwrap_or(0.0);
        let strike = self.strike.to_f64().unwrap_or(0.0);
        let rate = self.rate.to_f64().unwrap_or(0.0);
        let vol = self.volatility.to_f64().unwrap_or(0.0);
        let time = self.time.to_f64().unwrap_or(0.0);

        if spot <= 0.0 || strike <= 0.0 || vol <= 0.0 || time <= 0.0 {
            return 0.0;
        }

        ((spot / strike).ln() + (rate + vol * vol / 2.0) * time) / (vol * time.sqrt())
    }

    /// 计算 d2 参数
    fn d2(&self) -> f64 {
        let vol = self.volatility.to_f64().unwrap_or(0.0);
        let time = self.time.to_f64().unwrap_or(0.0);
        self.d1() - vol * time.sqrt()
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

    /// 计算看涨期权价格
    ///
    /// C = S * N(d1) - K * e^(-rT) * N(d2)
    pub fn call_price(&self) -> Decimal {
        let spot = self.spot.to_f64().unwrap_or(0.0);
        let strike = self.strike.to_f64().unwrap_or(0.0);
        let rate = self.rate.to_f64().unwrap_or(0.0);
        let time = self.time.to_f64().unwrap_or(0.0);

        let d1 = self.d1();
        let d2 = self.d2();

        let price = spot * self.norm_cdf(d1)
            - strike * (-rate * time).exp() * self.norm_cdf(d2);

        Decimal::from_f64(price).unwrap_or(Decimal::ZERO)
    }

    /// 计算看跌期权价格
    ///
    /// P = K * e^(-rT) * N(-d2) - S * N(-d1)
    pub fn put_price(&self) -> Decimal {
        let spot = self.spot.to_f64().unwrap_or(0.0);
        let strike = self.strike.to_f64().unwrap_or(0.0);
        let rate = self.rate.to_f64().unwrap_or(0.0);
        let time = self.time.to_f64().unwrap_or(0.0);

        let d1 = self.d1();
        let d2 = self.d2();

        let price = strike * (-rate * time).exp() * self.norm_cdf(-d2)
            - spot * self.norm_cdf(-d1);

        Decimal::from_f64(price).unwrap_or(Decimal::ZERO)
    }

    /// 计算期权价格（根据期权类型）
    pub fn price(&self, option_type: OptionType) -> Decimal {
        match option_type {
            OptionType::Call => self.call_price(),
            OptionType::Put => self.put_price(),
        }
    }
}

/// 期权合约
#[derive(Debug, Clone)]
pub struct OptionContract {
    /// 标的资产
    pub underlying: String,
    /// 期权类型
    pub option_type: OptionType,
    /// 行权价格
    pub strike: Decimal,
    /// 到期时间
    pub expiry: chrono::DateTime<chrono::Utc>,
    /// 权利金（市场价格）
    pub premium: Decimal,
}

impl OptionContract {
    /// 创建新的期权合约
    pub fn new(
        underlying: String,
        option_type: OptionType,
        strike: Decimal,
        expiry: chrono::DateTime<chrono::Utc>,
        premium: Decimal,
    ) -> Self {
        Self {
            underlying,
            option_type,
            strike,
            expiry,
            premium,
        }
    }

    /// 计算到期时间（年）
    pub fn time_to_expiry(&self) -> Decimal {
        let now = chrono::Utc::now();
        let duration = self.expiry - now;
        let seconds = duration.num_seconds() as f64;
        // 一年约 365.25 天 * 24 小时 * 3600 秒
        let years = seconds / (365.25 * 24.0 * 3600.0);
        Decimal::from_f64(years.max(0.0)).unwrap_or(Decimal::ZERO)
    }

    /// 计算内在价值
    pub fn intrinsic_value(&self, spot: Decimal) -> Decimal {
        match self.option_type {
            OptionType::Call => (spot - self.strike).max(Decimal::ZERO),
            OptionType::Put => (self.strike - spot).max(Decimal::ZERO),
        }
    }

    /// 计算时间价值
    pub fn time_value(&self, spot: Decimal) -> Decimal {
        self.premium - self.intrinsic_value(spot)
    }

    /// 是否为实值期权
    pub fn is_in_the_money(&self, spot: Decimal) -> bool {
        match self.option_type {
            OptionType::Call => spot > self.strike,
            OptionType::Put => spot < self.strike,
        }
    }

    /// 是否为虚值期权
    pub fn is_out_of_the_money(&self, spot: Decimal) -> bool {
        !self.is_in_the_money(spot)
    }

    /// 是否为平值期权
    pub fn is_at_the_money(&self, spot: Decimal) -> bool {
        let diff = (spot - self.strike).abs();
        let threshold = self.strike * Decimal::from_f64(0.01).unwrap_or(Decimal::ZERO);
        diff <= threshold
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rust_decimal::Decimal;

    #[test]
    fn test_black_scholes_call() {
        let bs = BlackScholes::new(
            Decimal::from(100),  // spot
            Decimal::from(100),  // strike (ATM)
            Decimal::from_f64(0.05).unwrap(),  // rate 5%
            Decimal::from_f64(0.20).unwrap(),  // volatility 20%
            Decimal::from(1),    // 1 year
        );

        let call_price = bs.call_price();
        // ATM call with these parameters should be around 10.45
        assert!(call_price > Decimal::from(10));
        assert!(call_price < Decimal::from(11));
    }

    #[test]
    fn test_black_scholes_put() {
        let bs = BlackScholes::new(
            Decimal::from(100),  // spot
            Decimal::from(100),  // strike (ATM)
            Decimal::from_f64(0.05).unwrap(),  // rate 5%
            Decimal::from_f64(0.20).unwrap(),  // volatility 20%
            Decimal::from(1),    // 1 year
        );

        let put_price = bs.put_price();
        // ATM put with these parameters should be around 5.57
        assert!(put_price > Decimal::from(5));
        assert!(put_price < Decimal::from(6));
    }

    #[test]
    fn test_put_call_parity() {
        let bs = BlackScholes::new(
            Decimal::from(100),  // spot
            Decimal::from(100),  // strike
            Decimal::from_f64(0.05).unwrap(),  // rate
            Decimal::from_f64(0.20).unwrap(),  // volatility
            Decimal::from(1),    // time
        );

        let call = bs.call_price();
        let put = bs.put_price();

        // Put-Call Parity: C - P = S - K * e^(-rT)
        let spot = Decimal::from(100);
        let strike = Decimal::from(100);
        let rate = Decimal::from_f64(0.05).unwrap();
        let time = Decimal::from(1);

        let discount = (-rate * time).to_f64().map(|r| r.exp()).unwrap_or(1.0);
        let pv_strike = strike * Decimal::from_f64(discount).unwrap_or(Decimal::ONE);

        let left = call - put;
        let right = spot - pv_strike;

        // Allow small rounding error
        let diff = (left - right).abs();
        assert!(diff < Decimal::from_f64(0.01).unwrap());
    }

    #[test]
    fn test_option_contract_intrinsic() {
        let contract = OptionContract::new(
            "BTC".to_string(),
            OptionType::Call,
            Decimal::from(50000),
            chrono::Utc::now() + chrono::Duration::days(30),
            Decimal::from(1000),
        );

        // ITM call
        assert_eq!(
            contract.intrinsic_value(Decimal::from(55000)),
            Decimal::from(5000)
        );

        // OTM call
        assert_eq!(
            contract.intrinsic_value(Decimal::from(45000)),
            Decimal::ZERO
        );
    }
}
