-- =================================================================
-- 高时间框架 K线聚合函数（从 kline_1m 聚合生成）
-- 覆盖所有9个时间框架：5m/15m/30m/1h/2h/4h/1d/3d/1w
--
-- 执行方式: psql -U your_user -d your_db -f sql/core/kline_aggregation_all.sql
--
-- ON CONFLICT 策略：
--   open: 保留首次写入的开盘价（不更新）
--   high: GREATEST(已有, 新值) — 取最高
--   low:  LEAST(已有, 新值) — 取最低
--   close: 替换为最新收盘价
--   volume/trade_count: 累加（支持增量聚合）
-- =================================================================

-- 先删除旧函数（参数签名变化时 PostgreSQL 不允许直接替换）
DROP FUNCTION IF EXISTS aggregate_kline_5m(VARCHAR, TIMESTAMPTZ, TIMESTAMPTZ);
DROP FUNCTION IF EXISTS aggregate_kline_15m(VARCHAR, TIMESTAMPTZ, TIMESTAMPTZ);
DROP FUNCTION IF EXISTS aggregate_kline_30m(VARCHAR, TIMESTAMPTZ, TIMESTAMPTZ);
DROP FUNCTION IF EXISTS aggregate_kline_1h(VARCHAR, TIMESTAMPTZ, TIMESTAMPTZ);
DROP FUNCTION IF EXISTS aggregate_kline_2h(VARCHAR, TIMESTAMPTZ, TIMESTAMPTZ);
DROP FUNCTION IF EXISTS aggregate_kline_4h(VARCHAR, TIMESTAMPTZ, TIMESTAMPTZ);
DROP FUNCTION IF EXISTS aggregate_kline_1d(VARCHAR, TIMESTAMPTZ, TIMESTAMPTZ);
DROP FUNCTION IF EXISTS aggregate_kline_3d(VARCHAR, TIMESTAMPTZ, TIMESTAMPTZ);
DROP FUNCTION IF EXISTS aggregate_kline_1w(VARCHAR, TIMESTAMPTZ, TIMESTAMPTZ);
DROP FUNCTION IF EXISTS aggregate_all_timeframes(VARCHAR, TIMESTAMPTZ, TIMESTAMPTZ);
DROP FUNCTION IF EXISTS aggregate_all_symbols_high_tf();

-- -----------------------------------------------------------------
-- 5分钟聚合
-- -----------------------------------------------------------------
CREATE OR REPLACE FUNCTION aggregate_kline_5m(
    p_symbol VARCHAR(20),
    p_start_time TIMESTAMPTZ DEFAULT NULL,
    p_end_time TIMESTAMPTZ DEFAULT NULL
) RETURNS INTEGER AS $$
DECLARE
    v_count INTEGER;
BEGIN
    INSERT INTO kline_5m (symbol, open_time, open, high, low, close, volume, trade_count)
    SELECT
        symbol,
        date_trunc('hour', timestamp) + (FLOOR(EXTRACT(MINUTE FROM timestamp) / 5) * 5) * interval '1 minute' AS bucket,
        (array_agg(open ORDER BY timestamp ASC))[1],
        MAX(high),
        MIN(low),
        (array_agg(close ORDER BY timestamp DESC))[1],
        SUM(volume),
        SUM(trade_count)
    FROM kline_1m
    WHERE symbol = p_symbol
      AND (p_start_time IS NULL OR timestamp >= p_start_time)
      AND (p_end_time IS NULL OR timestamp < p_end_time)
    GROUP BY symbol, bucket
    ORDER BY bucket
    ON CONFLICT (symbol, open_time) DO UPDATE SET
        high = GREATEST(kline_5m.high, EXCLUDED.high),
        low = LEAST(kline_5m.low, EXCLUDED.low),
        close = EXCLUDED.close,
        volume = kline_5m.volume + EXCLUDED.volume,
        trade_count = kline_5m.trade_count + EXCLUDED.trade_count;

    GET DIAGNOSTICS v_count = ROW_COUNT;
    RETURN v_count;
END;
$$ LANGUAGE plpgsql;

