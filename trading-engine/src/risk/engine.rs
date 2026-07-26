// risk/engine.rs
// 风控引擎实现

use chrono::{DateTime, NaiveDate, Timelike, Utc};
use rust_decimal::Decimal;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{Mutex, RwLock};
use tracing::{debug, info, warn};

use super::config::RiskConfig;
use crate::exchange::traits::Exchange;
use crate::exchange::types::{AccountInfo, IncomeRecord, OrderRequest};
use trading_common::data::types::TickData;

/// 风控决策（针对新订单）
#[derive(Debug, Clone)]
pub enum RiskDecision {
    /// 允许执行
    Allow,
    /// 拒绝执行，附带原因
    Reject(String),
    /// 允许但修改数量
    Modify(Decimal),
}

/// 持仓风控动作（针对已有持仓）
#[derive(Debug, Clone)]
pub enum RiskAction {
    /// 强制平仓某个持仓
    ForceClose {
        symbol: String,
        quantity: Decimal,
        reason: String,
    },
    /// 减仓（部分平仓）
    ReducePosition {
        symbol: String,
        current_quantity: Decimal,
        target_quantity: Decimal,
        reason: String,
    },
    /// 全部平仓（账户级风控触发）
    CloseAll {
        reason: String,
    },
}

impl RiskDecision {
    /// 是否接受订单
    pub fn is_accepted(&self) -> bool {
        match self {
            RiskDecision::Allow => true,
            RiskDecision::Modify(_) => true,
            RiskDecision::Reject(_) => false,
        }
    }

    /// 获取修改后的数量（如果有）
    pub fn modified_quantity(&self) -> Option<Decimal> {
        match self {
            RiskDecision::Modify(qty) => Some(*qty),
            _ => None,
        }
    }
}

/// 风控状态
#[derive(Debug, Clone)]
pub struct RiskState {
    /// 初始资金（从交易所账户获取）
    pub initial_capital: Decimal,
    /// 日盈亏
    pub daily_pnl: Decimal,
    /// 峰值权益
    pub peak_equity: Decimal,
    /// 当前权益
    pub current_equity: Decimal,
    /// 持仓信息
    pub positions: HashMap<String, PositionSnapshot>,
    /// 最后交易时间
    pub last_trade_time: HashMap<String, DateTime<Utc>>,
    /// 熔断结束时间
    pub circuit_breaker_until: Option<DateTime<Utc>>,
    /// 日交易次数
    pub daily_trade_count: u32,
    /// 最近价格历史 (用于黑天鹅检测)
    pub price_history: HashMap<String, Vec<(DateTime<Utc>, Decimal)>>,
    /// 上次每日重置的日期（用于自动跨天重置 peak_equity 和 daily_pnl）
    pub last_daily_reset_date: Option<NaiveDate>,
}

/// 持仓快照
#[derive(Debug, Clone)]
pub struct PositionSnapshot {
    pub symbol: String,
    pub quantity: Decimal,
    pub avg_entry_price: Decimal,
    pub current_price: Decimal,
    pub unrealized_pnl: Decimal,
}

use crate::storage::RiskConfigRepository;

/// 风控引擎
pub struct RiskEngine {
    config: RwLock<RiskConfig>,
    config_repo: Option<Arc<RiskConfigRepository>>,
    state: Arc<Mutex<RiskState>>,
}

impl RiskEngine {
    /// 获取当前风控配置（从缓存读取，不持有锁）
    pub async fn config(&self) -> RiskConfig {
        self.config.read().await.clone()
    }

    /// 创建新的风控引擎
    ///
    /// - `config_repo`: 数据库风控参数仓储，支持热更新
    /// - `fallback_config`: DB 加载失败时的兜底配置（来自 config.toml）
    pub async fn new(config_repo: Arc<RiskConfigRepository>, _fallback_config: RiskConfig) -> Self {
        let config = config_repo.get_config().await;
        info!("RiskEngine initialized with config from DB");

        let initial_state = RiskState {
            initial_capital: Decimal::ZERO,
            daily_pnl: Decimal::ZERO,
            peak_equity: Decimal::ZERO,
            current_equity: Decimal::ZERO,
            positions: HashMap::new(),
            last_trade_time: HashMap::new(),
            circuit_breaker_until: None,
            daily_trade_count: 0,
            price_history: HashMap::new(),
            last_daily_reset_date: None,
        };

        Self {
            config: RwLock::new(config),
            config_repo: Some(config_repo),
            state: Arc::new(Mutex::new(initial_state)),
        }
    }

