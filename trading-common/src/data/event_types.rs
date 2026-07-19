// data/event_types.rs
// 全链路交易事件定义
//
// 贯穿 策略分析 → 风控检查 → 下单 → 成交 → 止损/止盈 → 风控平仓
// signal_id 是全链路的唯一关联键

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// 全链路交易事件
///
/// 事件流：
/// 1. StrategyAnalyzed — 策略分析完成（来自 strategy_signals 表）
/// 2. RiskCheck — 风控检查结果
/// 3. OrderPlaced — 订单下单
/// 4. OrderFilled — 订单成交
/// 5. StopTriggered — 止损止盈触发
/// 6. RiskAction — 风控平仓/减仓
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum TradingEvent {
    /// 策略分析完成（从 strategy_signals 表读取，不是实时事件）
    StrategyAnalyzed {
        signal_id: Uuid,
        strategy_id: String,
        symbol: String,
        direction: String,
        entry_price: Decimal,
        confidence: Decimal,
        signal_strength: Option<Decimal>,
        stop_loss: Option<Decimal>,
        take_profit: Option<Decimal>,
        timeframe_details: serde_json::Value,
        market_context: Option<serde_json::Value>,
        reason: String,
        created_at: DateTime<Utc>,
    },
    /// 风控检查结果
    RiskCheck {
        signal_id: Option<Uuid>,
        exchange: String,
        market_type: String,
        symbol: String,
        check_type: String,
        result: String,
        reason: String,
        current_equity: Option<Decimal>,
        peak_equity: Option<Decimal>,
        daily_pnl: Option<Decimal>,
        details: serde_json::Value,
        timestamp: DateTime<Utc>,
    },
    /// 订单下单
    OrderPlaced {
        signal_id: Option<Uuid>,
        order_id: String,
        exchange: String,
        market_type: String,
        symbol: String,
        side: String,
        quantity: Decimal,
        order_type: String,
        timestamp: DateTime<Utc>,
    },
    /// 订单成交
    OrderFilled {
        signal_id: Option<Uuid>,
        order_id: String,
        exchange: String,
        market_type: String,
        symbol: String,
        side: String,
        quantity: Decimal,
        avg_price: Decimal,
        commission: Option<Decimal>,
        slippage: Option<Decimal>,
        pnl: Option<Decimal>,
        event_type: String,
        timestamp: DateTime<Utc>,
    },
    /// 止损止盈触发
    StopTriggered {
        signal_id: Option<Uuid>,
        order_id: Option<String>,
        exchange: String,
        market_type: String,
        symbol: String,
        trigger_type: String,
        trigger_price: Decimal,
        close_price: Decimal,
        quantity: Decimal,
        pnl: Decimal,
        timestamp: DateTime<Utc>,
    },
    /// 风控动作
    RiskAction {
        signal_id: Option<Uuid>,
        exchange: String,
        market_type: String,
        action_type: String,
        symbol: Option<String>,
        reason: String,
        details: serde_json::Value,
        timestamp: DateTime<Utc>,
    },
}

impl TradingEvent {
    /// 获取事件关联的 signal_id
    pub fn signal_id(&self) -> Option<Uuid> {
        match self {
            TradingEvent::StrategyAnalyzed { signal_id, .. } => Some(*signal_id),
            TradingEvent::RiskCheck { signal_id, .. } => *signal_id,
            TradingEvent::OrderPlaced { signal_id, .. } => *signal_id,
            TradingEvent::OrderFilled { signal_id, .. } => *signal_id,
            TradingEvent::StopTriggered { signal_id, .. } => *signal_id,
            TradingEvent::RiskAction { signal_id, .. } => *signal_id,
        }
    }

    /// 获取事件时间戳
    pub fn timestamp(&self) -> DateTime<Utc> {
        match self {
            TradingEvent::StrategyAnalyzed { created_at, .. } => *created_at,
            TradingEvent::RiskCheck { timestamp, .. } => *timestamp,
            TradingEvent::OrderPlaced { timestamp, .. } => *timestamp,
            TradingEvent::OrderFilled { timestamp, .. } => *timestamp,
            TradingEvent::StopTriggered { timestamp, .. } => *timestamp,
            TradingEvent::RiskAction { timestamp, .. } => *timestamp,
        }
    }

    /// 获取事件类型名称
    pub fn event_type_name(&self) -> &'static str {
        match self {
            TradingEvent::StrategyAnalyzed { .. } => "strategy_analyzed",
            TradingEvent::RiskCheck { .. } => "risk_check",
            TradingEvent::OrderPlaced { .. } => "order_placed",
            TradingEvent::OrderFilled { .. } => "order_filled",
            TradingEvent::StopTriggered { .. } => "stop_triggered",
            TradingEvent::RiskAction { .. } => "risk_action",
        }
    }
}
