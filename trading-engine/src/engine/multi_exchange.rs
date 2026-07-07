//! 多交易所交易循环
//!
//! 支持同时在多个交易所执行交易
//! 同一策略信号同时在所有配置的交易所下单、撤单、止盈、止损

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{broadcast, mpsc, RwLock};
use tokio::time::{interval, Duration};
use tracing::{error, info, warn};
use rust_decimal::Decimal;

use crate::exchange::traits::Exchange;
use crate::exchange::types::{OrderRequest, OrderResult, OrderSide, OrderType, OrderUpdate};
use crate::order::OrderManager;
use crate::portfolio::PortfolioManager;
use crate::risk::RiskEngine;
use trading_common::backtest::strategy::{Signal, Strategy};
use trading_common::data::types::TickData;

/// 交易所实例配置
#[derive(Debug, Clone)]
pub struct ExchangeInstanceConfig {
    pub exchange_id: String,
    pub testnet: bool,
    pub api_key: String,
    pub api_secret: String,
    pub passphrase: Option<String>,
    /// 该交易所支持的统一交易对名称
    pub symbols: Vec<String>,
    /// 该交易所的市场类型（spot/futures）
    pub market_type: String,
}

/// 交易对映射
#[derive(Debug, Clone)]
pub struct SymbolMapping {
    pub unified_symbol: String,      // 内部统一名称
    pub exchange: String,            // 交易所
    pub exchange_symbol: String,     // 交易所实际名称
    pub market_type: String,         // 市场类型
}

/// 多交易所交易循环
pub struct MultiExchangeLoop {
    /// 交易所实例映射 (exchange_id -> Exchange)
    exchanges: HashMap<String, Arc<dyn Exchange>>,
    /// 交易对映射 (unified_symbol -> [(exchange_id, exchange_symbol)])
    symbol_mappings: HashMap<String, Vec<(String, String)>>,
    /// 策略实例
    strategy: Arc<RwLock<Box<dyn Strategy>>>,
    /// 订单管理器
    order_manager: Arc<OrderManager>,
    /// 风控引擎
    risk_engine: Arc<RiskEngine>,
    /// 关闭信号
    shutdown_tx: broadcast::Sender<()>,
}

/// 批量操作结果
#[derive(Debug)]
pub struct BatchResult<T> {
    pub exchange_id: String,
    pub result: Result<T, String>,
}

impl MultiExchangeLoop {
    /// 创建多交易所交易循环
    pub fn new(
        configs: Vec<ExchangeInstanceConfig>,
        symbol_mappings: Vec<SymbolMapping>,
        strategy: Box<dyn Strategy>,
        order_manager: Arc<OrderManager>,
        risk_engine: Arc<RiskEngine>,
    ) -> Result<Self, Box<dyn std::error::Error>> {
        let mut exchanges = HashMap::new();
        let mut symbol_map: HashMap<String, Vec<(String, String)>> = HashMap::new();

        // 创建所有交易所实例
        for config in configs {
            let exchange = crate::exchange::ExchangeFactory::create(
                &config.exchange_id,
                config.testnet,
                &config.api_key,
                &config.api_secret,
                config.passphrase.as_deref(),
            )?;

            let exchange_id = config.exchange_id.clone();
            exchanges.insert(exchange_id.clone(), Arc::from(exchange));

            info!(
                "Exchange {} initialized with symbols: {:?}",
                exchange_id, config.symbols
            );
        }

        // 构建交易对映射
        for mapping in symbol_mappings {
            symbol_map
                .entry(mapping.unified_symbol)
                .or_insert_with(Vec::new)
                .push((mapping.exchange, mapping.exchange_symbol));
        }

        let (shutdown_tx, _) = broadcast::channel(1);

        Ok(Self {
            exchanges,
            symbol_mappings: symbol_map,
            strategy: Arc::new(RwLock::new(strategy)),
            order_manager,
            risk_engine,
            shutdown_tx,
        })
    }

    /// 启动交易循环
    pub async fn start(&self) -> Result<(), Box<dyn std::error::Error>> {
        info!(
            "Starting multi-exchange loop with {} exchanges",
            self.exchanges.len()
        );

        let mut handles = vec![];

        // 为每个交易所启动独立的处理任务
        for (exchange_id, exchange) in &self.exchanges {
            let exchange_clone = exchange.clone();
            let strategy_clone = self.strategy.clone();
            let order_manager_clone = self.order_manager.clone();
            let risk_engine_clone = self.risk_engine.clone();
            let shutdown_rx = self.shutdown_tx.subscribe();
            let exchange_id_clone = exchange_id.clone();

            let handle = tokio::spawn(async move {
                Self::run_exchange_loop(
                    &exchange_id_clone,
                    exchange_clone,
                    strategy_clone,
                    order_manager_clone,
                    risk_engine_clone,
                    shutdown_rx,
                )
                .await;
            });

            handles.push(handle);
        }

        // 等待所有交易所任务完成
        for handle in handles {
            if let Err(e) = handle.await {
                error!("Exchange task failed: {}", e);
            }
        }

        Ok(())
    }

