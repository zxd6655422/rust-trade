// trading-common/src/strategy/analysis.rs
//
// 定义策略分析结果的通用数据结构
// 用于替代简单的 Buy/Sell/Hold 信号，提供更丰富的市场分析信息

use serde::{Deserialize, Serialize};

// =================================================================
// 市场结构判断
// =================================================================

/// 市场结构类型
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum MarketStructureType {
    /// 上升趋势
    TrendingUp,
    /// 下降趋势
    TrendingDown,
    /// 震荡/盘整
    Ranging,
    /// 突破
    Breakout,
    /// 反转
    Reversal,
}

/// 市场结构判断
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarketStructure {
    /// 结构类型
    pub structure_type: MarketStructureType,
    /// 置信度 (0-100)
    pub confidence: f64,
    /// 人类可读的描述
    pub description: String,
}

// =================================================================
// 关键价位
// =================================================================

/// 关键价位集合
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KeyLevels {
    /// 支撑位（从近到远）
    pub support: Vec<f64>,
    /// 阻力位（从近到远）
    pub resistance: Vec<f64>,
    /// 枢轴点（可选）
    pub pivot: Option<f64>,
}

// =================================================================
// 交易偏向
// =================================================================

/// 交易方向
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum TradeDirection {
    /// 做多
    Long,
    /// 做空
    Short,
    /// 中性/观望
    Neutral,
}

/// 交易偏向（策略的分析结论，不是硬信号）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TradeBias {
    /// 方向
    pub direction: TradeDirection,
    /// 置信度 (0-100)
    pub confidence: f64,
    /// 理由
    pub reasoning: String,
}

// =================================================================
// 交易计划
// =================================================================

/// 入场条件类型
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum EntryConditionType {
    /// 价格触及某个区域
    PriceZone,
    /// 形态完成
    PatternComplete,
    /// 时间窗口
    TimeWindow,
    /// 指标确认
    IndicatorConfirm,
}

/// 入场条件
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntryCondition {
    /// 条件类型
    pub condition_type: EntryConditionType,
    /// 描述
    pub description: String,
    /// 是否已满足
    pub is_met: bool,
}

/// 交易计划
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TradeSetup {
    /// 入场区间 [低, 高]
    pub entry_zone: (f64, f64),
    /// 止损价位
    pub stop_loss: f64,
    /// 止盈目标（可能分批）
    pub take_profit: Vec<f64>,
    /// 风险收益比
    pub risk_reward: f64,
    /// 入场条件
    pub entry_conditions: Vec<EntryCondition>,
    /// 无效条件（什么情况下这笔交易作废）
    pub invalidation: String,
}

// =================================================================
// 可视化标注（给前端 K 线图用）
// =================================================================

/// 线条类型
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum LineType {
    Support,
    Resistance,
    Trendline,
    MovingAverage,
}

/// K 线图上的线条标注
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LineAnnotation {
    /// 价格位置
    pub price: f64,
    /// 标签
    pub label: String,
    /// 线条类型
    pub line_type: LineType,
}

/// 区间标注
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ZoneAnnotation {
    /// 区间下沿
    pub from_price: f64,
    /// 区间上沿
    pub to_price: f64,
    /// 标签
    pub label: String,
}

/// 标记类型
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MarkerType {
    Entry,
    Exit,
    StopLoss,
    TakeProfit,
}

/// K 线图上的标记
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarkerAnnotation {
    /// 时间戳（毫秒）
    pub time: i64,
    /// 价格位置
    pub price: f64,
    /// 标记类型
    pub marker_type: MarkerType,
    /// 标签
    pub label: String,
}

/// 可视化标注集合
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Annotations {
    /// 线条标注
    pub lines: Vec<LineAnnotation>,
    /// 区间标注
    pub zones: Vec<ZoneAnnotation>,
    /// 标记标注
    pub markers: Vec<MarkerAnnotation>,
}

// =================================================================
// 策略分析结果（完整输出）
// =================================================================

/// 策略分析结果
///
/// 这是策略层的完整输出，包含市场结构判断、关键价位、交易偏向、交易计划等。
/// 决策引擎接收多个 StrategyAnalysis，综合判断后产生可执行的交易信号。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StrategyAnalysis {
    /// 策略 ID
    pub strategy_id: String,
    /// 策略名称
    pub strategy_name: String,
    /// 分析时间（毫秒时间戳）
    pub timestamp: i64,
    /// 交易对
    pub symbol: String,

    // --- 市场分析 ---
    /// 市场结构判断
    pub market_structure: MarketStructure,
    /// 关键价位
    pub key_levels: KeyLevels,
    /// 交易偏向
    pub bias: TradeBias,

    // --- 交易计划（可选）---
    /// 如果策略认为可以交易，提供具体的交易计划
    pub trade_setup: Option<TradeSetup>,

    // --- 可视化（可选）---
    /// 给前端 K 线图用的标注
    pub annotations: Option<Annotations>,
}

