// simulation/brownian.rs
// 布朗运动模拟模块

use rand::rngs::StdRng;
use rand::SeedableRng;
use rand_distr::{Distribution, Normal};

/// 标准布朗运动
///
/// B(t) = μt + σW(t)
/// 其中 W(t) 是维纳过程（标准布朗运动）
#[derive(Debug, Clone)]
pub struct BrownianMotion {
    /// 漂移率 (drift)
    pub drift: f64,
    /// 扩散率 (diffusion/volatility)
    pub diffusion: f64,
}

impl BrownianMotion {
    /// 创建新的布朗运动实例
    pub fn new(drift: f64, diffusion: f64) -> Self {
        Self { drift, diffusion }
    }

    /// 生成布朗运动路径
    ///
    /// # Arguments
    /// * `n` - 时间步数
    /// * `dt` - 时间步长
    /// * `seed` - 随机种子（用于可重复性）
    ///
    /// # Returns
    /// 路径上的值序列
    pub fn generate(&self, n: usize, dt: f64, seed: u64) -> Vec<f64> {
        let mut rng = StdRng::seed_from_u64(seed);
        let normal = Normal::new(0.0, 1.0).unwrap();
        let mut path = Vec::with_capacity(n);
        let mut current = 0.0;

        for _ in 0..n {
            let noise = normal.sample(&mut rng);
            let increment = self.drift * dt + self.diffusion * noise * dt.sqrt();
            current += increment;
            path.push(current);
        }

        path
    }

    /// 生成多条布朗运动路径
    pub fn generate_paths(&self, n: usize, dt: f64, n_paths: usize, seed: u64) -> Vec<Vec<f64>> {
        (0..n_paths)
            .map(|i| self.generate(n, dt, seed + i as u64))
            .collect()
    }
}

/// 几何布朗运动 (GBM)
///
/// 用于模拟股票/加密货币价格
/// dS = μSdt + σSdW
/// S(t) = S(0) * exp((μ - σ²/2)t + σW(t))
#[derive(Debug, Clone)]
pub struct GeometricBrownianMotion {
    /// 漂移率 (预期收益率)
    pub drift: f64,
    /// 波动率
    pub volatility: f64,
}

impl GeometricBrownianMotion {
    /// 创建新的几何布朗运动实例
    pub fn new(drift: f64, volatility: f64) -> Self {
        Self { drift, volatility }
    }

    /// 生成几何布朗运动路径
    ///
    /// # Arguments
    /// * `initial_price` - 初始价格
    /// * `n` - 时间步数
    /// * `dt` - 时间步长
    /// * `seed` - 随机种子
    ///
    /// # Returns
    /// 价格路径序列
    pub fn generate(&self, initial_price: f64, n: usize, dt: f64, seed: u64) -> Vec<f64> {
        let mut rng = StdRng::seed_from_u64(seed);
        let normal = Normal::new(0.0, 1.0).unwrap();
        let mut path = Vec::with_capacity(n);
        let mut current = initial_price;

        for _ in 0..n {
            let noise = normal.sample(&mut rng);
            let increment = (self.drift - self.volatility.powi(2) / 2.0) * dt
                + self.volatility * noise * dt.sqrt();
            current *= increment.exp();
            path.push(current);
        }

        path
    }

    /// 生成多条价格路径
    pub fn generate_paths(
        &self,
        initial_price: f64,
        n: usize,
        dt: f64,
        n_paths: usize,
        seed: u64,
    ) -> Vec<Vec<f64>> {
        (0..n_paths)
            .map(|i| self.generate(initial_price, n, dt, seed + i as u64))
            .collect()
    }

    /// 计算路径的对数收益率
    pub fn log_returns(path: &[f64]) -> Vec<f64> {
        if path.len() < 2 {
            return Vec::new();
        }

        path.windows(2)
            .map(|w| (w[1] / w[0]).ln())
            .collect()
    }

    /// 计算路径的简单收益率
    pub fn simple_returns(path: &[f64]) -> Vec<f64> {
        if path.len() < 2 {
            return Vec::new();
        }

        path.windows(2)
            .map(|w| (w[1] - w[0]) / w[0])
            .collect()
    }
}