    /// 测试用构造函数（不依赖数据库）
    #[cfg(test)]
    pub fn new_for_test(config: RiskConfig) -> Self {
        let initial_state = RiskState {
            initial_capital: Decimal::ZERO,
            daily_pnl: Decimal::ZERO,
            peak_equity: Decimal::ZERO,
            current_equity: Decimal::ZERO,
            positions: HashMap::new(),
            last_trade_time: HashMap::new(),
            circuit_breaker_until: None,
            daily_trade_count: 0,
            price_history: HashMap::new(),
            last_daily_reset_date: None,
        };

        Self {
            config: RwLock::new(config),
            config_repo: None,
            state: Arc::new(Mutex::new(initial_state)),
        }
    }

    /// 从数据库重新加载风控配置（热更新）
    ///
    /// 由 sync_account_balance 每 30s 调用一次
    /// 测试模式下（config_repo = None）跳过
    pub async fn reload_config(&self) {
        let repo = match &self.config_repo {
            Some(repo) => repo,
            None => return, // 测试模式，无 DB
        };
        if let Err(e) = repo.reload().await {
            warn!("Failed to reload risk config from DB: {}", e);
            return;
        }
        let new_config = repo.get_config().await;
        *self.config.write().await = new_config;
        debug!("Risk config reloaded from database");
    }

    /// 设置初始资金（从交易所账户获取）
    ///
    /// capital 应为 account.total_equity（= totalWalletBalance + totalUnrealizedProfit）
    /// 已是完整权益，无需再加 daily_pnl
    pub async fn set_initial_capital(&self, capital: Decimal) {
        let mut state = self.state.lock().await;
        state.initial_capital = capital;
        state.current_equity = capital;
        if state.peak_equity < state.current_equity {
            state.peak_equity = state.current_equity;
        }
        info!("Risk engine initial capital set to: {}", capital);
    }

    /// 同步账户余额到风控状态
    ///
    /// 每次调用时：
    /// 1. 从 DB 热更新风控配置（约 30s 一次）
    /// 2. 自动检查是否跨交易日，跨日则重置 daily_pnl、daily_trade_count、peak_equity
    /// 3. 更新当前权益和峰值权益
    pub async fn sync_account_balance(&self, account: &AccountInfo) {
        // 1. 热更新风控配置
        self.reload_config().await;

        let config = self.config.read().await;
        let reset_hour = config.daily_reset_hour;
        drop(config);

        let mut state = self.state.lock().await;

        // 2. 自动跨交易日重置
        // 交易日起始时刻 = daily_reset_hour (UTC)
        // 例：daily_reset_hour=8 → UTC 8:00 后算新交易日
        let now = Utc::now();
        let effective_date = if now.hour() >= reset_hour {
            now.date_naive()
        } else {
            // 还没到今天的重置时刻，仍属于上一个交易日
            (now - chrono::Duration::days(1)).date_naive()
        };

        let should_reset = match state.last_daily_reset_date {
            Some(last_date) => effective_date > last_date,
            None => true, // 首次调用，初始化日期
        };
        if should_reset {
            info!(
                "Daily reset: effective date changed from {:?} to {} (reset_hour={} UTC)",
                state.last_daily_reset_date, effective_date, reset_hour
            );
            state.daily_pnl = Decimal::ZERO;
            state.daily_trade_count = 0;
            state.peak_equity = Decimal::ZERO;
            state.last_daily_reset_date = Some(effective_date);
        }

        // 3. 更新权益
        // account.total_equity = totalWalletBalance + totalUnrealizedProfit
        // 已经是完整的当前权益，不需要再加 daily_pnl 和 total_unrealized
        state.initial_capital = account.total_equity;
        state.current_equity = account.total_equity;
        if state.peak_equity < state.current_equity {
            state.peak_equity = state.current_equity;
        }
    }

