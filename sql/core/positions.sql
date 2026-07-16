-- =================================================================
-- positions 表结构
-- 实时持仓信息（用于交易引擎）
-- 更新时间：2026-07-16
-- =================================================================

CREATE TABLE IF NOT EXISTS positions (
    id UUID DEFAULT gen_random_uuid() PRIMARY KEY,
    exchange VARCHAR(20) NOT NULL DEFAULT 'binance',    -- 'binance' / 'okx'
    market_type VARCHAR(10) NOT NULL DEFAULT 'futures',  -- 'spot' / 'futures'
    uid VARCHAR(20),                            -- 用户标识（API Key 前缀）
    symbol VARCHAR(20) NOT NULL,                -- 交易对
    side VARCHAR(10) NOT NULL,                  -- 'LONG' / 'SHORT'
    quantity DECIMAL(20, 8) NOT NULL,           -- 持仓数量
    avg_entry_price DECIMAL(20, 8) NOT NULL,    -- 开仓均价
    current_price DECIMAL(20, 8),               -- 当前价格
    unrealized_pnl DECIMAL(20, 8),              -- 未实现盈亏
    realized_pnl DECIMAL(20, 8) DEFAULT 0 NOT NULL,  -- 已实现盈亏
    leverage INTEGER DEFAULT 1,                 -- 杠杆倍数
    margin_type VARCHAR(10) DEFAULT 'cross',    -- 'cross' / 'isolated'
    liquidation_price DECIMAL(20, 8),           -- 强平价格
    opened_at TIMESTAMPTZ NOT NULL,             -- 开仓时间
    updated_at TIMESTAMPTZ DEFAULT NOW() NOT NULL,

    CONSTRAINT positions_market_type_check CHECK (market_type IN ('spot', 'futures')),
    CONSTRAINT positions_side_check CHECK (side IN ('LONG', 'SHORT'))
);

-- 查询索引
CREATE INDEX IF NOT EXISTS idx_positions_exchange ON positions(exchange);
CREATE INDEX IF NOT EXISTS idx_positions_market_type ON positions(market_type);
CREATE INDEX IF NOT EXISTS idx_positions_uid ON positions(uid);
CREATE INDEX IF NOT EXISTS idx_positions_symbol ON positions(symbol);
CREATE INDEX IF NOT EXISTS idx_positions_updated ON positions(updated_at DESC);
