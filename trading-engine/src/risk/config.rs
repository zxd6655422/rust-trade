// risk/config.rs
// 风控配置

use rust_decimal::Decimal;
use serde::Deserialize;

fn default_risk_per_trade_pct() -> Decimal {
    Decimal::from(2) / Decimal::from(100) // 2%
}

fn default_max_position_pct() -> Decimal {
    Decimal::from(30) / Decimal::from(100) // 30%
}

/// 风控配置
#[derive(Debug, Clone, Deserialize)]
pub struct RiskConfig {
    // ===== 基础风控 =====

    /// 单笔最大仓位占权益百分比 (如 0.3 = 30%)
    /// order_value > equity * max_position_pct 时拒绝
    #[serde(default = "default_max_position_pct")]
    pub max_position_pct: Decimal,

    /// 止损百分比 (如 0.02 = 2%)
    pub stop_loss_pct: Decimal,

    /// 止盈百分比 (如 0.04 = 4%)
    pub take_profit_pct: Decimal,

    /// 单笔风险占账户权益的百分比 (如 0.02 = 2%)
    /// 用于动态仓位计算: position_value = equity * risk_per_trade_pct / stop_loss_pct
    #[serde(default = "default_risk_per_trade_pct")]
    pub risk_per_trade_pct: Decimal,

    // ===== 中级风控 =====

    /// 日最大亏损 (USDT)
    pub max_daily_loss: Decimal,

    /// 最大回撤保护 (如 0.15 = 15%)
    pub max_drawdown_pct: Decimal,

    /// 最大总曝光度 (如 0.8 = 80%)
    pub max_exposure_pct: Decimal,

    // ===== 高级风控 =====

    /// Kelly 公式分数 (如 0.25 = 1/4 Kelly)
    pub kelly_fraction: Decimal,

    /// 波动率计算回溯期 (tick 数量)
    pub volatility_lookback: u32,

    /// 目标波动率 (如 0.15 = 15%)
    pub volatility_target: Decimal,

    /// 黑天鹅检测阈值 (如 0.05 = 5% 瞬间波动)
    pub black_swan_threshold: Decimal,

    /// 熔断冷却时间 (秒)
    pub circuit_breaker_cooldown: u64,

    /// 每日重置小时 (UTC 0-23, 0=午夜)
    /// peak_equity 和 daily_pnl 在该小时跨天时重置
    #[serde(default)]
    pub daily_reset_hour: u32,
}

impl Default for RiskConfig {
    fn default() -> Self {
        Self {
            // 基础风控
            max_position_pct: Decimal::from(30) / Decimal::from(100), // 30%
            stop_loss_pct: Decimal::from(2) / Decimal::from(100), // 2%
            take_profit_pct: Decimal::from(4) / Decimal::from(100), // 4%
            risk_per_trade_pct: Decimal::from(2) / Decimal::from(100), // 2%

            // 中级风控
            max_daily_loss: Decimal::from(200),
            max_drawdown_pct: Decimal::from(15) / Decimal::from(100), // 15%
            max_exposure_pct: Decimal::from(80) / Decimal::from(100), // 80%

            // 高级风控
            kelly_fraction: Decimal::from(25) / Decimal::from(100), // 1/4 Kelly
            volatility_lookback: 20,
            volatility_target: Decimal::from(15) / Decimal::from(100), // 15%
            black_swan_threshold: Decimal::from(5) / Decimal::from(100), // 5%
            circuit_breaker_cooldown: 3600, // 1 小时
            daily_reset_hour: 0, // UTC 午夜
        }
    }
}
