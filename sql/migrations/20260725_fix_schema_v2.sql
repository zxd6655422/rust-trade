-- =================================================================
-- 迁移脚本: 修复 Schema V2 (基于实际数据库结构)
-- 执行方式: psql -U your_user -d your_db -f sql/migrations/20260725_fix_schema_v2.sql
-- 创建时间：2026-07-25
-- =================================================================

BEGIN;

-- =================================================================
-- 1. strategy_signals 表: 修复 signal_type 命名冲突
-- =================================================================

-- 问题: signal_type 存的是 'entry' (意图), 但代码期望 BUY/SELL/HOLD (方向)
-- 解决: signal_intent 已经存了意图, signal_type 应该存方向

-- 先备份现有数据到 signal_intent (如果 signal_intent 是空的)
UPDATE strategy_signals
SET signal_intent = signal_type
WHERE signal_intent IS NULL OR signal_intent = 'entry';

-- 重置 signal_type 为 HOLD (默认值)
UPDATE strategy_signals
SET signal_type = 'HOLD'
WHERE signal_type IN ('entry', 'exit', 'reverse');

-- 更新约束
ALTER TABLE strategy_signals
DROP CONSTRAINT IF EXISTS chk_signal_type;

ALTER TABLE strategy_signals
ADD CONSTRAINT chk_signal_type CHECK (signal_type IN ('BUY', 'SELL', 'HOLD'));

COMMENT ON COLUMN strategy_signals.signal_type IS '信号方向: BUY(买入), SELL(卖出), HOLD(持有)';
COMMENT ON COLUMN strategy_signals.signal_intent IS '信号意图: entry(开仓), exit(平仓), reverse(反手)';

-- =================================================================
-- 2. strategy_analysis_log 表: 添加缺失字段
-- =================================================================

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

-- 修复 signal_type 约束 (当前是 'entry', 需要改为 BUY/SELL/HOLD)
UPDATE strategy_analysis_log
SET signal_type = 'HOLD'
WHERE signal_type IN ('entry', 'exit', 'reverse');

ALTER TABLE strategy_analysis_log
DROP CONSTRAINT IF EXISTS chk_analysis_signal_type;

ALTER TABLE strategy_analysis_log
ADD CONSTRAINT chk_analysis_signal_type CHECK (signal_type IN ('BUY', 'SELL', 'HOLD'));

-- 添加市场类型约束
ALTER TABLE strategy_analysis_log
ADD CONSTRAINT chk_analysis_market_type CHECK (market_type IN ('futures', 'spot', 'both'));

-- 添加注释
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
ADD CONSTRAINT chk_live_market_type CHECK (market_type IN ('futures', 'spot', 'both'));

-- 添加注释
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

-- strategy_analysis_log 索引
CREATE INDEX IF NOT EXISTS idx_analysis_signal_type ON strategy_analysis_log(signal_type);
CREATE INDEX IF NOT EXISTS idx_analysis_market_type ON strategy_analysis_log(market_type);
CREATE INDEX IF NOT EXISTS idx_analysis_instance ON strategy_analysis_log(instance_id);
CREATE INDEX IF NOT EXISTS idx_analysis_signal_id ON strategy_analysis_log(signal_id);

-- live_strategy_log 索引
CREATE INDEX IF NOT EXISTS idx_live_market_type ON live_strategy_log(market_type);
CREATE INDEX IF NOT EXISTS idx_live_signal_id ON live_strategy_log(signal_id);

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

COMMIT;

-- =================================================================
-- 验证
-- =================================================================

-- 检查 strategy_signals 字段
SELECT 'strategy_signals' as table_name, column_name, data_type, column_default
FROM information_schema.columns
WHERE table_name = 'strategy_signals'
  AND column_name IN ('signal_type', 'signal_intent', 'market_type')
ORDER BY column_name;

-- 检查 strategy_analysis_log 字段
SELECT 'strategy_analysis_log' as table_name, column_name, data_type, column_default
FROM information_schema.columns
WHERE table_name = 'strategy_analysis_log'
  AND column_name IN ('signal_type', 'signal_intent', 'market_type', 'instance_id',
                       'signal_strength', 'stop_loss', 'take_profit', 'market_context', 'signal_id')
ORDER BY column_name;

-- 检查 live_strategy_log 字段
SELECT 'live_strategy_log' as table_name, column_name, data_type, column_default
FROM information_schema.columns
WHERE table_name = 'live_strategy_log'
  AND column_name IN ('signal_intent', 'market_type', 'signal_id', 'entry_price',
                       'stop_loss', 'take_profit', 'position_quantity', 'unrealized_pnl')
ORDER BY column_name;

-- 检查视图
SELECT viewname FROM pg_views WHERE viewname = 'v_signal_timeline';
