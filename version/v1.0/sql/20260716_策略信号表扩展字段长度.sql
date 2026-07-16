-- =================================================================
-- 策略信号表扩展字段长度
-- closed_reason 字段从 50 扩展到 500，容纳完整错误信息
-- 创建时间：2026-07-16
-- =================================================================

-- 扩展 closed_reason 字段长度
ALTER TABLE strategy_signals ALTER COLUMN closed_reason TYPE VARCHAR(500);

\echo 'Strategy signals closed_reason field expanded to VARCHAR(500)!'
