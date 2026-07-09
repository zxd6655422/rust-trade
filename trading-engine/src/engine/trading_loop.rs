// engine/trading_loop.rs
// 主交易循环

use std::cell::RefCell;
use std::sync::Arc;
use tokio::sync::{broadcast, mpsc};
use tokio::time::{interval, Duration};
use tracing::{error, info, warn};
use chrono;

use crate::config::{DataSourceType, Settings};
use crate::exchange::traits::Exchange;
use crate::exchange::types::{OrderStatus, OrderUpdate};
use crate::exchange::RedisDataSource;
use crate::order::OrderManager;
use crate::portfolio::{PortfolioManager, PositionReconciler};
use crate::risk::RiskEngine;
use crate::storage::RedisCache;
use trading_common::backtest::strategy::{Signal, Strategy};
use trading_common::data::types::TickData;

/// 交易循环
pub struct TradingLoop {
    exchange: Arc<dyn Exchange>,
    order_manager: Arc<OrderManager>,
    risk_engine: Arc<RiskEngine>,
    portfolio_manager: Arc<PortfolioManager>,
    reconciler: Arc<PositionReconciler>,
    cache: Arc<RedisCache>,
    redis_datasource: Arc<RedisDataSource>,
    strategy: RefCell<Box<dyn Strategy>>,
    symbols: Vec<String>,
    poll_interval_ms: u64,
    data_source: DataSourceType,
    shutdown_tx: broadcast::Sender<()>,
}

impl TradingLoop {
    /// 创建新的交易循环
    pub fn new(
        exchange: Arc<dyn Exchange>,
        order_manager: Arc<OrderManager>,
        risk_engine: Arc<RiskEngine>,
        portfolio_manager: Arc<PortfolioManager>,
        reconciler: Arc<PositionReconciler>,
        cache: Arc<RedisCache>,
        strategy: Box<dyn Strategy>,
        settings: &Settings,
    ) -> Self {
        let (shutdown_tx, _) = broadcast::channel(1);

        // 创建 Redis 数据源
        let redis_config = crate::exchange::RedisDataSourceConfig {
            poll_interval_ms: settings.trading.poll_interval_ms,
            enabled: true,
        };
        let redis_datasource = Arc::new(RedisDataSource::new(cache.clone(), redis_config));

        Self {
            exchange,
            order_manager,
            risk_engine,
            portfolio_manager,
            reconciler,
            cache,
            redis_datasource,
            strategy: RefCell::new(strategy),
            symbols: settings.trading.symbols.clone(),
            poll_interval_ms: settings.trading.poll_interval_ms,
            data_source: settings.trading.data_source.clone(),
            shutdown_tx,
        }
    }

