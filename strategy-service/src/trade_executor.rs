//! 交易执行器模块
//!
//! 严谨的交易执行逻辑，包括：
//! - 多交易所支持
//! - 现货/合约交易限制
//! - 订单重复检查
//! - 仓位阈值检查
//! - 账户余额检查
//! - 交易对精度处理

use anyhow::{anyhow, Result};
use chrono::Utc;
use rust_decimal::Decimal;
use rust_decimal::prelude::FromStr;
use sqlx::PgPool;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::{error, info, warn};
use uuid::Uuid;

use crate::exchange::{ExchangeClient, ExchangeApiConfig, SymbolPrecision as ExchangeSymbolPrecision};

// =================================================================
// 交易配置
// =================================================================

/// 交易所配置
#[derive(Debug, Clone)]
pub struct ExchangeConfig {
    /// 是否启用现货交易
    pub spot_enabled: bool,
    /// 是否启用合约交易
    pub futures_enabled: bool,
    /// 最大持仓数量
    pub max_positions: usize,
    /// 单笔最大下单金额（USDT）
    pub max_order_amount: Decimal,
    /// 最大总持仓金额（USDT）
    pub max_total_position: Decimal,
    /// 最小下单金额（USDT）
    pub min_order_amount: Decimal,
}

impl Default for ExchangeConfig {
    fn default() -> Self {
        Self {
            spot_enabled: true,
            futures_enabled: true,
            max_positions: 10,
            max_order_amount: Decimal::from(1000),
            max_total_position: Decimal::from(10000),
            min_order_amount: Decimal::from(10),
        }
    }
}

/// 交易对精度信息
#[derive(Debug, Clone)]
pub struct SymbolPrecision {
    /// 数量精度（小数位数）
    pub quantity_precision: u32,
    /// 价格精度（小数位数）
    pub price_precision: u32,
    /// 最小下单数量
    pub min_quantity: Decimal,
    /// 最小下单金额
    pub min_notional: Decimal,
}

// =================================================================
// 交易订单
// =================================================================

/// 交易订单状态
#[derive(Debug, Clone, PartialEq)]
pub enum OrderStatus {
    Pending,
    Filled,
    PartiallyFilled,
    Cancelled,
    Rejected,
    Failed,
}

impl OrderStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            OrderStatus::Pending => "pending",
            OrderStatus::Filled => "filled",
            OrderStatus::PartiallyFilled => "partially_filled",
            OrderStatus::Cancelled => "cancelled",
            OrderStatus::Rejected => "rejected",
            OrderStatus::Failed => "failed",
        }
    }
}

/// 交易订单请求
#[derive(Debug, Clone)]
pub struct TradeOrder {
    pub signal_id: Uuid,
    pub instance_id: Uuid,
    pub symbol: String,
    pub side: OrderSide,
    pub order_type: OrderType,
    pub quantity: Decimal,
    pub price: Option<Decimal>,
    pub stop_loss: Option<Decimal>,
    pub take_profit: Option<Decimal>,
    pub exchange: String,
    pub market_type: String,
}

/// 订单方向
#[derive(Debug, Clone, PartialEq)]
pub enum OrderSide {
    Buy,
    Sell,
}

impl OrderSide {
    pub fn as_str(&self) -> &'static str {
        match self {
            OrderSide::Buy => "BUY",
            OrderSide::Sell => "SELL",
        }
    }
}

/// 订单类型
#[derive(Debug, Clone)]
pub enum OrderType {
    Market,
    Limit,
    StopLoss,
    TakeProfit,
    StopMarket,
    TakeProfitMarket,
}

impl OrderType {
    pub fn as_str(&self) -> &'static str {
        match self {
            OrderType::Market => "market",
            OrderType::Limit => "limit",
            OrderType::StopLoss => "stop_loss",
            OrderType::TakeProfit => "take_profit",
            OrderType::StopMarket => "STOP_MARKET",
            OrderType::TakeProfitMarket => "TAKE_PROFIT_MARKET",
        }
    }

    /// 转换为交易所 API 类型
    pub fn to_exchange_type(&self) -> &'static str {
        match self {
            OrderType::Market => "MARKET",
            OrderType::Limit => "LIMIT",
            OrderType::StopLoss => "STOP_MARKET",
            OrderType::TakeProfit => "TAKE_PROFIT_MARKET",
            OrderType::StopMarket => "STOP_MARKET",
            OrderType::TakeProfitMarket => "TAKE_PROFIT_MARKET",
        }
    }
}

