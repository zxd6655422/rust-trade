-- =================================================================
-- 账户快照表添加 uid 字段
-- uid 来自交易所 API 返回的用户唯一标识
-- Binance Spot: /api/v3/account 返回的 uid (int64)
-- OKX: /api/v5/account/config 返回的 uid (string)
-- 创建时间：2026-07-15
-- =================================================================

-- account_snapshot 添加 uid 列
ALTER TABLE account_snapshot ADD COLUMN IF NOT EXISTS uid VARCHAR(50);
CREATE INDEX IF NOT EXISTS idx_account_snapshot_uid
    ON account_snapshot(uid, exchange, market_type, snapshot_at DESC);

-- asset_balance 添加 uid 列
ALTER TABLE asset_balance ADD COLUMN IF NOT EXISTS uid VARCHAR(50);
CREATE INDEX IF NOT EXISTS idx_asset_balance_uid
    ON asset_balance(uid, exchange, market_type, snapshot_at DESC);

-- position_snapshot 添加 uid 列
ALTER TABLE position_snapshot ADD COLUMN IF NOT EXISTS uid VARCHAR(50);
CREATE INDEX IF NOT EXISTS idx_position_snapshot_uid
    ON position_snapshot(uid, exchange, snapshot_at DESC);

-- trading_orders 添加 market_type 和 uid 列
ALTER TABLE trading_orders ADD COLUMN IF NOT EXISTS market_type VARCHAR(10) NOT NULL DEFAULT 'futures';
ALTER TABLE trading_orders ADD COLUMN IF NOT EXISTS uid VARCHAR(50);
CREATE INDEX IF NOT EXISTS idx_orders_market_type ON trading_orders(market_type);
CREATE INDEX IF NOT EXISTS idx_orders_uid ON trading_orders(uid);