    /// 单个交易所的处理循环
    async fn run_exchange_loop(
        exchange_id: &str,
        exchange: Arc<dyn Exchange>,
        strategy: Arc<RwLock<Box<dyn Strategy>>>,
        order_manager: Arc<OrderManager>,
        risk_engine: Arc<RiskEngine>,
        mut shutdown_rx: broadcast::Receiver<()>,
    ) {
        info!("Starting exchange loop for {}", exchange_id);

        let mut poll_interval = interval(Duration::from_millis(100));
        let mut shutdown_signal = false;

        loop {
            tokio::select! {
                _ = poll_interval.tick() => {
                    // 获取最新行情
                    // 这里简化处理，实际应该订阅 WebSocket
                }
                _ = shutdown_rx.recv() => {
                    info!("Shutdown signal received for {}", exchange_id);
                    shutdown_signal = true;
                }
            }

            if shutdown_signal {
                break;
            }
        }

        info!("Exchange loop {} stopped", exchange_id);
    }

    /// 获取统一交易对在指定交易所的实际名称
    fn get_exchange_symbol(&self, unified_symbol: &str, exchange_id: &str) -> String {
        if let Some(mappings) = self.symbol_mappings.get(unified_symbol) {
            for (ex_id, ex_symbol) in mappings {
                if ex_id == exchange_id {
                    return ex_symbol.clone();
                }
            }
        }
        // 如果没有映射，返回原始名称
        unified_symbol.to_string()
    }

    /// 在所有交易所下单（并行执行）
    ///
    /// 自动将统一交易对名称转换为各交易所的实际名称
    pub async fn place_order_on_all(
        &self,
        order: OrderRequest,
    ) -> Vec<BatchResult<OrderResult>> {
        let mut handles = vec![];

        for (exchange_id, exchange) in &self.exchanges {
            let exchange_clone = exchange.clone();
            let mut order_clone = order.clone();
            let exchange_id_clone = exchange_id.clone();
            let risk_engine = self.risk_engine.clone();

            // 转换交易对名称
            let exchange_symbol = self.get_exchange_symbol(&order.symbol, exchange_id);
            order_clone.symbol = exchange_symbol;

            let handle = tokio::spawn(async move {
                // 获取账户信息用于风控检查
                let account = match exchange_clone.get_account().await {
                    Ok(acc) => acc,
                    Err(e) => {
                        return BatchResult {
                            exchange_id: exchange_id_clone,
                            result: Err(format!("Failed to get account: {}", e)),
                        };
                    }
                };

                // 风控检查
                match risk_engine.check_order(&order_clone, &account).await {
                    Ok(decision) => {
                        if !decision.is_accepted() {
                            return BatchResult {
                                exchange_id: exchange_id_clone,
                                result: Err(format!("Rejected by risk engine: {:?}", decision)),
                            };
                        }
                    }
                    Err(e) => {
                        return BatchResult {
                            exchange_id: exchange_id_clone,
                            result: Err(format!("Risk check failed: {}", e)),
                        };
                    }
                }

                // 下单
                let result = exchange_clone.place_order(order_clone).await;
                BatchResult {
                    exchange_id: exchange_id_clone,
                    result: result.map_err(|e| e.to_string()),
                }
            });

            handles.push(handle);
        }

        // 等待所有下单完成
        let mut results = vec![];
        for handle in handles {
            match handle.await {
                Ok(result) => results.push(result),
                Err(e) => error!("Task failed: {}", e),
            }
        }

        results
    }

    /// 在所有交易所撤单（并行执行）
    ///
    /// order_ids: exchange_id -> order_id
    pub async fn cancel_order_on_all(
        &self,
        symbol: &str,
        order_ids: &HashMap<String, String>,
    ) -> Vec<BatchResult<()>> {
        let mut handles = vec![];

        for (exchange_id, exchange) in &self.exchanges {
            if let Some(order_id) = order_ids.get(exchange_id) {
                let exchange_clone = exchange.clone();
                // 转换交易对名称
                let exchange_symbol = self.get_exchange_symbol(symbol, exchange_id);
                let order_id_clone = order_id.clone();
                let exchange_id_clone = exchange_id.clone();

                let handle = tokio::spawn(async move {
                    let result = exchange_clone.cancel_order(&exchange_symbol, &order_id_clone).await;
                    BatchResult {
                        exchange_id: exchange_id_clone,
                        result: result.map_err(|e| e.to_string()),
                    }
                });

                handles.push(handle);
            }
        }

        // 等待所有撤单完成
        let mut results = vec![];
        for handle in handles {
            match handle.await {
                Ok(result) => results.push(result),
                Err(e) => error!("Task failed: {}", e),
            }
        }

        results
    }