// =================================================================
// 交易验证器
// =================================================================

/// 交易验证器 - 检查所有前置条件
pub struct TradeValidator {
    pool: PgPool,
    config: ExchangeConfig,
    exchange_client: Option<ExchangeClient>,
    symbol_precisions: Arc<RwLock<HashMap<String, SymbolPrecision>>>,
}

impl TradeValidator {
    pub fn new(pool: PgPool, config: ExchangeConfig) -> Self {
        // 尝试从环境变量创建交易所客户端
        let exchange_client = match ExchangeApiConfig::binance_from_env() {
            Ok(api_config) => Some(ExchangeClient::new(api_config)),
            Err(e) => {
                warn!("Failed to create exchange client: {}", e);
                None
            }
        };

        Self {
            pool,
            config,
            exchange_client,
            symbol_precisions: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// 验证交易是否可以执行
    pub async fn validate_trade(&self, order: &TradeOrder) -> Result<ValidationResult> {
        let mut result = ValidationResult::new();

        // 1. 检查交易类型是否启用
        self.check_market_type_enabled(order, &mut result).await?;

        // 2. 检查订单重复
        self.check_duplicate_order(order, &mut result).await?;

        // 3. 检查仓位阈值
        self.check_position_limit(order, &mut result).await?;

        // 4. 检查账户余额
        self.check_account_balance(order, &mut result).await?;

        // 5. 检查交易对精度
        self.check_symbol_precision(order, &mut result).await?;

        // 6. 检查最小下单金额
        self.check_min_order_amount(order, &mut result).await?;

        Ok(result)
    }

    /// 检查交易类型是否启用
    async fn check_market_type_enabled(
        &self,
        order: &TradeOrder,
        result: &mut ValidationResult,
    ) -> Result<()> {
        match order.market_type.as_str() {
            "spot" => {
                if !self.config.spot_enabled {
                    result.add_error("现货交易未启用".to_string());
                }
            }
            "futures" => {
                if !self.config.futures_enabled {
                    result.add_error("合约交易未启用".to_string());
                }
            }
            _ => {
                result.add_error(format!("未知的市场类型: {}", order.market_type));
            }
        }
        Ok(())
    }

    /// 检查订单重复
    async fn check_duplicate_order(
        &self,
        order: &TradeOrder,
        result: &mut ValidationResult,
    ) -> Result<()> {
        // 查询同一策略实例在同一交易对的未完成订单
        let pending_orders = sqlx::query_scalar::<_, i64>(
            r#"
            SELECT COUNT(*) FROM trades
            WHERE symbol = $1
              AND strategy_id = $2
              AND order_status IN ('pending', 'partially_filled')
              AND created_at > NOW() - INTERVAL '1 hour'
            "#
        )
        .bind(&order.symbol)
        .bind(order.instance_id.to_string())
        .fetch_one(&self.pool)
        .await?;

        if pending_orders > 0 {
            result.add_error(format!(
                "存在未完成的订单: {} (策略: {})",
                order.symbol, order.instance_id
            ));
        }

        Ok(())
    }

    /// 检查仓位阈值
    async fn check_position_limit(
        &self,
        order: &TradeOrder,
        result: &mut ValidationResult,
    ) -> Result<()> {
        // 检查当前持仓数量
        let position_count = sqlx::query_scalar::<_, i64>(
            "SELECT COUNT(*) FROM positions WHERE quantity > 0"
        )
        .fetch_one(&self.pool)
        .await?;

        if position_count >= self.config.max_positions as i64 {
            result.add_error(format!(
                "已达到最大持仓数量: {}/{}",
                position_count, self.config.max_positions
            ));
        }

        // 检查总持仓金额
        let total_position_value = sqlx::query_scalar::<_, Decimal>(
            "SELECT COALESCE(SUM(quantity * avg_entry_price), 0) FROM positions WHERE quantity > 0"
        )
        .fetch_one(&self.pool)
        .await?;

        if total_position_value >= self.config.max_total_position {
            result.add_error(format!(
                "已达到最大持仓金额: {} >= {}",
                total_position_value, self.config.max_total_position
            ));
        }

        // 检查是否已有该交易对的持仓
        let existing_position = sqlx::query_scalar::<_, Decimal>(
            "SELECT COALESCE(quantity, 0) FROM positions WHERE symbol = $1"
        )
        .bind(&order.symbol)
        .fetch_optional(&self.pool)
        .await?;

        if let Some(qty) = existing_position {
            if qty > Decimal::ZERO && order.side == OrderSide::Buy {
                result.add_warning(format!(
                    "已持有 {} 仓位: {}",
                    order.symbol, qty
                ));
            }
        }

        Ok(())
    }

    /// 检查账户余额
    async fn check_account_balance(
        &self,
        order: &TradeOrder,
        result: &mut ValidationResult,
    ) -> Result<()> {
        // 获取可用余额
        let available_balance = self.get_available_balance(&order.exchange, &order.market_type).await?;

        // 计算所需金额（市价单使用估算价格）
        let required_amount = if let Some(price) = order.price {
            order.quantity * price
        } else {
            // 市价单，使用 entry_price 估算
            order.quantity * order.price.unwrap_or(Decimal::from(50000)) // 默认 BTC 价格
        };

        if available_balance < required_amount {
            result.add_error(format!(
                "余额不足: 需要 {} USDT, 可用 {} USDT",
                required_amount, available_balance
            ));
        }

        Ok(())
    }

    /// 获取可用余额
    async fn get_available_balance(&self, exchange: &str, market_type: &str) -> Result<Decimal> {
        if let Some(client) = &self.exchange_client {
            match market_type {
                "futures" => {
                    // 从交易所 API 获取合约账户余额
                    match client.get_usdt_balance().await {
                        Ok(balance) => Ok(balance),
                        Err(e) => {
                            warn!("Failed to get futures balance from exchange: {}", e);
                            // 降级到数据库查询
                            self.get_balance_from_db(exchange, market_type).await
                        }
                    }
                }
                "spot" => {
                    // 从交易所 API 获取现货账户余额
                    match client.get_spot_usdt_balance().await {
                        Ok(balance) => Ok(balance),
                        Err(e) => {
                            warn!("Failed to get spot balance from exchange: {}", e);
                            // 降级到数据库查询
                            self.get_balance_from_db(exchange, market_type).await
                        }
                    }
                }
                _ => self.get_balance_from_db(exchange, market_type).await,
            }
        } else {
            // 没有交易所客户端，从数据库查询
            self.get_balance_from_db(exchange, market_type).await
        }
    }

    /// 从数据库获取余额（降级方案）
    async fn get_balance_from_db(&self, exchange: &str, market_type: &str) -> Result<Decimal> {
        // 从 Redis 或数据库获取缓存的余额
        // 暂时返回默认值
        warn!("Using default balance (10000 USDT)");
        Ok(Decimal::from(10000))
    }

    /// 检查交易对精度
    async fn check_symbol_precision(
        &self,
        order: &TradeOrder,
        result: &mut ValidationResult,
    ) -> Result<()> {
        let precision = self.get_symbol_precision(&order.symbol, &order.exchange).await?;

        // 检查数量精度
        let quantity_str = order.quantity.to_string();
        if let Some(dot_pos) = quantity_str.find('.') {
            let decimal_places = quantity_str.len() - dot_pos - 1;
            if decimal_places > precision.quantity_precision as usize {
                result.add_error(format!(
                    "数量精度超限: {} (最大 {} 位小数)",
                    order.quantity, precision.quantity_precision
                ));
            }
        }

        // 检查最小数量
        if order.quantity < precision.min_quantity {
            result.add_error(format!(
                "数量小于最小值: {} < {}",
                order.quantity, precision.min_quantity
            ));
        }

        Ok(())
    }

    /// 检查最小下单金额
    async fn check_min_order_amount(
        &self,
        order: &TradeOrder,
        result: &mut ValidationResult,
    ) -> Result<()> {
        let order_amount = order.quantity * order.price.unwrap_or(Decimal::ZERO);

        if order_amount < self.config.min_order_amount {
            result.add_error(format!(
                "下单金额小于最小值: {} < {}",
                order_amount, self.config.min_order_amount
            ));
        }

        Ok(())
    }

    /// 获取交易对精度
    async fn get_symbol_precision(&self, symbol: &str, exchange: &str) -> Result<SymbolPrecision> {
        // 先检查缓存
        {
            let precisions = self.symbol_precisions.read().await;
            if let Some(precision) = precisions.get(symbol) {
                return Ok(precision.clone());
            }
        }

        // 从交易所 API 获取精度信息
        let precision = if let Some(client) = &self.exchange_client {
            match client.get_symbol_precision(symbol).await {
                Ok(exchange_precision) => {
                    // 转换为内部格式
                    SymbolPrecision {
                        quantity_precision: exchange_precision.base_asset_precision,
                        price_precision: exchange_precision.quote_asset_precision,
                        min_quantity: exchange_precision.min_quantity,
                        min_notional: exchange_precision.min_notional,
                    }
                }
                Err(e) => {
                    warn!("Failed to get symbol precision from exchange: {}", e);
                    // 使用默认值
                    self.get_default_precision(symbol)
                }
            }
        } else {
            // 没有交易所客户端，使用默认值
            self.get_default_precision(symbol)
        };

        // 缓存
        {
            let mut precisions = self.symbol_precisions.write().await;
            precisions.insert(symbol.to_string(), precision.clone());
        }

        Ok(precision)
    }

    /// 获取默认精度（降级方案）
    fn get_default_precision(&self, symbol: &str) -> SymbolPrecision {
        // 根据交易对返回默认精度
        match symbol {
            s if s.contains("BTC") => SymbolPrecision {
                quantity_precision: 5,
                price_precision: 2,
                min_quantity: Decimal::from_str("0.00001").unwrap_or(Decimal::ZERO),
                min_notional: Decimal::from(10),
            },
            s if s.contains("ETH") => SymbolPrecision {
                quantity_precision: 4,
                price_precision: 2,
                min_quantity: Decimal::from_str("0.0001").unwrap_or(Decimal::ZERO),
                min_notional: Decimal::from(10),
            },
            s if s.contains("SOL") => SymbolPrecision {
                quantity_precision: 2,
                price_precision: 2,
                min_quantity: Decimal::from_str("0.01").unwrap_or(Decimal::ZERO),
                min_notional: Decimal::from(10),
            },
            _ => SymbolPrecision {
                quantity_precision: 3,
                price_precision: 2,
                min_quantity: Decimal::from_str("0.001").unwrap_or(Decimal::ZERO),
                min_notional: Decimal::from(10),
            },
        }
    }
}

/// 验证结果
#[derive(Debug)]
pub struct ValidationResult {
    pub is_valid: bool,
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
}

impl ValidationResult {
    pub fn new() -> Self {
        Self {
            is_valid: true,
            errors: Vec::new(),
            warnings: Vec::new(),
        }
    }

    pub fn add_error(&mut self, error: String) {
        self.is_valid = false;
        self.errors.push(error);
    }

    pub fn add_warning(&mut self, warning: String) {
        self.warnings.push(warning);
    }
}

// =================================================================
// 交易执行器
// =================================================================

/// 交易执行器
pub struct TradeExecutor {
    pool: PgPool,
    validator: TradeValidator,
}

impl TradeExecutor {
    pub fn new(pool: PgPool, config: ExchangeConfig) -> Self {
        let validator = TradeValidator::new(pool.clone(), config);
        Self { pool, validator }
    }

    /// 从策略信号创建交易订单（包含止损止盈）
    pub fn create_orders_from_signal(
        &self,
        signal: &crate::db::signals::StrategySignal,
        instance_id: Uuid,
        position_size_pct: Decimal,
        exchange: &str,
        market_type: &str,
    ) -> Vec<TradeOrder> {
        // 只处理买入和卖出信号
        let side = match signal.direction.as_str() {
            "bullish" => OrderSide::Buy,
            "bearish" => OrderSide::Sell,
            _ => return vec![],
        };

        // 计算下单数量
        let quantity = self.calculate_quantity(
            &signal.symbol,
            signal.entry_price,
            position_size_pct,
        );

        let mut orders = Vec::new();

        // 主订单（市价单）
        orders.push(TradeOrder {
            signal_id: signal.id,
            instance_id,
            symbol: signal.symbol.clone(),
            side: side.clone(),
            order_type: OrderType::Market,
            quantity,
            price: None,
            stop_loss: signal.stop_loss,
            take_profit: signal.take_profit,
            exchange: exchange.to_string(),
            market_type: market_type.to_string(),
        });

        // 止损单（如果设置了止损价格）
        if let Some(stop_loss_price) = signal.stop_loss {
            let stop_side = match side {
                OrderSide::Buy => OrderSide::Sell,  // 买入后止损是卖出
                OrderSide::Sell => OrderSide::Buy,   // 卖出后止损是买入
            };

            orders.push(TradeOrder {
                signal_id: signal.id,
                instance_id,
                symbol: signal.symbol.clone(),
                side: stop_side,
                order_type: OrderType::StopMarket,
                quantity,
                price: None,
                stop_loss: Some(stop_loss_price),
                take_profit: None,
                exchange: exchange.to_string(),
                market_type: market_type.to_string(),
            });
        }

        // 止盈单（如果设置了止盈价格）
        if let Some(take_profit_price) = signal.take_profit {
            let tp_side = match side {
                OrderSide::Buy => OrderSide::Sell,  // 买入后止盈是卖出
                OrderSide::Sell => OrderSide::Buy,   // 卖出后止盈是买入
            };

            orders.push(TradeOrder {
                signal_id: signal.id,
                instance_id,
                symbol: signal.symbol.clone(),
                side: tp_side,
                order_type: OrderType::TakeProfitMarket,
                quantity,
                price: None,
                stop_loss: None,
                take_profit: Some(take_profit_price),
                exchange: exchange.to_string(),
                market_type: market_type.to_string(),
            });
        }

        orders
    }

    /// 计算下单数量
    fn calculate_quantity(
        &self,
        symbol: &str,
        entry_price: Decimal,
        position_size_pct: Decimal,
    ) -> Decimal {
        // 简化处理：假设总资金为 10000 USDT
        // 实际应该从数据库或 Redis 获取账户余额
        let total_capital = Decimal::from(10000);
        let position_size = total_capital * position_size_pct / Decimal::from(100);
        let quantity = position_size / entry_price;

        // 根据交易对精度调整
        let precision = self.get_symbol_precision_static(symbol);
        self.round_quantity(quantity, precision)
    }

    /// 获取交易对精度（静态方法）
    fn get_symbol_precision_static(&self, symbol: &str) -> u32 {
        // 根据交易对返回不同的精度
        match symbol {
            s if s.contains("BTC") => 5,
            s if s.contains("ETH") => 4,
            s if s.contains("SOL") => 2,
            _ => 3,
        }
    }

    /// 四舍五入到指定精度
    fn round_quantity(&self, quantity: Decimal, precision: u32) -> Decimal {
        let factor = Decimal::from(10_i64.pow(precision));
        (quantity * factor).round() / factor
    }

    /// 执行交易（从信号到下单的完整流程，支持止损止盈）
    pub async fn execute_trade(
        &self,
        signal: &crate::db::signals::StrategySignal,
        instance_id: Uuid,
        position_size_pct: Decimal,
        exchange: &str,
        market_type: &str,
    ) -> Result<Vec<Uuid>> {
        // 创建订单（主订单 + 止损止盈单）
        let orders = self.create_orders_from_signal(
            signal,
            instance_id,
            position_size_pct,
            exchange,
            market_type,
        );

        if orders.is_empty() {
            return Ok(vec![]);
        }

        let mut order_ids = Vec::new();

        // 处理每个订单
        for (i, order) in orders.iter().enumerate() {
            // 验证订单
            let validation = self.validator.validate_trade(order).await?;

            // 输出警告
            for warning in &validation.warnings {
                warn!("Trade warning: {}", warning);
            }

            // 如果验证失败，拒绝订单
            if !validation.is_valid {
                let error_msg = validation.errors.join("; ");
                warn!(
                    "Trade rejected for {} {}: {}",
                    order.side.as_str(),
                    order.symbol,
                    error_msg
                );

                // 记录被拒绝的订单
                self.record_rejected_order(order, &error_msg).await?;
                continue;
            }

            // 提交订单
            match self.submit_order(order).await {
                Ok(order_id) => {
                    order_ids.push(order_id);
                    info!(
                        "Order {} submitted: {} {} {} (type: {})",
                        i + 1,
                        order.side.as_str(),
                        order.symbol,
                        order.order_type.as_str(),
                        order.order_type.to_exchange_type()
                    );
                }
                Err(e) => {
                    error!("Failed to submit order {}: {}", i + 1, e);
                }
            }
        }

        Ok(order_ids)
    }

    /// 将交易订单写入数据库
    async fn submit_order(&self, order: &TradeOrder) -> Result<Uuid> {
        let order_id = Uuid::new_v4();

        // 写入 trades 表
        sqlx::query(
            r#"
            INSERT INTO trades (
                id, signal_id, symbol, side, price, quantity,
                order_status, order_type, exchange, market_type,
                strategy_id, trade_time, created_at
            ) VALUES (
                $1, $2, $3, $4, $5, $6,
                $7, $8, $9, $10,
                $11, $12, $13
            )
            "#
        )
        .bind(order_id)
        .bind(order.signal_id)
        .bind(&order.symbol)
        .bind(order.side.as_str())
        .bind(order.price.unwrap_or(Decimal::ZERO))
        .bind(order.quantity)
        .bind(OrderStatus::Pending.as_str())
        .bind(order.order_type.as_str())
        .bind(&order.exchange)
        .bind(&order.market_type)
        .bind(order.instance_id.to_string())
        .bind(Utc::now())
        .bind(Utc::now())
        .execute(&self.pool)
        .await?;

        // 更新信号状态
        sqlx::query(
            "UPDATE strategy_signals SET executed = true, order_id = $1 WHERE id = $2"
        )
        .bind(order_id.to_string())
        .bind(order.signal_id)
        .execute(&self.pool)
        .await?;

        info!(
            "📝 Order submitted: {} {} {} qty={} (signal={}, order={})",
            order.side.as_str(),
            order.symbol,
            order.order_type.as_str(),
            order.quantity,
            order.signal_id,
            order_id
        );

        Ok(order_id)
    }

    /// 记录被拒绝的订单
    async fn record_rejected_order(&self, order: &TradeOrder, reason: &str) -> Result<()> {
        let order_id = Uuid::new_v4();

        sqlx::query(
            r#"
            INSERT INTO trades (
                id, signal_id, symbol, side, price, quantity,
                order_status, order_type, exchange, market_type,
                strategy_id, trade_time, created_at, metadata
            ) VALUES (
                $1, $2, $3, $4, $5, $6,
                $7, $8, $9, $10,
                $11, $12, $13, $14
            )
            "#
        )
        .bind(order_id)
        .bind(order.signal_id)
        .bind(&order.symbol)
        .bind(order.side.as_str())
        .bind(order.price.unwrap_or(Decimal::ZERO))
        .bind(order.quantity)
        .bind(OrderStatus::Rejected.as_str())
        .bind(order.order_type.as_str())
        .bind(&order.exchange)
        .bind(&order.market_type)
        .bind(order.instance_id.to_string())
        .bind(Utc::now())
        .bind(Utc::now())
        .bind(serde_json::json!({ "reject_reason": reason }))
        .execute(&self.pool)
        .await?;

        info!(
            "❌ Order rejected: {} {} (reason: {})",
            order.side.as_str(),
            order.symbol,
            reason
        );

        Ok(())
    }
}
