-- =================================================================
-- 迁移脚本: 优化日志表结构
-- 执行方式: psql -U your_user -d your_db -f sql/migrations/20260725_optimize_log_tables.sql
-- 创建时间：2026-07-25
-- =================================================================

BEGIN;

-- =================================================================
-- 1. strategy_analysis_log 表优化
-- =================================================================

-- 添加缺失字段
ALTER TABLE strategy_analysis_log
ADD COLUMN IF NOT EXISTS signal_type VARCHAR(10) DEFAULT 'HOLD';

ALTER TABLE strategy_analysis_log
ADD COLUMN IF NOT EXISTS signal_intent VARCHAR(20) DEFAULT 'entry';

ALTER TABLE strategy_analysis_log
ADD COLUMN IF NOT EXISTS market_type VARCHAR(20) DEFAULT 'futures';

ALTER TABLE strategy_analysis_log
ADD COLUMN IF NOT EXISTS instance_id UUID;

ALTER TABLE strategy_analysis_log
ADD COLUMN IF NOT EXISTS signal_strength DECIMAL(5, 4);

ALTER TABLE strategy_analysis_log
ADD COLUMN IF NOT EXISTS stop_loss DECIMAL(20, 8);

ALTER TABLE strategy_analysis_log
ADD COLUMN IF NOT EXISTS take_profit DECIMAL(20, 8);

ALTER TABLE strategy_analysis_log
ADD COLUMN IF NOT EXISTS market_context JSONB;

-- 添加约束
ALTER TABLE strategy_analysis_log
ADD CONSTRAINT chk_analysis_signal_type CHECK (signal_type IN ('BUY', 'SELL', 'HOLD'));

ALTER TABLE strategy_analysis_log
ADD CONSTRAINT chk_analysis_signal_intent CHECK (signal_intent IN ('entry', 'exit', 'reverse'));

ALTER TABLE strategy_analysis_log
ADD CONSTRAINT chk_analysis_market_type CHECK (market_type IN ('futures', 'spot', 'both'));

-- 添加注释
COMMENT ON COLUMN strategy_analysis_log.signal_type IS '信号方向: BUY/SELL/HOLD';
COMMENT ON COLUMN strategy_analysis_log.signal_intent IS '信号意图: entry(开仓)/exit(平仓)/reverse(反手)';
COMMENT ON COLUMN strategy_analysis_log.market_type IS '目标市场: futures/spot/both';
COMMENT ON COLUMN strategy_analysis_log.instance_id IS '策略实例ID';
COMMENT ON COLUMN strategy_analysis_log.signal_strength IS '信号强度(0-1)';
COMMENT ON COLUMN strategy_analysis_log.stop_loss IS '止损价';
COMMENT ON COLUMN strategy_analysis_log.take_profit IS '止盈价';
COMMENT ON COLUMN strategy_analysis_log.market_context IS '市场上下文(JSON)';

-- =================================================================
-- 2. live_strategy_log 表优化
-- =================================================================

-- 添加缺失字段
ALTER TABLE live_strategy_log
ADD COLUMN IF NOT EXISTS signal_intent VARCHAR(20) DEFAULT 'entry';

ALTER TABLE live_strategy_log
ADD COLUMN IF NOT EXISTS market_type VARCHAR(20) DEFAULT 'futures';

ALTER TABLE live_strategy_log
ADD COLUMN IF NOT EXISTS entry_price DECIMAL(20, 8);

ALTER TABLE live_strategy_log
ADD COLUMN IF NOT EXISTS stop_loss DECIMAL(20, 8);

ALTER TABLE live_strategy_log
ADD COLUMN IF NOT EXISTS take_profit DECIMAL(20, 8);

ALTER TABLE live_strategy_log
ADD COLUMN IF NOT EXISTS position_quantity DECIMAL(20, 8);

ALTER TABLE live_strategy_log
ADD COLUMN IF NOT EXISTS unrealized_pnl DECIMAL(20, 8);

-- 添加约束
ALTER TABLE live_strategy_log
ADD CONSTRAINT chk_live_signal_intent CHECK (signal_intent IN ('entry', 'exit', 'reverse'));

ALTER TABLE live_strategy_log
ADD CONSTRAINT chk_live_market_type CHECK (market_type IN ('futures', 'spot', 'both'));

-- 添加注释
COMMENT ON COLUMN live_strategy_log.signal_intent IS '信号意图: entry(开仓)/exit(平仓)/reverse(反手)';
COMMENT ON COLUMN live_strategy_log.market_type IS '目标市场: futures/spot/both';
COMMENT ON COLUMN live_strategy_log.entry_price IS '入场价';
COMMENT ON COLUMN live_strategy_log.stop_loss IS '止损价';
COMMENT ON COLUMN live_strategy_log.take_profit IS '止盈价';
COMMENT ON COLUMN live_strategy_log.position_quantity IS '持仓数量';
COMMENT ON COLUMN live_strategy_log.unrealized_pnl IS '未实现盈亏';

-- =================================================================
-- 3. 创建索引
-- =================================================================

CREATE INDEX IF NOT EXISTS idx_analysis_signal_type ON strategy_analysis_log(signal_type);
CREATE INDEX IF NOT EXISTS idx_analysis_signal_intent ON strategy_analysis_log(signal_intent);
CREATE INDEX IF NOT EXISTS idx_analysis_market_type ON strategy_analysis_log(market_type);
CREATE INDEX IF NOT EXISTS idx_analysis_instance ON strategy_analysis_log(instance_id);

CREATE INDEX IF NOT EXISTS idx_live_signal_intent ON live_strategy_log(signal_intent);
CREATE INDEX IF NOT EXISTS idx_live_market_type ON live_strategy_log(market_type);

COMMIT;

-- =================================================================
-- 验证
-- =================================================================

-- 检查 strategy_analysis_log 字段
SELECT 'strategy_analysis_log' as table_name, column_name, data_type, column_default
FROM information_schema.columns
WHERE table_name = 'strategy_analysis_log'
  AND column_name IN ('signal_type', 'signal_intent', 'market_type', 'instance_id',
                       'signal_strength', 'stop_loss', 'take_profit', 'market_context')
ORDER BY column_name;

-- 检查 live_strategy_log 字段
SELECT 'live_strategy_log' as table_name, column_name, data_type, column_default
FROM information_schema.columns
WHERE table_name = 'live_strategy_log'
  AND column_name IN ('signal_intent', 'market_type', 'entry_price',
                       'stop_loss', 'take_profit', 'position_quantity', 'unrealized_pnl')
ORDER BY column_name;
