// portfolio/reconciler.rs
// 持仓对账

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{info, warn};

use crate::exchange::traits::Exchange;
use crate::exchange::types::PositionInfo;
use crate::storage::PositionRepository;

use super::manager::PortfolioManager;

/// 对账结果
#[derive(Debug, Clone)]
pub struct ReconciliationResult {
    /// 对账时间
    pub timestamp: DateTime<Utc>,
    /// 交易所持仓
    pub exchange_positions: HashMap<String, PositionInfo>,
    /// 系统持仓
    pub system_positions: HashMap<String, PositionInfo>,
    /// 差异列表
    pub discrepancies: Vec<PositionDiscrepancy>,
    /// 是否一致
    pub is_consistent: bool,
}

/// 持仓差异
#[derive(Debug, Clone)]
pub struct PositionDiscrepancy {
    pub symbol: String,
    pub discrepancy_type: DiscrepancyType,
    pub exchange_value: Decimal,
    pub system_value: Decimal,
    pub difference: Decimal,
}

/// 差异类型
#[derive(Debug, Clone)]
pub enum DiscrepancyType {
    /// 数量不一致
    QuantityMismatch,
    /// 价格不一致
    PriceMismatch,
    /// 交易所存在但系统不存在
    MissingInSystem,
    /// 系统存在但交易所不存在
    MissingInExchange,
}

/// 持仓对账器
pub struct PositionReconciler {
    exchange: Arc<dyn Exchange>,
    position_repo: Arc<PositionRepository>,
    portfolio_manager: Arc<PortfolioManager>,
    last_reconciliation: Arc<Mutex<Option<ReconciliationResult>>>,
    tolerance_pct: Decimal,
}

impl PositionReconciler {
    /// 创建新的对账器
    pub fn new(
        exchange: Arc<dyn Exchange>,
        position_repo: Arc<PositionRepository>,
        portfolio_manager: Arc<PortfolioManager>,
    ) -> Self {
        Self {
            exchange,
            position_repo,
            portfolio_manager,
            last_reconciliation: Arc::new(Mutex::new(None)),
            tolerance_pct: Decimal::from(1) / Decimal::from(100), // 1% 容差
        }
    }

