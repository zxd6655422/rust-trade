-- =================================================================
-- K线表索引分析脚本
-- 用于检查当前数据库中的索引情况
--
-- 执行方式：
--   psql -U postgres -d trading_core -f analyze_kline_indexes.sql
-- =================================================================

-- 显示所有 K 线表的索引
SELECT
    tablename,
    indexname,
    indexdef,
    pg_size_pretty(pg_relation_size(indexname::regclass)) as index_size
FROM pg_indexes
WHERE tablename LIKE 'kline_%'
ORDER BY tablename, indexname;

-- 统计每个表的索引数量和大小
SELECT
    tablename,
    COUNT(*) as index_count,
    pg_size_pretty(SUM(pg_relation_size(indexname::regclass))) as total_index_size
FROM pg_indexes
WHERE tablename LIKE 'kline_%'
GROUP BY tablename
ORDER BY tablename;

-- 检查重复索引（基于列定义）
WITH index_columns AS (
    SELECT
        tablename,
        indexname,
        array_agg(attname ORDER BY ordinality) as columns
    FROM pg_indexes
    JOIN unnest(string_to_array(replace(replace(indexdef, '(', ','), ')', ''), ',')) WITH ORDINALITY AS t(col, ordinality)
        ON true
    JOIN pg_attribute ON attname = trim(col)
    WHERE tablename LIKE 'kline_%'
        AND indexname NOT LIKE '%pkey'
    GROUP BY tablename, indexname
)
SELECT
    a.tablename,
    a.indexname as index1,
    b.indexname as index2,
    a.columns
FROM index_columns a
JOIN index_columns b ON a.tablename = b.tablename
    AND a.columns = b.columns
    AND a.indexname < b.indexname
ORDER BY a.tablename, a.indexname;

-- 显示查询计划示例
EXPLAIN (ANALYZE, BUFFERS)
SELECT *
FROM kline_1m
WHERE symbol = 'BTCUSDT'
ORDER BY timestamp DESC
LIMIT 100;

-- 显示表大小
SELECT
    tablename,
    pg_size_pretty(pg_total_relation_size(tablename::regclass)) as total_size,
    pg_size_pretty(pg_relation_size(tablename::regclass)) as table_size,
    pg_size_pretty(pg_indexes_size(tablename::regclass)) as indexes_size
FROM pg_tables
WHERE tablename LIKE 'kline_%'
ORDER BY pg_total_relation_size(tablename::regclass) DESC;
