// data/account_types.rs
//
// 统一账户快照数据结构
// 支持 Binance / OKX 等多交易所

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use super::types::DataResult;

// =================================================================
// 账户快照（账户级别汇总）
// =================================================================

/// 账户快照（账户级别汇总）
///
/// 统一不同交易所的账户信息，便于策略计算和交易执行
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountSnapshot {
    pub exchange: String,           // "binance" / "okx"
    pub market_type: String,        // "spot" / "futures" / "swap"
    /// 交易所返回的用户唯一标识（Binance: uid, OKX: uid/mainUid）
    pub uid: Option<String>,
    pub snapshot_at: DateTime<Utc>,

    // ============ 余额相关 ============
    /// 总权益（USD）
    /// Binance合约: totalMarginBalance
    /// OKX: totalEq
    /// Binance现货: sum(free + locked * price)
    pub total_equity: Decimal,

    /// 总余额（不含未实现盈亏）
    /// Binance合约: totalWalletBalance
    /// OKX: sum(details[].eq)
    /// Binance现货: sum(free + locked)
    pub total_balance: Decimal,

    /// 可用余额
    /// Binance合约: availableBalance
    /// OKX: details[].availBal (USDT)
    /// Binance现货: USDT.free
    pub available_balance: Decimal,

    /// 冻结余额
    /// Binance合约: 0 (需计算)
    /// OKX: details[].frozenBal (USDT)
    /// Binance现货: USDT.locked
    pub frozen_balance: Decimal,

    // ============ 盈亏相关 ============
    /// 未实现盈亏
    /// Binance合约: totalUnrealizedProfit
    /// OKX: sum(details[].upl)
    /// Binance现货: 0
    pub unrealized_pnl: Decimal,

    // ============ 保证金相关（仅合约） ============
    /// 初始保证金
    /// Binance合约: totalInitialMargin
    /// OKX: imr
    pub initial_margin: Option<Decimal>,

    /// 维持保证金
    /// Binance合约: totalMaintMargin
    /// OKX: mmr
    pub maint_margin: Option<Decimal>,

    /// 保证金率
    /// Binance合约: 无（需计算）
    /// OKX: mgnRatio
    pub margin_ratio: Option<Decimal>,

    // ============ 持仓相关 ============
    pub position_count: i32,

    // ============ 原始数据 ============
    pub raw_data: Option<serde_json::Value>,
}

impl AccountSnapshot {
    /// 计算保证金率（如果未提供）
    pub fn calc_margin_ratio(&self) -> Option<Decimal> {
        if let Some(ratio) = self.margin_ratio {
            return Some(ratio);
        }
        // 如果有维持保证金和总权益，计算保证金率
        if let (Some(mmr), Some(eq)) = (self.maint_margin, self.initial_margin) {
            if mmr > Decimal::ZERO {
                return Some(eq / mmr);
            }
        }
        None
    }

    /// 是否为合约账户
    pub fn is_futures(&self) -> bool {
        self.market_type == "futures" || self.market_type == "swap"
    }
}

// =================================================================
// 资产余额详情
// =================================================================

/// 资产余额详情
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AssetBalance {
    pub exchange: String,
    pub market_type: String,
    /// 交易所返回的用户唯一标识
    pub uid: Option<String>,
    pub asset: String,              // "USDT" / "BTC"
    pub snapshot_at: DateTime<Utc>,

    /// 总余额
    /// Binance现货: free + locked
    /// Binance合约: walletBalance
    /// OKX: eq
    pub total: Decimal,

    /// 可用余额
    /// Binance现货: free
    /// Binance合约: availableBalance
    /// OKX: availBal
    pub available: Decimal,

    /// 冻结余额
    /// Binance现货: locked
    /// Binance合约: 0
    /// OKX: frozenBal
    pub frozen: Decimal,

    /// 未实现盈亏
    /// Binance现货: 0
    /// Binance合约: unrealizedProfit
    /// OKX: upl
    pub unrealized_pnl: Decimal,

    /// USD价值
    /// Binance现货: 需计算
    /// Binance合约: marginBalance
    /// OKX: eqUsd
    pub usd_value: Option<Decimal>,
}

