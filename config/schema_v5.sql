-- =================================================================
-- Trading System Database Schema V5
-- 交易所和市场类型支持
-- =================================================================

-- =================================================================
-- 1. 系统配置表
-- =================================================================

CREATE TABLE IF NOT EXISTS system_config (
    key VARCHAR(50) PRIMARY KEY,
    value TEXT NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- 初始化调度器状态
INSERT INTO system_config (key, value) VALUES ('scheduler_paused', 'false')
ON CONFLICT (key) DO NOTHING;

-- =================================================================
-- 2. 为 trades 表添加交易所和市场类型字段
-- =================================================================

-- 添加交易所字段
ALTER TABLE trades ADD COLUMN IF NOT EXISTS exchange VARCHAR(20) NOT NULL DEFAULT 'binance';

-- 添加市场类型字段
ALTER TABLE trades ADD COLUMN IF NOT EXISTS market_type VARCHAR(10) NOT NULL DEFAULT 'futures'
    CHECK (market_type IN ('spot', 'futures'));

-- 添加索引
CREATE INDEX IF NOT EXISTS idx_trades_exchange ON trades(exchange);
CREATE INDEX IF NOT EXISTS idx_trades_market_type ON trades(market_type);
CREATE INDEX IF NOT EXISTS idx_trades_exchange_symbol ON trades(exchange, symbol);

-- =================================================================
-- 2. 为 positions 表添加交易所和市场类型字段
-- =================================================================

-- 添加交易所字段
ALTER TABLE positions ADD COLUMN IF NOT EXISTS exchange VARCHAR(20) NOT NULL DEFAULT 'binance';

-- 添加市场类型字段
ALTER TABLE positions ADD COLUMN IF NOT EXISTS market_type VARCHAR(10) NOT NULL DEFAULT 'futures'
    CHECK (market_type IN ('spot', 'futures'));

-- 添加索引
CREATE INDEX IF NOT EXISTS idx_positions_exchange ON positions(exchange);
CREATE INDEX IF NOT EXISTS idx_positions_market_type ON positions(market_type);

-- =================================================================
-- 3. 完成
-- =================================================================

DO $$
BEGIN
    RAISE NOTICE '========================================';
    RAISE NOTICE 'Schema V5 迁移完成';
    RAISE NOTICE '  - trades 表添加 exchange, market_type';
    RAISE NOTICE '  - positions 表添加 exchange, market_type';
    RAISE NOTICE '========================================';
END $$;