    /// 启动交易循环
    pub async fn start(&self) -> Result<(), Box<dyn std::error::Error>> {
        info!("Starting trading loop for symbols: {:?}", self.symbols);
        info!("Strategy: {}", self.strategy.borrow().name());
        info!("Data source: {}", self.data_source);
        info!("Poll interval: {}ms", self.poll_interval_ms);

        // 同步初始账户余额到风控引擎
        match self.exchange.get_account().await {
            Ok(account) => {
                self.risk_engine.sync_account_balance(&account).await;
                info!("Initial account balance synced: equity={}", account.total_equity);
            }
            Err(e) => {
                warn!("Failed to sync initial account balance: {}", e);
            }
        }

        // 创建 tick 数据通道
        let (tick_tx, mut tick_rx) = mpsc::channel::<TickData>(1000);

        // 根据数据源类型启动对应的数据订阅
        match self.data_source {
            DataSourceType::Trades => {
                self.start_trades_source(tick_tx.clone()).await;
            }
            DataSourceType::Tickers => {
                self.start_tickers_source(tick_tx.clone()).await;
            }
            DataSourceType::Candle1m => {
                self.start_candle_source(tick_tx.clone()).await;
            }
        }

        // 启动 Redis 数据源作为备用 (所有模式都启用)
        let redis_datasource = self.redis_datasource.clone();
        let symbols = self.symbols.clone();
        let shutdown_rx = self.shutdown_tx.subscribe();
        let tick_tx_redis = tick_tx.clone();

        tokio::spawn(async move {
            if let Err(e) = redis_datasource
                .start_polling(
                    &symbols,
                    Box::new(move |tick| {
                        let tick_tx = tick_tx_redis.clone();
                        tokio::spawn(async move {
                            if let Err(e) = tick_tx.send(tick).await {
                                error!("Failed to send tick from Redis: {}", e);
                            }
                        });
                    }),
                    shutdown_rx,
                )
                .await
            {
                error!("Redis data source failed: {}", e);
            }
        });

        // 启动用户数据流 WebSocket (订单状态实时推送)
        self.start_user_data_stream().await;

        // 主处理循环
        // tick 处理间隔（高频）
        let mut tick_poll_interval = interval(Duration::from_millis(self.poll_interval_ms));
        // 安全兜底轮询（低频）— WebSocket 负责实时推送，这里仅做兜底
        let mut safety_poll_interval = interval(Duration::from_secs(30));
        let mut reconciliation_interval = interval(Duration::from_secs(3600)); // 每小时对账一次
        let mut shutdown_rx = self.shutdown_tx.subscribe();

        loop {
            tokio::select! {
                Some(tick) = tick_rx.recv() => {
                    if let Err(e) = self.process_tick(&tick).await {
                        error!("Failed to process tick {}: {}", tick.symbol, e);
                    }
                }
                _ = tick_poll_interval.tick() => {
                    // 定期同步持仓（轻量级检查）
                    if self.portfolio_manager.needs_sync().await {
                        if let Err(e) = self.portfolio_manager.sync_positions().await {
                            warn!("Failed to sync positions: {}", e);
                        }
                    }
                }
                _ = safety_poll_interval.tick() => {
                    // 安全兜底：定期检查活动订单状态（WebSocket 应该已经处理了大部分更新）
                    // 只检查超过 60 秒未更新的订单
                    if let Err(e) = self.check_stale_orders().await {
                        warn!("Safety poll: failed to check stale orders: {}", e);
                    }
                }
                _ = reconciliation_interval.tick() => {
                    // 定期对账
                    info!("Running scheduled position reconciliation...");
                    match self.reconciler.reconcile().await {
                        Ok(result) => {
                            if !result.is_consistent {
                                warn!("Position discrepancies detected, consider running auto-reconcile");
                            }
                        }
                        Err(e) => {
                            error!("Reconciliation failed: {}", e);
                        }
                    }
                }
                _ = shutdown_rx.recv() => {
                    info!("Shutdown signal received, stopping trading loop");
                    break;
                }
            }
        }

        // 关闭时取消所有订单
        if let Err(e) = self.order_manager.cancel_all_orders().await {
            error!("Failed to cancel orders on shutdown: {}", e);
        }

        info!("Trading loop stopped");
        Ok(())
    }

    /// 启动 trades 数据源 (逐笔成交，高频)
    async fn start_trades_source(&self, tick_tx: mpsc::Sender<TickData>) {
        let exchange = self.exchange.clone();
        let symbols = self.symbols.clone();
        let shutdown_rx = self.shutdown_tx.subscribe();

        tokio::spawn(async move {
            info!("Starting trades WebSocket data source");
            if let Err(e) = exchange
                .subscribe_trades(
                    &symbols,
                    Box::new(move |tick| {
                        let tick_tx = tick_tx.clone();
                        tokio::spawn(async move {
                            if let Err(e) = tick_tx.send(tick).await {
                                error!("Failed to send tick: {}", e);
                            }
                        });
                    }),
                    shutdown_rx,
                )
                .await
            {
                error!("Trade subscription failed: {}", e);
            }
        });
    }

    /// 启动 tickers 数据源 (行情快照，中频)
    async fn start_tickers_source(&self, tick_tx: mpsc::Sender<TickData>) {
        // tickers 也通过 subscribe_trades 获取
        // 交易所 adapter 内部根据 channel 区分
        // 这里复用同一接口，adapter 层做适配
        let exchange = self.exchange.clone();
        let symbols = self.symbols.clone();
        let shutdown_rx = self.shutdown_tx.subscribe();

        tokio::spawn(async move {
            info!("Starting tickers WebSocket data source");
            if let Err(e) = exchange
                .subscribe_trades(
                    &symbols,
                    Box::new(move |tick| {
                        let tick_tx = tick_tx.clone();
                        tokio::spawn(async move {
                            if let Err(e) = tick_tx.send(tick).await {
                                error!("Failed to send tick: {}", e);
                            }
                        });
                    }),
                    shutdown_rx,
                )
                .await
            {
                error!("Ticker subscription failed: {}", e);
            }
        });
    }

