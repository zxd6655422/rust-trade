-- =================================================================
-- 迁移脚本: 为日志表添加 signal_id 实现全链路追踪
-- 执行方式: psql -U your_user -d your_db -f sql/migrations/20260725_add_signal_id_to_logs.sql
-- 创建时间：2026-07-25
-- =================================================================
--
-- 全链路追踪架构:
--
-- strategy_analysis_log (分析)
--         ↓ signal_id
-- strategy_signals (信号)
--         ↓ signal_id
-- risk_logs (风控)
--         ↓ signal_id
-- trade_logs (成交)
--         ↓ signal_id
-- live_strategy_log (实时日志)
--
-- 查询: SELECT * FROM strategy_analysis_log WHERE signal_id = 'xxx'
--        SELECT * FROM trade_logs WHERE signal_id = 'xxx'
--        SELECT * FROM risk_logs WHERE signal_id = 'xxx'
--        SELECT * FROM live_strategy_log WHERE signal_id = 'xxx'
--

BEGIN;

-- =================================================================
-- 1. strategy_analysis_log 添加 signal_id
-- =================================================================

ALTER TABLE strategy_analysis_log
ADD COLUMN IF NOT EXISTS signal_id UUID;

COMMENT ON COLUMN strategy_analysis_log.signal_id IS '关联的策略信号ID，用于全链路追踪';

CREATE INDEX IF NOT EXISTS idx_analysis_signal_id ON strategy_analysis_log(signal_id);

-- =================================================================
-- 2. live_strategy_log 添加 signal_id
-- =================================================================

ALTER TABLE live_strategy_log
ADD COLUMN IF NOT EXISTS signal_id UUID;

COMMENT ON COLUMN live_strategy_log.signal_id IS '关联的策略信号ID，用于全链路追踪';

CREATE INDEX IF NOT EXISTS idx_live_signal_id ON live_strategy_log(signal_id);

-- =================================================================
-- 3. 确保 trade_logs 和 risk_logs 有 signal_id (可能已有)
-- =================================================================

-- trade_logs 通常已有 signal_id，检查并添加索引
CREATE INDEX IF NOT EXISTS idx_trade_signal_id ON trade_logs(signal_id);

-- risk_logs 通常已有 signal_id，检查并添加索引
CREATE INDEX IF NOT EXISTS idx_risk_signal_id ON risk_logs(signal_id);

-- =================================================================
-- 4. 创建全链路查询视图
-- =================================================================

-- 全链路事件视图
CREATE OR REPLACE VIEW v_signal_timeline AS
SELECT
    'analysis' as event_source,
    a.signal_id,
    a.created_at as event_time,
    'strategy_analyzed' as event_type,
    a.symbol,
    a.strategy_id,
    a.direction,
    a.entry_price,
    a.signal_type,
    a.signal_intent,
    a.market_type,
    a.overall_confidence as confidence,
    a.signal_strength,
    a.stop_loss,
    a.take_profit,
    a.status,
    NULL::jsonb as trade_details
FROM strategy_analysis_log a
WHERE a.signal_id IS NOT NULL

UNION ALL

SELECT
    'signal' as event_source,
    s.id as signal_id,
    s.created_at as event_time,
    'signal_generated' as event_type,
    s.symbol,
    s.strategy_id,
    s.direction,
    s.entry_price,
    s.signal_type,
    s.signal_intent,
    s.market_type,
    s.overall_confidence as confidence,
    s.signal_strength,
    s.stop_loss,
    s.take_profit,
    s.status,
    NULL::jsonb as trade_details
FROM strategy_signals s

UNION ALL

