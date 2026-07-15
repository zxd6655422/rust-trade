// exchange/adapters/mock_exchange.rs
// Mock 交易所适配器 - 用于本地开发和测试
// 支持从 PostgreSQL 加载历史数据回放，模拟订单撮合

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use std::collections::HashMap;
use std::str::FromStr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use tokio::sync::{broadcast, RwLock};
use tracing::{info, warn};

use crate::exchange::errors::ExchangeError;
use crate::exchange::traits::{MarketDataProvider, TradingOperations, SymbolPrecision};
use crate::exchange::types::*;
use trading_common::data::types::TickData;

/// Mock 交易所配置
#[derive(Debug, Clone)]
pub struct MockExchangeConfig {
    /// 交易对列表
    pub symbols: Vec<String>,
    /// 初始余额 (USDT)
    pub initial_balance: Decimal,
    /// 回放速度 (1.0 = 实时, 2.0 = 2倍速)
    pub replay_speed: f64,
    /// 手续费率 (0.1% = 0.001)
    pub commission_rate: Decimal,
}

impl Default for MockExchangeConfig {
    fn default() -> Self {
        Self {
            symbols: vec!["BTCUSDT".to_string(), "ETHUSDT".to_string()],
            initial_balance: Decimal::from(10000),
            replay_speed: 1.0,
            commission_rate: Decimal::from_str("0.001").unwrap(),
        }
    }
}

/// 模拟账户
#[derive(Debug, Clone)]
struct MockAccount {
    /// USDT 余额
    balance: Decimal,
    /// 持仓 (symbol -> quantity)
    positions: HashMap<String, Decimal>,
    /// 平均入场价 (symbol -> price)
    avg_prices: HashMap<String, Decimal>,
    /// 未实现盈亏
    unrealized_pnl: Decimal,
}

/// Mock 交易所适配器
pub struct MockExchange {
    config: MockExchangeConfig,
    /// K线数据缓存 (symbol -> Vec<Kline>)
    kline_cache: Arc<RwLock<HashMap<String, Vec<Kline>>>>,
    /// 当前价格 (symbol -> price)
    current_prices: Arc<RwLock<HashMap<String, Decimal>>>,
    /// 模拟账户
    account: Arc<RwLock<MockAccount>>,
    /// 模拟订单 (order_id -> OrderInfo)
    orders: Arc<RwLock<HashMap<String, OrderInfo>>>,
    /// 订单计数器
    order_counter: AtomicU64,
    /// 交易历史
    trade_history: Arc<RwLock<Vec<TradeInfo>>>,
}

impl MockExchange {
    /// 创建新的 Mock 交易所
    pub fn new(config: MockExchangeConfig) -> Result<Self, ExchangeError> {
        let initial_balance = config.initial_balance;

        Ok(Self {
            config,
            kline_cache: Arc::new(RwLock::new(HashMap::new())),
            current_prices: Arc::new(RwLock::new(HashMap::new())),
            account: Arc::new(RwLock::new(MockAccount {
                balance: initial_balance,
                positions: HashMap::new(),
                avg_prices: HashMap::new(),
                unrealized_pnl: Decimal::ZERO,
            })),
            orders: Arc::new(RwLock::new(HashMap::new())),
            order_counter: AtomicU64::new(1),
            trade_history: Arc::new(RwLock::new(Vec::new())),
        })
    }

    /// 加载历史 K线数据 (从 Vec 加载)
    pub async fn load_klines(&self, symbol: &str, klines: Vec<Kline>) {
        let mut cache = self.kline_cache.write().await;
        cache.insert(symbol.to_string(), klines);
        info!("Loaded {} klines for {}", cache.get(symbol).map_or(0, |k| k.len()), symbol);
    }

    /// 设置当前价格
    pub async fn set_price(&self, symbol: &str, price: Decimal) {
        let mut prices = self.current_prices.write().await;
        prices.insert(symbol.to_string(), price);
    }

