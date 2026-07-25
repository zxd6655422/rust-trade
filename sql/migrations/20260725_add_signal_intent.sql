-- =================================================================
-- 迁移脚本: 统一添加信号意图字段
-- 执行方式: psql -U your_user -d your_db -f sql/migrations/20260725_add_signal_intent.sql
-- 创建时间：2026-07-25
-- =================================================================
--
-- 信号类型说明:
-- signal_type: 方向类型 (BUY/SELL/HOLD)
-- signal_intent: 意图类型 (entry/exit/reverse)
--
-- 组合含义:
-- BUY  + entry   = 开多仓
-- BUY  + exit    = 平空仓 (止损/止盈/趋势反转)
-- SELL + entry   = 开空仓
-- SELL + exit    = 平多仓 (止损/止盈/趋势反转)
-- BUY  + reverse = 平空 + 开多
-- SELL + reverse = 平多 + 开空
--

BEGIN;

-- =================================================================
-- 1. strategy_signals 表
-- =================================================================

-- 添加 signal_type 字段 (方向: BUY/SELL/HOLD)
ALTER TABLE strategy_signals
ADD COLUMN IF NOT EXISTS signal_type VARCHAR(10) DEFAULT 'HOLD';

-- 添加 signal_intent 字段 (意图: entry/exit/reverse)
ALTER TABLE strategy_signals
ADD COLUMN IF NOT EXISTS signal_intent VARCHAR(20) DEFAULT 'entry';

-- 添加约束
ALTER TABLE strategy_signals
ADD CONSTRAINT chk_signal_type CHECK (signal_type IN ('BUY', 'SELL', 'HOLD'));

ALTER TABLE strategy_signals
ADD CONSTRAINT chk_signal_intent CHECK (signal_intent IN ('entry', 'exit', 'reverse'));

-- 添加注释
COMMENT ON COLUMN strategy_signals.signal_type IS '信号方向: BUY(买入), SELL(卖出), HOLD(持有)';
COMMENT ON COLUMN strategy_signals.signal_intent IS '信号意图: entry(开仓), exit(平仓), reverse(反手)';

-- =================================================================
-- 2. strategy_analysis_log 表
-- =================================================================

-- 添加 signal_intent 字段
ALTER TABLE strategy_analysis_log
ADD COLUMN IF NOT EXISTS signal_intent VARCHAR(20) DEFAULT 'entry';

ALTER TABLE strategy_analysis_log
ADD CONSTRAINT chk_analysis_signal_intent CHECK (signal_intent IN ('entry', 'exit', 'reverse'));

COMMENT ON COLUMN strategy_analysis_log.signal_intent IS '信号意图: entry(开仓), exit(平仓), reverse(反手)';

-- =================================================================
-- 3. live_strategy_log 表
-- =================================================================

-- 添加 signal_intent 字段
ALTER TABLE live_strategy_log
ADD COLUMN IF NOT EXISTS signal_intent VARCHAR(20) DEFAULT 'entry';

ALTER TABLE live_strategy_log
ADD CONSTRAINT chk_live_signal_intent CHECK (signal_intent IN ('entry', 'exit', 'reverse'));

COMMENT ON COLUMN live_strategy_log.signal_intent IS '信号意图: entry(开仓), exit(平仓), reverse(反手)';

-- =================================================================
-- 4. 创建索引
-- =================================================================

CREATE INDEX IF NOT EXISTS idx_signals_intent ON strategy_signals(signal_intent);
CREATE INDEX IF NOT EXISTS idx_analysis_intent ON strategy_analysis_log(signal_intent);
CREATE INDEX IF NOT EXISTS idx_live_intent ON live_strategy_log(signal_intent);

-- =================================================================
-- 5. 更新现有数据 (向后兼容)
-- =================================================================

-- 将现有信号标记为 entry (默认)
UPDATE strategy_signals SET signal_intent = 'entry' WHERE signal_intent IS NULL;
UPDATE strategy_analysis_log SET signal_intent = 'entry' WHERE signal_intent IS NULL;
UPDATE live_strategy_log SET signal_intent = 'entry' WHERE signal_intent IS NULL;

COMMIT;

-- =================================================================
-- 验证
-- =================================================================

-- 检查字段是否添加成功
SELECT table_name, column_name, data_type, column_default
FROM information_schema.columns
WHERE column_name IN ('signal_type', 'signal_intent')
  AND table_name IN ('strategy_signals', 'strategy_analysis_log', 'live_strategy_log')
ORDER BY table_name, column_name;

-- 检查数据分布
SELECT 'strategy_signals' as table_name, signal_type, signal_intent, COUNT(*)
FROM strategy_signals
GROUP BY signal_type, signal_intent
UNION ALL
SELECT 'live_strategy_log', signal_type, signal_intent, COUNT(*)
FROM live_strategy_log
GROUP BY signal_type, signal_intent;