SELECT
    'trade' as event_source,
    t.signal_id,
    t.timestamp as event_time,
    t.event_type,
    t.symbol,
    t.strategy_id,
    NULL as direction,
    t.price as entry_price,
    t.side as signal_type,
    NULL as signal_intent,
    t.market_type,
    NULL as confidence,
    NULL as signal_strength,
    NULL as stop_loss,
    NULL as take_profit,
    NULL as status,
    jsonb_build_object(
        'order_id', t.order_id,
        'quantity', t.quantity,
        'commission', t.commission,
        'slippage', t.slippage,
        'pnl', t.pnl,
        'notes', t.notes
    ) as trade_details
FROM trade_logs t
WHERE t.signal_id IS NOT NULL

UNION ALL

SELECT
    'risk' as event_source,
    r.signal_id,
    r.timestamp as event_time,
    r.event_type,
    r.symbol,
    NULL as strategy_id,
    NULL as direction,
    NULL as entry_price,
    NULL as signal_type,
    NULL as signal_intent,
    r.market_type,
    NULL as confidence,
    NULL as signal_strength,
    NULL as stop_loss,
    NULL as take_profit,
    r.check_result as status,
    jsonb_build_object(
        'decision', r.decision,
        'current_equity', r.current_equity,
        'peak_equity', r.peak_equity,
        'daily_pnl', r.daily_pnl,
        'details', r.details
    ) as trade_details
FROM risk_logs r
WHERE r.signal_id IS NOT NULL

UNION ALL

SELECT
    'live' as event_source,
    l.signal_id,
    l.timestamp as event_time,
    'tick_processed' as event_type,
    l.symbol,
    l.strategy_id,
    NULL as direction,
    l.entry_price,
    l.signal_type,
    l.signal_intent,
    l.market_type,
    NULL as confidence,
    NULL as signal_strength,
    l.stop_loss,
    l.take_profit,
    NULL as status,
    jsonb_build_object(
        'current_price', l.current_price,
        'portfolio_value', l.portfolio_value,
        'total_pnl', l.total_pnl,
        'unrealized_pnl', l.unrealized_pnl,
        'position_quantity', l.position_quantity,
        'cache_hit', l.cache_hit,
        'processing_time_us', l.processing_time_us
    ) as trade_details
FROM live_strategy_log l
WHERE l.signal_id IS NOT NULL

ORDER BY event_time ASC;

-- 全链路统计视图
CREATE OR REPLACE VIEW v_signal_summary AS
SELECT
    s.id as signal_id,
    s.symbol,
    s.strategy_id,
    s.direction,
    s.entry_price,
    s.signal_type,
    s.signal_intent,
    s.market_type,
    s.created_at as signal_time,
    s.status as signal_status,
    -- 成交统计
    COUNT(DISTINCT t.id) as trade_count,
    MIN(t.timestamp) as first_trade_time,
    MAX(t.timestamp) as last_trade_time,
    -- 风控统计
    COUNT(DISTINCT r.id) as risk_check_count,
    -- 实时日志统计
    COUNT(DISTINCT l.id) as tick_count,
    MIN(l.timestamp) as first_tick_time,
    MAX(l.timestamp) as last_tick_time,
    -- 盈亏
    SUM(t.pnl) as total_pnl,
    s.actual_return_pct
FROM strategy_signals s
LEFT JOIN trade_logs t ON t.signal_id = s.id
LEFT JOIN risk_logs r ON r.signal_id = s.id
LEFT JOIN live_strategy_log l ON l.signal_id = s.id
GROUP BY s.id, s.symbol, s.strategy_id, s.direction, s.entry_price,
         s.signal_type, s.signal_intent, s.market_type, s.created_at,
         s.status, s.actual_return_pct;

COMMIT;

-- =================================================================
-- 验证
-- =================================================================

-- 检查字段
SELECT table_name, column_name, data_type
FROM information_schema.columns
WHERE column_name = 'signal_id'
  AND table_name IN ('strategy_analysis_log', 'live_strategy_log', 'trade_logs', 'risk_logs')
ORDER BY table_name;

-- 检查视图
SELECT viewname FROM pg_views WHERE viewname IN ('v_signal_timeline', 'v_signal_summary');
