-- =================================================================
-- Trading System Database Schema V4
-- 交易对管理增强
-- =================================================================

-- =================================================================
-- 1. 交易对配置表 (trading_pairs)
-- 用途：管理所有交易对的配置，包括现货/合约
-- =================================================================

CREATE TABLE IF NOT EXISTS trading_pairs (
    -- 【ID】自增主键
    id SERIAL PRIMARY KEY,

    -- 【交易对】如 'BTCUSDT'
    symbol VARCHAR(20) NOT NULL UNIQUE,

    -- 【市场类型】spot(现货) / futures(合约)
    market_type VARCHAR(10) NOT NULL CHECK (market_type IN ('spot', 'futures')),

    -- 【交易所】binance / okx
    exchange VARCHAR(20) NOT NULL DEFAULT 'binance',

    -- 【状态】active(启用) / paused(暂停) / archived(归档)
    status VARCHAR(20) NOT NULL DEFAULT 'active'
        CHECK (status IN ('active', 'paused', 'archived')),

    -- 【备注】
    note TEXT,

    -- 【创建时间】
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    -- 【更新时间】
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- 索引
CREATE INDEX IF NOT EXISTS idx_trading_pairs_symbol ON trading_pairs(symbol);
CREATE INDEX IF NOT EXISTS idx_trading_pairs_status ON trading_pairs(status);
CREATE INDEX IF NOT EXISTS idx_trading_pairs_market ON trading_pairs(market_type);

-- =================================================================
-- 2. 初始化默认交易对
-- =================================================================

INSERT INTO trading_pairs (symbol, market_type, exchange, status) VALUES
    ('BTCUSDT', 'futures', 'binance', 'active'),
    ('ETHUSDT', 'futures', 'binance', 'active'),
    ('SOLUSDT', 'futures', 'binance', 'active'),
    ('SUIUSDT', 'futures', 'binance', 'active'),
    ('BNBUSDT', 'futures', 'binance', 'active')
ON CONFLICT (symbol) DO NOTHING;

-- =================================================================
-- 3. 状态说明
-- =================================================================
-- active:   正常采集，策略分析
-- paused:   暂停采集，保留历史数据，可随时恢复
-- archived: 归档删除，不再采集，历史数据可选保留

-- 完成
DO $$
BEGIN
    RAISE NOTICE '========================================';
    RAISE NOTICE 'Schema V4 迁移完成';
    RAISE NOTICE '  trading_pairs (交易对配置)';
    RAISE NOTICE '========================================';
END $$;
