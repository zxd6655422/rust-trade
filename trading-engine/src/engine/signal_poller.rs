// engine/signal_poller.rs
//
// 交易引擎主循环（唯一入口）
//
// 职责：
// 1. 从 strategy_signals 表轮询待执行信号 → 广播到所有 TradingUnit 执行
// 2. 定期检查止损止盈（每个 TradingUnit 独立检查）
// 3. 定期同步持仓（每个 TradingUnit 独立同步到 RiskEngine）
// 4. 定期检查持仓风控（RiskEngine 聚合所有 TradingUnit 持仓）
// 5. 定期清理过期信号
//
// 多交易所多模式：每个 TradingUnit 独立运行，信号广播到所有已启用的 unit

use std::sync::Arc;
use std::time::Duration;
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use sqlx::{PgPool, Row};
use tokio::time::interval;
use tracing::{debug, error, info, warn};
use uuid::Uuid;

use trading_common::backtest::strategy::{Signal, SignalIntent};

use crate::engine::trading_unit::TradingUnit;
use crate::risk::RiskEngine;

/// 信号记录（从 strategy_signals 表读取）
#[derive(Debug, Clone)]
struct SignalRecord {
    pub id: Uuid,
    pub symbol: String,
    pub strategy_id: String,
    pub direction: String,
    pub entry_price: Decimal,
    pub overall_confidence: Decimal,
    pub entry_allowed: bool,
    pub status: String,
    pub created_at: DateTime<Utc>,
    /// 目标市场类型: "futures", "spot", "both" (默认 "futures")
    pub market_type: Option<String>,
    /// 信号类型: "entry", "exit", "reverse" (默认 "entry")
    pub signal_type: Option<String>,
    /// 以下字段用于全链路日志追踪
    pub signal_strength: Option<Decimal>,
    pub stop_loss: Option<Decimal>,
    pub take_profit: Option<Decimal>,
    pub timeframe_details: serde_json::Value,
    pub market_context: Option<serde_json::Value>,
}

/// 信号轮询器配置
pub struct SignalPollerConfig {
    /// 信号轮询间隔（毫秒）
    pub poll_interval_ms: u64,
    /// 每次最多获取的信号数
    pub batch_size: i64,
    /// 信号过期时间（小时）
    pub signal_expire_hours: i64,
    /// 默认下单数量（当信号没有指定数量时）
    pub default_quantity: Decimal,
    /// 止损止盈检查间隔（秒）
    pub stop_check_interval_secs: u64,
    /// 持仓同步间隔（秒）
    pub position_sync_interval_secs: u64,
    /// 持仓风控检查间隔（秒）
    pub risk_check_interval_secs: u64,
}

impl Default for SignalPollerConfig {
    fn default() -> Self {
        Self {
            poll_interval_ms: 5000,
            batch_size: 10,
            signal_expire_hours: 1,
            default_quantity: Decimal::from(100),
            stop_check_interval_secs: 5,
            position_sync_interval_secs: 300,  // 5 分钟
            risk_check_interval_secs: 30,      // 30 秒
        }
    }
}

/// 交易引擎主循环
///
/// 支持多交易所多模式：
/// - 每个 TradingUnit 是一个独立的 交易所+模式 交易实例
/// - 信号广播到所有已启用的 TradingUnit
/// - 每个 TradingUnit 独立执行止损止盈、持仓同步
/// - RiskEngine 聚合所有 TradingUnit 持仓进行统一风控
pub struct SignalPoller {
    pool: PgPool,
    risk_engine: Arc<RiskEngine>,
    trading_units: Vec<Arc<TradingUnit>>,
    config: SignalPollerConfig,
}

impl SignalPoller {
    pub fn new(
        pool: PgPool,
        risk_engine: Arc<RiskEngine>,
        trading_units: Vec<Arc<TradingUnit>>,
        config: SignalPollerConfig,
    ) -> Self {
        Self {
            pool,
            risk_engine,
            trading_units,
            config,
        }
    }