    /// 获取当前价格 (内部使用)
    async fn get_current_price(&self, symbol: &str) -> Result<Decimal, ExchangeError> {
        let prices = self.current_prices.read().await;
        prices.get(symbol)
            .cloned()
            .ok_or_else(|| ExchangeError::InvalidSymbol(format!("No price for {}", symbol)))
    }

    /// 生成订单 ID
    fn next_order_id(&self) -> String {
        let id = self.order_counter.fetch_add(1, Ordering::SeqCst);
        format!("MOCK-{}", id)
    }
}

/// MarketDataProvider 实现
#[async_trait]
impl MarketDataProvider for MockExchange {
    fn exchange_id(&self) -> &str {
        "mock"
    }

    fn is_testnet(&self) -> bool {
        true
    }

    async fn get_server_time(&self) -> Result<DateTime<Utc>, ExchangeError> {
        Ok(Utc::now())
    }

    async fn get_symbol_precision(&self, symbol: &str) -> Result<SymbolPrecision, ExchangeError> {
        // 返回默认精度
        Ok(SymbolPrecision {
            symbol: symbol.to_string(),
            base_asset_precision: 8,
            quote_asset_precision: 8,
            min_quantity: Decimal::from_str("0.001").unwrap(),
            max_quantity: Decimal::from(1000000),
            min_notional: Decimal::from(5),
            step_size: Decimal::from_str("0.001").unwrap(),
            tick_size: Decimal::from_str("0.01").unwrap(),
        })
    }

    async fn get_ticker(&self, symbol: &str) -> Result<Ticker, ExchangeError> {
        let price = self.get_current_price(symbol).await?;

        Ok(Ticker {
            symbol: symbol.to_string(),
            last_price: price,
            bid_price: price * Decimal::from_str("0.999").unwrap(),
            ask_price: price * Decimal::from_str("1.001").unwrap(),
            high_price: price * Decimal::from_str("1.02").unwrap(),
            low_price: price * Decimal::from_str("0.98").unwrap(),
            volume: Decimal::from(1000),
            quote_volume: price * Decimal::from(1000),
            price_change: Decimal::ZERO,
            price_change_percent: Decimal::ZERO,
            timestamp: Utc::now(),
        })
    }

    async fn get_tickers(&self, symbols: &[String]) -> Result<Vec<Ticker>, ExchangeError> {
        let mut tickers = Vec::new();
        for symbol in symbols {
            tickers.push(self.get_ticker(symbol).await?);
        }
        Ok(tickers)
    }

    async fn get_mark_price(&self, symbol: &str) -> Result<MarkPrice, ExchangeError> {
        let price = self.get_current_price(symbol).await?;

        Ok(MarkPrice {
            symbol: symbol.to_string(),
            mark_price: price,
            index_price: price,
            estimated_settle_price: None,
            last_funding_rate: Decimal::ZERO,
            next_funding_time: Utc::now(),
            interest_rate: Decimal::ZERO,
            time: Utc::now(),
        })
    }

    async fn get_funding_rate(&self, symbol: &str, _limit: Option<u32>) -> Result<Vec<FundingRate>, ExchangeError> {
        Ok(vec![FundingRate {
            symbol: symbol.to_string(),
            funding_rate: Decimal::from_str("0.0001").unwrap(),
            funding_time: Utc::now(),
            next_funding_time: Some(Utc::now() + chrono::Duration::hours(8)),
        }])
    }

    async fn get_klines(&self, symbol: &str, _interval: &str, limit: Option<u32>) -> Result<Vec<Kline>, ExchangeError> {
        let cache = self.kline_cache.read().await;
        let klines = cache.get(symbol)
            .cloned()
            .unwrap_or_default();

        let limit = limit.unwrap_or(500) as usize;
        let start = if klines.len() > limit { klines.len() - limit } else { 0 };
        Ok(klines[start..].to_vec())
    }

