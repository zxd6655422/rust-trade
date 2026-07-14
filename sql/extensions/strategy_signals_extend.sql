-- strategy_signals 表扩展字段
-- 用于传递策略意图给执行层

-- 新增仓位建议百分比
ALTER TABLE public.strategy_signals
    ADD COLUMN IF NOT EXISTS position_size_pct numeric(10,4) DEFAULT NULL;

COMMENT ON COLUMN strategy_signals.position_size_pct IS '策略建议仓位百分比，如 0.02 = 2%，由执行层决定是否采纳';

-- 确保 stop_loss 和 take_profit 字段存在（schema 中已有，这里做防御性检查）
-- stop_loss: 策略建议的止损价格
-- take_profit: 策略建议的止盈价格