-- -----------------------------------------------------------------
-- 15分钟聚合
-- -----------------------------------------------------------------
CREATE OR REPLACE FUNCTION aggregate_kline_15m(
    p_symbol VARCHAR(20),
    p_start_time TIMESTAMPTZ DEFAULT NULL,
    p_end_time TIMESTAMPTZ DEFAULT NULL
) RETURNS INTEGER AS $$
DECLARE
    v_count INTEGER;
BEGIN
    INSERT INTO kline_15m (symbol, open_time, open, high, low, close, volume, trade_count)
    SELECT
        symbol,
        date_trunc('hour', timestamp) + (FLOOR(EXTRACT(MINUTE FROM timestamp) / 15) * 15) * interval '1 minute' AS bucket,
        (array_agg(open ORDER BY timestamp ASC))[1],
        MAX(high),
        MIN(low),
        (array_agg(close ORDER BY timestamp DESC))[1],
        SUM(volume),
        SUM(trade_count)
    FROM kline_1m
    WHERE symbol = p_symbol
      AND (p_start_time IS NULL OR timestamp >= p_start_time)
      AND (p_end_time IS NULL OR timestamp < p_end_time)
    GROUP BY symbol, bucket
    ORDER BY bucket
    ON CONFLICT (symbol, open_time) DO UPDATE SET
        high = GREATEST(kline_15m.high, EXCLUDED.high),
        low = LEAST(kline_15m.low, EXCLUDED.low),
        close = EXCLUDED.close,
        volume = kline_15m.volume + EXCLUDED.volume,
        trade_count = kline_15m.trade_count + EXCLUDED.trade_count;

    GET DIAGNOSTICS v_count = ROW_COUNT;
    RETURN v_count;
END;
$$ LANGUAGE plpgsql;

-- -----------------------------------------------------------------
-- 30分钟聚合
-- -----------------------------------------------------------------
CREATE OR REPLACE FUNCTION aggregate_kline_30m(
    p_symbol VARCHAR(20),
    p_start_time TIMESTAMPTZ DEFAULT NULL,
    p_end_time TIMESTAMPTZ DEFAULT NULL
) RETURNS INTEGER AS $$
DECLARE
    v_count INTEGER;
BEGIN
    INSERT INTO kline_30m (symbol, open_time, open, high, low, close, volume, trade_count)
    SELECT
        symbol,
        date_trunc('hour', timestamp) + (FLOOR(EXTRACT(MINUTE FROM timestamp) / 30) * 30) * interval '1 minute' AS bucket,
        (array_agg(open ORDER BY timestamp ASC))[1],
        MAX(high),
        MIN(low),
        (array_agg(close ORDER BY timestamp DESC))[1],
        SUM(volume),
        SUM(trade_count)
    FROM kline_1m
    WHERE symbol = p_symbol
      AND (p_start_time IS NULL OR timestamp >= p_start_time)
      AND (p_end_time IS NULL OR timestamp < p_end_time)
    GROUP BY symbol, bucket
    ORDER BY bucket
    ON CONFLICT (symbol, open_time) DO UPDATE SET
        high = GREATEST(kline_30m.high, EXCLUDED.high),
        low = LEAST(kline_30m.low, EXCLUDED.low),
        close = EXCLUDED.close,
        volume = kline_30m.volume + EXCLUDED.volume,
        trade_count = kline_30m.trade_count + EXCLUDED.trade_count;

    GET DIAGNOSTICS v_count = ROW_COUNT;
    RETURN v_count;
END;
$$ LANGUAGE plpgsql;

-- -----------------------------------------------------------------
-- 1小时聚合
-- -----------------------------------------------------------------
CREATE OR REPLACE FUNCTION aggregate_kline_1h(
    p_symbol VARCHAR(20),
    p_start_time TIMESTAMPTZ DEFAULT NULL,
    p_end_time TIMESTAMPTZ DEFAULT NULL
) RETURNS INTEGER AS $$
DECLARE
    v_count INTEGER;