    async fn get_order_book(&self, symbol: &str, _limit: Option<u32>) -> Result<OrderBook, ExchangeError> {
        let price = self.get_current_price(symbol).await?;
        let spread = price * Decimal::from_str("0.001").unwrap();
        let spread2 = spread * Decimal::from(2);

        Ok(OrderBook {
            symbol: symbol.to_string(),
            bids: vec![
                OrderBookEntry { price: price - spread, quantity: Decimal::from(10) },
                OrderBookEntry { price: price - spread2, quantity: Decimal::from(20) },
            ],
            asks: vec![
                OrderBookEntry { price: price + spread, quantity: Decimal::from(10) },
                OrderBookEntry { price: price + spread2, quantity: Decimal::from(20) },
            ],
            last_update_id: 1,
        })
    }

    async fn get_recent_trades(&self, symbol: &str, _limit: Option<u32>) -> Result<Vec<PublicTrade>, ExchangeError> {
        let price = self.get_current_price(symbol).await?;

        Ok(vec![PublicTrade {
            id: "1".to_string(),
            symbol: symbol.to_string(),
            price,
            quantity: Decimal::from_str("0.1").unwrap(),
            timestamp: Utc::now(),
            is_buyer_maker: false,
        }])
    }

    async fn subscribe_trades(
        &self,
        symbols: &[String],
        callback: Box<dyn Fn(TickData) + Send + Sync>,
        mut shutdown_rx: broadcast::Receiver<()>,
    ) -> Result<(), ExchangeError> {
        let prices = self.current_prices.clone();
        let symbols = symbols.to_vec();

        tokio::spawn(async move {
            let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(1));

            loop {
                tokio::select! {
                    _ = interval.tick() => {
                        let prices = prices.read().await;
                        for symbol in &symbols {
                            if let Some(price) = prices.get(symbol) {
                                let tick = TickData {
                                    timestamp: Utc::now(),
                                    symbol: symbol.clone(),
                                    price: *price,
                                    quantity: Decimal::from_str("0.01").unwrap(),
                                    side: trading_common::data::types::TradeSide::Buy,
                                    trade_id: "1".to_string(),
                                    is_buyer_maker: false,
                                };
                                callback(tick);
                            }
                        }
                    }
                    _ = shutdown_rx.recv() => {
                        info!("Mock exchange subscribe_trades shutdown");
                        break;
                    }
                }
            }
        });

        Ok(())
    }
}

/// TradingOperations 实现
#[async_trait]
impl TradingOperations for MockExchange {
    async fn get_account(&self) -> Result<AccountInfo, ExchangeError> {
        let account = self.account.read().await;
        let prices = self.current_prices.read().await;

        // 计算未实现盈亏
        let mut unrealized_pnl = Decimal::ZERO;
        let mut balances = vec![Balance {
            asset: "USDT".to_string(),
            free: account.balance,
            locked: Decimal::ZERO,
        }];

        // 将持仓也加入余额列表，方便 OrderManager 检查卖出余额
        for (symbol, quantity) in &account.positions {
            if let Some(price) = prices.get(symbol) {
                if let Some(avg_price) = account.avg_prices.get(symbol) {
                    unrealized_pnl += (*price - *avg_price) * *quantity;
                }
            }
            if *quantity > Decimal::ZERO {
                let base_asset = symbol.replace("USDT", "").replace("BUSD", "");
                balances.push(Balance {
                    asset: base_asset,
                    free: *quantity,
                    locked: Decimal::ZERO,
                });
            }
        }

        Ok(AccountInfo {
            balances,
            total_equity: account.balance + unrealized_pnl,
            available_balance: account.balance,
            unrealized_pnl,
            margin_used: Decimal::ZERO,
            margin_ratio: None,
            uid: Some("mock_user".to_string()),
        })
    }

