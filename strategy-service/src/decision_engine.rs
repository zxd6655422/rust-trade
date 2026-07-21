// strategy-service/src/decision_engine.rs
//
// 决策引擎：汇总多个策略的分析结果，产生可执行的交易信号
//
// 核心逻辑：
// 1. 接收多个 StrategyAnalysis
// 2. 过滤低置信度的分析
// 3. 检查策略共识（方向一致性）
// 4. 综合多个策略的入场区间
// 5. 生成最终的交易信号

use tracing::{debug, info, warn};
use serde::{Deserialize, Serialize};

use trading_common::strategy::analysis::{
    KeyLevels, MarketStructure, StrategyAnalysis, TradeDirection, TradeSetup,
};

// =================================================================
// 决策配置
// =================================================================

/// 决策引擎配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecisionConfig {
    /// 最低置信度阈值 (0-100)
    pub min_confidence: f64,
    /// 最少需要几个策略共识
    pub min_consensus_count: usize,
    /// 是否启用风险收益比检查
    pub check_risk_reward: bool,
    /// 最低风险收益比
    pub min_risk_reward: f64,
}

impl Default for DecisionConfig {
    fn default() -> Self {
        Self {
            min_confidence: 70.0,
            min_consensus_count: 2,
            check_risk_reward: true,
            min_risk_reward: 1.5,
        }
    }
}

// =================================================================
// 决策结果
// =================================================================

/// 决策结果
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DecisionResult {
    /// 是否产生交易信号
    pub should_trade: bool,
    /// 交易方向
    pub direction: TradeDirection,
    /// 综合置信度
    pub confidence: f64,
    /// 共识的策略列表
    pub consensus_strategies: Vec<String>,
    /// 综合的交易计划
    pub trade_setup: Option<TradeSetup>,
    /// 综合的关键价位
    pub key_levels: KeyLevels,
    /// 综合的市场结构
    pub market_structure: MarketStructure,
    /// 决策理由
    pub reasoning: String,
}

// =================================================================
// 决策引擎
// =================================================================

/// 决策引擎
pub struct DecisionEngine {
    config: DecisionConfig,
}

impl DecisionEngine {
    /// 创建新的决策引擎
    pub fn new(config: DecisionConfig) -> Self {
        Self { config }
    }

    /// 使用默认配置创建
    pub fn with_defaults() -> Self {
        Self::new(DecisionConfig::default())
    }

