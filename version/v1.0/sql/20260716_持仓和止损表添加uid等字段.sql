-- =================================================================
-- positions 和 stop_orders 表添加 uid 等字段
-- 支持多用户场景
-- 创建时间：2026-07-16
-- =================================================================

-- positions 表添加新字段
ALTER TABLE positions ADD COLUMN IF NOT EXISTS uid VARCHAR(20);
ALTER TABLE positions ADD COLUMN IF NOT EXISTS leverage INTEGER DEFAULT 1;
ALTER TABLE positions ADD COLUMN IF NOT EXISTS margin_type VARCHAR(10) DEFAULT 'cross';
ALTER TABLE positions ADD COLUMN IF NOT EXISTS liquidation_price DECIMAL(20, 8);

-- positions 表添加索引
CREATE INDEX IF NOT EXISTS idx_positions_uid ON positions(uid);
CREATE INDEX IF NOT EXISTS idx_positions_updated ON positions(updated_at DESC);

-- stop_orders 表添加 uid 字段
ALTER TABLE stop_orders ADD COLUMN IF NOT EXISTS uid VARCHAR(20);

-- stop_orders 表添加索引
CREATE INDEX IF NOT EXISTS idx_stop_orders_uid ON stop_orders(uid);