    /// 从 PortfolioManager 同步交易所实时持仓到风控引擎
    ///
    /// 持仓 key 格式为 "exchange_id:symbol"，如 "binance-futures:BTCUSDT"
    /// 不同 TradingUnit 的持仓通过 key 前缀区分
    ///
    /// 此方法在以下时机被调用：
    /// - 启动时初始同步
    /// - 定时同步（5分钟间隔）
    /// - 订单成交后
    pub async fn sync_positions_from_unit(
        &self,
        exchange_id: &str,
        market_type: &str,
        positions: &HashMap<String, crate::portfolio::manager::PositionSnapshot>,
    ) {
        let mut state = self.state.lock().await;

        // 先清除该 unit 的旧持仓
        let prefix = format!("{}:", exchange_id);
        state.positions.retain(|k, _| !k.starts_with(&prefix));

        // 写入新持仓
        for (symbol, pos) in positions {
            state.positions.insert(
                symbol.clone(),  // key 已经是 "exchange_id:symbol" 格式
                PositionSnapshot {
                    symbol: pos.symbol.clone(),
                    quantity: pos.quantity,
                    avg_entry_price: pos.avg_entry_price,
                    current_price: pos.current_price,
                    unrealized_pnl: pos.unrealized_pnl,
                },
            );
        }

        info!(
            "RiskEngine synced {} positions from {} {}",
            positions.len(), exchange_id, market_type
        );
    }

    /// 核心方法：检查订单是否允许执行
    pub async fn check_order(
        &self,
        order: &OrderRequest,
        account: &AccountInfo,
    ) -> Result<RiskDecision, RiskError> {
        let config = self.config.read().await.clone();
        let state = self.state.lock().await;

        // 1. 熔断检查
        if let Some(until) = state.circuit_breaker_until {
            if Utc::now() < until {
                return Ok(RiskDecision::Reject(format!(
                    "Circuit breaker active until {}",
                    until
                )));
            }
        }

        // 2. 日亏损限制
        if state.daily_pnl < -config.max_daily_loss {
            return Ok(RiskDecision::Reject(format!(
                "Daily loss limit reached: {} / {}",
                state.daily_pnl, config.max_daily_loss
            )));
        }

        // 3. 最大回撤检查
        if state.peak_equity > Decimal::ZERO {
            let drawdown = (state.peak_equity - state.current_equity) / state.peak_equity;
            if drawdown > config.max_drawdown_pct {
                return Ok(RiskDecision::Reject(format!(
                    "Max drawdown breached: {}% / {}%",
                    drawdown * Decimal::from(100),
                    config.max_drawdown_pct * Decimal::from(100)
                )));
            }
        }

        // 4. 单笔仓位大小检查（占权益百分比）
        let order_value = order.quantity * order.price.unwrap_or(Decimal::ZERO);
        let max_position_value = account.total_equity * config.max_position_pct;
        if order_value > max_position_value {
            return Ok(RiskDecision::Reject(format!(
                "Order value {} exceeds max position {} ({}% of equity {})",
                order_value, max_position_value, config.max_position_pct * Decimal::from(100), account.total_equity
            )));
        }

        // 5. 总曝光度检查
        let total_exposure: Decimal = state
            .positions
            .values()
            .map(|p| p.quantity * p.current_price)
            .sum();
        let new_exposure = total_exposure + order_value;
        let max_exposure = account.total_equity * config.max_exposure_pct;
        if new_exposure > max_exposure {
            return Ok(RiskDecision::Reject(format!(
                "Total exposure {} would exceed limit {}",
                new_exposure, max_exposure
            )));
        }

        // 6. 黑天鹅检测
        if self.detect_black_swan(&state, &order.symbol, &config) {
            return Ok(RiskDecision::Reject(
                "Black swan detected, trading halted".to_string(),
            ));
        }

        // 7. Kelly 仓位调整
        let adjusted_quantity = self.calculate_kelly_position(order, &state, &config);
        if adjusted_quantity < order.quantity {
            return Ok(RiskDecision::Modify(adjusted_quantity));
        }

        Ok(RiskDecision::Allow)
    }

