-- =================================================================
-- 迁移脚本: 更新策略参数 (基于第14次分析优化结果)
-- 执行方式: psql -U your_user -d your_db -f sql/migrations/20260724_update_strategy_params.sql
-- 创建时间：2026-07-24
-- =================================================================

BEGIN;

-- =================================================================
-- 1. BTC 策略优化
-- 优化结果: +67.88%, 最大亏损-2.00%, 盈亏比10.30
-- =================================================================

UPDATE strategy_instances
SET params = '{
    "fast_ma_period": 288,
    "slow_ma_period": 488,
    "stop_mode": "ma288",
    "hard_stop_pct": 1.0,
    "take_profit_mode": "trailing",
    "trailing_activate_pct": 3.0,
    "trailing_callback_pct": 3.0,
    "slope_threshold": 5.0,
    "bbw_threshold": 2.0,
    "vol_threshold": 0.6,
    "use_30m_expanding": false,
    "use_5m_expanding": true,
    "min_angle_5m": 0,
    "entry_timeframe": "30m"
}',
note = '双均线趋势回踩策略 - BTC配置 (硬止损1%, 移动止盈3+3, 斜率+BBW+量过滤, 回测+67.88%)',
updated_at = NOW()
WHERE id = '32eba113-71ee-4718-b322-e2efb849ecc3';

-- =================================================================
-- 2. ETH 策略优化
-- 优化结果: +82.50%, 最大亏损-1.50%, 盈亏比55.00
-- =================================================================

UPDATE strategy_instances
SET params = '{
    "fast_ma_period": 288,
    "slow_ma_period": 488,
    "stop_mode": "ma288",
    "hard_stop_pct": 1.5,
    "take_profit_mode": "trailing",
    "trailing_activate_pct": 6.0,
    "trailing_callback_pct": 8.0,
    "slope_threshold": 0,
    "bbw_threshold": 0,
    "vol_threshold": 0,
    "use_30m_expanding": true,
    "use_5m_expanding": true,
    "min_angle_5m": 0,
    "entry_timeframe": "30m"
}',
note = '双均线趋势回踩策略 - ETH配置 (硬止损1.5%, 移动止盈6+8, 30m+5m双扩散, 无过滤, 回测+82.50%)',
updated_at = NOW()
WHERE id = 'f56ad8cc-adb4-42b9-b141-990165e29b9c';

-- =================================================================
-- 3. SOL 策略优化
-- 优化结果: +68.13%, 最大亏损-2.50%, 盈亏比10.37
-- =================================================================

UPDATE strategy_instances
SET params = '{
    "fast_ma_period": 288,
    "slow_ma_period": 488,
    "stop_mode": "ma288",
    "hard_stop_pct": 2.5,
    "take_profit_mode": "trailing",
    "trailing_activate_pct": 8.0,
    "trailing_callback_pct": 2.0,
    "slope_threshold": 0,
    "bbw_threshold": 0,
    "vol_threshold": 0,
    "use_30m_expanding": true,
    "use_5m_expanding": true,
    "min_angle_5m": 0,
    "entry_timeframe": "30m"
}',
note = '双均线趋势回踩策略 - SOL配置 (硬止损2.5%, 移动止盈8+2, 30m+5m双扩散, 无过滤, 回测+68.13%)',
updated_at = NOW()
WHERE id = '6beb73a5-a532-4023-aef7-b7819cde33fb';

COMMIT;

-- =================================================================
-- 验证更新结果
-- =================================================================

SELECT
    id,
    display_name,
    params->>'hard_stop_pct' as hard_stop,
    params->>'trailing_activate_pct' as activate,
    params->>'trailing_callback_pct' as callback,
    params->>'slope_threshold' as slope,
    params->>'use_30m_expanding' as use_30m,
    params->>'use_5m_expanding' as use_5m,
    note
FROM strategy_instances
WHERE id IN (
    '32eba113-71ee-4718-b322-e2efb849ecc3',
    'f56ad8cc-adb4-42b9-b141-990165e29b9c',
    '6beb73a5-a532-4023-aef7-b7819cde33fb'
);
