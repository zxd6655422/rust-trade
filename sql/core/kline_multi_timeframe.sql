-- =================================================================
-- 多时间框架 K 线表
-- 用于完整的多时间框架分析支持
-- 创建时间：2026-07-08
-- =================================================================

-- 5分钟K线表
CREATE TABLE IF NOT EXISTS kline_5m (
    symbol VARCHAR(20) NOT NULL,
    open_time TIMESTAMPTZ NOT NULL,
    open DECIMAL(20,8) NOT NULL,
    high DECIMAL(20,8) NOT NULL,
    low DECIMAL(20,8) NOT NULL,
    close DECIMAL(20,8) NOT NULL,
    volume DECIMAL(20,8) NOT NULL,
    trade_count INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (symbol, open_time)
);

CREATE INDEX IF NOT EXISTS idx_kline_5m_symbol_time ON kline_5m(symbol, open_time DESC);

-- 15分钟K线表
CREATE TABLE IF NOT EXISTS kline_15m (
    symbol VARCHAR(20) NOT NULL,
    open_time TIMESTAMPTZ NOT NULL,
    open DECIMAL(20,8) NOT NULL,
    high DECIMAL(20,8) NOT NULL,
    low DECIMAL(20,8) NOT NULL,
    close DECIMAL(20,8) NOT NULL,
    volume DECIMAL(20,8) NOT NULL,
    trade_count INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (symbol, open_time)
);

CREATE INDEX IF NOT EXISTS idx_kline_15m_symbol_time ON kline_15m(symbol, open_time DESC);

-- 30分钟K线表
CREATE TABLE IF NOT EXISTS kline_30m (
    symbol VARCHAR(20) NOT NULL,
    open_time TIMESTAMPTZ NOT NULL,
    open DECIMAL(20,8) NOT NULL,
    high DECIMAL(20,8) NOT NULL,
    low DECIMAL(20,8) NOT NULL,
    close DECIMAL(20,8) NOT NULL,
    volume DECIMAL(20,8) NOT NULL,
    trade_count INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (symbol, open_time)
);

CREATE INDEX IF NOT EXISTS idx_kline_30m_symbol_time ON kline_30m(symbol, open_time DESC);

-- 1小时K线表
CREATE TABLE IF NOT EXISTS kline_1h (
    symbol VARCHAR(20) NOT NULL,
    open_time TIMESTAMPTZ NOT NULL,
    open DECIMAL(20,8) NOT NULL,
    high DECIMAL(20,8) NOT NULL,
    low DECIMAL(20,8) NOT NULL,
    close DECIMAL(20,8) NOT NULL,
    volume DECIMAL(20,8) NOT NULL,
    trade_count INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (symbol, open_time)
);

CREATE INDEX IF NOT EXISTS idx_kline_1h_symbol_time ON kline_1h(symbol, open_time DESC);

-- 2小时K线表
CREATE TABLE IF NOT EXISTS kline_2h (
    symbol VARCHAR(20) NOT NULL,
    open_time TIMESTAMPTZ NOT NULL,
    open DECIMAL(20,8) NOT NULL,
    high DECIMAL(20,8) NOT NULL,
    low DECIMAL(20,8) NOT NULL,
    close DECIMAL(20,8) NOT NULL,
    volume DECIMAL(20,8) NOT NULL,
    trade_count INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (symbol, open_time)
);

CREATE INDEX IF NOT EXISTS idx_kline_2h_symbol_time ON kline_2h(symbol, open_time DESC);

-- =================================================================
-- 说明：
-- 1. 本文件创建 5m/15m/30m/1h/2h 五张高时间框架 K 线表
-- 2. 已有的表（无需在此创建）：
--    - kline_1m（1分钟，主数据源）
--    - kline_4h（4小时）
--    - kline_1d（日线）
--    - kline_3d（3日线）
--    - kline_1w（周线）
-- 3. Rust 代码中 get_high_tf_table_name() 已支持全部时间框架映射
-- 4. 按需聚合框架（3m/45m/6h/8h/12h）不创建数据库表，仅在 Redis 中聚合
-- =================================================================
