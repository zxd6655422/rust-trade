// engine/trading_loop.rs
// 主交易循环

use std::cell::RefCell;
use std::sync::Arc;
use tokio::sync::{broadcast, mpsc};
use tokio::time::{interval, Duration};
use tracing::{error, info, warn};

use crate::config::Settings;
use crate::exchange::traits::Exchange;
use crate::order::OrderManager;
use crate::risk::RiskEngine;
use trading_common::backtest::strategy::{Signal, Strategy};
use trading_common::data::types::TickData;

/// 交易循环
pub struct TradingLoop {
    exchange: Arc<dyn Exchange>,
    order_manager: Arc<OrderManager>,
    risk_engine: Arc<RiskEngine>,
    strategy: RefCell<Box<dyn Strategy>>,
    symbols: Vec<String>,
    poll_interval_ms: u64,
    shutdown_tx: broadcast::Sender<()>,
}

impl TradingLoop {
    /// 创建新的交易循环
    pub fn new(
        exchange: Arc<dyn Exchange>,
        order_manager: Arc<OrderManager>,
        risk_engine: Arc<RiskEngine>,
        strategy: Box<dyn Strategy>,
        settings: &Settings,
    ) -> Self {
        let (shutdown_tx, _) = broadcast::channel(1);

        Self {
            exchange,
            order_manager,
            risk_engine,
            strategy: RefCell::new(strategy),
            symbols: settings.trading.symbols.clone(),
            poll_interval_ms: settings.trading.poll_interval_ms,
            shutdown_tx,
        }
    }

    /// 启动交易循环
    pub async fn start(&self) -> Result<(), Box<dyn std::error::Error>> {
        info!("Starting trading loop for symbols: {:?}", self.symbols);
        info!("Strategy: {}", self.strategy.borrow().name());
        info!("Poll interval: {}ms", self.poll_interval_ms);

        // 创建 tick 数据通道
        let (tick_tx, mut tick_rx) = mpsc::channel::<TickData>(1000);

        // 启动数据订阅任务
        let exchange = self.exchange.clone();
        let symbols = self.symbols.clone();
        let shutdown_rx = self.shutdown_tx.subscribe();

        tokio::spawn(async move {
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

        // 主处理循环
        let mut poll_interval = interval(Duration::from_millis(self.poll_interval_ms));
        let mut shutdown_rx = self.shutdown_tx.subscribe();

        loop {
            tokio::select! {
                Some(tick) = tick_rx.recv() => {
                    if let Err(e) = self.process_tick(&tick).await {
                        error!("Failed to process tick {}: {}", tick.symbol, e);
                    }
                }
                _ = poll_interval.tick() => {
                    // 定期检查活动订单状态
                    if let Err(e) = self.check_active_orders().await {
                        error!("Failed to check active orders: {}", e);
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

    /// 处理单个 tick 数据
    async fn process_tick(&self, tick: &TickData) -> Result<(), Box<dyn std::error::Error>> {
        // 1. 更新风控状态
        self.risk_engine.update_market_data(tick).await;

        // 2. 策略计算信号
        let signal = self.strategy.borrow_mut().on_tick(tick);

        // 3. 根据信号执行交易
        match &signal {
            Signal::Buy { symbol, quantity } => {
                info!(
                    "BUY signal: {} {} @ {}",
                    symbol, quantity, tick.price
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
            Signal::Sell { symbol, quantity } => {
                info!(
                    "SELL signal: {} {} @ {}",
                    symbol, quantity, tick.price
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

    /// 检查活动订单状态
    async fn check_active_orders(&self) -> Result<(), Box<dyn std::error::Error>> {
        let active_orders = self.order_manager.get_active_orders().await;

        for order in &active_orders {
            // 查询订单最新状态
            match self
                .exchange
                .get_order(&order.symbol, &order.order_id)
                .await
            {
                Ok(updated_order) => {
                    if updated_order.status != order.status {
                        info!(
                            "Order {} status changed: {} -> {}",
                            order.order_id, order.status, updated_order.status
                        );
                    }
                }
                Err(e) => {
                    warn!("Failed to get order status: {}", e);
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