    /// 启动 candle1m 数据源 (K线推送，低频，资源最省)
    async fn start_candle_source(&self, tick_tx: mpsc::Sender<TickData>) {
        // K线模式: 定时拉取 K线数据，转换为 TickData
        let exchange = self.exchange.clone();
        let symbols = self.symbols.clone();
        let mut shutdown_rx = self.shutdown_tx.subscribe();

        tokio::spawn(async move {
            info!("Starting candle1m REST polling data source");
            let mut poll_interval = tokio::time::interval(Duration::from_secs(60)); // 每分钟拉取一次

            loop {
                tokio::select! {
                    _ = poll_interval.tick() => {
                        for symbol in &symbols {
                            match exchange.get_klines(symbol, "1m", Some(1)).await {
                                Ok(klines) => {
                                    if let Some(kline) = klines.last() {
                                        // 将 K线收盘价转换为 TickData
                                        let tick = TickData {
                                            timestamp: kline.close_time,
                                            symbol: symbol.clone(),
                                            price: kline.close,
                                            quantity: kline.volume,
                                            side: trading_common::data::types::TradeSide::Buy,
                                            trade_id: format!("candle_{}", kline.close_time.timestamp_millis()),
                                            is_buyer_maker: false,
                                        };
                                        if let Err(e) = tick_tx.send(tick).await {
                                            error!("Failed to send candle tick: {}", e);
                                        }
                                    }
                                }
                                Err(e) => {
                                    warn!("Failed to fetch kline for {}: {}", symbol, e);
                                }
                            }
                        }
                    }
                    _ = shutdown_rx.recv() => {
                        info!("Candle1m data source shutting down");
                        break;
                    }
                }
            }
        });
    }

    /// 启动用户数据流 WebSocket (订单状态实时推送，带自动重连)
    async fn start_user_data_stream(&self) {
        let exchange = self.exchange.clone();
        let order_manager = self.order_manager.clone();
        let portfolio_manager = self.portfolio_manager.clone();
        let mut shutdown_rx = self.shutdown_tx.subscribe();

        tokio::spawn(async move {
            let mut retry_count: u32 = 0;
            let max_retries: u32 = 20; // 最多重试 20 次后重置计数
            let base_delay_ms: u64 = 1000; // 基础延迟 1 秒
            let max_delay_ms: u64 = 60000; // 最大延迟 60 秒

            loop {
                info!("Starting user data stream WebSocket (attempt {})", retry_count + 1);

                let order_cb = {
                    let order_manager = order_manager.clone();
                    let portfolio_manager = portfolio_manager.clone();
                    Box::new(move |update: OrderUpdate| {
                        let order_manager = order_manager.clone();
                        let portfolio_manager = portfolio_manager.clone();
                        tokio::spawn(async move {
                            info!(
                                "Order update received: {} {} {:?} {:?}",
                                update.symbol, update.order_id, update.status, update.side
                            );
                            // 更新订单管理器中的订单状态
                            order_manager.handle_order_update(update.clone()).await;

                            // 订单成交后同步持仓
                            if update.status == OrderStatus::Filled {
                                if let Err(e) = portfolio_manager.sync_positions().await {
                                    warn!("Failed to sync positions after order fill: {}", e);
                                }
                            }
                        });
                    }) as Box<dyn Fn(OrderUpdate) + Send + Sync>
                };

                let sub_shutdown_rx = shutdown_rx.resubscribe();
                match exchange.subscribe_user_data(order_cb, sub_shutdown_rx).await {
                    Ok(_) => {
                        // 正常退出（收到 shutdown 信号），不再重连
                        info!("User data stream stopped gracefully");
                        return;
                    }
                    Err(e) => {
                        retry_count += 1;
                        if retry_count >= max_retries {
                            retry_count = 0; // 重置计数，继续重试
                        }

                        // 指数退避: 1s, 2s, 4s, 8s, ... 最大 60s
                        let delay_ms = (base_delay_ms * 2u64.pow(retry_count.min(6))).min(max_delay_ms);
                        warn!(
                            "User data stream failed: {}. Retrying in {}ms...",
                            e, delay_ms
                        );

                        tokio::select! {
                            _ = tokio::time::sleep(Duration::from_millis(delay_ms)) => {}
                            _ = shutdown_rx.recv() => {
                                info!("Shutdown signal received during reconnect backoff");
                                return;
                            }
                        }
                    }
                }
            }
        });
    }