    async fn get_futures_account(&self) -> Result<FuturesAccountInfo, ExchangeError> {
        let account_info = self.get_account().await?;
        let total_equity = account_info.total_equity;
        let unrealized_pnl = account_info.unrealized_pnl;

        Ok(FuturesAccountInfo {
            account_info,
            can_trade: true,
            can_withdraw: true,
            fee_tier: 0,
            max_withdraw_amount: Decimal::from(10000),
            total_initial_margin: Decimal::ZERO,
            total_maint_margin: Decimal::ZERO,
            total_wallet_balance: total_equity,
            total_unrealized_pnl: unrealized_pnl,
            total_margin_balance: total_equity,
        })
    }

    async fn get_position(&self, symbol: &str) -> Result<PositionInfo, ExchangeError> {
        let account = self.account.read().await;
        let prices = self.current_prices.read().await;

        let quantity = account.positions.get(symbol).cloned().unwrap_or(Decimal::ZERO);
        let avg_price = account.avg_prices.get(symbol).cloned().unwrap_or(Decimal::ZERO);
        let mark_price = prices.get(symbol).cloned();

        let unrealized_pnl = if let Some(price) = mark_price {
            (price - avg_price) * quantity
        } else {
            Decimal::ZERO
        };

        Ok(PositionInfo {
            symbol: symbol.to_string(),
            side: if quantity > Decimal::ZERO { PositionSide::Long } else if quantity < Decimal::ZERO { PositionSide::Short } else { PositionSide::None },
            quantity: quantity.abs(),
            avg_entry_price: avg_price,
            mark_price,
            unrealized_pnl,
            leverage: 1,
            margin: Decimal::ZERO,
            liquidation_price: None,
        })
    }

    async fn get_positions(&self) -> Result<Vec<PositionInfo>, ExchangeError> {
        let account = self.account.read().await;
        let mut positions = Vec::new();

        for symbol in account.positions.keys() {
            positions.push(self.get_position(symbol).await?);
        }

        Ok(positions)
    }