    /// 执行对账
    pub async fn reconcile(&self) -> Result<ReconciliationResult, ReconciliationError> {
        info!("Starting position reconciliation...");

        // 1. 从交易所获取持仓
        let exchange_positions = self
            .exchange
            .get_positions()
            .await
            .map_err(|e| ReconciliationError::ExchangeError(e.to_string()))?;

        let mut exchange_map: HashMap<String, PositionInfo> = HashMap::new();
        for pos in &exchange_positions {
            exchange_map.insert(pos.symbol.clone(), pos.clone());
        }

        // 2. 从系统获取持仓
        let system_positions = self.portfolio_manager.get_all_positions().await;
        let mut system_map: HashMap<String, PositionInfo> = HashMap::new();
        for pos in &system_positions {
            system_map.insert(
                pos.symbol.clone(),
                PositionInfo {
                    symbol: pos.symbol.clone(),
                    side: pos.side.clone(),
                    quantity: pos.quantity,
                    avg_entry_price: pos.avg_entry_price,
                    mark_price: Some(pos.current_price),
                    unrealized_pnl: pos.unrealized_pnl,
                    leverage: pos.leverage,
                    margin: pos.margin,
                    liquidation_price: None,
                },
            );
        }

        // 3. 检查差异
        let mut discrepancies = Vec::new();

        // 检查交易所存在但系统不存在的持仓
        for (symbol, exchange_pos) in &exchange_map {
            if !system_map.contains_key(symbol) {
                discrepancies.push(PositionDiscrepancy {
                    symbol: symbol.clone(),
                    discrepancy_type: DiscrepancyType::MissingInSystem,
                    exchange_value: exchange_pos.quantity,
                    system_value: Decimal::ZERO,
                    difference: exchange_pos.quantity,
                });
            }
        }

        // 检查系统存在但交易所不存在的持仓
        for (symbol, system_pos) in &system_map {
            if !exchange_map.contains_key(symbol) {
                discrepancies.push(PositionDiscrepancy {
                    symbol: symbol.clone(),
                    discrepancy_type: DiscrepancyType::MissingInExchange,
                    exchange_value: Decimal::ZERO,
                    system_value: system_pos.quantity,
                    difference: system_pos.quantity,
                });
            }
        }

        // 检查数量和价格不一致
        for (symbol, exchange_pos) in &exchange_map {
            if let Some(system_pos) = system_map.get(symbol) {
                // 检查数量差异
                let quantity_diff = (exchange_pos.quantity - system_pos.quantity).abs();
                let max_quantity = exchange_pos.quantity.max(system_pos.quantity);

                if max_quantity > Decimal::ZERO {
                    let quantity_diff_pct = quantity_diff / max_quantity;
                    if quantity_diff_pct > self.tolerance_pct {
                        discrepancies.push(PositionDiscrepancy {
                            symbol: symbol.clone(),
                            discrepancy_type: DiscrepancyType::QuantityMismatch,
                            exchange_value: exchange_pos.quantity,
                            system_value: system_pos.quantity,
                            difference: quantity_diff,
                        });
                    }
                }

                // 检查价格差异
                if let Some(mark_price) = exchange_pos.mark_price {
                    let price_diff = (mark_price - system_pos.avg_entry_price).abs();
                    let price_diff_pct = if system_pos.avg_entry_price > Decimal::ZERO {
                        price_diff / system_pos.avg_entry_price
                    } else {
                        Decimal::ZERO
                    };

                    if price_diff_pct > self.tolerance_pct {
                        discrepancies.push(PositionDiscrepancy {
                            symbol: symbol.clone(),
                            discrepancy_type: DiscrepancyType::PriceMismatch,
                            exchange_value: mark_price,
                            system_value: system_pos.avg_entry_price,
                            difference: price_diff,
                        });
                    }
                }
            }
        }

        let is_consistent = discrepancies.is_empty();

        let result = ReconciliationResult {
            timestamp: Utc::now(),
            exchange_positions: exchange_map,
            system_positions: system_map,
            discrepancies,
            is_consistent,
        };

        // 保存对账结果
        let mut last_reconciliation = self.last_reconciliation.lock().await;
        *last_reconciliation = Some(result.clone());

        if is_consistent {
            info!("Position reconciliation passed - all positions consistent");
        } else {
            warn!(
                "Position reconciliation found {} discrepancies",
                result.discrepancies.len()
            );
            for discrepancy in &result.discrepancies {
                warn!(
                    "  - {}: {:?} (Exchange: {}, System: {})",
                    discrepancy.symbol,
                    discrepancy.discrepancy_type,
                    discrepancy.exchange_value,
                    discrepancy.system_value
                );
            }
        }

        Ok(result)
    }

    /// 获取最后对账结果
    pub async fn get_last_reconciliation(&self) -> Option<ReconciliationResult> {
        let last = self.last_reconciliation.lock().await;
        last.clone()
    }

    /// 自动修复差异 (谨慎使用)
    pub async fn auto_reconcile(&self) -> Result<(), ReconciliationError> {
        let result = self.reconcile().await?;

        if result.is_consistent {
            return Ok(());
        }

        warn!("Auto-reconciliation: syncing positions from exchange...");
        self.portfolio_manager.sync_positions().await?;

        Ok(())
    }
}

/// 对账错误
#[derive(Debug, thiserror::Error)]
pub enum ReconciliationError {
    #[error("Exchange error: {0}")]
    ExchangeError(String),

    #[error("Sync error: {0}")]
    SyncError(String),

    #[error("Database error: {0}")]
    DatabaseError(String),

    #[error("Portfolio error: {0}")]
    PortfolioError(String),
}

impl From<super::manager::PortfolioError> for ReconciliationError {
    fn from(err: super::manager::PortfolioError) -> Self {
        ReconciliationError::PortfolioError(err.to_string())
    }
}