    /// 处理单个 tick 数据
    async fn process_tick(&self, tick: &TickData) -> Result<(), Box<dyn std::error::Error>> {
        // 1. 更新风控状态
        self.risk_engine.update_market_data(tick).await;

        // 2. 更新持仓价格
        self.portfolio_manager.update_price(&tick.symbol, tick.price).await;

        // 3. 保存价格到缓存
        if let Err(e) = self.cache.set_price(&tick.symbol, tick.price).await {
            warn!("Failed to cache price for {}: {}", tick.symbol, e);
        }

        // 4. 检查止损止盈
        if let Some(stop_action) = self.order_manager.check_stop_orders(&tick.symbol, tick.price).await {
            warn!("Stop order triggered for {}: {:?}", tick.symbol, stop_action);
            match self.order_manager.execute_stop_action(stop_action).await {
                Ok(result) => {
                    info!("Stop order executed: {}", result.order_id);
                }
                Err(e) => {
                    error!("Failed to execute stop order: {}", e);
                }
            }
            return Ok(());
        }

        // 5. 策略计算信号
        let signal = self.strategy.borrow_mut().on_tick(tick);

        // 6. 根据信号执行交易
        match &signal {
            Signal::Buy {
                symbol,
                quantity,
                entry_price,
            } => {
                info!(
                    "BUY signal: {} {} @ {}",
                    symbol, quantity, entry_price
                );
                match self.order_manager.execute_signal(signal).await {
                    Ok(result) => {
                        info!("Order executed: {}", result.order_id);
                    }
                    Err(e) => {
                        warn!("Order execution failed: {}", e);
                    }
                }
            }
            Signal::Sell {
                symbol,
                quantity,
                entry_price,
            } => {
                info!(
                    "SELL signal: {} {} @ {}",
                    symbol, quantity, entry_price
                );
                match self.order_manager.execute_signal(signal).await {
                    Ok(result) => {
                        info!("Order executed: {}", result.order_id);
                    }
                    Err(e) => {
                        warn!("Order execution failed: {}", e);
                    }
                }
            }
            Signal::Hold => {
                // 不输出 Hold 信号，避免日志过多
            }
        }

        Ok(())
    }

    /// 安全兜底：检查长时间未更新的活动订单
    ///
    /// WebSocket 用户数据流负责实时推送订单更新。
    /// 此方法仅作为兜底机制，检查可能因 WebSocket 断连而遗漏的订单。
    async fn check_stale_orders(&self) -> Result<(), Box<dyn std::error::Error>> {
        let active_orders = self.order_manager.get_active_orders().await;
        if active_orders.is_empty() {
            return Ok(());
        }

        // 只检查超过 60 秒未更新的订单（WebSocket 正常情况下秒级推送）
        let stale_threshold = chrono::Duration::seconds(60);
        let now = chrono::Utc::now();

        for order in &active_orders {
            let age = now - order.updated_at;
            if age < stale_threshold {
                continue; // 还没过期，跳过
            }

            // 查询订单最新状态
            match self
                .exchange
                .get_order(&order.symbol, &order.order_id)
                .await
            {
                Ok(updated_order) => {
                    if updated_order.status != order.status {
                        warn!(
                            "Stale order {} status mismatch (WS may have missed update): {} -> {}",
                            order.order_id, order.status, updated_order.status
                        );
                        // 通过 order_manager 处理更新，保持一致性
                        let update = OrderUpdate {
                            order_id: updated_order.order_id.clone(),
                            client_order_id: updated_order.client_order_id.clone(),
                            symbol: updated_order.symbol.clone(),
                            side: updated_order.side.clone(),
                            order_type: updated_order.order_type.clone(),
                            status: updated_order.status.clone(),
                            quantity: updated_order.quantity,
                            filled_quantity: updated_order.filled_quantity,
                            price: updated_order.price,
                            avg_price: None,
                            commission: None,
                            commission_asset: None,
                            timestamp: chrono::Utc::now(),
                        };
                        self.order_manager.handle_order_update(update).await;
                    }
                }
                Err(e) => {
                    warn!("Failed to get order status for {}: {}", order.order_id, e);
                }
            }
        }

        Ok(())
    }

    /// 停止交易循环
    pub fn shutdown(&self) {
        let _ = self.shutdown_tx.send(());
    }
}