    async fn place_order(&self, order: OrderRequest) -> Result<OrderResult, ExchangeError> {
        let mut account = self.account.write().await;
        let prices = self.current_prices.read().await;

        let price = match order.order_type {
            OrderType::Market => {
                prices.get(&order.symbol)
                    .cloned()
                    .ok_or_else(|| ExchangeError::InvalidSymbol(format!("No price for {}", order.symbol)))?
            }
            _ => {
                order.price.unwrap_or_else(|| {
                    prices.get(&order.symbol).cloned().unwrap_or(Decimal::ZERO)
                })
            }
        };

        // 计算手续费
        let notional = price * order.quantity;
        let commission = notional * self.config.commission_rate;

        // 获取当前持仓和均价
        let current_position = account.positions.get(&order.symbol).cloned().unwrap_or(Decimal::ZERO);
        let current_avg_price = account.avg_prices.get(&order.symbol).cloned().unwrap_or(Decimal::ZERO);

        // 检查余额并更新
        if order.side == OrderSide::Buy {
            let required = notional + commission;
            if account.balance < required {
                return Err(ExchangeError::InsufficientBalance(format!(
                    "Required: {}, Available: {}", required, account.balance
                )));
            }
            account.balance -= required;

            // 更新持仓
            let total_cost = current_avg_price * current_position + price * order.quantity;
            let new_position = current_position + order.quantity;
            let new_avg_price = if new_position != Decimal::ZERO {
                total_cost / new_position
            } else {
                Decimal::ZERO
            };
            account.positions.insert(order.symbol.clone(), new_position);
            account.avg_prices.insert(order.symbol.clone(), new_avg_price);
        } else {
            // 卖出
            if current_position < order.quantity {
                return Err(ExchangeError::InsufficientBalance(format!(
                    "Position: {}, Requested: {}", current_position, order.quantity
                )));
            }

            let pnl = (price - current_avg_price) * order.quantity;
            account.balance += notional - commission + pnl;
            let new_position = current_position - order.quantity;

            if new_position == Decimal::ZERO {
                account.positions.remove(&order.symbol);
                account.avg_prices.remove(&order.symbol);
            } else {
                account.positions.insert(order.symbol.clone(), new_position);
            }
        }

        // 记录交易
        let trade = TradeInfo {
            id: self.next_order_id(),
            symbol: order.symbol.clone(),
            price,
            quantity: order.quantity,
            quote_quantity: notional,
            commission,
            commission_asset: "USDT".to_string(),
            time: Utc::now(),
            is_buyer: order.side == OrderSide::Buy,
            is_maker: false,
            realized_pnl: Decimal::ZERO,
        };
        self.trade_history.write().await.push(trade);

        let order_id = self.next_order_id();
        let order_info = OrderInfo {
            order_id: order_id.clone(),
            client_order_id: order.client_order_id.clone(),
            symbol: order.symbol.clone(),
            side: order.side.clone(),
            order_type: order.order_type.clone(),
            status: OrderStatus::Filled,
            quantity: order.quantity,
            filled_quantity: order.quantity,
            remaining_quantity: Decimal::ZERO,
            price: Some(price),
            stop_price: order.stop_price,
            time_in_force: order.time_in_force.unwrap_or(TimeInForce::Gtc),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        self.orders.write().await.insert(order_id.clone(), order_info);

        Ok(OrderResult {
            order_id,
            client_order_id: order.client_order_id,
            symbol: order.symbol,
            side: order.side,
            order_type: order.order_type,
            status: OrderStatus::Filled,
            quantity: order.quantity,
            filled_quantity: order.quantity,
            price: Some(price),
            avg_price: Some(price),
            commission: Some(commission),
            commission_asset: Some("USDT".to_string()),
            created_at: Utc::now(),
            updated_at: Utc::now(),
        })
    }

    async fn cancel_order(&self, symbol: &str, order_id: &str) -> Result<(), ExchangeError> {
        let mut orders = self.orders.write().await;
        if let Some(order) = orders.get_mut(order_id) {
            if order.symbol == symbol {
                order.status = OrderStatus::Canceled;
                return Ok(());
            }
        }
        Err(ExchangeError::OrderNotFound(order_id.to_string()))
    }

    async fn cancel_all_orders(&self, symbol: Option<&str>) -> Result<(), ExchangeError> {
        let mut orders = self.orders.write().await;
        for order in orders.values_mut() {
            if symbol.map_or(true, |s| order.symbol == s) {
                if order.status == OrderStatus::New {
                    order.status = OrderStatus::Canceled;
                }
            }
        }
        Ok(())
    }

    async fn get_open_orders(&self, symbol: Option<&str>) -> Result<Vec<OrderInfo>, ExchangeError> {
        let orders = self.orders.read().await;
        Ok(orders.values()
            .filter(|o| {
                symbol.map_or(true, |s| o.symbol == s) &&
                (o.status == OrderStatus::New || o.status == OrderStatus::PartiallyFilled)
            })
            .cloned()
            .collect())
    }

    async fn get_order(&self, _symbol: &str, order_id: &str) -> Result<OrderInfo, ExchangeError> {
        let orders = self.orders.read().await;
        orders.get(order_id)
            .cloned()
            .ok_or_else(|| ExchangeError::OrderNotFound(order_id.to_string()))
    }

    async fn get_all_orders(&self, symbol: &str, limit: Option<u32>) -> Result<Vec<OrderInfo>, ExchangeError> {
        let orders = self.orders.read().await;
        let limit = limit.unwrap_or(100) as usize;
        let mut result: Vec<OrderInfo> = orders.values()
            .filter(|o| o.symbol == symbol)
            .cloned()
            .collect();
        result.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        result.truncate(limit);
        Ok(result)
    }

    async fn get_trade_history(&self, symbol: &str, limit: Option<u32>) -> Result<Vec<TradeInfo>, ExchangeError> {
        let trades = self.trade_history.read().await;
        let limit = limit.unwrap_or(100) as usize;
        let mut result: Vec<TradeInfo> = trades.iter()
            .filter(|t| t.symbol == symbol)
            .cloned()
            .collect();
        result.sort_by(|a, b| b.time.cmp(&a.time));
        result.truncate(limit);
        Ok(result)
    }

    async fn batch_place_orders(&self, orders: Vec<BatchOrderRequest>) -> Result<Vec<BatchOrderResult>, ExchangeError> {
        let mut results = Vec::new();
        for order in orders {
            match self.place_order(OrderRequest {
                symbol: order.symbol,
                side: order.side,
                order_type: order.order_type,
                quantity: order.quantity,
                price: order.price,
                stop_price: order.stop_price,
                time_in_force: order.time_in_force,
                client_order_id: order.client_order_id,
            }).await {
                Ok(r) => results.push(BatchOrderResult {
                    order_id: r.order_id,
                    client_order_id: r.client_order_id,
                    symbol: r.symbol,
                    status: r.status,
                    error_code: None,
                    error_message: None,
                }),
                Err(e) => results.push(BatchOrderResult {
                    order_id: String::new(),
                    client_order_id: None,
                    symbol: String::new(),
                    status: OrderStatus::Rejected,
                    error_code: Some(-1),
                    error_message: Some(e.to_string()),
                }),
            }
        }
        Ok(results)
    }

    async fn batch_cancel_orders(&self, symbol: &str, order_ids: Vec<String>) -> Result<Vec<BatchOrderResult>, ExchangeError> {
        let mut results = Vec::new();
        for order_id in order_ids {
            match self.cancel_order(symbol, &order_id).await {
                Ok(_) => results.push(BatchOrderResult {
                    order_id: order_id.clone(),
                    client_order_id: None,
                    symbol: symbol.to_string(),
                    status: OrderStatus::Canceled,
                    error_code: None,
                    error_message: None,
                }),
                Err(e) => results.push(BatchOrderResult {
                    order_id: order_id.clone(),
                    client_order_id: None,
                    symbol: symbol.to_string(),
                    status: OrderStatus::Rejected,
                    error_code: Some(-1),
                    error_message: Some(e.to_string()),
                }),
            }
        }
        Ok(results)
    }

    async fn set_leverage(&self, _symbol: &str, _leverage: u32) -> Result<(), ExchangeError> {
        // Mock 模式忽略杠杆设置
        Ok(())
    }

    async fn set_margin_type(&self, _symbol: &str, _margin_type: MarginType) -> Result<(), ExchangeError> {
        // Mock 模式忽略保证金模式
        Ok(())
    }

    async fn subscribe_user_data(
        &self,
        _order_callback: Box<dyn Fn(OrderUpdate) + Send + Sync>,
        mut shutdown_rx: broadcast::Receiver<()>,
    ) -> Result<(), ExchangeError> {
        // Mock 模式不需要用户数据流
        tokio::select! {
            _ = shutdown_rx.recv() => {
                info!("Mock exchange subscribe_user_data shutdown");
            }
        }
        Ok(())
    }

    async fn place_conditional_order(
        &self,
        order: ConditionalOrderRequest,
    ) -> Result<ConditionalOrderResult, ExchangeError> {
        let strategy_id = format!("mock_cond_{}", Utc::now().timestamp_millis());
        info!(
            "Mock conditional order: {} {} {} stop_price={}",
            order.symbol, order.side, order.order_type, order.stop_price
        );
        Ok(ConditionalOrderResult {
            strategy_id,
            symbol: order.symbol,
            side: order.side,
            order_type: order.order_type,
            stop_price: order.stop_price,
            quantity: order.quantity,
            close_position: order.close_position,
            status: "NEW".to_string(),
            created_at: Utc::now(),
        })
    }

    async fn cancel_conditional_order(
        &self,
        _symbol: &str,
        _strategy_id: &str,
    ) -> Result<(), ExchangeError> {
        Ok(())
    }

    async fn get_conditional_orders(
        &self,
        _symbol: Option<&str>,
    ) -> Result<Vec<ConditionalOrderResult>, ExchangeError> {
        Ok(vec![])
    }

    async fn get_income_history(
        &self,
        _symbol: Option<&str>,
        _income_type: Option<&str>,
        _start_time: Option<DateTime<Utc>>,
        _end_time: Option<DateTime<Utc>>,
        _limit: Option<u32>,
    ) -> Result<Vec<IncomeRecord>, ExchangeError> {
        Ok(vec![])
    }
}

// ===== AccountProvider 实现 =====

#[async_trait]
impl trading_common::data::account_types::AccountProvider for MockExchange {
    async fn get_account_snapshot(
        &self,
        market_type: &str,
    ) -> trading_common::data::types::DataResult<trading_common::data::account_types::AccountSnapshot> {
        let account = TradingOperations::get_account(self).await
            .map_err(|e| trading_common::data::types::DataError::InvalidFormat(e.to_string()))?;

        Ok(trading_common::data::account_types::AccountSnapshot {
            exchange: "mock".to_string(),
            market_type: market_type.to_string(),
            uid: account.uid.clone(),
            snapshot_at: chrono::Utc::now(),
            total_equity: account.total_equity,
            total_balance: account.total_equity,
            available_balance: account.available_balance,
            frozen_balance: Decimal::ZERO,
            unrealized_pnl: account.unrealized_pnl,
            initial_margin: None,
            maint_margin: None,
            margin_ratio: account.margin_ratio,
            position_count: 0,
            raw_data: None,
        })
    }