    /// 检查已有持仓的风控状态
    ///
    /// 定期调用，检查以下规则：
    /// 1. 日亏损限制 → 全部平仓
    /// 2. 最大回撤 → 全部平仓
    /// 3. 总曝光度超限 → 减仓
    /// 4. 单个持仓过大 → 减仓
    ///
    /// 返回需要执行的风控动作列表
    pub async fn check_positions(
        &self,
        account: &AccountInfo,
    ) -> Vec<RiskAction> {
        let config = self.config.read().await.clone();
        let state = self.state.lock().await;
        let mut actions = Vec::new();

        // 1. 日亏损限制 → 全部平仓
        if state.daily_pnl < -config.max_daily_loss {
            warn!(
                "⚠️ Daily loss limit breached: {} / {}, force closing all positions",
                state.daily_pnl, config.max_daily_loss
            );
            actions.push(RiskAction::CloseAll {
                reason: format!(
                    "Daily loss {} exceeds limit {}",
                    state.daily_pnl, config.max_daily_loss
                ),
            });
            return actions; // 优先级最高，直接返回
        }

        // 2. 最大回撤 → 全部平仓
        if state.peak_equity > Decimal::ZERO {
            let drawdown = (state.peak_equity - state.current_equity) / state.peak_equity;
            if drawdown > config.max_drawdown_pct {
                warn!(
                    "⚠️ Max drawdown breached: {}% / {}%, force closing all positions",
                    drawdown * Decimal::from(100),
                    config.max_drawdown_pct * Decimal::from(100)
                );
                actions.push(RiskAction::CloseAll {
                    reason: format!(
                        "Drawdown {}% exceeds limit {}%",
                        drawdown * Decimal::from(100),
                        config.max_drawdown_pct * Decimal::from(100)
                    ),
                });
                return actions;
            }
        }

        // 3. 总曝光度检查 → 减仓
        let total_exposure: Decimal = state
            .positions
            .values()
            .map(|p| p.quantity * p.current_price)
            .sum();
        let max_exposure = account.total_equity * config.max_exposure_pct;

        if total_exposure > max_exposure && !state.positions.is_empty() {
            let excess_ratio = (total_exposure - max_exposure) / total_exposure;
            warn!(
                "⚠️ Total exposure {} exceeds limit {}, reducing by {}%",
                total_exposure,
                max_exposure,
                excess_ratio * Decimal::from(100)
            );

            // 按持仓价值从大到小减仓
            let mut sorted_positions: Vec<_> = state.positions.values().collect();
            sorted_positions.sort_by(|a, b| {
                let val_a = a.quantity * a.current_price;
                let val_b = b.quantity * b.current_price;
                val_b.cmp(&val_a)
            });

            let mut remaining_excess = total_exposure - max_exposure;
            for pos in sorted_positions {
                if remaining_excess <= Decimal::ZERO {
                    break;
                }
                let pos_value = pos.quantity * pos.current_price;
                let reduce_value = pos_value.min(remaining_excess);
                let reduce_qty = if pos.current_price > Decimal::ZERO {
                    reduce_value / pos.current_price
                } else {
                    Decimal::ZERO
                };

                if reduce_qty > Decimal::ZERO && reduce_qty < pos.quantity {
                    let target_qty = pos.quantity - reduce_qty;
                    actions.push(RiskAction::ReducePosition {
                        symbol: pos.symbol.clone(),
                        current_quantity: pos.quantity,
                        target_quantity: target_qty,
                        reason: format!("Exposure reduction: {} USDT", reduce_value),
                    });
                    remaining_excess -= reduce_value;
                } else if reduce_qty >= pos.quantity {
                    actions.push(RiskAction::ForceClose {
                        symbol: pos.symbol.clone(),
                        quantity: pos.quantity,
                        reason: format!("Exposure reduction (full close): {} USDT", pos_value),
                    });
                    remaining_excess -= pos_value;
                }
            }
        }

        // 4. 单个持仓过大 → 减仓（占权益百分比）
        let max_position_value = account.total_equity * config.max_position_pct;
        for pos in state.positions.values() {
            let pos_value = pos.quantity * pos.current_price;
            if pos_value > max_position_value {
                let target_qty = if pos.current_price > Decimal::ZERO {
                    max_position_value / pos.current_price
                } else {
                    Decimal::ZERO
                };
                if target_qty < pos.quantity {
                    warn!(
                        "⚠️ Position {} value {} exceeds max {} ({}% of equity {}), reducing to {}",
                        pos.symbol, pos_value, max_position_value,
                        config.max_position_pct * Decimal::from(100), account.total_equity, target_qty
                    );
                    actions.push(RiskAction::ReducePosition {
                        symbol: pos.symbol.clone(),
                        current_quantity: pos.quantity,
                        target_quantity: target_qty,
                        reason: format!(
                            "Position value {} exceeds max {} ({}% of equity)",
                            pos_value, max_position_value,
                            config.max_position_pct * Decimal::from(100)
                        ),
                    });
                }
            }
        }

        actions
    }

