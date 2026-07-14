-- =================================================================
-- K线表索引优化分析与清理脚本
-- 创建时间：2026-07-14
--
-- 分析结果：
-- 1. kline_1m 表有 5 个索引，存在重复
-- 2. 主键 (symbol, timestamp) 已经是 B-tree 索引
-- 3. idx_kline_1m_symbol_time 与主键完全重复
-- 4. idx_kline_1m_symbol_latest 和 idx_kline_1m_cover 功能重复
--
-- 执行方式：
--   psql -U postgres -d trading_core -f 20260714_optimize_kline_indexes.sql
-- =================================================================

DO $$
BEGIN
    RAISE NOTICE '========================================';
    RAISE NOTICE 'K线索引优化开始...';
    RAISE NOTICE '========================================';
END $$;


-- =================================================================
-- kline_1m 表索引优化
-- =================================================================

DO $$
BEGIN
    RAISE NOTICE '';
    RAISE NOTICE '--- kline_1m 表 ---';
    RAISE NOTICE '当前索引:';
    RAISE NOTICE '  1. kline_1m_pkey (symbol, timestamp) - 主键';
    RAISE NOTICE '  2. idx_kline_1m_symbol_latest (symbol, timestamp DESC) INCLUDE(...) - 覆盖索引';
    RAISE NOTICE '  3. idx_kline_1m_symbol_time (symbol, timestamp DESC) - 与主键重复!';
    RAISE NOTICE '  4. idx_kline_1m_timestamp (timestamp) - 单列索引';
    RAISE NOTICE '  5. idx_kline_1m_cover (symbol, timestamp DESC) INCLUDE(...) - 与#2重复!';
    RAISE NOTICE '';
    RAISE NOTICE '优化方案:';
    RAISE NOTICE '  - 删除 idx_kline_1m_symbol_time (与主键重复)';
    RAISE NOTICE '  - 删除 idx_kline_1m_symbol_latest (与 cover 重复)';
    RAISE NOTICE '  - 保留 idx_kline_1m_cover (最完整的覆盖索引)';
    RAISE NOTICE '  - 保留 idx_kline_1m_timestamp (用于全表时间范围查询)';
END $$;

-- 删除重复索引
DROP INDEX IF EXISTS idx_kline_1m_symbol_time;
DROP INDEX IF EXISTS idx_kline_1m_symbol_latest;

-- 确保覆盖索引存在（包含所有查询需要的字段）
CREATE INDEX IF NOT EXISTS idx_kline_1m_cover
ON kline_1m (symbol, timestamp DESC)
INCLUDE (open, high, low, close, volume, trade_count);

-- 确保单列时间索引存在（用于 MIN/MAX 查询）
CREATE INDEX IF NOT EXISTS idx_kline_1m_timestamp
ON kline_1m (timestamp);


-- =================================================================
-- 其他 K 线表索引优化（如果有重复）
-- =================================================================

-- kline_5m
DO $$
BEGIN
    -- 检查是否存在重复索引
    IF EXISTS (SELECT 1 FROM pg_indexes WHERE indexname = 'idx_kline_5m_symbol_time') THEN
        DROP INDEX IF EXISTS idx_kline_5m_symbol_time;
        RAISE NOTICE 'kline_5m: 删除重复索引 idx_kline_5m_symbol_time';
    END IF;
END $$;

-- 确保覆盖索引存在
CREATE INDEX IF NOT EXISTS idx_kline_5m_cover
ON kline_5m (symbol, open_time DESC)
INCLUDE (open, high, low, close, volume, trade_count);


-- kline_15m
DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM pg_indexes WHERE indexname = 'idx_kline_15m_symbol_time') THEN
        DROP INDEX IF EXISTS idx_kline_15m_symbol_time;
        RAISE NOTICE 'kline_15m: 删除重复索引 idx_kline_15m_symbol_time';
    END IF;
END $$;

CREATE INDEX IF NOT EXISTS idx_kline_15m_cover
ON kline_15m (symbol, open_time DESC)
INCLUDE (open, high, low, close, volume, trade_count);


