-- =================================================================
-- 清理高时间框架 K 线数据
-- 执行后会清空所有高时间框架表，重启 trading-core 后自动从 backfill_start_date 重新拉取
-- 注意：kline_1m 表不受影响
-- =================================================================

-- 先查看各表数据量
SELECT 'kline_5m' AS table_name, COUNT(*) AS row_count FROM kline_5m
UNION ALL SELECT 'kline_15m', COUNT(*) FROM kline_15m
UNION ALL SELECT 'kline_30m', COUNT(*) FROM kline_30m
UNION ALL SELECT 'kline_1h', COUNT(*) FROM kline_1h
UNION ALL SELECT 'kline_2h', COUNT(*) FROM kline_2h
UNION ALL SELECT 'kline_4h', COUNT(*) FROM kline_4h
UNION ALL SELECT 'kline_1d', COUNT(*) FROM kline_1d
UNION ALL SELECT 'kline_3d', COUNT(*) FROM kline_3d
UNION ALL SELECT 'kline_1w', COUNT(*) FROM kline_1w;

-- 清空所有高时间框架表（TRUNCATE 比 DELETE 快，且重置自增ID）
TRUNCATE TABLE kline_5m;
TRUNCATE TABLE kline_15m;
TRUNCATE TABLE kline_30m;
TRUNCATE TABLE kline_1h;
TRUNCATE TABLE kline_2h;
TRUNCATE TABLE kline_4h;
TRUNCATE TABLE kline_1d;
TRUNCATE TABLE kline_3d;
TRUNCATE TABLE kline_1w;

-- 验证清空结果
SELECT 'kline_5m' AS table_name, COUNT(*) AS row_count FROM kline_5m
UNION ALL SELECT 'kline_15m', COUNT(*) FROM kline_15m
UNION ALL SELECT 'kline_30m', COUNT(*) FROM kline_30m
UNION ALL SELECT 'kline_1h', COUNT(*) FROM kline_1h
UNION ALL SELECT 'kline_2h', COUNT(*) FROM kline_2h
UNION ALL SELECT 'kline_4h', COUNT(*) FROM kline_4h
UNION ALL SELECT 'kline_1d', COUNT(*) FROM kline_1d
UNION ALL SELECT 'kline_3d', COUNT(*) FROM kline_3d
UNION ALL SELECT 'kline_1w', COUNT(*) FROM kline_1w;
