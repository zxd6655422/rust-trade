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
use crate::risk::RiskEngine;
use crate::storage::{PositionRepository, RedisCache};

/// 持仓快照
#[derive(Debug, Clone)]
pub struct PositionSnapshot {
    pub symbol: String,
    pub exchange_id: String,
    pub market_type: String,
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
///
/// 职责：
/// 1. 从交易所同步持仓数据
/// 2. 更新持仓价格和未实现盈亏
/// 3. 持久化到数据库和 Redis 缓存（供前端展示）
/// 4. 同步持仓到 RiskEngine（供风控计算）
pub struct PortfolioManager {
    exchange: Arc<dyn Exchange>,
    exchange_id: String,
    market_type: String,
    position_repo: Arc<PositionRepository>,
    cache: Arc<RedisCache>,
    risk_engine: Arc<RiskEngine>,
    /// 持仓 key 为 "unit_id:symbol"，如 "binance-futures:BTCUSDT"
    positions: Arc<Mutex<HashMap<String, PositionSnapshot>>>,
    last_sync_at: Arc<Mutex<Option<DateTime<Utc>>>>,
}

impl PortfolioManager {
    /// 创建新的持仓管理器
    pub fn new(
        exchange: Arc<dyn Exchange>,
        position_repo: Arc<PositionRepository>,
        cache: Arc<RedisCache>,
        risk_engine: Arc<RiskEngine>,
    ) -> Self {
        Self {
            exchange,
            exchange_id: "unknown".to_string(),
            market_type: "futures".to_string(),
            position_repo,
            cache,
            risk_engine,
            positions: Arc::new(Mutex::new(HashMap::new())),
            last_sync_at: Arc::new(Mutex::new(None)),
        }
    }

    /// 创建带标识的持仓管理器
    pub fn with_identity(
        exchange: Arc<dyn Exchange>,
        position_repo: Arc<PositionRepository>,
        cache: Arc<RedisCache>,
        risk_engine: Arc<RiskEngine>,
        exchange_id: String,
        market_type: String,
    ) -> Self {
        Self {
            exchange,
            exchange_id,
            market_type,
            position_repo,
            cache,
            risk_engine,
            positions: Arc::new(Mutex::new(HashMap::new())),
            last_sync_at: Arc::new(Mutex::new(None)),
        }
    }

    /// 从交易所同步持仓
    pub async fn sync_positions(&self) -> Result<usize, PortfolioError> {
        info!("Syncing positions from {} {}...", self.exchange_id, self.market_type);

        // 从交易所获取持仓
        let exchange_positions = self
            .exchange
            .get_positions()
            .await
            .map_err(|e| PortfolioError::SyncError(e.to_string()))?;

        let mut positions = self.positions.lock().await;
        positions.clear();

        for pos in &exchange_positions {
            let unit_key = format!("{}:{}", self.exchange_id, pos.symbol);

            let snapshot = PositionSnapshot {
                symbol: pos.symbol.clone(),
                exchange_id: self.exchange_id.clone(),
                market_type: self.market_type.clone(),
                side: pos.side.clone(),
                quantity: pos.quantity,
                avg_entry_price: pos.avg_entry_price,
                current_price: pos.mark_price.unwrap_or(pos.avg_entry_price),
                unrealized_pnl: pos.unrealized_pnl,
                leverage: pos.leverage,
                margin: pos.margin,
                updated_at: Utc::now(),
            };

            positions.insert(unit_key, snapshot);

            // 保存到数据库（使用实际的 exchange 和 market_type）
            if let Err(e) = self
                .position_repo
                .upsert_position(
                    &self.exchange_id,
                    &self.market_type,
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
                .set_position(&self.exchange_id, &pos.symbol, pos.quantity, pos.avg_entry_price)
                .await
            {
                warn!("Failed to cache position: {}", e);
            }
        }

        // 更新同步时间
        let mut last_sync = self.last_sync_at.lock().await;
        *last_sync = Some(Utc::now());

        // 同步持仓到风控引擎（使用 unit_id 前缀区分不同交易所）
        self.risk_engine.sync_positions_from_unit(
            &self.exchange_id,
            &self.market_type,
            &*positions,
        ).await;

        info!("Synced {} positions from {} {}", exchange_positions.len(), self.exchange_id, self.market_type);
        Ok(exchange_positions.len())
    }

    /// 更新持仓价格
    pub async fn update_price(&self, symbol: &str, current_price: Decimal) {
        let unit_key = format!("{}:{}", self.exchange_id, symbol);
        let mut positions = self.positions.lock().await;

        if let Some(position) = positions.get_mut(&unit_key) {
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

    /// 获取指定交易对的持仓（使用 unit_id:symbol 作为 key）
    pub async fn get_position(&self, symbol: &str) -> Option<PositionSnapshot> {
        let unit_key = format!("{}:{}", self.exchange_id, symbol);
        let positions = self.positions.lock().await;
        positions.get(&unit_key).cloned()
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