    async fn get_asset_balances(
        &self,
        market_type: &str,
    ) -> trading_common::data::types::DataResult<Vec<trading_common::data::account_types::AssetBalance>> {
        let account = TradingOperations::get_account(self).await
            .map_err(|e| trading_common::data::types::DataError::InvalidFormat(e.to_string()))?;

        let now = chrono::Utc::now();
        Ok(account.balances.iter().map(|b| {
            trading_common::data::account_types::AssetBalance {
                exchange: "mock".to_string(),
                market_type: market_type.to_string(),
                uid: account.uid.clone(),
                asset: b.asset.clone(),
                snapshot_at: now,
                total: b.free + b.locked,
                available: b.free,
                frozen: b.locked,
                unrealized_pnl: Decimal::ZERO,
                usd_value: None,
            }
        }).collect())
    }

    async fn get_positions(&self) -> trading_common::data::types::DataResult<Vec<trading_common::data::account_types::PositionInfo>> {
        let positions = TradingOperations::get_positions(self).await
            .map_err(|e| trading_common::data::types::DataError::InvalidFormat(e.to_string()))?;

        let now = chrono::Utc::now();
        Ok(positions.iter().map(|p| {
            trading_common::data::account_types::PositionInfo {
                exchange: "mock".to_string(),
                uid: None,
                symbol: p.symbol.clone(),
                raw_symbol: p.symbol.clone(),
                snapshot_at: now,
                position_side: match p.side {
                    crate::exchange::types::PositionSide::Long => trading_common::data::account_types::PositionSide::Long,
                    crate::exchange::types::PositionSide::Short => trading_common::data::account_types::PositionSide::Short,
                    crate::exchange::types::PositionSide::None => trading_common::data::account_types::PositionSide::Net,
                },
                position_amt: p.quantity,
                entry_price: p.avg_entry_price,
                mark_price: p.mark_price.unwrap_or_default(),
                unrealized_pnl: p.unrealized_pnl,
                leverage: p.leverage,
                margin_type: trading_common::data::account_types::MarginType::Cross,
                initial_margin: p.margin,
                maint_margin: Decimal::ZERO,
                liquidation_price: p.liquidation_price,
                notional: p.quantity * p.mark_price.unwrap_or_default(),
                break_even_price: None,
                isolated_wallet: None,
                raw_data: None,
            }
        }).collect())
    }

    fn normalize_symbol(&self, raw_symbol: &str) -> String {
        raw_symbol.to_string()
    }
}

// ===== Exchange trait 实现 =====

impl crate::exchange::traits::Exchange for MockExchange {
    fn as_account_provider(&self) -> &dyn trading_common::data::account_types::AccountProvider {
        self
    }
}
