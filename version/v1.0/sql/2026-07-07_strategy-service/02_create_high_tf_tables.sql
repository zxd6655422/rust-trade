-- =================================================================
-- 高时间框架 K 线表
-- 用于大周期分析（周K/3日K/日K/4小时K）
-- =================================================================

-- 4小时K线表
CREATE TABLE IF NOT EXISTS kline_4h (
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

CREATE INDEX IF NOT EXISTS idx_kline_4h_symbol_time ON kline_4h(symbol, open_time DESC);

-- 日K线表
CREATE TABLE IF NOT EXISTS kline_1d (
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

CREATE INDEX IF NOT EXISTS idx_kline_1d_symbol_time ON kline_1d(symbol, open_time DESC);

-- 3日K线表
CREATE TABLE IF NOT EXISTS kline_3d (
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

CREATE INDEX IF NOT EXISTS idx_kline_3d_symbol_time ON kline_3d(symbol, open_time DESC);

-- 周K线表
CREATE TABLE IF NOT EXISTS kline_1w (
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

CREATE INDEX IF NOT EXISTS idx_kline_1w_symbol_time ON kline_1w(symbol, open_time DESC);


-- =================================================================
-- 聚合函数：从 1m K线生成高时间框架 K线
-- =================================================================

-- 生成4小时K线
CREATE OR REPLACE FUNCTION aggregate_kline_4h(
    p_symbol VARCHAR(20),
    p_start_time TIMESTAMPTZ DEFAULT NULL,
    p_end_time TIMESTAMPTZ DEFAULT NULL
) RETURNS INTEGER AS $$
DECLARE
    inserted_count INTEGER := 0;
BEGIN
    INSERT INTO kline_4h (symbol, open_time, open, high, low, close, volume, trade_count)
    SELECT
        symbol,
        date_trunc('hour', timestamp) - ((EXTRACT(HOUR FROM timestamp)::int % 4) * INTERVAL '1 hour') as open_time,
        (array_agg(open ORDER BY timestamp))[1] as open,
        MAX(high) as high,
        MIN(low) as low,
        (array_agg(close ORDER BY timestamp DESC))[1] as close,
        SUM(volume) as volume,
        SUM(trade_count) as trade_count
    FROM kline_1m
    WHERE symbol = p_symbol
        AND (p_start_time IS NULL OR timestamp >= p_start_time)
        AND (p_end_time IS NULL OR timestamp < p_end_time)
    GROUP BY symbol, date_trunc('hour', timestamp) - ((EXTRACT(HOUR FROM timestamp)::int % 4) * INTERVAL '1 hour')
    ON CONFLICT (symbol, open_time) DO UPDATE SET
        high = GREATEST(kline_4h.high, EXCLUDED.high),
        low = LEAST(kline_4h.low, EXCLUDED.low),
        close = EXCLUDED.close,
        volume = kline_4h.volume + EXCLUDED.volume,
        trade_count = kline_4h.trade_count + EXCLUDED.trade_count;

    GET DIAGNOSTICS inserted_count = ROW_COUNT;
    RETURN inserted_count;
END;
$$ LANGUAGE plpgsql;

-- 生成日K线
CREATE OR REPLACE FUNCTION aggregate_kline_1d(
    p_symbol VARCHAR(20),
    p_start_time TIMESTAMPTZ DEFAULT NULL,
    p_end_time TIMESTAMPTZ DEFAULT NULL
) RETURNS INTEGER AS $$
DECLARE
    inserted_count INTEGER := 0;
BEGIN
    INSERT INTO kline_1d (symbol, open_time, open, high, low, close, volume, trade_count)
    SELECT
        symbol,
        date_trunc('day', timestamp) as open_time,
        (array_agg(open ORDER BY timestamp))[1] as open,
        MAX(high) as high,
        MIN(low) as low,
        (array_agg(close ORDER BY timestamp DESC))[1] as close,
        SUM(volume) as volume,
        SUM(trade_count) as trade_count
    FROM kline_1m
    WHERE symbol = p_symbol
        AND (p_start_time IS NULL OR timestamp >= p_start_time)
        AND (p_end_time IS NULL OR timestamp < p_end_time)
    GROUP BY symbol, date_trunc('day', timestamp)
    ON CONFLICT (symbol, open_time) DO UPDATE SET
        high = GREATEST(kline_1d.high, EXCLUDED.high),
        low = LEAST(kline_1d.low, EXCLUDED.low),
        close = EXCLUDED.close,
        volume = kline_1d.volume + EXCLUDED.volume,
        trade_count = kline_1d.trade_count + EXCLUDED.trade_count;

    GET DIAGNOSTICS inserted_count = ROW_COUNT;
    RETURN inserted_count;
END;
$$ LANGUAGE plpgsql;

-- 生成3日K线
CREATE OR REPLACE FUNCTION aggregate_kline_3d(
    p_symbol VARCHAR(20),
    p_start_time TIMESTAMPTZ DEFAULT NULL,
    p_end_time TIMESTAMPTZ DEFAULT NULL
) RETURNS INTEGER AS $$
DECLARE
    inserted_count INTEGER := 0;
BEGIN
    INSERT INTO kline_3d (symbol, open_time, open, high, low, close, volume, trade_count)
    SELECT
        symbol,
        date_trunc('day', timestamp) - ((EXTRACT(DAY FROM timestamp)::int - 1) % 3 * INTERVAL '1 day') as open_time,
        (array_agg(open ORDER BY timestamp))[1] as open,
        MAX(high) as high,
        MIN(low) as low,
        (array_agg(close ORDER BY timestamp DESC))[1] as close,
        SUM(volume) as volume,
        SUM(trade_count) as trade_count
    FROM kline_1m
    WHERE symbol = p_symbol
        AND (p_start_time IS NULL OR timestamp >= p_start_time)
        AND (p_end_time IS NULL OR timestamp < p_end_time)
    GROUP BY symbol, date_trunc('day', timestamp) - ((EXTRACT(DAY FROM timestamp)::int - 1) % 3 * INTERVAL '1 day')
    ON CONFLICT (symbol, open_time) DO UPDATE SET
        high = GREATEST(kline_3d.high, EXCLUDED.high),
        low = LEAST(kline_3d.low, EXCLUDED.low),
        close = EXCLUDED.close,
        volume = kline_3d.volume + EXCLUDED.volume,
        trade_count = kline_3d.trade_count + EXCLUDED.trade_count;

    GET DIAGNOSTICS inserted_count = ROW_COUNT;
    RETURN inserted_count;
END;
$$ LANGUAGE plpgsql;

-- 生成周K线
CREATE OR REPLACE FUNCTION aggregate_kline_1w(
    p_symbol VARCHAR(20),
    p_start_time TIMESTAMPTZ DEFAULT NULL,
    p_end_time TIMESTAMPTZ DEFAULT NULL
) RETURNS INTEGER AS $$
DECLARE
    inserted_count INTEGER := 0;
BEGIN
    INSERT INTO kline_1w (symbol, open_time, open, high, low, close, volume, trade_count)
    SELECT
        symbol,
        date_trunc('week', timestamp) as open_time,
        (array_agg(open ORDER BY timestamp))[1] as open,
        MAX(high) as high,
        MIN(low) as low,
        (array_agg(close ORDER BY timestamp DESC))[1] as close,
        SUM(volume) as volume,
        SUM(trade_count) as trade_count
    FROM kline_1m
    WHERE symbol = p_symbol
        AND (p_start_time IS NULL OR timestamp >= p_start_time)
        AND (p_end_time IS NULL OR timestamp < p_end_time)
    GROUP BY symbol, date_trunc('week', timestamp)
    ON CONFLICT (symbol, open_time) DO UPDATE SET
        high = GREATEST(kline_1w.high, EXCLUDED.high),
        low = LEAST(kline_1w.low, EXCLUDED.low),
        close = EXCLUDED.close,
        volume = kline_1w.volume + EXCLUDED.volume,
        trade_count = kline_1w.trade_count + EXCLUDED.trade_count;

    GET DIAGNOSTICS inserted_count = ROW_COUNT;
    RETURN inserted_count;
END;
$$ LANGUAGE plpgsql;


-- =================================================================
-- 批量聚合所有交易对
-- =================================================================

CREATE OR REPLACE FUNCTION aggregate_all_symbols_high_tf()
RETURNS TABLE(symbol VARCHAR, timeframe TEXT, inserted INTEGER) AS $$
DECLARE
    sym RECORD;
    result INTEGER;
BEGIN
    FOR sym IN SELECT DISTINCT symbol FROM kline_1m
    LOOP
        -- 4小时K线
        result := aggregate_kline_4h(sym.symbol);
        symbol := sym.symbol;
        timeframe := '4h';
        inserted := result;
        RETURN NEXT;

        -- 日K线
        result := aggregate_kline_1d(sym.symbol);
        symbol := sym.symbol;
        timeframe := '1d';
        inserted := result;
        RETURN NEXT;

        -- 3日K线
        result := aggregate_kline_3d(sym.symbol);
        symbol := sym.symbol;
        timeframe := '3d';
        inserted := result;
        RETURN NEXT;

        -- 周K线
        result := aggregate_kline_1w(sym.symbol);
        symbol := sym.symbol;
        timeframe := '1w';
        inserted := result;
        RETURN NEXT;
    END LOOP;
END;
$$ LANGUAGE plpgsql;
