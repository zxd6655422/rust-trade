-- K线表覆盖索引优化
-- 解决慢查询问题：SELECT * FROM kline_xxm WHERE symbol = $1 ORDER BY time DESC LIMIT $2
-- 使用 INCLUDE 避免回表，实现 Index Only Scan

-- kline_1m: 1640 万行
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_kline_1m_cover
ON kline_1m (symbol, "timestamp" DESC)
INCLUDE (open, high, low, close, volume, trade_count);

-- kline_5m: 400 万行
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_kline_5m_cover
ON kline_5m (symbol, open_time DESC)
INCLUDE (open, high, low, close, volume, trade_count);

-- kline_15m
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_kline_15m_cover
ON kline_15m (symbol, open_time DESC)
INCLUDE (open, high, low, close, volume, trade_count);

-- kline_30m
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_kline_30m_cover
ON kline_30m (symbol, open_time DESC)
INCLUDE (open, high, low, close, volume, trade_count);

-- kline_1h
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_kline_1h_cover
ON kline_1h (symbol, open_time DESC)
INCLUDE (open, high, low, close, volume, trade_count);

-- kline_2h
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_kline_2h_cover
ON kline_2h (symbol, open_time DESC)
INCLUDE (open, high, low, close, volume, trade_count);

-- kline_4h
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_kline_4h_cover
ON kline_4h (symbol, open_time DESC)
INCLUDE (open, high, low, close, volume, trade_count);

-- kline_1d
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_kline_1d_cover
ON kline_1d (symbol, open_time DESC)
INCLUDE (open, high, low, close, volume, trade_count);

-- kline_3d
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_kline_3d_cover
ON kline_3d (symbol, open_time DESC)
INCLUDE (open, high, low, close, volume, trade_count);

-- kline_1w
CREATE INDEX CONCURRENTLY IF NOT EXISTS idx_kline_1w_cover
ON kline_1w (symbol, open_time DESC)
INCLUDE (open, high, low, close, volume, trade_count);