    /// 更新市场数据
    pub async fn update_market_data(&self, tick: &TickData) {
        let mut state = self.state.lock().await;

        // 更新价格历史
        let history = state
            .price_history
            .entry(tick.symbol.clone())
            .or_insert_with(Vec::new);
        history.push((tick.timestamp, tick.price));

        // 保持历史记录在合理范围内
        if history.len() > 1000 {
            history.drain(0..500);
        }

        // 更新持仓的当前价格和未实现盈亏
        if let Some(position) = state.positions.get_mut(&tick.symbol) {
            position.current_price = tick.price;
            position.unrealized_pnl =
                (tick.price - position.avg_entry_price) * position.quantity;
        }

        // 注意：不再在 tick 级别更新 current_equity
        // current_equity 由 sync_account_balance 每30s从交易所同步一次
        // account.total_equity = totalWalletBalance + totalUnrealizedProfit，已是完整权益
        // 避免 daily_pnl 和 total_unrealized 的双重计算
    }

    /// 记录交易结果
    pub async fn record_trade_result(
        &self,
        symbol: &str,
        side: &str,
        quantity: Decimal,
        price: Decimal,
    ) {
        let mut state = self.state.lock().await;

        // 更新日盈亏 (简化计算)
        // 实际应该从成交记录计算
        state.daily_trade_count += 1;
        state
            .last_trade_time
            .insert(symbol.to_string(), Utc::now());

        // 更新持仓
        if side == "BUY" {
            let position = state
                .positions
                .entry(symbol.to_string())
                .or_insert_with(|| PositionSnapshot {
                    symbol: symbol.to_string(),
                    quantity: Decimal::ZERO,
                    avg_entry_price: Decimal::ZERO,
                    current_price: price,
                    unrealized_pnl: Decimal::ZERO,
                });

            let total_cost = position.quantity * position.avg_entry_price + quantity * price;
            position.quantity += quantity;
            if position.quantity > Decimal::ZERO {
                position.avg_entry_price = total_cost / position.quantity;
            }
        } else if side == "SELL" {
            if let Some(position) = state.positions.get_mut(symbol) {
                let realized_pnl = (price - position.avg_entry_price) * quantity.min(position.quantity);
                let new_quantity = position.quantity - quantity;
                position.quantity = new_quantity;
                if position.quantity <= Decimal::ZERO {
                    state.positions.remove(symbol);
                }
                state.daily_pnl += realized_pnl;
            }
        }

        debug!(
            "Trade recorded: {} {} {} @ {} | Daily PnL: {}",
            symbol, side, quantity, price, state.daily_pnl
        );
    }

    /// 获取风控状态
    pub async fn get_status(&self) -> RiskStatus {
        let state = self.state.lock().await;
        RiskStatus {
            daily_pnl: state.daily_pnl,
            peak_equity: state.peak_equity,
            current_equity: state.current_equity,
            daily_trade_count: state.daily_trade_count,
            is_circuit_breaker_active: state
                .circuit_breaker_until
                .map(|until| Utc::now() < until)
                .unwrap_or(false),
            position_count: state.positions.len(),
        }
    }

    /// 手动触发熔断
    pub async fn trigger_circuit_breaker(&self, reason: &str) {
        let config = self.config.read().await;
        let mut state = self.state.lock().await;
        let until = Utc::now() + chrono::Duration::seconds(config.circuit_breaker_cooldown as i64);
        state.circuit_breaker_until = Some(until);
        warn!(
            "Circuit breaker triggered: {} | Will resume at: {}",
            reason, until
        );
    }

    /// 重置日统计（手动调用时使用，自动跨天重置由 sync_account_balance 处理）
    pub async fn reset_daily_stats(&self) {
        let mut state = self.state.lock().await;
        state.daily_pnl = Decimal::ZERO;
        state.daily_trade_count = 0;
        state.peak_equity = Decimal::ZERO;
        state.last_daily_reset_date = Some(Utc::now().date_naive());
        info!("Daily risk stats reset (including peak_equity)");
    }

    /// 从交易所同步已实现盈亏（替代简化计算）
    ///
    /// 调用 exchange.get_income_history() 获取当日 REALIZED_PNL，
    /// 累加得到真实的 daily_pnl
    pub async fn sync_realized_pnl(
        &self,
        exchange: &dyn Exchange,
        exchange_id: &str,
    ) {
        // 获取今日零点时间戳
        let today_start = Utc::now()
            .date_naive()
            .and_hms_opt(0, 0, 0)
            .unwrap()
            .and_utc();

        match exchange.get_income_history(
            None,                    // 所有交易对
            Some("REALIZED_PNL"),    // 只查已实现盈亏
            Some(today_start),
            None,
            Some(1000),
        ).await {
            Ok(records) => {
                let total_realized: Decimal = records.iter().map(|r| r.income).sum();
                let mut state = self.state.lock().await;
                state.daily_pnl = total_realized;
                debug!(
                    "[{}] Synced realized PnL from exchange: {} USDT ({} records)",
                    exchange_id, total_realized, records.len()
                );
            }
            Err(e) => {
                warn!(
                    "[{}] Failed to sync realized PnL: {}, using in-memory calculation",
                    exchange_id, e
                );
            }
        }
    }