BEGIN
    INSERT INTO kline_1h (symbol, open_time, open, high, low, close, volume, trade_count)
    SELECT
        symbol,
        date_trunc('hour', timestamp) AS bucket,
        (array_agg(open ORDER BY timestamp ASC))[1],
        MAX(high),
        MIN(low),
        (array_agg(close ORDER BY timestamp DESC))[1],
        SUM(volume),
        SUM(trade_count)
    FROM kline_1m
    WHERE symbol = p_symbol
      AND (p_start_time IS NULL OR timestamp >= p_start_time)
      AND (p_end_time IS NULL OR timestamp < p_end_time)
    GROUP BY symbol, bucket
    ORDER BY bucket
    ON CONFLICT (symbol, open_time) DO UPDATE SET
        high = GREATEST(kline_1h.high, EXCLUDED.high),
        low = LEAST(kline_1h.low, EXCLUDED.low),
        close = EXCLUDED.close,
        volume = kline_1h.volume + EXCLUDED.volume,
        trade_count = kline_1h.trade_count + EXCLUDED.trade_count;

    GET DIAGNOSTICS v_count = ROW_COUNT;
    RETURN v_count;
END;
$$ LANGUAGE plpgsql;

-- -----------------------------------------------------------------
-- 2小时聚合
-- -----------------------------------------------------------------
CREATE OR REPLACE FUNCTION aggregate_kline_2h(
    p_symbol VARCHAR(20),
    p_start_time TIMESTAMPTZ DEFAULT NULL,
    p_end_time TIMESTAMPTZ DEFAULT NULL
) RETURNS INTEGER AS $$
DECLARE
    v_count INTEGER;
BEGIN
    INSERT INTO kline_2h (symbol, open_time, open, high, low, close, volume, trade_count)
    SELECT
        symbol,
        date_trunc('day', timestamp) + (FLOOR(EXTRACT(HOUR FROM timestamp) / 2) * 2) * interval '1 hour' AS bucket,
        (array_agg(open ORDER BY timestamp ASC))[1],
        MAX(high),
        MIN(low),
        (array_agg(close ORDER BY timestamp DESC))[1],
        SUM(volume),
        SUM(trade_count)
    FROM kline_1m
    WHERE symbol = p_symbol
      AND (p_start_time IS NULL OR timestamp >= p_start_time)
      AND (p_end_time IS NULL OR timestamp < p_end_time)
    GROUP BY symbol, bucket
    ORDER BY bucket
    ON CONFLICT (symbol, open_time) DO UPDATE SET
        high = GREATEST(kline_2h.high, EXCLUDED.high),
        low = LEAST(kline_2h.low, EXCLUDED.low),
        close = EXCLUDED.close,
        volume = kline_2h.volume + EXCLUDED.volume,
        trade_count = kline_2h.trade_count + EXCLUDED.trade_count;

    GET DIAGNOSTICS v_count = ROW_COUNT;
    RETURN v_count;
END;
$$ LANGUAGE plpgsql;

-- -----------------------------------------------------------------
-- 4小时聚合
-- -----------------------------------------------------------------
CREATE OR REPLACE FUNCTION aggregate_kline_4h(
    p_symbol VARCHAR(20),
    p_start_time TIMESTAMPTZ,
    p_end_time TIMESTAMPTZ
) RETURNS INTEGER AS $$
DECLARE
    v_count INTEGER;
BEGIN
    INSERT INTO kline_4h (symbol, open_time, open, high, low, close, volume, trade_count)
    SELECT
        symbol,
        date_trunc('hour', timestamp) - ((EXTRACT(HOUR FROM timestamp)::int % 4) * INTERVAL '1 hour') AS bucket,
        (array_agg(open ORDER BY timestamp ASC))[1],
        MAX(high),
        MIN(low),
        (array_agg(close ORDER BY timestamp DESC))[1],
        SUM(volume),
        SUM(trade_count)
    FROM kline_1m
    WHERE symbol = p_symbol
      AND (p_start_time IS NULL OR timestamp >= p_start_time)
      AND (p_end_time IS NULL OR timestamp < p_end_time)
    GROUP BY symbol, bucket
    ORDER BY bucket
    ON CONFLICT (symbol, open_time) DO UPDATE SET
        high = GREATEST(kline_4h.high, EXCLUDED.high),
        low = LEAST(kline_4h.low, EXCLUDED.low),
        close = EXCLUDED.close,
        volume = kline_4h.volume + EXCLUDED.volume,
        trade_count = kline_4h.trade_count + EXCLUDED.trade_count;

    GET DIAGNOSTICS v_count = ROW_COUNT;
    RETURN v_count;
END;
$$ LANGUAGE plpgsql;