    /// 启动主循环（交易引擎唯一入口）
    ///
    /// 每个定时任务独立 spawn，互不阻塞
    pub async fn start(self: Arc<Self>) {
        let enabled_units: Vec<_> = self.trading_units.iter()
            .filter(|u| u.enabled)
            .collect();

        info!("=== Trading Engine Started ===");
        info!("Trading units: {}", enabled_units.len());
        for unit in &enabled_units {
            info!("  - {} ({} {}, leverage={}x)",
                unit.id, unit.exchange_id, unit.market_type, unit.leverage);
        }
        info!("Signal poll interval: {}ms", self.config.poll_interval_ms);
        info!("Stop check interval: {}s", self.config.stop_check_interval_secs);
        info!("Risk check interval: {}s", self.config.risk_check_interval_secs);
        info!("Position sync interval: {}s", self.config.position_sync_interval_secs);

        // 初始同步所有 TradingUnit 的持仓
        for unit in &enabled_units {
            if let Err(e) = unit.portfolio_manager.sync_positions().await {
                warn!("Initial sync failed for {}: {}", unit.id, e);
            }
        }

        // 任务1: 信号轮询执行（广播到所有 TradingUnit）
        let s = self.clone();
        tokio::spawn(async move {
            let mut interval = interval(Duration::from_millis(s.config.poll_interval_ms));
            loop {
                interval.tick().await;
                if let Err(e) = s.poll_and_execute().await {
                    error!("Signal poll error: {}", e);
                }
            }
        });

        // 任务2: 止损止盈检查（每个 TradingUnit 独立检查）
        let s = self.clone();
        tokio::spawn(async move {
            let mut interval = interval(Duration::from_secs(s.config.stop_check_interval_secs));
            loop {
                interval.tick().await;
                s.check_all_stop_orders().await;
            }
        });

        // 任务3: 持仓风控检查（RiskEngine 聚合所有 unit）
        let s = self.clone();
        tokio::spawn(async move {
            let mut interval = interval(Duration::from_secs(s.config.risk_check_interval_secs));
            loop {
                interval.tick().await;
                if let Err(e) = s.check_position_risk().await {
                    warn!("Position risk check error: {}", e);
                }
            }
        });

        // 任务4: 持仓同步（每个 TradingUnit 独立同步）
        let s = self.clone();
        tokio::spawn(async move {
            let mut interval = interval(Duration::from_secs(s.config.position_sync_interval_secs));
            loop {
                interval.tick().await;
                for unit in &s.trading_units {
                    if !unit.enabled { continue; }
                    if let Err(e) = unit.portfolio_manager.sync_positions().await {
                        warn!("Position sync failed for {}: {}", unit.id, e);
                    }
                }
            }
        });

        // 任务5: 过期信号清理
        let s = self.clone();
        tokio::spawn(async move {
            let mut interval = interval(Duration::from_secs(3600));
            loop {
                interval.tick().await;
                if let Err(e) = s.expire_old_signals().await {
                    warn!("Failed to expire old signals: {}", e);
                }
            }
        });

        // 任务6: WebSocket tick 订阅（喂数据给风控引擎的黑天鹅检测和 Kelly 仓位）
        // 使用 channel 将 tick 从同步回调转发到异步处理
        let s = self.clone();
        let enabled_units_for_tick: Vec<_> = enabled_units.iter().map(|u| u.exchange.clone()).collect();
        let risk_engine_for_tick = self.risk_engine.clone();
        tokio::spawn(async move {
            // 每 5 分钟刷新一次交易对列表
            let mut refresh_interval = interval(Duration::from_secs(300));
            let mut last_symbols: Vec<String> = vec![];

            loop {
                refresh_interval.tick().await;

                // 查询当前活跃交易对
                let symbols = match s.get_active_symbols().await {
                    Ok(syms) => syms,
                    Err(e) => {
                        warn!("Failed to get active symbols for tick subscription: {}", e);
                        continue;
                    }
                };

                if symbols.is_empty() {
                    debug!("No active symbols for tick subscription, skipping");
                    continue;
                }

                // 交易对没变化则跳过重连
                if symbols == last_symbols {
                    continue;
                }
                last_symbols = symbols.clone();

                info!("Subscribing to tick data for: {:?}", symbols);

                // 为每个交易所订阅 tick 数据
                for exchange in &enabled_units_for_tick {
                    let symbols = symbols.clone();
                    let risk_engine = risk_engine_for_tick.clone();
                    let exchange_clone = exchange.clone();

                    // 创建 channel：回调(同步) → 处理任务(异步)
                    let (tx, mut rx) = tokio::sync::mpsc::channel::<trading_common::data::types::TickData>(10000);

                    // 接收端：异步调用 update_market_data
                    let risk_engine_recv = risk_engine.clone();
                    tokio::spawn(async move {
                        while let Some(tick) = rx.recv().await {
                            risk_engine_recv.update_market_data(&tick).await;
                        }
                    });

                    // 订阅端：WebSocket 回调通过 channel 发送
                    let (shutdown_tx, shutdown_rx) = tokio::sync::broadcast::channel::<()>(1);
                    let callback = Box::new(move |tick: trading_common::data::types::TickData| {
                        let _ = tx.try_send(tick);
                    });

                    // subscribe_trades 是长阻塞调用，在独立任务中运行
                    tokio::spawn(async move {
                        if let Err(e) = exchange_clone.subscribe_trades(&symbols, callback, shutdown_rx).await {
                            warn!("Tick WebSocket disconnected: {}, will retry in 5 min", e);
                        }
                    });

                    // 保存 shutdown_tx 以便下次刷新时关闭旧连接
                    // （5 分钟后 refresh_interval 触发时会自然重连）
                    let _ = shutdown_tx;
                }
            }
        });

        info!("All 6 tasks spawned, running in parallel");

        // 保持主任务存活
        loop {
            tokio::time::sleep(Duration::from_secs(3600)).await;
        }
    }