    /// 检测黑天鹅事件
    fn detect_black_swan(&self, state: &RiskState, symbol: &str, config: &RiskConfig) -> bool {
        if let Some(history) = state.price_history.get(symbol) {
            if history.len() < 2 {
                return false;
            }

            // 检查最近 N 个 tick 的价格波动
            let recent_count = 10.min(history.len());
            let recent_prices: Vec<Decimal> = history
                .iter()
                .rev()
                .take(recent_count)
                .map(|(_, price)| *price)
                .collect();

            if recent_prices.len() < 2 {
                return false;
            }

            // 计算最大价格变动
            let min_price = recent_prices.iter().cloned().min().unwrap_or(Decimal::ZERO);
            let max_price = recent_prices.iter().cloned().max().unwrap_or(Decimal::ZERO);

            if min_price > Decimal::ZERO {
                let change = (max_price - min_price) / min_price;
                if change > config.black_swan_threshold {
                    warn!(
                        "Black swan detected for {}: price changed {}% in {} ticks",
                        symbol,
                        change * Decimal::from(100),
                        recent_count
                    );
                    return true;
                }
            }
        }

        false
    }

    /// Kelly 公式仓位计算
    fn calculate_kelly_position(&self, order: &OrderRequest, state: &RiskState, config: &RiskConfig) -> Decimal {
        // 简化版本：基于历史胜率和盈亏比
        // 实际应该从交易历史中计算

        // 默认使用保守的 1/4 Kelly
        let base_kelly = config.kelly_fraction;

        // 波动率调整
        if let Some(history) = state.price_history.get(&order.symbol) {
            if history.len() >= 2 {
                let prices: Vec<Decimal> = history.iter().map(|(_, p)| *p).collect();
                let volatility = self.calculate_volatility(&prices);

                // 波动率越高，仓位越小
                if volatility > config.volatility_target {
                    let adjustment = config.volatility_target / volatility;
                    return order.quantity * base_kelly * adjustment;
                }
            }
        }

        order.quantity * base_kelly
    }

    /// 计算波动率
    fn calculate_volatility(&self, prices: &[Decimal]) -> Decimal {
        if prices.len() < 2 {
            return Decimal::ZERO;
        }

        // 计算收益率
        let returns: Vec<Decimal> = prices
            .windows(2)
            .map(|w| {
                if w[0] > Decimal::ZERO {
                    (w[1] - w[0]) / w[0]
                } else {
                    Decimal::ZERO
                }
            })
            .collect();

        if returns.is_empty() {
            return Decimal::ZERO;
        }

        // 计算标准差
        let mean = returns.iter().sum::<Decimal>() / Decimal::from(returns.len());
        let variance = returns
            .iter()
            .map(|r| (*r - mean) * (*r - mean))
            .sum::<Decimal>()
            / Decimal::from(returns.len());

        // 近似平方根
        self.approximate_sqrt(variance)
    }

    /// 近似平方根 (牛顿法)
    fn approximate_sqrt(&self, value: Decimal) -> Decimal {
        if value <= Decimal::ZERO {
            return Decimal::ZERO;
        }

        let mut x = value / Decimal::from(2);
        for _ in 0..10 {
            x = (x + value / x) / Decimal::from(2);
        }
        x
    }
}

/// 风控状态摘要
#[derive(Debug, Clone)]
pub struct RiskStatus {
    pub daily_pnl: Decimal,
    pub peak_equity: Decimal,
    pub current_equity: Decimal,
    pub daily_trade_count: u32,
    pub is_circuit_breaker_active: bool,
    pub position_count: usize,
}

/// 风控错误
#[derive(Debug, thiserror::Error)]
pub enum RiskError {
    #[error("Risk check failed: {0}")]
    CheckFailed(String),

    #[error("Invalid configuration: {0}")]
    InvalidConfig(String),
}
