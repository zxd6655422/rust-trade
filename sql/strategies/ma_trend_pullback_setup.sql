-- MA Trend Pullback Strategy 配置脚本
-- 双均线趋势回踩策略: MA288/MA488 趋势判断 + MA288 交叉入场
-- 可选: 30m/5m双均线扩散过滤 + 硬止损保护
--
-- =================================================================
-- 第十四次分析优化结果 (2026-07-24)
-- =================================================================
--
-- BTC: +67.88% (硬止损1%, 移动止盈3+3, 斜率+BBW+量过滤, 盈亏比10.30)
-- ETH: +82.50% (硬止损1.5%, 移动止盈6+8, 30m+5m双扩散, 无过滤, 盈亏比55.00)
-- SOL: +68.13% (硬止损2.5%, 移动止盈8+2, 30m+5m双扩散, 无过滤, 盈亏比10.37)
--
-- =================================================================
-- 参数说明
-- =================================================================
--
-- 【止损参数】
-- stop_mode: 止损模式
--   - "ma288": MA288趋势止损 (收盘价穿越MA288时平仓)
--   - "fixed": 固定百分比止损
--
-- hard_stop_pct: 硬止损百分比 (从入场价计算)
--   - 0: 禁用硬止损
--   - 1.0: 入场价下方1% (适合BTC)
--   - 1.5: 入场价下方1.5% (适合ETH)
--   - 2.5: 入场价下方2.5% (适合SOL)
--   - 硬止损用K线极值判断，优先级高于MA288止损
--
-- 【止盈参数】
-- take_profit_mode: 止盈模式
--   - "trailing": 移动止盈 (推荐)
--   - "ma48": MA48交叉止盈
--   - "bb": 布林带止盈
--   - "none": 无止盈
--
-- trailing_activate_pct: 移动止盈激活阈值
--   - 盈利达到此百分比后启动跟踪
--
-- trailing_callback_pct: 移动止盈回撤阈值
--   - 从最高盈利回撤此百分比时平仓
--
-- 【入场过滤参数】
-- slope_threshold: 斜率阈值 (默认0, 禁用)
--   - 5.0: 只在MA288斜率>5时入场 (适合BTC)
--   - 0: 不过滤斜率
--
-- bbw_threshold: 布林带宽度阈值 (默认0, 禁用)
--   - 2.0: 只在BBW>2时入场 (适合BTC)
--   - 0: 不过滤BBW
--
-- vol_threshold: 成交量比率阈值 (默认0, 禁用)
--   - 0.6: 只在成交量>MA20的60%时入场 (适合BTC)
--   - 0: 不过滤成交量
--
-- 【扩散过滤参数】
-- use_30m_expanding: 30m双均线扩散过滤 (默认false)
--   - true: 只在30m MA288/MA488价差扩大时入场
--   - false: 不使用30m扩散过滤
--
-- use_5m_expanding: 5m双均线扩散过滤 (默认false)
--   - true: 只在5m MA288/MA488价差扩大时入场
--   - false: 不使用5m扩散过滤
--
-- min_angle_5m: 5m最小夹角阈值 (默认0, 禁用)
--   - 0: 不限制夹角
--   - 1.0: 只保留强趋势
--
-- entry_timeframe: 入场K线周期 (默认"30m")
--   - "30m": 用30m K线检测入场信号
--   - "5m": 用5m K线检测入场信号，趋势仍用30m判断
--
-- =================================================================
-- 币种配置建议
-- =================================================================
--
-- BTC: 需要斜率+BBW+量过滤，不用扩散
--   - slope_threshold: 5.0
--   - bbw_threshold: 2.0
--   - vol_threshold: 0.6
--   - use_30m_expanding: false
--   - use_5m_expanding: true
--
-- ETH: 不需要过滤，用30m+5m双扩散
--   - slope_threshold: 0
--   - bbw_threshold: 0
--   - vol_threshold: 0
--   - use_30m_expanding: true
--   - use_5m_expanding: true
--
-- SOL: 不需要过滤，用30m+5m双扩散
--   - slope_threshold: 0
--   - bbw_threshold: 0
--   - vol_threshold: 0
--   - use_30m_expanding: true
--   - use_5m_expanding: true
--

