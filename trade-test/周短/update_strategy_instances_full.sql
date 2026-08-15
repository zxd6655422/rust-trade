-- 第十六次分析优化: 全量UPDATE格式 (含 realized_vol_threshold)
-- 日期: 2026-08-15

UPDATE public.strategy_instances
SET strategy_type='ma_trend_pullback', display_name='MA趋势回踩-BTC', params='{"stop_mode": "ma288", "min_angle_5m": 0, "bbw_threshold": 0, "hard_stop_pct": 1.5, "vol_threshold": 0, "realized_vol_threshold": 0.426, "fast_ma_period": 288, "slow_ma_period": 480, "entry_timeframe": "30m", "slope_threshold": 0, "take_profit_mode": "trailing", "use_5m_expanding": false, "use_30m_expanding": false, "trailing_activate_pct": 4.0, "trailing_callback_pct": 1.0}'::jsonb, status='active', symbols='{BTCUSDT}', auto_trade=false, position_size_pct=10.00, exchange='binance', market_type='futures', note='30m穿越入场, 硬止损1.5%, 移动止盈4+1, realized_vol_48<0.426过滤, 第16次分析优化', created_at='2026-07-24 02:17:43.078', updated_at=NOW(), is_default=false, default_for=NULL
WHERE id='32eba113-71ee-4718-b322-e2efb849ecc3'::uuid;

UPDATE public.strategy_instances
SET strategy_type='ma_trend_pullback', display_name='MA趋势回踩-SOL', params='{"stop_mode": "ma288", "min_angle_5m": 0, "bbw_threshold": 0, "hard_stop_pct": 2.0, "vol_threshold": 0, "realized_vol_threshold": 0.790, "fast_ma_period": 288, "slow_ma_period": 480, "entry_timeframe": "30m", "slope_threshold": 0, "take_profit_mode": "trailing", "use_5m_expanding": false, "use_30m_expanding": false, "trailing_activate_pct": 4.0, "trailing_callback_pct": 1.0}'::jsonb, status='active', symbols='{SOLUSDT}', auto_trade=false, position_size_pct=10.00, exchange='binance', market_type='futures', note='30m穿越入场, 硬止损2%, 移动止盈4+1, realized_vol_48<0.790过滤, 第16次分析优化', created_at='2026-07-24 02:17:43.145', updated_at=NOW(), is_default=false, default_for=NULL
WHERE id='6beb73a5-a532-4023-aef7-b7819cde33fb'::uuid;

UPDATE public.strategy_instances
SET strategy_type='ma_trend_pullback', display_name='MA趋势回踩-ETH', params='{"stop_mode": "ma288", "min_angle_5m": 0, "bbw_threshold": 0, "hard_stop_pct": 1.5, "vol_threshold": 0, "realized_vol_threshold": 0.445, "fast_ma_period": 288, "slow_ma_period": 480, "entry_timeframe": "30m", "slope_threshold": 0, "take_profit_mode": "trailing", "use_5m_expanding": false, "use_30m_expanding": false, "trailing_activate_pct": 5.0, "trailing_callback_pct": 1.0}'::jsonb, status='active', symbols='{ETHUSDT}', auto_trade=false, position_size_pct=10.00, exchange='binance', market_type='futures', note='30m穿越入场, 硬止损1.5%, 移动止盈5+1, realized_vol_48<0.445过滤, 第16次分析优化', created_at='2026-07-24 02:17:43.112', updated_at=NOW(), is_default=false, default_for=NULL
WHERE id='f56ad8cc-adb4-42b9-b141-990165e29b9c'::uuid;