-- kline_30m
DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM pg_indexes WHERE indexname = 'idx_kline_30m_symbol_time') THEN
        DROP INDEX IF EXISTS idx_kline_30m_symbol_time;
        RAISE NOTICE 'kline_30m: 删除重复索引 idx_kline_30m_symbol_time';
    END IF;
END $$;

CREATE INDEX IF NOT EXISTS idx_kline_30m_cover
ON kline_30m (symbol, open_time DESC)
INCLUDE (open, high, low, close, volume, trade_count);


-- kline_1h
DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM pg_indexes WHERE indexname = 'idx_kline_1h_symbol_time') THEN
        DROP INDEX IF EXISTS idx_kline_1h_symbol_time;
        RAISE NOTICE 'kline_1h: 删除重复索引 idx_kline_1h_symbol_time';
    END IF;
END $$;

CREATE INDEX IF NOT EXISTS idx_kline_1h_cover
ON kline_1h (symbol, open_time DESC)
INCLUDE (open, high, low, close, volume, trade_count);


-- kline_2h
DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM pg_indexes WHERE indexname = 'idx_kline_2h_symbol_time') THEN
        DROP INDEX IF EXISTS idx_kline_2h_symbol_time;
        RAISE NOTICE 'kline_2h: 删除重复索引 idx_kline_2h_symbol_time';
    END IF;
END $$;

CREATE INDEX IF NOT EXISTS idx_kline_2h_cover
ON kline_2h (symbol, open_time DESC)
INCLUDE (open, high, low, close, volume, trade_count);


-- kline_4h
DO $$
BEGIN
    IF EXISTS (SELECT 1 FROM pg_indexes WHERE indexname = 'idx_kline_4h_symbol_time') THEN
        DROP INDEX IF EXISTS idx_kline_4h_symbol_time;
        RAISE NOTICE 'kline_4h: 删除重复索引 idx_kline_4h_symbol_time';
    END IF;
END $$;

CREATE INDEX IF NOT EXISTS idx_kline_4h_cover
ON kline_4h (symbol, open_time DESC)
INCLUDE (open, high, low, close, volume, trade_count);


-- =================================================================
-- 验证优化结果
-- =================================================================

DO $$
DECLARE
    idx_count INTEGER;
BEGIN
    RAISE NOTICE '';
    RAISE NOTICE '========================================';
    RAISE NOTICE '优化后索引统计:';
    RAISE NOTICE '========================================';

    -- 统计每个表的索引数量
    SELECT COUNT(*) INTO idx_count
    FROM pg_indexes WHERE tablename = 'kline_1m';
    RAISE NOTICE 'kline_1m: % 个索引', idx_count;

    SELECT COUNT(*) INTO idx_count
    FROM pg_indexes WHERE tablename = 'kline_5m';
    RAISE NOTICE 'kline_5m: % 个索引', idx_count;

    SELECT COUNT(*) INTO idx_count
    FROM pg_indexes WHERE tablename = 'kline_15m';
    RAISE NOTICE 'kline_15m: % 个索引', idx_count;

    SELECT COUNT(*) INTO idx_count
    FROM pg_indexes WHERE tablename = 'kline_30m';
    RAISE NOTICE 'kline_30m: % 个索引', idx_count;

    SELECT COUNT(*) INTO idx_count
    FROM pg_indexes WHERE tablename = 'kline_1h';
    RAISE NOTICE 'kline_1h: % 个索引', idx_count;

    SELECT COUNT(*) INTO idx_count
    FROM pg_indexes WHERE tablename = 'kline_2h';
    RAISE NOTICE 'kline_2h: % 个索引', idx_count;

    SELECT COUNT(*) INTO idx_count
    FROM pg_indexes WHERE tablename = 'kline_4h';
    RAISE NOTICE 'kline_4h: % 个索引', idx_count;

    RAISE NOTICE '';
    RAISE NOTICE '========================================';
    RAISE NOTICE '索引优化完成！';
    RAISE NOTICE '========================================';
END $$;


-- =================================================================
-- 显示优化后的索引详情
-- =================================================================

DO $$
BEGIN
    RAISE NOTICE '';
    RAISE NOTICE '--- kline_1m 索引详情 ---';
END $$;

SELECT indexname, indexdef
FROM pg_indexes
WHERE tablename = 'kline_1m'
ORDER BY indexname;
