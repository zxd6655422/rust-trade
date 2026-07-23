-- MA Trend Pullback Strategy 配置脚本
-- 双均线趋势回踩策略: MA288/MA488 趋势判断 + MA288 交叉入场
-- 可选: 5m双均线扩散过滤 (第十三次分析优化)
--
-- 回测验证结果 (30m + MA288止损, 无扩散过滤):
--   BTC: +42.79% (移动止盈5+5, 胜率15.2%, 盈亏比10.75)
--   ETH: +39.47% (MA48止盈3根, 胜率21.4%, 盈亏比7.23)
--   SOL: +41.47% (移动止盈5+5, 胜率15.3%, 盈亏比2.87)
--
-- 回测验证结果 (30m + MA288止损 + 5m扩散过滤):
--   BTC: +40.46% (5m入场+30m趋势+5m扩散, 胜率18.4%)
--   ETH: +69.44% (5m入场+30m趋势+5m扩散+夹角>1°, 胜率23.3%)
--   SOL: +84.45% (30m MA288+5m+30m双扩散, 胜率21.1%)

-- ============================================================
-- 止盈策略类型说明
-- ============================================================
--
-- 1. trailing (移动止盈)
--    - trailing_activate_pct: 激活阈值，盈利达到此百分比后启动跟踪
--    - trailing_callback_pct: 回撤阈值，从最高盈利回撤此百分比时平仓
--    - 示例: 5+5 表示盈利5%激活，回撤5%平仓
--
-- 2. ma48 (MA48交叉止盈)
--    - ma48_tp_bars: 连续N根K线收盘价穿越MA48时平仓
--    - 示例: 3表示连续3根K线确认后平仓
--
-- 3. bb (布林带止盈)
--    - bb_tp_pct: 价格触及布林带上轨/下轨的百分比位置时平仓
--    - 示例: 90表示价格在布林带90%位置时平仓
--
-- 4. none (无止盈，仅依赖止损)
--
-- ============================================================
-- 5m扩散过滤参数说明 (第十三次分析优化)
-- ============================================================
--
-- use_5m_expanding: 是否启用5m双均线扩散过滤 (默认false)
--   - true: 只在5m双均线扩散阶段入场，过滤收敛阶段的假信号
--   - false: 不使用扩散过滤，保持原有逻辑
--
-- min_angle_5m: 最小夹角阈值 (默认0, 禁用)
--   - 0.3: 过滤小角度扩散，只保留强趋势
--   - 0: 不限制夹角
--
-- entry_timeframe: 入场K线周期 (默认"30m")
--   - "30m": 用30m K线检测入场信号 (原有逻辑)
--   - "5m": 用5m K线检测入场信号，趋势仍用30m判断 (更精准入场)
--
-- 适用建议:
--   - BTC: 不建议启用扩散 (回测显示收益下降), 用30m入场
--   - ETH: 建议启用5m入场+扩散+夹角>1° (收益+69.44%)
--   - SOL: 建议启用30m入场+扩散 (收益+84.45%)
--

-- ============================================================
-- 1. BTCUSDT 策略实例 (移动止盈 5+5)
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
        "take_profit_mode": "trailing",
        "trailing_activate_pct": 5.0,
        "trailing_callback_pct": 5.0,
        "primary_timeframe": "30m",
        "slope_threshold": 0,
        "bbw_threshold": 0,
        "vol_threshold": 0
    }'::jsonb,
    'active',
    ARRAY['BTCUSDT'],
    false,  -- 模拟模式，不开自动交易
    10.0,   -- 仓位比例 10%
    'binance',
    'futures',
    '双均线趋势回踩策略 - BTC配置 (移动止盈5+5, 回测+42.79%)'
);

-- ============================================================
-- 2. ETHUSDT 策略实例 (5m入场 + 5m扩散过滤 + 移动止盈)
-- ============================================================
-- 第十三次分析优化: 5m入场+扩散+夹角>1°使ETH收益从+29.95%提升到+69.44%

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
        "take_profit_mode": "trailing",
        "trailing_activate_pct": 5.0,
        "trailing_callback_pct": 5.0,
        "primary_timeframe": "30m",
        "entry_timeframe": "5m",
        "slope_threshold": 0,
        "bbw_threshold": 0,
        "vol_threshold": 0,
        "use_5m_expanding": true,
        "min_angle_5m": 1.0
    }'::jsonb,
    'active',
    ARRAY['ETHUSDT'],
    false,
    10.0,
    'binance',
    'futures',
    '双均线趋势回踩策略 - ETH配置 (5m入场+5m扩散+夹角>1°, 回测+69.44%)'
);

-- ============================================================
-- 3. SOLUSDT 策略实例 (5m+30m双扩散 + 移动止盈)
-- ============================================================
-- 第十三次分析优化: 5m+30m双扩散使SOL收益从+41.47%提升到+84.45%

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
        "take_profit_mode": "trailing",
        "trailing_activate_pct": 5.0,
        "trailing_callback_pct": 5.0,
        "primary_timeframe": "30m",
        "slope_threshold": 0,
        "bbw_threshold": 0,
        "vol_threshold": 0,
        "use_5m_expanding": true,
        "min_angle_5m": 0
    }'::jsonb,
    'active',
    ARRAY['SOLUSDT'],
    false,
    10.0,
    'binance',
    'futures',
    '双均线趋势回踩策略 - SOL配置 (5m扩散过滤, 回测+84.45%)'
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
    params->>'take_profit_mode' as tp_mode,
    params->>'trailing_activate_pct' as trailing_activate,
    params->>'trailing_callback_pct' as trailing_callback,
    params->>'ma48_tp_bars' as ma48_bars,
    params->>'entry_timeframe' as entry_tf,
    params->>'use_5m_expanding' as use_5m_expanding,
    params->>'min_angle_5m' as min_angle_5m,
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