    // ============================================================
    // 信号执行（广播到所有 TradingUnit）
    // ============================================================

    /// 轮询并执行信号
    async fn poll_and_execute(&self) -> Result<(), String> {
        let signals = self.get_pending_signals().await?;

        if signals.is_empty() {
            return Ok(());
        }

        debug!("Found {} pending signals", signals.len());

        for record in signals {
            let signal = self.convert_signal(&record);
            let signal_id = record.id;

            info!(
                "Broadcasting signal: {} {} @ {} (confidence: {}, strategy: {})",
                record.direction, record.symbol, record.entry_price,
                record.overall_confidence, record.strategy_id
            );

            // 获取信号的目标市场类型（默认 "futures"）
            let signal_market_type = record.market_type.as_deref().unwrap_or("futures");

            // 广播到所有已启用的 TradingUnit
            let mut any_success = false;
            let mut last_error = String::new();

            for unit in &self.trading_units {
                if !unit.enabled { continue; }

                let unit_id = unit.id.clone();

                // 检查信号是否匹配当前 TradingUnit 的市场类型
                let should_execute = match signal_market_type {
                    "both" => true,  // "both" 表示所有市场都执行
                    "futures" => unit.market_type == "futures",
                    "spot" => unit.market_type == "spot",
                    _ => {
                        warn!("[{}] Unknown signal market_type: {}, skipping", unit_id, signal_market_type);
                        false
                    }
                };

                if !should_execute {
                    debug!(
                        "[{}] Signal market_type={} doesn't match unit market_type={}, skipping",
                        unit_id, signal_market_type, unit.market_type
                    );
                    continue;
                }

                // 检查是否已有同方向持仓（避免重复开仓）
                if let Err(e) = self.check_existing_position(unit, &record.symbol, &record.direction).await {
                    warn!("[{}] Position check failed: {}", unit_id, e);
                    // 不阻断，继续执行
                } else {
                    // 如果已有同方向持仓，跳过
                    if self.has_same_direction_position(unit, &record.symbol, &record.direction).await {
                        info!(
                            "[{}] Already have {} position for {}, skipping signal",
                            unit_id, record.direction, record.symbol
                        );
                        any_success = true; // 标记为成功（已处理）
                        continue;
                    }
                }

                match unit.order_manager.execute_signal(signal.clone(), Some(signal_id)).await {
                    Ok(result) => {
                        info!(
                            "[{}] Signal executed: {} -> order_id={}",
                            unit_id, signal_id, result.order_id
                        );
                        any_success = true;
                    }
                    Err(e) => {
                        let error_msg = e.to_string();
                        // 区分"无持仓"错误和其他错误
                        if error_msg.contains("InsufficientPosition") {
                            // 无持仓错误：可能是手动平仓或止损单触发
                            // 标记为成功（已处理），避免重复尝试
                            info!(
                                "[{}] No position for {} (may have been closed manually or by stop loss), marking as handled",
                                unit_id, record.symbol
                            );
                            any_success = true;
                        } else {
                            warn!(
                                "[{}] Signal execution failed: {} - {}",
                                unit_id, signal_id, error_msg
                            );
                            last_error = error_msg;
                        }
                    }
                }
            }

            // 根据执行结果标记信号状态
            if any_success {
                self.mark_signal_executed(signal_id, "broadcast").await;
            } else {
                // 所有 unit 都失败，标记为失败
                warn!("All trading units failed for signal {}, marking as failed", signal_id);
                self.mark_signal_failed(signal_id, &last_error).await;
            }
        }

        Ok(())
    }

