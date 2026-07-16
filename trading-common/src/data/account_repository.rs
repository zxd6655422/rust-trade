// data/account_repository.rs
//
// 账户快照数据仓库
// 负责账户快照、资产余额、持仓数据的持久化和查询
// 支持按 uid（交易所返回的用户唯一标识）过滤

use chrono::{DateTime, Utc};
use rust_decimal::Decimal;
use sqlx::{PgPool, postgres::PgRow, Row};
use tracing::{debug, error, info};

use super::account_types::{AccountSnapshot, AssetBalance, MarginType, PositionInfo, PositionSide};
use super::types::DataResult;

/// 账户数据仓库
pub struct AccountRepository {
    pool: PgPool,
}

impl AccountRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    // =================================================================
    // 写入方法
    // =================================================================

    /// 保存账户快照（带 uid）
    pub async fn save_snapshot(&self, snapshot: &AccountSnapshot, uid: &str) -> DataResult<()> {
        sqlx::query(
            "INSERT INTO account_snapshot \
             (exchange, market_type, uid, snapshot_at, total_equity, total_balance, \
              available_balance, frozen_balance, unrealized_pnl, initial_margin, \
              maint_margin, margin_ratio, position_count) \
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13) \
             ON CONFLICT (exchange, market_type, uid, snapshot_at) DO UPDATE SET \
              total_equity = EXCLUDED.total_equity, \
              total_balance = EXCLUDED.total_balance, \
              available_balance = EXCLUDED.available_balance, \
              frozen_balance = EXCLUDED.frozen_balance, \
              unrealized_pnl = EXCLUDED.unrealized_pnl, \
              initial_margin = EXCLUDED.initial_margin, \
              maint_margin = EXCLUDED.maint_margin, \
              margin_ratio = EXCLUDED.margin_ratio, \
              position_count = EXCLUDED.position_count"
        )
        .bind(&snapshot.exchange)
        .bind(&snapshot.market_type)
        .bind(uid)
        .bind(snapshot.snapshot_at)
        .bind(snapshot.total_equity)
        .bind(snapshot.total_balance)
        .bind(snapshot.available_balance)
        .bind(snapshot.frozen_balance)
        .bind(snapshot.unrealized_pnl)
        .bind(snapshot.initial_margin)
        .bind(snapshot.maint_margin)
        .bind(snapshot.margin_ratio)
        .bind(snapshot.position_count)
        .execute(&self.pool)
        .await?;

        debug!("Saved account snapshot: {} {} uid={}", snapshot.exchange, snapshot.market_type, uid);
        Ok(())
    }

    /// 批量保存资产余额（带 uid）
    pub async fn save_asset_balances(&self, balances: &[AssetBalance], uid: &str) -> DataResult<()> {
        if balances.is_empty() {
            return Ok(());
        }

        let mut tx = self.pool.begin().await?;

        for balance in balances {
            sqlx::query(
                "INSERT INTO asset_balance \
                 (exchange, market_type, uid, asset, snapshot_at, total, available, frozen, unrealized_pnl, usd_value) \
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10) \
                 ON CONFLICT (exchange, market_type, uid, asset, snapshot_at) DO UPDATE SET \
                  total = EXCLUDED.total, \
                  available = EXCLUDED.available, \
                  frozen = EXCLUDED.frozen, \
                  unrealized_pnl = EXCLUDED.unrealized_pnl, \
                  usd_value = EXCLUDED.usd_value"
            )
            .bind(&balance.exchange)
            .bind(&balance.market_type)
            .bind(uid)
            .bind(&balance.asset)
            .bind(balance.snapshot_at)
            .bind(balance.total)
            .bind(balance.available)
            .bind(balance.frozen)
            .bind(balance.unrealized_pnl)
            .bind(balance.usd_value)
            .execute(&mut *tx)
            .await?;
        }

        tx.commit().await?;
        debug!("Saved {} asset balances uid={}", balances.len(), uid);
        Ok(())
    }

    /// 批量保存持仓快照（带 uid）
    pub async fn save_positions(&self, positions: &[PositionInfo], uid: &str) -> DataResult<()> {
        if positions.is_empty() {
            return Ok(());
        }

        let mut tx = self.pool.begin().await?;

        for pos in positions {
            let pnl_ratio = if pos.entry_price > Decimal::ZERO {
                Some(pos.unrealized_pnl / (pos.entry_price * pos.position_amt.abs()))
            } else {
                None
            };

            sqlx::query(
                "INSERT INTO position_snapshot \
                 (exchange, market_type, uid, symbol, raw_symbol, snapshot_at, \
                  position_side, position_amt, entry_price, mark_price, unrealized_pnl, \
                  leverage, margin_type, initial_margin, maint_margin, \
                  liquidation_price, notional, break_even_price, isolated_wallet, \
                  pnl_ratio) \
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16, $17, $18, $19, $20) \
                 ON CONFLICT (exchange, symbol, position_side, uid, snapshot_at) DO UPDATE SET \
                  market_type = EXCLUDED.market_type, \
                  raw_symbol = EXCLUDED.raw_symbol, \
                  position_amt = EXCLUDED.position_amt, \
                  entry_price = EXCLUDED.entry_price, \
                  mark_price = EXCLUDED.mark_price, \
                  unrealized_pnl = EXCLUDED.unrealized_pnl, \
                  leverage = EXCLUDED.leverage, \
                  margin_type = EXCLUDED.margin_type, \
                  initial_margin = EXCLUDED.initial_margin, \
                  maint_margin = EXCLUDED.maint_margin, \
                  liquidation_price = EXCLUDED.liquidation_price, \
                  notional = EXCLUDED.notional, \
                  break_even_price = EXCLUDED.break_even_price, \
                  isolated_wallet = EXCLUDED.isolated_wallet, \
                  pnl_ratio = EXCLUDED.pnl_ratio"
            )
            .bind(&pos.exchange)
            .bind("futures") // 持仓主要是合约
            .bind(uid)
            .bind(&pos.symbol)
            .bind(&pos.raw_symbol)
            .bind(pos.snapshot_at)
            .bind(pos.position_side.as_str())
            .bind(pos.position_amt)
            .bind(pos.entry_price)
            .bind(pos.mark_price)
            .bind(pos.unrealized_pnl)
            .bind(pos.leverage as i32)
            .bind(pos.margin_type.as_str())
            .bind(pos.initial_margin)
            .bind(pos.maint_margin)
            .bind(pos.liquidation_price)
            .bind(pos.notional)
            .bind(pos.break_even_price)
            .bind(pos.isolated_wallet)
            .bind(pnl_ratio)
            .execute(&mut *tx)
            .await?;
        }

        tx.commit().await?;
        debug!("Saved {} positions uid={}", positions.len(), uid);
        Ok(())
    }

    // =================================================================
    // 查询方法（uid 可选）
    // =================================================================

    /// 获取所有交易所的最新账户快照
    /// uid 为 None 时返回所有用户的最新快照
    pub async fn get_latest_snapshots(&self, uid: Option<&str>) -> DataResult<Vec<AccountSnapshot>> {
        let rows = match uid {
            Some(uid) => {
                sqlx::query(
                    "SELECT DISTINCT ON (exchange, market_type) * \
                     FROM account_snapshot \
                     WHERE uid = $1 \
                     ORDER BY exchange, market_type, snapshot_at DESC"
                )
                .bind(uid)
                .fetch_all(&self.pool)
                .await?
            }
            None => {
                sqlx::query(
                    "SELECT DISTINCT ON (exchange, market_type, uid) * \
                     FROM account_snapshot \
                     ORDER BY exchange, market_type, uid, snapshot_at DESC"
                )
                .fetch_all(&self.pool)
                .await?
            }
        };

        Ok(rows.iter().map(|r| Self::map_snapshot(r)).collect())
    }

    /// 获取指定交易所的最新资产余额
    pub async fn get_latest_balances(
        &self,
        exchange: &str,
        market_type: &str,
        uid: Option<&str>,
    ) -> DataResult<Vec<AssetBalance>> {
        let latest_time: Option<DateTime<Utc>> = match uid {
            Some(uid) => sqlx::query_scalar(
                "SELECT MAX(snapshot_at) FROM asset_balance \
                 WHERE exchange = $1 AND market_type = $2 AND uid = $3"
            )
            .bind(exchange)
            .bind(market_type)
            .bind(uid)
            .fetch_one(&self.pool)
            .await?,
            None => sqlx::query_scalar(
                "SELECT MAX(snapshot_at) FROM asset_balance \
                 WHERE exchange = $1 AND market_type = $2"
            )
            .bind(exchange)
            .bind(market_type)
            .fetch_one(&self.pool)
            .await?,
        };

        let snapshot_at = match latest_time {
            Some(t) => t,
            None => return Ok(Vec::new()),
        };

        let rows = match uid {
            Some(uid) => sqlx::query(
                "SELECT * FROM asset_balance \
                 WHERE exchange = $1 AND market_type = $2 AND uid = $3 AND snapshot_at = $4 \
                 ORDER BY asset"
            )
            .bind(exchange)
            .bind(market_type)
            .bind(uid)
            .bind(snapshot_at)
            .fetch_all(&self.pool)
            .await?,
            None => sqlx::query(
                "SELECT * FROM asset_balance \
                 WHERE exchange = $1 AND market_type = $2 AND snapshot_at = $3 \
                 ORDER BY asset"
            )
            .bind(exchange)
            .bind(market_type)
            .bind(snapshot_at)
            .fetch_all(&self.pool)
            .await?,
        };

        Ok(rows.iter().map(|r| Self::map_asset_balance(r)).collect())
    }

    /// 获取指定交易所的最新持仓
    pub async fn get_latest_positions(
        &self,
        exchange: &str,
        uid: Option<&str>,
    ) -> DataResult<Vec<PositionInfo>> {
        let latest_time: Option<DateTime<Utc>> = match uid {
            Some(uid) => sqlx::query_scalar(
                "SELECT MAX(snapshot_at) FROM position_snapshot \
                 WHERE exchange = $1 AND uid = $2"
            )
            .bind(exchange)
            .bind(uid)
            .fetch_one(&self.pool)
            .await?,
            None => sqlx::query_scalar(
                "SELECT MAX(snapshot_at) FROM position_snapshot \
                 WHERE exchange = $1"
            )
            .bind(exchange)
            .fetch_one(&self.pool)
            .await?,
        };

        let snapshot_at = match latest_time {
            Some(t) => t,
            None => return Ok(Vec::new()),
        };

        let rows = match uid {
            Some(uid) => sqlx::query(
                "SELECT * FROM position_snapshot \
                 WHERE exchange = $1 AND uid = $2 AND snapshot_at = $3 \
                 ORDER BY symbol"
            )
            .bind(exchange)
            .bind(uid)
            .bind(snapshot_at)
            .fetch_all(&self.pool)
            .await?,
            None => sqlx::query(
                "SELECT * FROM position_snapshot \
                 WHERE exchange = $1 AND snapshot_at = $2 \
                 ORDER BY symbol"
            )
            .bind(exchange)
            .bind(snapshot_at)
            .fetch_all(&self.pool)
            .await?,
        };

        Ok(rows.iter().map(|r| Self::map_position(r)).collect())
    }

    /// 查询历史快照（用于前端图表）
    pub async fn get_snapshot_history(
        &self,
        exchange: &str,
        market_type: &str,
        uid: Option<&str>,
        start: DateTime<Utc>,
        end: DateTime<Utc>,
    ) -> DataResult<Vec<AccountSnapshot>> {
        let rows = match uid {
            Some(uid) => sqlx::query(
                "SELECT * FROM account_snapshot \
                 WHERE exchange = $1 AND market_type = $2 AND uid = $3 \
                   AND snapshot_at BETWEEN $4 AND $5 \
                 ORDER BY snapshot_at"
            )
            .bind(exchange)
            .bind(market_type)
            .bind(uid)
            .bind(start)
            .bind(end)
            .fetch_all(&self.pool)
            .await?,
            None => sqlx::query(
                "SELECT * FROM account_snapshot \
                 WHERE exchange = $1 AND market_type = $2 \
                   AND snapshot_at BETWEEN $3 AND $4 \
                 ORDER BY snapshot_at"
            )
            .bind(exchange)
            .bind(market_type)
            .bind(start)
            .bind(end)
            .fetch_all(&self.pool)
            .await?,
        };

        Ok(rows.iter().map(|r| Self::map_snapshot(r)).collect())
    }

    /// 获取所有已知 uid 列表
    pub async fn get_known_uids(&self) -> DataResult<Vec<String>> {
        let rows: Vec<(String,)> = sqlx::query_as(
            "SELECT DISTINCT uid FROM account_snapshot WHERE uid IS NOT NULL ORDER BY uid"
        )
        .fetch_all(&self.pool)
        .await?;

        Ok(rows.into_iter().map(|(uid,)| uid).collect())
    }

    // =================================================================
    // 行映射辅助方法
    // =================================================================

    fn map_snapshot(r: &PgRow) -> AccountSnapshot {
        AccountSnapshot {
            exchange: r.get("exchange"),
            market_type: r.get("market_type"),
            uid: r.get("uid"),
            snapshot_at: r.get("snapshot_at"),
            total_equity: r.get("total_equity"),
            total_balance: r.get("total_balance"),
            available_balance: r.get("available_balance"),
            frozen_balance: r.get("frozen_balance"),
            unrealized_pnl: r.get("unrealized_pnl"),
            initial_margin: r.get("initial_margin"),
            maint_margin: r.get("maint_margin"),
            margin_ratio: r.get("margin_ratio"),
            position_count: r.get("position_count"),
        }
    }

    fn map_asset_balance(r: &PgRow) -> AssetBalance {
        AssetBalance {
            exchange: r.get("exchange"),
            market_type: r.get("market_type"),
            uid: r.get("uid"),
            asset: r.get("asset"),
            snapshot_at: r.get("snapshot_at"),
            total: r.get("total"),
            available: r.get("available"),
            frozen: r.get("frozen"),
            unrealized_pnl: r.get("unrealized_pnl"),
            usd_value: r.get("usd_value"),
        }
    }

    fn map_position(r: &PgRow) -> PositionInfo {
        PositionInfo {
            exchange: r.get("exchange"),
            uid: r.get("uid"),
            symbol: r.get("symbol"),
            raw_symbol: r.get("raw_symbol"),
            snapshot_at: r.get("snapshot_at"),
            position_side: PositionSide::from_str(r.get::<&str, _>("position_side")),
            position_amt: r.get("position_amt"),
            entry_price: r.get("entry_price"),
            mark_price: r.get("mark_price"),
            unrealized_pnl: r.get("unrealized_pnl"),
            leverage: r.get::<i32, _>("leverage") as u32,
            margin_type: MarginType::from_str(r.get::<&str, _>("margin_type")),
            initial_margin: r.get("initial_margin"),
            maint_margin: r.get("maint_margin"),
            liquidation_price: r.get("liquidation_price"),
            notional: r.get("notional"),
            break_even_price: r.get("break_even_price"),
            isolated_wallet: r.get("isolated_wallet"),
        }
    }
}
