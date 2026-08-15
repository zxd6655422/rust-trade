-- 新增 use_trend_reversal 开关：按币种控制「趋势反转出场」是否启用
-- 日期: 2026-08-16
-- 说明: 生产 engine.rs 的 check_exit_conditions() 已支持 use_trend_reversal 参数。
--       true(默认) = 启用趋势反转出场(MA288 与 slow MA 交叉平仓)；false = 关闭。
-- 回测结论(MA480 + vol过滤, 复利):
--   ETH  必须保留(ON +72.5pp)  -> 保持 true(默认, 无需改)
--   BTC  中性(±0.4pp)          -> 保持 true(默认)
--   SOL  取消更优(+58.4pp)      -> 设 false
--   (BNB/SUI/HYPE 若上线: BNB/HYPE 建议 false, SUI 中性)

-- SOL: 关闭趋势反转出场
UPDATE public.strategy_instances
SET params = params || '{"use_trend_reversal": false}'::jsonb,
    updated_at = NOW()
WHERE id = '6beb73a5-a532-4023-aef7-b7819cde33fb'::uuid;  -- SOLUSDT

-- BTC / ETH 保持默认 true（不关闭），如需显式声明可执行：
-- UPDATE public.strategy_instances
-- SET params = params || '{"use_trend_reversal": true}'::jsonb, updated_at = NOW()
-- WHERE id IN ('32eba113-71ee-4718-b322-e2efb849ecc3', 'f56ad8cc-adb4-42b9-b141-990165e29b9c');

-- 验证
SELECT id, display_name, params->>'use_trend_reversal' AS use_trend_reversal
FROM strategy_instances
WHERE id IN (
    '32eba113-71ee-4718-b322-e2efb849ecc3',  -- BTC
    'f56ad8cc-adb4-42b9-b141-990165e29b9c',  -- ETH
    '6beb73a5-a532-4023-aef7-b7819cde33fb'   -- SOL
);