// =================================================================
// 辅助方法
// =================================================================

impl StrategyAnalysis {
    /// 判断是否建议交易（置信度 > 阈值 且 方向不是中性）
    pub fn should_consider_trading(&self, min_confidence: f64) -> bool {
        self.bias.direction != TradeDirection::Neutral && self.bias.confidence >= min_confidence
    }

    /// 获取入场价位（如果有交易计划）
    pub fn get_entry_price(&self) -> Option<f64> {
        self.trade_setup.as_ref().map(|setup| {
            let (low, high) = setup.entry_zone;
            (low + high) / 2.0
        })
    }

    /// 获取止损价位
    pub fn get_stop_loss(&self) -> Option<f64> {
        self.trade_setup.as_ref().map(|setup| setup.stop_loss)
    }

    /// 获取第一个止盈目标
    pub fn get_take_profit(&self) -> Option<f64> {
        self.trade_setup.as_ref().and_then(|setup| setup.take_profit.first().copied())
    }
}

impl MarketStructureType {
    /// 转换为人类可读字符串
    pub fn as_str(&self) -> &'static str {
        match self {
            MarketStructureType::TrendingUp => "trending_up",
            MarketStructureType::TrendingDown => "trending_down",
            MarketStructureType::Ranging => "ranging",
            MarketStructureType::Breakout => "breakout",
            MarketStructureType::Reversal => "reversal",
        }
    }

    /// 从字符串解析
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "trending_up" => Some(MarketStructureType::TrendingUp),
            "trending_down" => Some(MarketStructureType::TrendingDown),
            "ranging" => Some(MarketStructureType::Ranging),
            "breakout" => Some(MarketStructureType::Breakout),
            "reversal" => Some(MarketStructureType::Reversal),
            _ => None,
        }
    }
}

impl TradeDirection {
    /// 转换为人类可读字符串
    pub fn as_str(&self) -> &'static str {
        match self {
            TradeDirection::Long => "long",
            TradeDirection::Short => "short",
            TradeDirection::Neutral => "neutral",
        }
    }

    /// 从字符串解析
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "long" => Some(TradeDirection::Long),
            "short" => Some(TradeDirection::Short),
            "neutral" => Some(TradeDirection::Neutral),
            _ => None,
        }
    }
}

// =================================================================
// 从旧 Signal 转换为 StrategyAnalysis（兼容性）
// =================================================================

/// 旧的信号类型（用于兼容）
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum LegacySignalType {
    Buy,
    Sell,
    Hold,
}

/// 从旧的 Signal 结构转换为 StrategyAnalysis
///
/// 用于渐进式迁移：现有策略可以先用这个函数转换，后续再逐步填充新字段
pub fn from_legacy_signal(
    strategy_id: &str,
    strategy_name: &str,
    symbol: &str,
    signal_type: LegacySignalType,
    signal_strength: f64,
    entry_price: f64,
    stop_loss: Option<f64>,
    take_profit: Option<f64>,
    confidence: f64,
    reason: String,
) -> StrategyAnalysis {
    let direction = match signal_type {
        LegacySignalType::Buy => TradeDirection::Long,
        LegacySignalType::Sell => TradeDirection::Short,
        LegacySignalType::Hold => TradeDirection::Neutral,
    };

    let trade_setup = if direction != TradeDirection::Neutral {
        Some(TradeSetup {
            entry_zone: (entry_price * 0.995, entry_price * 1.005), // 默认 0.5% 区间
            stop_loss: stop_loss.unwrap_or(entry_price * 0.98),     // 默认 2% 止损
            take_profit: take_profit.map(|tp| vec![tp]).unwrap_or_default(),
            risk_reward: if let (Some(sl), Some(tp)) = (stop_loss, take_profit) {
                (tp - entry_price).abs() / (entry_price - sl).abs()
            } else {
                2.0 // 默认 1:2 风险收益比
            },
            entry_conditions: vec![],
            invalidation: "价格跌破止损位".to_string(),
        })
    } else {
        None
    };

    StrategyAnalysis {
        strategy_id: strategy_id.to_string(),
        strategy_name: strategy_name.to_string(),
        timestamp: chrono::Utc::now().timestamp_millis(),
        symbol: symbol.to_string(),
        market_structure: MarketStructure {
            structure_type: MarketStructureType::Ranging, // 默认，后续策略可覆盖
            confidence: confidence,
            description: reason.clone(),
        },
        key_levels: KeyLevels {
            support: stop_loss.map(|sl| vec![sl]).unwrap_or_default(),
            resistance: take_profit.map(|tp| vec![tp]).unwrap_or_default(),
            pivot: None,
        },
        bias: TradeBias {
            direction,
            confidence,
            reasoning: reason,
        },
        trade_setup,
        annotations: None,
    }
}
