-- =================================================================
-- 账户余额快照表
-- 定时从交易所同步账户余额，用于降级查询
-- =================================================================

CREATE TABLE IF NOT EXISTS account_snapshot (
    id SERIAL PRIMARY KEY,

    -- 交易所名称（binance, okx）
    exchange VARCHAR(20) NOT NULL,

    -- 市场类型（spot, futures）
    market_type VARCHAR(10) NOT NULL CHECK (market_type IN ('spot', 'futures')),

    -- 可用余额（USDT）
    available_balance NUMERIC(20, 8) NOT NULL,

    -- 总余额（钱包余额 + 未实现盈亏）
    total_balance NUMERIC(20, 8) NOT NULL,

    -- 持仓数量
    position_count INT NOT NULL DEFAULT 0,

    -- 快照时间
    snapshot_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- 索引：按交易所+市场类型查最新快照
CREATE INDEX IF NOT EXISTS idx_snapshot_latest
    ON account_snapshot(exchange, market_type, snapshot_at DESC);
