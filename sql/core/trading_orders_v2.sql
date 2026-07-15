-- =================================================================
-- trading_orders 表结构升级
-- 添加 market_type, uid, position_side, source, signal_id, strategy_id
-- 创建时间：2026-07-15
-- =================================================================

-- 添加 market_type 列
ALTER TABLE trading_orders ADD COLUMN IF NOT EXISTS market_type VARCHAR(10) NOT NULL DEFAULT 'futures';
ALTER TABLE trading_orders ADD CONSTRAINT trading_orders_market_type_check
    CHECK (market_type IN ('spot', 'futures')) NOT VALID;

-- 添加 uid 列（交易所用户标识）
ALTER TABLE trading_orders ADD COLUMN IF NOT EXISTS uid VARCHAR(50);

-- 添加 position_side 列（合约持仓方向）
ALTER TABLE trading_orders ADD COLUMN IF NOT EXISTS position_side VARCHAR(10) DEFAULT 'BOTH';

-- 添加 source 列（订单来源）
ALTER TABLE trading_orders ADD COLUMN IF NOT EXISTS source VARCHAR(20) NOT NULL DEFAULT 'unknown';
-- 'auto' = 程序自动下单, 'manual' = 手动下单, 'unknown' = 未知

-- 添加 signal_id 列（关联策略信号）
ALTER TABLE trading_orders ADD COLUMN IF NOT EXISTS signal_id UUID;

-- 添加 strategy_id 列（关联策略）
ALTER TABLE trading_orders ADD COLUMN IF NOT EXISTS strategy_id VARCHAR(50);

-- 添加 time_in_force 列
ALTER TABLE trading_orders ADD COLUMN IF NOT EXISTS time_in_force VARCHAR(10) DEFAULT 'GTC';

-- 添加 stop_price 列（止损/止盈价格）
ALTER TABLE trading_orders ADD COLUMN IF NOT EXISTS stop_price DECIMAL(20, 8);

-- 创建索引
CREATE INDEX IF NOT EXISTS idx_orders_market_type ON trading_orders(market_type);
CREATE INDEX IF NOT EXISTS idx_orders_uid ON trading_orders(uid);
CREATE INDEX IF NOT EXISTS idx_orders_source ON trading_orders(source);
CREATE INDEX IF NOT EXISTS idx_orders_signal ON trading_orders(signal_id);
