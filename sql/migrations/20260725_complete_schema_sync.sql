-- =================================================================
-- 迁移脚本: 完整同步数据库 Schema
-- 执行方式: psql -U your_user -d your_db -f sql/migrations/20260725_complete_schema_sync.sql
-- 创建时间：2026-07-25
-- =================================================================

BEGIN;

-- =================================================================
-- 1. strategy_signals 表: 修复 signal_type + 添加 signal_intent
-- =================================================================

-- signal_type 当前存的是 'entry' (意图), 需要改为 BUY/SELL/HOLD (方向)
-- 先添加 signal_intent 字段存储意图
ALTER TABLE strategy_signals
ADD COLUMN IF NOT EXISTS signal_intent VARCHAR(20) DEFAULT 'entry';

-- 更新现有数据: 把 signal_type 的值移到 signal_intent
UPDATE strategy_signals
SET signal_intent = signal_type
WHERE signal_type IN ('entry', 'exit', 'reverse');

-- 重置 signal_type 为默认值
UPDATE strategy_signals
SET signal_type = 'HOLD'
WHERE signal_type IN ('entry', 'exit', 'reverse');

-- 添加约束
ALTER TABLE strategy_signals
DROP CONSTRAINT IF EXISTS chk_signal_type;

ALTER TABLE strategy_signals
ADD CONSTRAINT chk_signal_type CHECK (signal_type IN ('BUY', 'SELL', 'HOLD'));

ALTER TABLE strategy_signals
ADD CONSTRAINT chk_signal_intent CHECK (signal_intent IN ('entry', 'exit', 'reverse'));

COMMENT ON COLUMN strategy_signals.signal_type IS '信号方向: BUY(买入), SELL(卖出), HOLD(持有)';
COMMENT ON COLUMN strategy_signals.signal_intent IS '信号意图: entry(开仓), exit(平仓), reverse(反手)';

-- =================================================================
-- 2. strategy_analysis_log 表: 添加缺失字段
-- =================================================================

-- 添加 signal_type (方向)
ALTER TABLE strategy_analysis_log
ADD COLUMN IF NOT EXISTS signal_type VARCHAR(10) DEFAULT 'HOLD';

-- 添加 signal_intent (意图)
ALTER TABLE strategy_analysis_log
ADD COLUMN IF NOT EXISTS signal_intent VARCHAR(20) DEFAULT 'entry';

-- 添加 market_type (市场类型)
ALTER TABLE strategy_analysis_log
ADD COLUMN IF NOT EXISTS market_type VARCHAR(20) DEFAULT 'futures';

-- 添加 instance_id (策略实例)
ALTER TABLE strategy_analysis_log
ADD COLUMN IF NOT EXISTS instance_id UUID;

-- 添加 signal_strength (信号强度)
ALTER TABLE strategy_analysis_log
ADD COLUMN IF NOT EXISTS signal_strength DECIMAL(5, 4);

-- 添加 stop_loss (止损价)
ALTER TABLE strategy_analysis_log
ADD COLUMN IF NOT EXISTS stop_loss DECIMAL(20, 8);

-- 添加 take_profit (止盈价)
ALTER TABLE strategy_analysis_log
ADD COLUMN IF NOT EXISTS take_profit DECIMAL(20, 8);

-- 添加 market_context (市场上下文)
ALTER TABLE strategy_analysis_log
ADD COLUMN IF NOT EXISTS market_context JSONB;

-- 添加 signal_id (全链路追踪)
ALTER TABLE strategy_analysis_log
ADD COLUMN IF NOT EXISTS signal_id UUID;

-- 添加约束
ALTER TABLE strategy_analysis_log
ADD CONSTRAINT chk_analysis_signal_type CHECK (signal_type IN ('BUY', 'SELL', 'HOLD'));

ALTER TABLE strategy_analysis_log
ADD CONSTRAINT chk_analysis_signal_intent CHECK (signal_intent IN ('entry', 'exit', 'reverse'));

ALTER TABLE strategy_analysis_log
ADD CONSTRAINT chk_analysis_market_type CHECK (market_type IN ('futures', 'spot', 'both'));

-- 添加注释
COMMENT ON COLUMN strategy_analysis_log.signal_type IS '信号方向: BUY/SELL/HOLD';
COMMENT ON COLUMN strategy_analysis_log.signal_intent IS '信号意图: entry/exit/reverse';
COMMENT ON COLUMN strategy_analysis_log.market_type IS '目标市场: futures/spot/both';
COMMENT ON COLUMN strategy_analysis_log.instance_id IS '策略实例ID';
COMMENT ON COLUMN strategy_analysis_log.signal_strength IS '信号强度(0-1)';
COMMENT ON COLUMN strategy_analysis_log.stop_loss IS '止损价';
COMMENT ON COLUMN strategy_analysis_log.take_profit IS '止盈价';
COMMENT ON COLUMN strategy_analysis_log.market_context IS '市场上下文(JSON)';
COMMENT ON COLUMN strategy_analysis_log.signal_id IS '关联的策略信号ID(全链路追踪)';

