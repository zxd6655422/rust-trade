// portfolio/manager.rs
// 持仓管理器

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{debug, info, warn};

use crate::exchange::traits::Exchange;
use crate::exchange::types::PositionSide;
use crate::storage::{PositionRepository, RedisCache};

/// 持仓快照
#[derive(Debug, Clone)]
pub struct PositionSnapshot {
    pub symbol: String,
    pub side: PositionSide,
    pub quantity: Decimal,
    pub avg_entry_price: Decimal,
    pub current_price: Decimal,
    pub unrealized_pnl: Decimal,
    pub leverage: u32,
    pub margin: Decimal,
    pub updated_at: DateTime<Utc>,
}

/// 持仓管理器
pub struct PortfolioManager {
    exchange: Arc<dyn Exchange>,
    position_repo: Arc<PositionRepository>,
    cache: Arc<RedisCache>,
    positions: Arc<Mutex<HashMap<String, PositionSnapshot>>>,
    last_sync_at: Arc<Mutex<Option<DateTime<Utc>>>>,
}

impl PortfolioManager {
    /// 创建新的持仓管理器
    pub fn new(
        exchange: Arc<dyn Exchange>,
        position_repo: Arc<PositionRepository>,
        cache: Arc<RedisCache>,
    ) -> Self {
        Self {
            exchange,
            position_repo,
            cache,
            positions: Arc::new(Mutex::new(HashMap::new())),
            last_sync_at: Arc::new(Mutex::new(None)),
        }
    }

    /// 从交易所同步持仓
    pub async fn sync_positions(&self) -> Result<usize, PortfolioError> {
        info!("Syncing positions from exchange...");

        // 从交易所获取持仓
        let exchange_positions = self
            .exchange
            .get_positions()
            .await
            .map_err(|e| PortfolioError::SyncError(e.to_string()))?;

        let mut positions = self.positions.lock().await;
        positions.clear();

        for pos in &exchange_positions {
            let snapshot = PositionSnapshot {
                symbol: pos.symbol.clone(),
                side: pos.side.clone(),
                quantity: pos.quantity,
                avg_entry_price: pos.avg_entry_price,
                current_price: pos.mark_price.unwrap_or(pos.avg_entry_price),
                unrealized_pnl: pos.unrealized_pnl,
                leverage: pos.leverage,
                margin: pos.margin,
                updated_at: Utc::now(),
            };

            positions.insert(pos.symbol.clone(), snapshot);

            // 保存到数据库
            if let Err(e) = self
                .position_repo
                .upsert_position(
                    "exchange", // 默认交易所标识
                    &pos.symbol,
                    &format!("{:?}", pos.side),
                    pos.quantity,
                    pos.avg_entry_price,
                )
                .await
            {
                warn!("Failed to save position to database: {}", e);
            }

            // 保存到 Redis 缓存
            if let Err(e) = self
                .cache
                .set_position("exchange", &pos.symbol, pos.quantity, pos.avg_entry_price)
                .await
            {
                warn!("Failed to cache position: {}", e);
            }
        }

        // 更新同步时间
        let mut last_sync = self.last_sync_at.lock().await;
        *last_sync = Some(Utc::now());

        info!("Synced {} positions from exchange", exchange_positions.len());
        Ok(exchange_positions.len())
    }

    /// 更新持仓价格
    pub async fn update_price(&self, symbol: &str, current_price: Decimal) {
        let mut positions = self.positions.lock().await;

        if let Some(position) = positions.get_mut(symbol) {
            position.current_price = current_price;

            // 重新计算未实现盈亏
            match position.side {
                PositionSide::Long => {
                    position.unrealized_pnl =
                        (current_price - position.avg_entry_price) * position.quantity;
                }
                PositionSide::Short => {
                    position.unrealized_pnl =
                        (position.avg_entry_price - current_price) * position.quantity;
                }
                PositionSide::None => {
                    position.unrealized_pnl = Decimal::ZERO;
                }
            }

            position.updated_at = Utc::now();

            debug!(
                "Position updated: {} | Price: {} | PnL: {}",
                symbol, current_price, position.unrealized_pnl
            );
        }
    }

    /// 获取所有持仓
    pub async fn get_all_positions(&self) -> Vec<PositionSnapshot> {
        let positions = self.positions.lock().await;
        positions.values().cloned().collect()
    }

    /// 获取指定交易对的持仓
    pub async fn get_position(&self, symbol: &str) -> Option<PositionSnapshot> {
        let positions = self.positions.lock().await;
        positions.get(symbol).cloned()
    }

    /// 获取总未实现盈亏
    pub async fn get_total_unrealized_pnl(&self) -> Decimal {
        let positions = self.positions.lock().await;
        positions.values().map(|p| p.unrealized_pnl).sum()
    }

    /// 获取总持仓价值
    pub async fn get_total_position_value(&self) -> Decimal {
        let positions = self.positions.lock().await;
        positions
            .values()
            .map(|p| p.current_price * p.quantity)
            .sum()
    }

    /// 获取最后同步时间
    pub async fn get_last_sync_time(&self) -> Option<DateTime<Utc>> {
        let last_sync = self.last_sync_at.lock().await;
        *last_sync
    }

    /// 检查是否需要重新同步 (超过 5 分钟)
    pub async fn needs_sync(&self) -> bool {
        let last_sync = self.last_sync_at.lock().await;
        match *last_sync {
            Some(last) => {
                let elapsed = Utc::now() - last;
                elapsed.num_seconds() > 300 // 5 分钟
            }
            None => true,
        }
    }
}

/// 持仓错误
#[derive(Debug, thiserror::Error)]
pub enum PortfolioError {
    #[error("Sync error: {0}")]
    SyncError(String),

    #[error("Position not found: {0}")]
    NotFound(String),

    #[error("Database error: {0}")]
    DatabaseError(String),
}