-- ============================================================
-- 1. BTCUSDT 策略实例
-- 优化结果: +67.88%, 最大亏损-2.00%, 盈亏比10.30
-- ============================================================

INSERT INTO strategy_instances (
    strategy_type,
    display_name,
    params,
    status,
    symbols,
    auto_trade,
    position_size_pct,
    exchange,
    market_type,
    note
) VALUES (
    'ma_trend_pullback',
    'MA趋势回踩-BTC',
    '{
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
    }'::jsonb,
    'active',
    ARRAY['BTCUSDT'],
    false,  -- 模拟模式，不开自动交易
    10.0,   -- 仓位比例 10%
    'binance',
    'futures',
    '双均线趋势回踩策略 - BTC配置 (硬止损1%, 移动止盈3+3, 斜率+BBW+量过滤, 回测+67.88%)'
);

-- ============================================================
-- 2. ETHUSDT 策略实例
-- 优化结果: +82.50%, 最大亏损-1.50%, 盈亏比55.00
-- ============================================================

INSERT INTO strategy_instances (
    strategy_type,
    display_name,
    params,
    status,
    symbols,
    auto_trade,
    position_size_pct,
    exchange,
    market_type,
    note
) VALUES (
    'ma_trend_pullback',
    'MA趋势回踩-ETH',
    '{
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
    }'::jsonb,
    'active',
    ARRAY['ETHUSDT'],
    false,
    10.0,
    'binance',
    'futures',
    '双均线趋势回踩策略 - ETH配置 (硬止损1.5%, 移动止盈6+8, 30m+5m双扩散, 无过滤, 回测+82.50%)'
);

-- ============================================================
-- 3. SOLUSDT 策略实例
-- 优化结果: +68.13%, 最大亏损-2.50%, 盈亏比10.37
-- ============================================================

INSERT INTO strategy_instances (
    strategy_type,
    display_name,
    params,
    status,
    symbols,
    auto_trade,
    position_size_pct,
    exchange,
    market_type,
    note
) VALUES (
    'ma_trend_pullback',
    'MA趋势回踩-SOL',
    '{
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
    }'::jsonb,
    'active',
    ARRAY['SOLUSDT'],
    false,
    10.0,
    'binance',
    'futures',
    '双均线趋势回踩策略 - SOL配置 (硬止损2.5%, 移动止盈8+2, 30m+5m双扩散, 无过滤, 回测+68.13%)'
);

-- ============================================================
-- 4. 查询已配置的策略实例
-- ============================================================

SELECT
    id,
    display_name,
    symbols,
    auto_trade,
    status,
    params->>'fast_ma_period' as fast_ma,
    params->>'slow_ma_period' as slow_ma,
    params->>'stop_mode' as stop_mode,
    params->>'hard_stop_pct' as hard_stop,
    params->>'take_profit_mode' as tp_mode,
    params->>'trailing_activate_pct' as trailing_activate,
    params->>'trailing_callback_pct' as trailing_callback,
    params->>'slope_threshold' as slope,
    params->>'bbw_threshold' as bbw,
    params->>'vol_threshold' as vol,
    params->>'use_30m_expanding' as use_30m,
    params->>'use_5m_expanding' as use_5m,
    params->>'entry_timeframe' as entry_tf,
    note,
    created_at
FROM strategy_instances
WHERE strategy_type = 'ma_trend_pullback'
ORDER BY created_at DESC;

-- ============================================================
-- 5. 启用自动交易（验证后使用）
-- ============================================================

-- 当回测和模拟交易验证通过后，启用自动交易
-- UPDATE strategy_instances
-- SET auto_trade = true, updated_at = now()
-- WHERE strategy_type = 'ma_trend_pullback'
--   AND display_name = 'MA趋势回踩-BTC';

-- ============================================================
-- 6. 暂停/恢复策略
-- ============================================================

-- 暂停策略
-- UPDATE strategy_instances
-- SET status = 'paused', updated_at = now()
-- WHERE strategy_type = 'ma_trend_pullback'
--   AND display_name = 'MA趋势回踩-BTC';

-- 恢复策略
-- UPDATE strategy_instances
-- SET status = 'active', updated_at = now()
-- WHERE strategy_type = 'ma_trend_pullback'
--   AND display_name = 'MA趋势回踩-BTC';