// =================================================================
// 持仓信息
// =================================================================

/// 持仓方向
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PositionSide {
    /// 多头
    Long,
    /// 空头
    Short,
    /// 净持仓（单向模式）
    Net,
}

impl PositionSide {
    pub fn as_str(&self) -> &'static str {
        match self {
            PositionSide::Long => "LONG",
            PositionSide::Short => "SHORT",
            PositionSide::Net => "NET",
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s.to_uppercase().as_str() {
            "LONG" => PositionSide::Long,
            "SHORT" => PositionSide::Short,
            _ => PositionSide::Net,
        }
    }
}

/// 保证金模式
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MarginType {
    /// 全仓
    Cross,
    /// 逐仓
    Isolated,
}

impl MarginType {
    pub fn as_str(&self) -> &'static str {
        match self {
            MarginType::Cross => "cross",
            MarginType::Isolated => "isolated",
        }
    }

    pub fn from_str(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "isolated" => MarginType::Isolated,
            _ => MarginType::Cross,
        }
    }
}

/// 持仓信息
///
/// 统一不同交易所的持仓信息
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PositionInfo {
    pub exchange: String,
    /// 交易所返回的用户唯一标识
    pub uid: Option<String>,
    /// 统一格式: "BTCUSDT"
    pub symbol: String,
    /// 原始格式: "BTCUSDT" / "BTC-USDT-SWAP"
    pub raw_symbol: String,
    pub snapshot_at: DateTime<Utc>,

    // ============ 持仓基本信息 ============
    pub position_side: PositionSide,
    /// 持仓数量（正数表示多头，负数表示空头）
    pub position_amt: Decimal,
    /// 开仓均价
    pub entry_price: Decimal,
    /// 标记价格
    pub mark_price: Decimal,
    /// 未实现盈亏
    pub unrealized_pnl: Decimal,

    // ============ 杠杆和保证金 ============
    pub leverage: u32,
    pub margin_type: MarginType,
    pub initial_margin: Decimal,
    pub maint_margin: Decimal,

    // ============ 风控相关 ============
    /// 强平价格
    pub liquidation_price: Option<Decimal>,
    /// 名义价值
    pub notional: Decimal,

    // ============ 盈亏平衡 ============
    /// 盈亏平衡价 (Binance: breakEvenPrice)
    pub break_even_price: Option<Decimal>,
    /// 逐仓钱包余额 (Binance: isolatedWallet)
    pub isolated_wallet: Option<Decimal>,

    // ============ 原始数据 ============
    pub raw_data: Option<serde_json::Value>,
}

impl PositionInfo {
    /// 计算盈亏比例
    pub fn pnl_ratio(&self) -> Decimal {
        let cost = self.entry_price * self.position_amt.abs();
        if cost > Decimal::ZERO {
            self.unrealized_pnl / cost
        } else {
            Decimal::ZERO
        }
    }

    /// 是否为多头
    pub fn is_long(&self) -> bool {
        self.position_amt > Decimal::ZERO
    }

    /// 是否为空头
    pub fn is_short(&self) -> bool {
        self.position_amt < Decimal::ZERO
    }

    /// 持仓名义价值（绝对值）
    pub fn abs_notional(&self) -> Decimal {
        self.notional.abs()
    }
}

// =================================================================
// 账户提供者接口
// =================================================================

/// 账户信息统一接口
///
/// 不同交易所实现此接口，提供统一的账户查询能力
#[async_trait::async_trait]
pub trait AccountProvider: Send + Sync {
    /// 获取账户快照
    async fn get_account_snapshot(&self, market_type: &str) -> DataResult<AccountSnapshot>;

    /// 获取资产余额列表
    async fn get_asset_balances(&self, market_type: &str) -> DataResult<Vec<AssetBalance>>;

    /// 获取持仓列表
    async fn get_positions(&self) -> DataResult<Vec<PositionInfo>>;

    /// 获取统一格式的交易对
    /// Binance: "BTCUSDT" -> "BTCUSDT"
    /// OKX: "BTC-USDT-SWAP" -> "BTCUSDT"
    fn normalize_symbol(&self, raw_symbol: &str) -> String;
}
