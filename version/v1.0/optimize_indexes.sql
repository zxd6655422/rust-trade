-- 数据库索引优化脚本
-- 用于提升查询性能

-- ============================================
-- K线数据表索引
-- ============================================

-- 按交易对和时间戳排序查询（最常用）
CREATE INDEX IF NOT EXISTS idx_kline_1m_symbol_time
ON kline_1m(symbol, timestamp DESC);

-- 按交易对查询最新数据
CREATE INDEX IF NOT EXISTS idx_kline_1m_symbol_latest
ON kline_1m(symbol, timestamp DESC)
INCLUDE (open, high, low, close, volume);

-- ============================================
-- Tick 数据表索引
-- ============================================

-- 按交易对和时间戳排序查询
CREATE INDEX IF NOT EXISTS idx_tick_data_symbol_time
ON tick_data(symbol, timestamp DESC);

-- ============================================
-- 回测结果表索引
-- ============================================

-- 按策略和交易对查询
CREATE INDEX IF NOT EXISTS idx_backtest_results_strategy_symbol
ON backtest_results(strategy_id, symbol);

-- 按创建时间排序
CREATE INDEX IF NOT EXISTS idx_backtest_results_created
ON backtest_results(created_at DESC);

-- ============================================
-- 策略信号表索引
-- ============================================

-- 按策略和时间查询
CREATE INDEX IF NOT EXISTS idx_signals_strategy_time
ON strategy_signals(strategy_id, signal_time DESC);

-- 按交易对和时间查询
CREATE INDEX IF NOT EXISTS idx_signals_symbol_time
ON strategy_signals(symbol, signal_time DESC);

-- ============================================
-- 交易记录表索引
-- ============================================

-- 按交易对和时间查询
CREATE INDEX IF NOT EXISTS idx_trades_symbol_time
ON trades(symbol, trade_time DESC);

-- 按策略查询
CREATE INDEX IF NOT EXISTS idx_trades_strategy
ON trades(strategy_id, trade_time DESC);

-- ============================================
-- 分析查询性能
-- ============================================

-- 查看表大小
SELECT
    schemaname,
    tablename,
    pg_size_pretty(pg_total_relation_size(schemaname||'.'||tablename)) as total_size,
    pg_size_pretty(pg_relation_size(schemaname||'.'||tablename)) as table_size,
    pg_size_pretty(pg_total_relation_size(schemaname||'.'||tablename) - pg_relation_size(schemaname||'.'||tablename)) as index_size
FROM pg_tables
WHERE schemaname = 'public'
ORDER BY pg_total_relation_size(schemaname||'.'||tablename) DESC;

-- 查看索引使用情况
SELECT
    schemaname,
    tablename,
    indexname,
    idx_scan as index_scans,
    idx_tup_read as tuples_read,
    idx_tup_fetch as tuples_fetched
FROM pg_stat_user_indexes
ORDER BY idx_scan DESC;