-- =================================================================
-- 3. live_strategy_log 表: 添加缺失字段
-- =================================================================

-- 添加 signal_intent (意图)
ALTER TABLE live_strategy_log
ADD COLUMN IF NOT EXISTS signal_intent VARCHAR(20) DEFAULT 'entry';

-- 添加 market_type (市场类型)
ALTER TABLE live_strategy_log
ADD COLUMN IF NOT EXISTS market_type VARCHAR(20) DEFAULT 'futures';

-- 添加 signal_id (全链路追踪)
ALTER TABLE live_strategy_log
ADD COLUMN IF NOT EXISTS signal_id UUID;

-- 添加 entry_price (入场价)
ALTER TABLE live_strategy_log
ADD COLUMN IF NOT EXISTS entry_price DECIMAL(20, 8);

-- 添加 stop_loss (止损价)
ALTER TABLE live_strategy_log
ADD COLUMN IF NOT EXISTS stop_loss DECIMAL(20, 8);

-- 添加 take_profit (止盈价)
ALTER TABLE live_strategy_log
ADD COLUMN IF NOT EXISTS take_profit DECIMAL(20, 8);

-- 添加 position_quantity (持仓数量)
ALTER TABLE live_strategy_log
ADD COLUMN IF NOT EXISTS position_quantity DECIMAL(20, 8);

-- 添加 unrealized_pnl (未实现盈亏)
ALTER TABLE live_strategy_log
ADD COLUMN IF NOT EXISTS unrealized_pnl DECIMAL(20, 8);

-- 添加约束
ALTER TABLE live_strategy_log
ADD CONSTRAINT chk_live_signal_intent CHECK (signal_intent IN ('entry', 'exit', 'reverse'));

ALTER TABLE live_strategy_log
ADD CONSTRAINT chk_live_market_type CHECK (market_type IN ('futures', 'spot', 'both'));

-- 添加注释
COMMENT ON COLUMN live_strategy_log.signal_intent IS '信号意图: entry/exit/reverse';
COMMENT ON COLUMN live_strategy_log.market_type IS '目标市场: futures/spot/both';
COMMENT ON COLUMN live_strategy_log.signal_id IS '关联的策略信号ID(全链路追踪)';
COMMENT ON COLUMN live_strategy_log.entry_price IS '入场价';
COMMENT ON COLUMN live_strategy_log.stop_loss IS '止损价';
COMMENT ON COLUMN live_strategy_log.take_profit IS '止盈价';
COMMENT ON COLUMN live_strategy_log.position_quantity IS '持仓数量';
COMMENT ON COLUMN live_strategy_log.unrealized_pnl IS '未实现盈亏';

-- =================================================================
-- 4. 创建索引
-- =================================================================

-- strategy_signals 索引
CREATE INDEX IF NOT EXISTS idx_signals_intent ON strategy_signals(signal_intent);
CREATE INDEX IF NOT EXISTS idx_signals_signal_id ON strategy_signals(id);

-- strategy_analysis_log 索引
CREATE INDEX IF NOT EXISTS idx_analysis_signal_type ON strategy_analysis_log(signal_type);
CREATE INDEX IF NOT EXISTS idx_analysis_signal_intent ON strategy_analysis_log(signal_intent);
CREATE INDEX IF NOT EXISTS idx_analysis_market_type ON strategy_analysis_log(market_type);
CREATE INDEX IF NOT EXISTS idx_analysis_instance ON strategy_analysis_log(instance_id);
CREATE INDEX IF NOT EXISTS idx_analysis_signal_id ON strategy_analysis_log(signal_id);

-- live_strategy_log 索引
CREATE INDEX IF NOT EXISTS idx_live_signal_intent ON live_strategy_log(signal_intent);
CREATE INDEX IF NOT EXISTS idx_live_market_type ON live_strategy_log(market_type);
CREATE INDEX IF NOT EXISTS idx_live_signal_id ON live_strategy_log(signal_id);

-- trade_logs 和 risk_logs 索引 (如果不存在)
CREATE INDEX IF NOT EXISTS idx_trade_signal_id ON trade_logs(signal_id);
CREATE INDEX IF NOT EXISTS idx_risk_signal_id ON risk_logs(signal_id);

-- =================================================================
-- 5. 创建全链路查询视图
-- =================================================================

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
    COUNT(DISTINCT t.id) as trade_count,
    MIN(t.timestamp) as first_trade_time,
    MAX(t.timestamp) as last_trade_time,
    COUNT(DISTINCT r.id) as risk_check_count,
    COUNT(DISTINCT l.id) as tick_count,
    MIN(l.timestamp) as first_tick_time,
    MAX(l.timestamp) as last_tick_time,
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

-- 检查所有表的字段
SELECT table_name, column_name, data_type, column_default
FROM information_schema.columns
WHERE column_name IN ('signal_type', 'signal_intent', 'market_type', 'signal_id')
  AND table_name IN ('strategy_signals', 'strategy_analysis_log', 'live_strategy_log')
ORDER BY table_name, column_name;

-- 检查视图
SELECT viewname FROM pg_views
WHERE viewname IN ('v_signal_timeline', 'v_signal_summary');
