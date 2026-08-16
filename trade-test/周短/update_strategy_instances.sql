-- 第十六次分析优化: 新增 realized_vol_threshold 波动率过滤参数
-- 日期: 2026-08-15
-- 说明: 在现有参数基础上新增 realized_vol_threshold 字段，其他参数不变
-- 阈值来源: studies/001-per-coin-vol-threshold (walk-forward中位数)

-- BTC: realized_vol_threshold = 0.426
UPDATE public.strategy_instances
SET params = params || '{"realized_vol_threshold": 0.426}'::jsonb,
    note = '30m穿越入场, 硬止损1.5%, 移动止盈4+1, realized_vol_48<0.426过滤, 第16次分析优化',
    updated_at = NOW()
WHERE id = '32eba113-71ee-4718-b322-e2efb849ecc3'::uuid;

-- SOL: realized_vol_threshold = 0.790
UPDATE public.strategy_instances
SET params = params || '{"realized_vol_threshold": 0.790}'::jsonb,
    note = '30m穿越入场, 硬止损2%, 移动止盈4+1, realized_vol_48<0.790过滤, 第16次分析优化',
    updated_at = NOW()
WHERE id = '6beb73a5-a532-4023-aef7-b7819cde33fb'::uuid;

-- ETH: realized_vol_threshold = 0.445
UPDATE public.strategy_instances
SET params = params || '{"realized_vol_threshold": 0.445}'::jsonb,
    note = '30m穿越入场, 硬止损1.5%, 移动止盈5+1, realized_vol_48<0.445过滤, 第16次分析优化',
    updated_at = NOW()
WHERE id = 'f56ad8cc-adb4-42b9-b141-990165e29b9c'::uuid;

-- 验证: 查询更新后的参数
SELECT id, display_name, params->>'realized_vol_threshold' as vol_threshold, note
FROM strategy_instances
WHERE id IN (
    '32eba113-71ee-4718-b322-e2efb849ecc3'::uuid,
    '6beb73a5-a532-4023-aef7-b7819cde33fb'::uuid,
    'f56ad8cc-adb4-42b9-b141-990165e29b9c'::uuid
);
