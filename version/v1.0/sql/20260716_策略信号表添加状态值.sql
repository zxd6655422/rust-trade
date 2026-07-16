-- =================================================================
-- 策略信号表添加新的状态值
-- 添加 executed 和 failed 状态
-- 创建时间：2026-07-16
-- =================================================================

-- 删除旧的约束
ALTER TABLE strategy_signals DROP CONSTRAINT IF EXISTS chk_engine_status;

-- 添加新的约束（包含所有状态值）
ALTER TABLE strategy_signals ADD CONSTRAINT chk_engine_status CHECK (
    status IN ('pending', 'confirmed', 'invalidated', 'expired', 'superseded', 'executed', 'failed', 'rejected')
);

\echo 'Strategy signals status constraint updated!'