    /// 汇总多个策略分析结果，产生决策
    ///
    /// # 参数
    /// - `analyses`: 多个策略的分析结果
    ///
    /// # 返回
    /// - `DecisionResult`: 决策结果
    pub fn decide(&self, analyses: &[StrategyAnalysis]) -> DecisionResult {
        if analyses.is_empty() {
            return DecisionResult::no_trade("没有策略分析结果");
        }

        debug!("决策引擎收到 {} 个策略分析", analyses.len());

        // 1. 过滤低置信度的分析
        let valid: Vec<&StrategyAnalysis> = analyses
            .iter()
            .filter(|a| a.bias.confidence >= self.config.min_confidence)
            .collect();

        if valid.is_empty() {
            return DecisionResult::no_trade(&format!(
                "所有策略置信度都低于阈值 {}%",
                self.config.min_confidence
            ));
        }

        debug!("过滤后剩余 {} 个有效分析", valid.len());

        // 2. 统计方向票数
        let long_count = valid
            .iter()
            .filter(|a| a.bias.direction == TradeDirection::Long)
            .count();
        let short_count = valid
            .iter()
            .filter(|a| a.bias.direction == TradeDirection::Short)
            .count();

        // 3. 判断是否有足够共识
        let (direction, consensus_count) = if long_count >= self.config.min_consensus_count
            && long_count > short_count
        {
            (TradeDirection::Long, long_count)
        } else if short_count >= self.config.min_consensus_count && short_count > long_count {
            (TradeDirection::Short, short_count)
        } else {
            return DecisionResult::no_trade(&format!(
                "策略共识不足 (做多: {}, 做空: {}, 需要: {})",
                long_count, short_count, self.config.min_consensus_count
            ));
        };

        info!(
            "方向共识: {:?} ({}个策略一致)",
            direction, consensus_count
        );

        // 4. 收集共识策略的分析
        let consensus_analyses: Vec<&StrategyAnalysis> = valid
            .iter()
            .filter(|a| a.bias.direction == direction)
            .cloned()
            .collect();

        // 5. 计算综合置信度（加权平均）
        let total_confidence: f64 = consensus_analyses
            .iter()
            .map(|a| a.bias.confidence)
            .sum();
        let avg_confidence = total_confidence / consensus_analyses.len() as f64;

        // 6. 综合关键价位
        let key_levels = self.merge_key_levels(&consensus_analyses);

        // 7. 综合交易计划
        let trade_setup = self.merge_trade_setups(&consensus_analyses, &direction);

        // 8. 检查风险收益比
        if self.config.check_risk_reward {
            if let Some(ref setup) = trade_setup {
                if setup.risk_reward < self.config.min_risk_reward {
                    return DecisionResult::no_trade(&format!(
                        "风险收益比不足: {:.2} < {:.2}",
                        setup.risk_reward, self.config.min_risk_reward
                    ));
                }
            }
        }

        // 9. 综合市场结构（取置信度最高的）
        let market_structure = consensus_analyses
            .iter()
            .max_by(|a, b| {
                a.market_structure
                    .confidence
                    .partial_cmp(&b.market_structure.confidence)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .map(|a| a.market_structure.clone())
            .unwrap_or(MarketStructure {
                structure_type: trading_common::strategy::analysis::MarketStructureType::Ranging,
                confidence: 50.0,
                description: "无法确定市场结构".to_string(),
            });

        // 10. 生成决策理由
        let strategy_names: Vec<String> = consensus_analyses
            .iter()
            .map(|a| a.strategy_name.clone())
            .collect();
        let reasoning = format!(
            "{}个策略共识做{}: {} (平均置信度: {:.1}%)",
            consensus_count,
            match direction {
                TradeDirection::Long => "多",
                TradeDirection::Short => "空",
                TradeDirection::Neutral => "中性",
            },
            strategy_names.join(" + "),
            avg_confidence
        );

        DecisionResult {
            should_trade: true,
            direction,
            confidence: avg_confidence,
            consensus_strategies: strategy_names,
            trade_setup,
            key_levels,
            market_structure,
            reasoning,
        }
    }

    /// 综合多个策略的关键价位
    fn merge_key_levels(&self, analyses: &[&StrategyAnalysis]) -> KeyLevels {
        let mut all_support: Vec<f64> = Vec::new();
        let mut all_resistance: Vec<f64> = Vec::new();
        let mut pivots: Vec<f64> = Vec::new();

        for a in analyses {
            all_support.extend(&a.key_levels.support);
            all_resistance.extend(&a.key_levels.resistance);
            if let Some(p) = a.key_levels.pivot {
                pivots.push(p);
            }
        }

        // 去重并排序
        all_support.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        all_support.dedup();

        all_resistance.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
        all_resistance.dedup();

        // 取中位数作为枢轴点
        let pivot = if !pivots.is_empty() {
            pivots.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
            Some(pivots[pivots.len() / 2])
        } else {
            None
        };

        KeyLevels {
            support: all_support,
            resistance: all_resistance,
            pivot,
        }
    }

    /// 综合多个策略的交易计划
    fn merge_trade_setups(
        &self,
        analyses: &[&StrategyAnalysis],
        direction: &TradeDirection,
    ) -> Option<TradeSetup> {
        // 收集所有有交易计划的分析
        let setups: Vec<&TradeSetup> = analyses
            .iter()
            .filter_map(|a| a.trade_setup.as_ref())
            .collect();

        if setups.is_empty() {
            return None;
        }

        // 综合入场区间（取交集或平均）
        let entry_low = setups
            .iter()
            .map(|s| s.entry_zone.0)
            .max_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
            .unwrap_or(0.0);
        let entry_high = setups
            .iter()
            .map(|s| s.entry_zone.1)
            .min_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
            .unwrap_or(0.0);

        // 如果交集为空，取平均值
        let (final_entry_low, final_entry_high) = if entry_low <= entry_high {
            (entry_low, entry_high)
        } else {
            let avg_low: f64 = setups.iter().map(|s| s.entry_zone.0).sum::<f64>()
                / setups.len() as f64;
            let avg_high: f64 = setups.iter().map(|s| s.entry_zone.1).sum::<f64>()
                / setups.len() as f64;
            (avg_low, avg_high)
        };

        // 综合止损（做多取最高，做空取最低）
        let stop_loss = match direction {
            TradeDirection::Long => setups
                .iter()
                .map(|s| s.stop_loss)
                .max_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
                .unwrap_or(0.0),
            TradeDirection::Short => setups
                .iter()
                .map(|s| s.stop_loss)
                .min_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
                .unwrap_or(0.0),
            TradeDirection::Neutral => setups.iter().map(|s| s.stop_loss).sum::<f64>()
                / setups.len() as f64,
        };

        // 综合止盈（取平均值）
        let all_tp: Vec<f64> = setups.iter().flat_map(|s| s.take_profit.clone()).collect();
        let take_profit = if !all_tp.is_empty() {
            vec![all_tp.iter().sum::<f64>() / all_tp.len() as f64]
        } else {
            vec![]
        };

        // 计算风险收益比
        let entry_mid = (final_entry_low + final_entry_high) / 2.0;
        let risk = (entry_mid - stop_loss).abs();
        let reward = take_profit.first().map(|tp| (tp - entry_mid).abs()).unwrap_or(0.0);
        let risk_reward = if risk > 0.0 {
            reward / risk
        } else {
            0.0
        };

        // 综合无效条件
        let invalidations: Vec<String> = setups.iter().map(|s| s.invalidation.clone()).collect();
        let invalidation = invalidations.join(" 或 ");

        Some(TradeSetup {
            entry_zone: (final_entry_low, final_entry_high),
            stop_loss,
            take_profit,
            risk_reward,
            entry_conditions: vec![], // 入场条件需要更复杂的逻辑
            invalidation,
        })
    }
}

// =================================================================
// DecisionResult 辅助方法
// =================================================================

impl DecisionResult {
    /// 创建不交易的决策结果
    fn no_trade(reason: &str) -> Self {
        Self {
            should_trade: false,
            direction: TradeDirection::Neutral,
            confidence: 0.0,
            consensus_strategies: vec![],
            trade_setup: None,
            key_levels: KeyLevels {
                support: vec![],
                resistance: vec![],
                pivot: None,
            },
            market_structure: MarketStructure {
                structure_type: trading_common::strategy::analysis::MarketStructureType::Ranging,
                confidence: 0.0,
                description: reason.to_string(),
            },
            reasoning: reason.to_string(),
        }
    }

    /// 转换为 Signal（兼容现有系统）
    pub fn to_signal(&self) -> Option<trading_common::strategy::analysis::LegacySignalType> {
        if !self.should_trade {
            return None;
        }
        match self.direction {
            TradeDirection::Long => Some(trading_common::strategy::analysis::LegacySignalType::Buy),
            TradeDirection::Short => Some(trading_common::strategy::analysis::LegacySignalType::Sell),
            TradeDirection::Neutral => None,
        }
    }
}

// =================================================================
// 测试
// =================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use trading_common::strategy::analysis::{
        MarketStructureType, TradeBias,
    };