    /// 检查是否已有同方向持仓
    async fn check_existing_position(
        &self,
        unit: &TradingUnit,
        symbol: &str,
        direction: &str,
    ) -> Result<(), String> {
        if unit.market_type == "futures" {
            // 合约模式：查询合约持仓
            let position = unit.exchange.get_position(symbol).await
                .map_err(|e| format!("Failed to get position: {}", e))?;

            let has_position = match direction.to_lowercase().as_str() {
                "bullish" | "buy" => position.quantity > Decimal::ZERO && position.side == crate::exchange::types::PositionSide::Long,
                "bearish" | "sell" => position.quantity > Decimal::ZERO && position.side == crate::exchange::types::PositionSide::Short,
                _ => false,
            };

            if has_position {
                info!(
                    "[{}] Existing {} position for {}: qty={}",
                    unit.id, direction, symbol, position.quantity
                );
            }
        }
        // 现货模式：不检查持仓（直接查询余额）
        Ok(())
    }

    /// 检查是否已有同方向持仓（返回布尔值）
    async fn has_same_direction_position(
        &self,
        unit: &TradingUnit,
        symbol: &str,
        direction: &str,
    ) -> bool {
        if unit.market_type == "futures" {
            if let Ok(position) = unit.exchange.get_position(symbol).await {
                return match direction.to_lowercase().as_str() {
                    "bullish" | "buy" => position.quantity > Decimal::ZERO && position.side == crate::exchange::types::PositionSide::Long,
                    "bearish" | "sell" => position.quantity > Decimal::ZERO && position.side == crate::exchange::types::PositionSide::Short,
                    _ => false,
                };
            }
        }
        false
    }

    /// 获取待执行信号
    async fn get_pending_signals(&self) -> Result<Vec<SignalRecord>, String> {
        let rows = sqlx::query(
            "SELECT id, symbol, strategy_id, direction, entry_price, \
                    overall_confidence, entry_allowed, status, created_at, \
                    market_type, signal_type, signal_strength, stop_loss, take_profit, \
                    timeframe_details, market_context \
             FROM strategy_signals \
             WHERE status='pending' AND entry_allowed=true \
             ORDER BY created_at DESC LIMIT $1"
        )
        .bind(self.config.batch_size)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| format!("Failed to query signals: {}", e))?;

        let records: Vec<SignalRecord> = rows.iter().map(|r| SignalRecord {
            id: r.get::<Uuid, _>("id"),
            symbol: r.get::<String, _>("symbol"),
            strategy_id: r.get::<String, _>("strategy_id"),
            direction: r.get::<String, _>("direction"),
            entry_price: r.get::<Decimal, _>("entry_price"),
            overall_confidence: r.get::<Decimal, _>("overall_confidence"),
            entry_allowed: r.get::<bool, _>("entry_allowed"),
            status: r.get::<String, _>("status"),
            created_at: r.get::<DateTime<Utc>, _>("created_at"),
            market_type: r.try_get::<String, _>("market_type").ok(),
            signal_type: r.try_get::<String, _>("signal_type").ok(),
            signal_strength: r.try_get::<Decimal, _>("signal_strength").ok(),
            stop_loss: r.try_get::<Decimal, _>("stop_loss").ok(),
            take_profit: r.try_get::<Decimal, _>("take_profit").ok(),
            timeframe_details: r.try_get::<serde_json::Value, _>("timeframe_details")
                .unwrap_or(serde_json::json!({})),
            market_context: r.try_get::<serde_json::Value, _>("market_context").ok(),
        }).collect();

