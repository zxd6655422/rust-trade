// simulation/mod.rs
// 随机过程模拟模块

pub mod brownian;
pub mod monte_carlo;

// Re-export commonly used types
pub use brownian::{BrownianMotion, GeometricBrownianMotion, PathStatistics};
pub use monte_carlo::{calculate_cvar, calculate_var, MonteCarloSimulator, PriceDistribution};