    fn create_test_analysis(
        strategy_id: &str,
        direction: TradeDirection,
        confidence: f64,
    ) -> StrategyAnalysis {
        StrategyAnalysis {
            strategy_id: strategy_id.to_string(),
            strategy_name: strategy_id.to_string(),
            timestamp: 0,
            symbol: "BTCUSDT".to_string(),
            market_structure: MarketStructure {
                structure_type: MarketStructureType::TrendingUp,
                confidence: 80.0,
                description: "上升趋势".to_string(),
            },
            key_levels: KeyLevels {
                support: vec![65000.0],
                resistance: vec![68000.0],
                pivot: Some(66500.0),
            },
            bias: TradeBias {
                direction,
                confidence,
                reasoning: format!("{} 策略信号", strategy_id),
            },
            trade_setup: Some(TradeSetup {
                entry_zone: (65500.0, 66000.0),
                stop_loss: 64000.0,
                take_profit: vec![68000.0, 70000.0],
                risk_reward: 2.0,
                entry_conditions: vec![],
                invalidation: "跌破止损".to_string(),
            }),
            annotations: None,
        }
    }

    #[test]
    fn test_no_analyses() {
        let engine = DecisionEngine::with_defaults();
        let result = engine.decide(&[]);
        assert!(!result.should_trade);
    }

    #[test]
    fn test_low_confidence() {
        let engine = DecisionEngine::with_defaults();
        let analyses = vec![create_test_analysis(
            "rsi",
            TradeDirection::Long,
            50.0, // 低于阈值 70%
        )];
        let result = engine.decide(&analyses);
        assert!(!result.should_trade);
    }

    #[test]
    fn test_single_strategy_insufficient_consensus() {
        let config = DecisionConfig {
            min_consensus_count: 2,
            ..Default::default()
        };
        let engine = DecisionEngine::new(config);
        let analyses = vec![create_test_analysis(
            "rsi",
            TradeDirection::Long,
            80.0,
        )];
        let result = engine.decide(&analyses);
        assert!(!result.should_trade); // 需要至少2个策略共识
    }

    #[test]
    fn test_two_strategies_consensus() {
        let engine = DecisionEngine::with_defaults();
        let analyses = vec![
            create_test_analysis("rsi", TradeDirection::Long, 80.0),
            create_test_analysis("macd", TradeDirection::Long, 75.0),
        ];
        let result = engine.decide(&analyses);
        assert!(result.should_trade);
        assert_eq!(result.direction, TradeDirection::Long);
        assert_eq!(result.consensus_strategies.len(), 2);
    }

    #[test]
    fn test_mixed_signals_no_consensus() {
        let engine = DecisionEngine::with_defaults();
        let analyses = vec![
            create_test_analysis("rsi", TradeDirection::Long, 80.0),
            create_test_analysis("macd", TradeDirection::Short, 75.0),
        ];
        let result = engine.decide(&analyses);
        assert!(!result.should_trade); // 方向不一致
    }
}