        Ok(records)
    }

    /// 标记信号为已执行
    async fn mark_signal_executed(&self, signal_id: Uuid, order_id: &str) {
        if let Err(e) = sqlx::query(
            "UPDATE strategy_signals SET status='executed', closed_reason=$2 \
             WHERE id=$1 AND status='pending'"
        )
        .bind(signal_id)
        .bind(order_id)
        .execute(&self.pool)
        .await
        {
            warn!("Failed to mark signal as executed: {}", e);
        }
    }

    /// 标记信号为已拒绝
    async fn mark_signal_rejected(&self, signal_id: Uuid, reason: &str) {
        // 截断错误信息，closed_reason 字段限制 500 字符
        let truncated_reason = if reason.len() > 495 {
            format!("{}...", &reason[..495])
        } else {
            reason.to_string()
        };

        if let Err(e) = sqlx::query(
            "UPDATE strategy_signals SET status='rejected', closed_reason=$2 \
             WHERE id=$1 AND status='pending'"
        )
        .bind(signal_id)
        .bind(&truncated_reason)
        .execute(&self.pool)
        .await
        {
            warn!("Failed to mark signal as rejected: {}", e);
        }
    }

    /// 标记信号为失败
    async fn mark_signal_failed(&self, signal_id: Uuid, reason: &str) {
        // 截断错误信息，closed_reason 字段限制 500 字符
        let truncated_reason = if reason.len() > 495 {
            format!("{}...", &reason[..495])
        } else {
            reason.to_string()
        };

        if let Err(e) = sqlx::query(
            "UPDATE strategy_signals SET status='failed', closed_reason=$2 \
             WHERE id=$1 AND status='pending'"
        )
        .bind(signal_id)
        .bind(&truncated_reason)
        .execute(&self.pool)
        .await
        {
            warn!("Failed to mark signal as failed: {}", e);
        }
    }

    /// 将数据库信号转换为交易引擎的 Signal 格式
    ///
    /// quantity 传 0，由 OrderManager 根据账户权益动态计算
    /// stop_loss 从策略信号表读取，由策略计算（MA288止损 或 hard_stop_pct）
    fn convert_signal(&self, record: &SignalRecord) -> Signal {
        let direction = record.direction.to_lowercase();
        let entry_price = record.entry_price;
        let symbol = record.symbol.clone();
        // quantity = 0 表示由 OrderManager 动态计算仓位
        let quantity = Decimal::ZERO;

        // 从数据库读取策略计算的止损价
        let stop_loss = record.stop_loss;

        // 从数据库读取信号意图，默认为 Entry
        let intent = record.signal_type.as_deref()
            .map(|t| match t {
                "exit" | "close" | "stop_loss" | "take_profit" => SignalIntent::Exit,
                "reverse" => SignalIntent::Reverse,
                _ => SignalIntent::Entry,
            })
            .unwrap_or(SignalIntent::Entry);

        if direction == "bullish" || direction == "buy" {
            Signal::Buy {
                symbol,
                quantity,
                entry_price,
                intent,
                stop_loss,
            }
        } else if direction == "bearish" || direction == "sell" {
            Signal::Sell {
                symbol,
                quantity,
                entry_price,
                intent,
                stop_loss,
            }
        } else {
            warn!("Unknown signal direction: {}, treating as Hold", direction);
            Signal::Hold
        }
    }

    // ============================================================
    // 止损止盈检查（每个 TradingUnit 独立）
    // ============================================================

    /// 检查所有 TradingUnit 的止损止盈
    async fn check_all_stop_orders(&self) {
        for unit in &self.trading_units {
            if !unit.enabled { continue; }

            let active_stops = unit.stop_loss_manager.get_active_stop_orders().await;
            if active_stops.is_empty() {
                continue;
            }

            for stop_order in active_stops {
                match unit.exchange.get_ticker(&stop_order.symbol).await {
                    Ok(ticker) => {
                        let current_price = ticker.last_price;

                        if let Some(action) = unit.stop_loss_manager.check_price(
                            &stop_order.symbol,
                            current_price,
                        ).await {
                            warn!(
                                "[{}] Stop triggered for {}: {:?} at {}",
                                unit.id, stop_order.symbol, action, current_price
                            );

                            match unit.order_manager.execute_stop_action(action).await {
                                Ok(result) => {
                                    info!("[{}] Stop order executed: {}", unit.id, result.order_id);
                                }
                                Err(e) => {
                                    error!("[{}] Failed to execute stop for {}: {}",
                                        unit.id, stop_order.symbol, e);
                                }
                            }
                        }
                    }
                    Err(e) => {
                        warn!(
                            "[{}] Failed to get ticker for {} (stop check skipped): {}",
                            unit.id, stop_order.symbol, e
                        );
                    }
                }
            }
        }
    }

    // ============================================================
    // 持仓风控检查（RiskEngine 聚合）
    // ============================================================

    /// 检查持仓风控（每个交易所独立计算）
    async fn check_position_risk(&self) -> Result<(), String> {
        for unit in &self.trading_units {
            if !unit.enabled { continue; }

            // 1. 从交易所获取账户信息
            let account = match unit.exchange.get_account().await {
                Ok(acc) => acc,
                Err(e) => {
                    warn!("[{}] Failed to get account: {}", unit.id, e);
                    continue;
                }
            };

            // 2. 同步账户余额到风控引擎
            self.risk_engine.sync_account_balance(&account).await;

            // 3. 从交易所同步已实现盈亏（替代简化计算）
            self.risk_engine.sync_realized_pnl(
                unit.exchange.as_ref(),
                &unit.id,
            ).await;

            // 4. 执行持仓风控检查
            let actions = self.risk_engine.check_positions(&account).await;

            if actions.is_empty() {
                continue;
            }

            warn!("[{}] ⚠️ Position risk check triggered {} actions", unit.id, actions.len());

            // 5. 执行风控动作
            for action in actions {
                if let Err(e) = unit.order_manager.execute_risk_action(action).await {
                    error!("[{}] Failed to execute risk action: {}", unit.id, e);
                }
            }
        }

        Ok(())
    }

    // ============================================================
    // 信号过期
    // ============================================================

    /// 清理过期信号
    async fn expire_old_signals(&self) -> Result<(), String> {
        let result = sqlx::query(
            "UPDATE strategy_signals SET status='expired', closed_reason='expired', \
             closed_at=NOW(), close_price=entry_price, actual_return_pct=0 \
             WHERE status='pending' AND created_at < NOW() - INTERVAL '1 hour' * $1"
        )
        .bind(self.config.signal_expire_hours)
        .execute(&self.pool)
        .await
        .map_err(|e| format!("Failed to expire signals: {}", e))?;

        let count = result.rows_affected();
        if count > 0 {
            info!("Expired {} old signals", count);
        }

        Ok(())
    }

    // ============================================================
    // 辅助方法
    // ============================================================

    /// 查询当前活跃交易对（有持仓或有待执行信号的交易对）
    ///
    /// 用于 WebSocket tick 订阅，喂数据给风控引擎的黑天鹅检测和 Kelly 仓位计算
    async fn get_active_symbols(&self) -> Result<Vec<String>, String> {
        let rows = sqlx::query_scalar::<_, String>(
            r#"
            SELECT DISTINCT symbol FROM (
                -- 有持仓的交易对
                SELECT symbol FROM trading_positions WHERE quantity != 0
                UNION
                -- 有 pending 信号的交易对
                SELECT symbol FROM strategy_signals WHERE status = 'pending'
            ) AS active
            ORDER BY symbol
            "#,
        )
        .fetch_all(&self.pool)
        .await
        .map_err(|e| format!("Failed to query active symbols: {}", e))?;

        Ok(rows)
    }
}