-- -----------------------------------------------------------------
-- 日K线聚合
-- -----------------------------------------------------------------
CREATE OR REPLACE FUNCTION aggregate_kline_1d(
    p_symbol VARCHAR(20),
    p_start_time TIMESTAMPTZ,
    p_end_time TIMESTAMPTZ
) RETURNS INTEGER AS $$
DECLARE
    v_count INTEGER;
BEGIN
    INSERT INTO kline_1d (symbol, open_time, open, high, low, close, volume, trade_count)
    SELECT
        symbol,
        date_trunc('day', timestamp) AS bucket,
        (array_agg(open ORDER BY timestamp ASC))[1],
        MAX(high),
        MIN(low),
        (array_agg(close ORDER BY timestamp DESC))[1],
        SUM(volume),
        SUM(trade_count)
    FROM kline_1m
    WHERE symbol = p_symbol
      AND (p_start_time IS NULL OR timestamp >= p_start_time)
      AND (p_end_time IS NULL OR timestamp < p_end_time)
    GROUP BY symbol, bucket
    ORDER BY bucket
    ON CONFLICT (symbol, open_time) DO UPDATE SET
        high = GREATEST(kline_1d.high, EXCLUDED.high),
        low = LEAST(kline_1d.low, EXCLUDED.low),
        close = EXCLUDED.close,
        volume = kline_1d.volume + EXCLUDED.volume,
        trade_count = kline_1d.trade_count + EXCLUDED.trade_count;

    GET DIAGNOSTICS v_count = ROW_COUNT;
    RETURN v_count;
END;
$$ LANGUAGE plpgsql;

-- -----------------------------------------------------------------
-- 3日K线聚合
-- -----------------------------------------------------------------
CREATE OR REPLACE FUNCTION aggregate_kline_3d(
    p_symbol VARCHAR(20),
    p_start_time TIMESTAMPTZ,
    p_end_time TIMESTAMPTZ
) RETURNS INTEGER AS $$
DECLARE
    v_count INTEGER;
BEGIN
    INSERT INTO kline_3d (symbol, open_time, open, high, low, close, volume, trade_count)
    SELECT
        symbol,
        date_trunc('day', timestamp) - ((EXTRACT(DAY FROM timestamp)::int - 1) % 3 * INTERVAL '1 day') AS bucket,
        (array_agg(open ORDER BY timestamp ASC))[1],
        MAX(high),
        MIN(low),
        (array_agg(close ORDER BY timestamp DESC))[1],
        SUM(volume),
        SUM(trade_count)
    FROM kline_1m
    WHERE symbol = p_symbol
      AND (p_start_time IS NULL OR timestamp >= p_start_time)
      AND (p_end_time IS NULL OR timestamp < p_end_time)
    GROUP BY symbol, bucket
    ORDER BY bucket
    ON CONFLICT (symbol, open_time) DO UPDATE SET
        high = GREATEST(kline_3d.high, EXCLUDED.high),
        low = LEAST(kline_3d.low, EXCLUDED.low),
        close = EXCLUDED.close,
        volume = kline_3d.volume + EXCLUDED.volume,
        trade_count = kline_3d.trade_count + EXCLUDED.trade_count;

    GET DIAGNOSTICS v_count = ROW_COUNT;
    RETURN v_count;
END;
$$ LANGUAGE plpgsql;

-- -----------------------------------------------------------------
-- 周K线聚合
-- -----------------------------------------------------------------
CREATE OR REPLACE FUNCTION aggregate_kline_1w(
    p_symbol VARCHAR(20),
    p_start_time TIMESTAMPTZ,
    p_end_time TIMESTAMPTZ
) RETURNS INTEGER AS $$
DECLARE
    v_count INTEGER;
BEGIN
    INSERT INTO kline_1w (symbol, open_time, open, high, low, close, volume, trade_count)
    SELECT
        symbol,
        date_trunc('week', timestamp) AS bucket,
        (array_agg(open ORDER BY timestamp ASC))[1],
        MAX(high),
        MIN(low),
        (array_agg(close ORDER BY timestamp DESC))[1],
        SUM(volume),
        SUM(trade_count)
    FROM kline_1m
    WHERE symbol = p_symbol
      AND (p_start_time IS NULL OR timestamp >= p_start_time)
      AND (p_end_time IS NULL OR timestamp < p_end_time)
    GROUP BY symbol, bucket
    ORDER BY bucket
    ON CONFLICT (symbol, open_time) DO UPDATE SET
        high = GREATEST(kline_1w.high, EXCLUDED.high),
        low = LEAST(kline_1w.low, EXCLUDED.low),
        close = EXCLUDED.close,
        volume = kline_1w.volume + EXCLUDED.volume,
        trade_count = kline_1w.trade_count + EXCLUDED.trade_count;

    GET DIAGNOSTICS v_count = ROW_COUNT;
    RETURN v_count;
