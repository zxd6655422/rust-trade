-- =================================================================
-- 账户快照表结构优化（合并脚本）
-- 1. 添加 uid 字段
-- 2. 更新唯一约束（包含 uid）
-- 3. 删除未使用的 raw_data 字段
-- 4. 优化表空间
--
-- 执行方式: psql -U your_user -d your_db -f version/v1.0/sql/20260716_账户快照表结构优化.sql
-- 创建时间：2026-07-16
-- =================================================================

BEGIN;

-- ============ 1. 添加 uid 字段 ============

-- account_snapshot 添加 uid 列
ALTER TABLE account_snapshot ADD COLUMN IF NOT EXISTS uid VARCHAR(20);
CREATE INDEX IF NOT EXISTS idx_account_snapshot_uid
    ON account_snapshot(uid, exchange, market_type, snapshot_at DESC);

-- asset_balance 添加 uid 列
ALTER TABLE asset_balance ADD COLUMN IF NOT EXISTS uid VARCHAR(20);
CREATE INDEX IF NOT EXISTS idx_asset_balance_uid
    ON asset_balance(uid, exchange, market_type, snapshot_at DESC);

-- position_snapshot 添加 uid 列
ALTER TABLE position_snapshot ADD COLUMN IF NOT EXISTS uid VARCHAR(20);
CREATE INDEX IF NOT EXISTS idx_position_snapshot_uid
    ON position_snapshot(uid, exchange, snapshot_at DESC);

-- ============ 2. 更新唯一约束（包含 uid）============

-- account_snapshot: 删除旧约束，添加新约束
ALTER TABLE account_snapshot DROP CONSTRAINT IF EXISTS account_snapshot_exchange_market_type_snapshot_at_key;
ALTER TABLE account_snapshot ADD CONSTRAINT account_snapshot_uid_unique
    UNIQUE(exchange, market_type, uid, snapshot_at);

-- asset_balance: 删除旧约束，添加新约束
ALTER TABLE asset_balance DROP CONSTRAINT IF EXISTS asset_balance_exchange_market_type_asset_snapshot_at_key;
ALTER TABLE asset_balance ADD CONSTRAINT asset_balance_uid_unique
    UNIQUE(exchange, market_type, uid, asset, snapshot_at);

-- position_snapshot: 删除旧约束，添加新约束
ALTER TABLE position_snapshot DROP CONSTRAINT IF EXISTS position_snapshot_exchange_symbol_position_side_snapshot_at_key;
ALTER TABLE position_snapshot ADD CONSTRAINT position_snapshot_uid_unique
    UNIQUE(exchange, symbol, position_side, uid, snapshot_at);

-- ============ 3. 删除未使用的 raw_data 字段 ============

ALTER TABLE account_snapshot DROP COLUMN IF EXISTS raw_data;
ALTER TABLE position_snapshot DROP COLUMN IF EXISTS raw_data;

-- ============ 4. 优化表空间 ============

VACUUM FULL account_snapshot;
VACUUM FULL asset_balance;
VACUUM FULL position_snapshot;

COMMIT;

\echo 'Account snapshot tables optimized successfully!'
