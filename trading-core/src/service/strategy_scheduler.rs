// service/strategy_scheduler.rs
// 策略分析定时任务 — 自动为每个交易对运行策略分析并保存结果

use std::sync::Arc;
use tokio::sync::broadcast;
use tokio::time::{interval, Duration};
use tracing::{debug, error, info, warn};
use rust_decimal::Decimal;
use std::str::FromStr;

use trading_common::backtest::strategy::{
    create_multi_timeframe_strategy, get_strategy_info,
    EntryDirection, MultiTimeframeStrategy, TrendDirection,
};
use trading_common::data::aggregator::KlineAggregator;
use trading_common::data::repository::TickDataRepository;
use trading_common::data::types::Timeframe;

/// 策略分析调度器配置
#[derive(Debug, Clone)]
pub struct StrategySchedulerConfig {
    /// 分析间隔（秒），默认 300（5分钟）
    pub interval_secs: u64,
    /// 策略 ID，默认 "trend"
    pub strategy_id: String,
    /// 信号过期时间（小时），默认 24
    pub signal_max_age_hours: i64,
    /// 确认阈值（收益率%），默认 0.5
    pub confirm_threshold_pct: Decimal,
    /// 止损阈值（收益率%），默认 -2.0
    pub stop_loss_pct: Decimal,
    /// 止盈阈值（收益率%），默认 3.0
    pub take_profit_pct: Decimal,
}

impl Default for StrategySchedulerConfig {
    fn default() -> Self {
        Self {
            interval_secs: 300,
            strategy_id: "trend".to_string(),
            signal_max_age_hours: 24,
            confirm_threshold_pct: Decimal::from_str("0.5").unwrap(),
            stop_loss_pct: Decimal::from_str("-2.0").unwrap(),
            take_profit_pct: Decimal::from_str("3.0").unwrap(),
        }
    }
}

/// 策略分析调度器
pub struct StrategyAnalysisScheduler {
    repository: Arc<TickDataRepository>,
    config: StrategySchedulerConfig,
    shutdown_rx: broadcast::Receiver<()>,
}

impl StrategyAnalysisScheduler {
    pub fn new(
        repository: Arc<TickDataRepository>,
        config: StrategySchedulerConfig,
        shutdown_rx: broadcast::Receiver<()>,
    ) -> Self {
        Self { repository, config, shutdown_rx }
    }

    /// 启动定时任务
    pub async fn start(mut self) {
        info!(
            "Strategy scheduler started: strategy={}, interval={}s",
            self.config.strategy_id, self.config.interval_secs
        );

        let mut tick = interval(Duration::from_secs(self.config.interval_secs));

        // 首次启动立即执行一次（检查数据库暂停状态）
        match self.repository.is_scheduler_paused().await {
            Ok(true) => info!("Scheduler is paused (from DB), skipping initial cycle"),
            Ok(false) => self.run_analysis_cycle().await,
            Err(e) => warn!("Failed to check scheduler pause state: {}", e),
        }

        loop {
            tokio::select! {
                _ = tick.tick() => {
                    // 从数据库检查是否暂停
                    match self.repository.is_scheduler_paused().await {
                        Ok(true) => {
                            debug!("Scheduler is paused, skipping cycle");
                            continue;
                        }
                        Ok(false) => {}
                        Err(e) => {
                            warn!("Failed to check scheduler pause state: {}", e);
                            // 出错时继续执行，避免停止交易
                        }
                    }
                    self.run_analysis_cycle().await;
                }
                _ = self.shutdown_rx.recv() => {
                    info!("Strategy scheduler shutting down");
                    break;
                }
            }
        }
    }

    /// 执行一轮分析（遍历所有启用的交易对）
    async fn run_analysis_cycle(&self) {
        // 1. 获取启用的交易对
        let symbols = match self.repository.get_enabled_symbols().await {
            Ok(s) => s,
            Err(e) => {
                error!("Failed to get enabled symbols: {}", e);
                return;
            }
        };

        if symbols.is_empty() {
            warn!("No enabled symbols, skipping analysis cycle");
            return;
        }

        info!("Analysis cycle: {} symbols, strategy={}", symbols.len(), self.config.strategy_id);

        // 2. 清理过期信号
        if let Err(e) = self.repository.close_expired_analysis(self.config.signal_max_age_hours).await {
            warn!("Failed to close expired analysis: {}", e);
        }

        // 3. 逐个交易对分析
        for symbol in &symbols {
            if let Err(e) = self.analyze_symbol(symbol).await {
                warn!("Failed to analyze {}: {}", symbol, e);
            }
        }
    }