END;
$$ LANGUAGE plpgsql;

-- =================================================================
-- 统一聚合入口：一次性聚合所有时间框架
-- =================================================================
CREATE OR REPLACE FUNCTION aggregate_all_timeframes(
    p_symbol VARCHAR(20),
    p_start_time TIMESTAMPTZ,
    p_end_time TIMESTAMPTZ
) RETURNS TABLE(timeframe TEXT, rows_affected INTEGER) AS $$
BEGIN
    RETURN QUERY SELECT '5m'::TEXT, aggregate_kline_5m(p_symbol, p_start_time, p_end_time);
    RETURN QUERY SELECT '15m'::TEXT, aggregate_kline_15m(p_symbol, p_start_time, p_end_time);
    RETURN QUERY SELECT '30m'::TEXT, aggregate_kline_30m(p_symbol, p_start_time, p_end_time);
    RETURN QUERY SELECT '1h'::TEXT, aggregate_kline_1h(p_symbol, p_start_time, p_end_time);
    RETURN QUERY SELECT '2h'::TEXT, aggregate_kline_2h(p_symbol, p_start_time, p_end_time);
    RETURN QUERY SELECT '4h'::TEXT, aggregate_kline_4h(p_symbol, p_start_time, p_end_time);
    RETURN QUERY SELECT '1d'::TEXT, aggregate_kline_1d(p_symbol, p_start_time, p_end_time);
    RETURN QUERY SELECT '3d'::TEXT, aggregate_kline_3d(p_symbol, p_start_time, p_end_time);
    RETURN QUERY SELECT '1w'::TEXT, aggregate_kline_1w(p_symbol, p_start_time, p_end_time);
END;
$$ LANGUAGE plpgsql;

-- =================================================================
-- 批量聚合所有交易对（所有时间框架）
-- =================================================================
CREATE OR REPLACE FUNCTION aggregate_all_symbols_high_tf()
RETURNS TABLE(symbol VARCHAR, timeframe TEXT, inserted INTEGER) AS $$
DECLARE
    sym RECORD;
    result INTEGER;
BEGIN
    FOR sym IN SELECT DISTINCT symbol FROM kline_1m
    LOOP
        result := aggregate_kline_5m(sym.symbol, NULL, NULL);
        symbol := sym.symbol; timeframe := '5m'; inserted := result; RETURN NEXT;

        result := aggregate_kline_15m(sym.symbol, NULL, NULL);
        symbol := sym.symbol; timeframe := '15m'; inserted := result; RETURN NEXT;

        result := aggregate_kline_30m(sym.symbol, NULL, NULL);
        symbol := sym.symbol; timeframe := '30m'; inserted := result; RETURN NEXT;

        result := aggregate_kline_1h(sym.symbol, NULL, NULL);
        symbol := sym.symbol; timeframe := '1h'; inserted := result; RETURN NEXT;

        result := aggregate_kline_2h(sym.symbol, NULL, NULL);
        symbol := sym.symbol; timeframe := '2h'; inserted := result; RETURN NEXT;

        result := aggregate_kline_4h(sym.symbol, NULL, NULL);
        symbol := sym.symbol; timeframe := '4h'; inserted := result; RETURN NEXT;

        result := aggregate_kline_1d(sym.symbol, NULL, NULL);
        symbol := sym.symbol; timeframe := '1d'; inserted := result; RETURN NEXT;

        result := aggregate_kline_3d(sym.symbol, NULL, NULL);
        symbol := sym.symbol; timeframe := '3d'; inserted := result; RETURN NEXT;

        result := aggregate_kline_1w(sym.symbol, NULL, NULL);
        symbol := sym.symbol; timeframe := '1w'; inserted := result; RETURN NEXT;
    END LOOP;
END;
$$ LANGUAGE plpgsql;