    /// 在所有交易所批量撤单（并行执行）
    pub async fn cancel_all_orders_on_all(
        &self,
        symbol: Option<&str>,
    ) -> Vec<BatchResult<()>> {
        let mut handles = vec![];

        for (exchange_id, exchange) in &self.exchanges {
            let exchange_clone = exchange.clone();
            // 转换交易对名称
            let exchange_symbol = symbol.map(|s| self.get_exchange_symbol(s, exchange_id));
            let exchange_id_clone = exchange_id.clone();

            let handle = tokio::spawn(async move {
                let result = exchange_clone.cancel_all_orders(exchange_symbol.as_deref()).await;
                BatchResult {
                    exchange_id: exchange_id_clone,
                    result: result.map_err(|e| e.to_string()),
                }
            });

            handles.push(handle);
        }

        // 等待所有撤单完成
        let mut results = vec![];
        for handle in handles {
            match handle.await {
                Ok(result) => results.push(result),
                Err(e) => error!("Task failed: {}", e),
            }
        }

        results
    }

    /// 在所有交易所设置止损单（并行执行）
    ///
    /// 统一交易对名称会自动转换为各交易所的实际名称
    pub async fn place_stop_loss_on_all(
        &self,
        symbol: &str,
        side: &str,
        quantity: Decimal,
        stop_price: Decimal,
    ) -> Vec<BatchResult<OrderResult>> {
        let order = OrderRequest {
            symbol: symbol.to_string(),  // 统一名称，会在 place_order_on_all 中转换
            side: if side == "BUY" { OrderSide::Buy } else { OrderSide::Sell },
            order_type: OrderType::StopLoss,
            quantity,
            price: None,
            stop_price: Some(stop_price),
            time_in_force: None,
            client_order_id: None,
        };

        self.place_order_on_all(order).await
    }

    /// 在所有交易所设置止盈单（并行执行）
    ///
    /// 统一交易对名称会自动转换为各交易所的实际名称
    pub async fn place_take_profit_on_all(
        &self,
        symbol: &str,
        side: &str,
        quantity: Decimal,
        take_profit_price: Decimal,
    ) -> Vec<BatchResult<OrderResult>> {
        let order = OrderRequest {
            symbol: symbol.to_string(),  // 统一名称，会在 place_order_on_all 中转换
            side: if side == "BUY" { OrderSide::Buy } else { OrderSide::Sell },
            order_type: OrderType::TakeProfit,
            quantity,
            price: None,
            stop_price: Some(take_profit_price),
            time_in_force: None,
            client_order_id: None,
        };

        self.place_order_on_all(order).await
    }

    /// 获取所有交易所的持仓（并行执行）
    pub async fn get_positions_on_all(
        &self,
    ) -> Vec<BatchResult<Vec<crate::exchange::types::PositionInfo>>> {
        let mut handles = vec![];

        for (exchange_id, exchange) in &self.exchanges {
            let exchange_clone = exchange.clone();
            let exchange_id_clone = exchange_id.clone();

            let handle = tokio::spawn(async move {
                let result = exchange_clone.get_positions().await;
                BatchResult {
                    exchange_id: exchange_id_clone,
                    result: result.map_err(|e| e.to_string()),
                }
            });

            handles.push(handle);
        }

        // 等待所有查询完成
        let mut results = vec![];
        for handle in handles {
            match handle.await {
                Ok(result) => results.push(result),
                Err(e) => error!("Task failed: {}", e),
            }
        }

        results
    }

    /// 获取所有交易所的账户余额（并行执行）
    pub async fn get_account_on_all(
        &self,
    ) -> Vec<BatchResult<crate::exchange::types::AccountInfo>> {
        let mut handles = vec![];

        for (exchange_id, exchange) in &self.exchanges {
            let exchange_clone = exchange.clone();
            let exchange_id_clone = exchange_id.clone();

            let handle = tokio::spawn(async move {
                let result = exchange_clone.get_account().await;
                BatchResult {
                    exchange_id: exchange_id_clone,
                    result: result.map_err(|e| e.to_string()),
                }
            });

            handles.push(handle);
        }

        // 等待所有查询完成
        let mut results = vec![];
        for handle in handles {
            match handle.await {
                Ok(result) => results.push(result),
                Err(e) => error!("Task failed: {}", e),
            }
        }

        results
    }

    /// 获取交易所数量
    pub fn exchange_count(&self) -> usize {
        self.exchanges.len()
    }

    /// 获取所有交易所 ID
    pub fn exchange_ids(&self) -> Vec<String> {
        self.exchanges.keys().cloned().collect()
    }
}