    /// 分析单个交易对
    async fn analyze_symbol(&self, symbol: &str) -> Result<(), String> {
        // 1. 获取 1m K线（2000根 ≈ 33小时）
        let klines_1m = self.repository.get_klines(symbol, 2000).await
            .map_err(|e| format!("get_klines: {}", e))?;

        if klines_1m.is_empty() {
            return Err("No kline data".to_string());
        }

        let current_price = klines_1m.last().unwrap().close;

        // 2. 聚合多时间框架
        let mut aggregator = KlineAggregator::new();
        for kline in &klines_1m {
            aggregator.update(kline.clone());
        }

        // 3. 策略分析
        let mut strategy = create_multi_timeframe_strategy(&self.config.strategy_id)
            .map_err(|e| format!("create_strategy: {}", e))?;

        let mut tf_klines = std::collections::HashMap::new();
        for tf in strategy.required_timeframes() {
            tf_klines.insert(tf, aggregator.get_klines(tf, 200));
        }

        let analysis = strategy.analyze(&tf_klines);

        let dir_str = match analysis.overall_direction {
            TrendDirection::Bullish => "bullish",
            TrendDirection::Bearish => "bearish",
            _ => "neutral",
        };
        let entry_dir = analysis.entry_direction.map(|d| match d {
            EntryDirection::Long => "long",
            EntryDirection::Short => "short",
        });

        // 4. 构建 timeframe_details JSON
        let tf_json = build_timeframe_json(&analysis, &aggregator);

        // 5. 生命周期闭环
        let mut need_save = true;
        if let Some(prev) = self.repository.get_pending_analysis(symbol, &self.config.strategy_id).await
            .map_err(|e| format!("get_pending: {}", e))?
        {
            let is_same_dir = prev.direction == dir_str;
            let age_hours = (chrono::Utc::now() - prev.created_at).num_hours();
            let return_pct = calc_return_pct(&prev.direction, prev.entry_price, current_price);

            // 5.1 止损检查
            if return_pct <= self.config.stop_loss_pct {
                let _ = self.repository.close_analysis(
                    prev.id, "invalidated", "stop_loss", current_price, return_pct
                ).await;
                info!("[{}] Stop loss triggered {} (return={}%)", symbol, prev.id, return_pct);
                need_save = true;
            }
            // 5.2 止盈检查
            else if return_pct >= self.config.take_profit_pct {
                let _ = self.repository.close_analysis(
                    prev.id, "confirmed", "take_profit", current_price, return_pct
                ).await;
                info!("[{}] Take profit triggered {} (return={}%)", symbol, prev.id, return_pct);
                need_save = true;
            }
            // 5.3 同方向+未过期 → 更新验证
            else if is_same_dir && age_hours <= self.config.signal_max_age_hours {
                let _ = self.repository.update_analysis_eval(prev.id, current_price).await;
                let confirmed = match prev.direction.as_str() {
                    "bullish" => return_pct > self.config.confirm_threshold_pct,
                    "bearish" => return_pct < -self.config.confirm_threshold_pct,
                    _ => false,
                };
                if confirmed {
                    let _ = self.repository.close_analysis(
                        prev.id, "confirmed", "price_confirmed", current_price, return_pct
                    ).await;
                    info!("[{}] Confirmed analysis {} (return={}%)", symbol, prev.id, return_pct);
                    need_save = true;
                } else {
                    need_save = false;
                }
            }
            // 5.4 方向反转
            else if !is_same_dir {
                let _ = self.repository.close_analysis(
                    prev.id, "superseded", "direction_changed", current_price, return_pct
                ).await;
                info!("[{}] Superseded analysis {} ({} -> {})", symbol, prev.id, prev.direction, dir_str);
            }
            // 5.5 超时
            else {
                let _ = self.repository.close_analysis(
                    prev.id, "expired", "timeout", current_price, return_pct
                ).await;
                info!("[{}] Expired analysis {} ({}h)", symbol, prev.id, age_hours);
            }
        }

        // 6. 保存新分析
        if need_save {
            let _ = self.repository.save_analysis_log(
                symbol, &self.config.strategy_id, dir_str, current_price,
                analysis.overall_confidence, analysis.entry_allowed,
                entry_dir, tf_json,
            ).await.map_err(|e| format!("save_analysis: {}", e))?;
        }

        Ok(())
    }
}

/// 构建 timeframe_details JSON
fn build_timeframe_json(
    analysis: &trading_common::backtest::strategy::MultiTimeframeAnalysis,
    _aggregator: &KlineAggregator,
) -> serde_json::Value {
    let mut map = serde_json::Map::new();
    let tf_labels = [
        (Timeframe::FourHours, "4h"),
        (Timeframe::OneHour, "1h"),
        (Timeframe::FifteenMinutes, "15m"),
    ];
    for (tf, label) in &tf_labels {
        if let Some(ta) = analysis.timeframe_analyses.get(tf) {
            map.insert(label.to_string(), serde_json::json!({
                "direction": format!("{:?}", ta.direction).to_lowercase(),
                "confidence": ta.confidence.to_string(),
                "description": ta.description
            }));
        }
    }
    serde_json::Value::Object(map)
}

/// 计算收益率%
fn calc_return_pct(direction: &str, entry_price: Decimal, current_price: Decimal) -> Decimal {
    if entry_price == Decimal::ZERO { return Decimal::ZERO; }
    let pct = (current_price - entry_price) / entry_price * Decimal::from(100);
    match direction {
        "bullish" => pct,
        "bearish" => -pct,
        _ => Decimal::ZERO,
    }
}
