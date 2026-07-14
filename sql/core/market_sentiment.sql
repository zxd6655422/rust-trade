-- =================================================================
-- 市场情绪数据表
-- 用于存储资金费率、持仓量、多空比
-- 创建时间：2026-07-08
-- =================================================================

-- 资金费率表
-- 每8小时结算一次，数据量极小
CREATE TABLE IF NOT EXISTS funding_rate (
    symbol VARCHAR(20) NOT NULL,
    funding_rate DECIMAL(10,8) NOT NULL,      -- 费率（如 0.0001 = 0.01%）
    funding_time TIMESTAMPTZ NOT NULL,         -- 结算时间
    mark_price DECIMAL(20,8),                  -- 标记价格
    PRIMARY KEY (symbol, funding_time)
);

CREATE INDEX IF NOT EXISTS idx_funding_rate_symbol_time ON funding_rate(symbol, funding_time DESC);

-- 持仓量表
-- 每分钟采集一次
CREATE TABLE IF NOT EXISTS open_interest (
    symbol VARCHAR(20) NOT NULL,
    open_interest DECIMAL(20,8) NOT NULL,      -- 未平仓合约数量
    open_value DECIMAL(20,8),                  -- 未平仓合约价值(USDT)
    timestamp TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (symbol, timestamp)
);

CREATE INDEX IF NOT EXISTS idx_open_interest_symbol_time ON open_interest(symbol, timestamp DESC);

-- 多空比表
-- 每5分钟采集一次
CREATE TABLE IF NOT EXISTS long_short_ratio (
    symbol VARCHAR(20) NOT NULL,
    long_ratio DECIMAL(10,8) NOT NULL,         -- 多头账户比例
    short_ratio DECIMAL(10,8) NOT NULL,        -- 空头账户比例
    ratio DECIMAL(10,8) NOT NULL,              -- 多空比（long/short）
    timestamp TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (symbol, timestamp)
);

CREATE INDEX IF NOT EXISTS idx_long_short_ratio_symbol_time ON long_short_ratio(symbol, timestamp DESC);