/// 路径统计信息
#[derive(Debug, Clone)]
pub struct PathStatistics {
    /// 终值
    pub final_value: f64,
    /// 最大值
    pub max_value: f64,
    /// 最小值
    pub min_value: f64,
    /// 均值
    pub mean: f64,
    /// 标准差
    pub std_dev: f64,
    /// 最大回撤
    pub max_drawdown: f64,
}

impl PathStatistics {
    /// 计算路径统计信息
    pub fn from_path(path: &[f64]) -> Self {
        if path.is_empty() {
            return Self {
                final_value: 0.0,
                max_value: 0.0,
                min_value: 0.0,
                mean: 0.0,
                std_dev: 0.0,
                max_drawdown: 0.0,
            };
        }

        let final_value = *path.last().unwrap();
        let max_value = path.iter().fold(f64::NEG_INFINITY, |a, &b| a.max(b));
        let min_value = path.iter().fold(f64::INFINITY, |a, &b| a.min(b));
        let mean = path.iter().sum::<f64>() / path.len() as f64;

        let variance = path.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / path.len() as f64;
        let std_dev = variance.sqrt();

        // 计算最大回撤
        let mut max_drawdown = 0.0;
        let mut peak = path[0];
        for &value in path.iter().skip(1) {
            if value > peak {
                peak = value;
            }
            let drawdown = (peak - value) / peak;
            if drawdown > max_drawdown {
                max_drawdown = drawdown;
            }
        }

        Self {
            final_value,
            max_value,
            min_value,
            mean,
            std_dev,
            max_drawdown,
        }
    }

    /// 计算多条路径的统计信息
    pub fn from_paths(paths: &[Vec<f64>]) -> Vec<Self> {
        paths.iter().map(|p| Self::from_path(p)).collect()
    }

    /// 计算多条路径的终值分布统计
    pub fn final_value_distribution(stats: &[Self]) -> (f64, f64, f64, f64) {
        let finals: Vec<f64> = stats.iter().map(|s| s.final_value).collect();
        let mean = finals.iter().sum::<f64>() / finals.len() as f64;
        let variance = finals.iter().map(|x| (x - mean).powi(2)).sum::<f64>() / finals.len() as f64;
        let std_dev = variance.sqrt();
        let min = finals.iter().fold(f64::INFINITY, |a, &b| a.min(b));
        let max = finals.iter().fold(f64::NEG_INFINITY, |a, &b| a.max(b));
        (mean, std_dev, min, max)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_brownian_motion() {
        let bm = BrownianMotion::new(0.0, 1.0);
        let path = bm.generate(100, 0.01, 42);

        assert_eq!(path.len(), 100);
        // 路径应该有一定的变化
        let first = path[0];
        let last = path[path.len() - 1];
        assert_ne!(first, last);
    }

    #[test]
    fn test_geometric_brownian_motion() {
        let gbm = GeometricBrownianMotion::new(0.05, 0.2);
        let path = gbm.generate(100.0, 252, 1.0 / 252.0, 42);

        assert_eq!(path.len(), 252);
        // 价格应该保持正值
        for price in &path {
            assert!(*price > 0.0);
        }
    }

    #[test]
    fn test_gbm_multiple_paths() {
        let gbm = GeometricBrownianMotion::new(0.05, 0.2);
        let paths = gbm.generate_paths(100.0, 252, 1.0 / 252.0, 1000, 42);

        assert_eq!(paths.len(), 1000);
        assert_eq!(paths[0].len(), 252);
    }

    #[test]
    fn test_path_statistics() {
        let gbm = GeometricBrownianMotion::new(0.05, 0.2);
        let path = gbm.generate(100.0, 252, 1.0 / 252.0, 42);
        let stats = PathStatistics::from_path(&path);

        assert!(stats.final_value > 0.0);
        assert!(stats.max_value >= stats.min_value);
        assert!(stats.std_dev >= 0.0);
        assert!(stats.max_drawdown >= 0.0);
        assert!(stats.max_drawdown <= 1.0);
    }

    #[test]
    fn test_returns() {
        let path = vec![100.0, 105.0, 102.0, 110.0];

        let log_rets = GeometricBrownianMotion::log_returns(&path);
        assert_eq!(log_rets.len(), 3);

        let simple_rets = GeometricBrownianMotion::simple_returns(&path);
        assert_eq!(simple_rets.len(), 3);
        assert!((simple_rets[0] - 0.05).abs() < 1e-10);
    }
}
