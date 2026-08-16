# 研究基线（必读）
- 每次开始研究前，先读 `最新研究结论.md`，以「最新结论」为下一次研究的基础，不要回到无过滤基线。
- 策略区分：30m 双均线回踩（生产已部署）、5m 双均线回踩（后续新测试，尚未开始）。

# 数据
- 目录 F:\rust-projects\trade-test\data_2026-08-13
- BTC数据： 
- F:\rust-projects\trade-test/data_2026-08-13/kline_30m_202608131242_BTC.csv
- F:\rust-projects\trade-test/data_2026-08-13/kline_5m_202608131243_BTC.csv
- ETH数据：
- F:\rust-projects\trade-test/data_2026-08-13/kline_30m_202608131245_ETH.csv
- F:\rust-projects\trade-test/data_2026-08-13/kline_5m_202608131246_ETH.csv
- SOL数据
- F:\rust-projects\trade-test/data_2026-08-13/kline_30m_202608131247_SOL.csv
- F:\rust-projects\trade-test/data_2026-08-13/kline_5m_202608131248_SOL.csv

# 策略参数
- BTC:
|id|strategy_type|display_name|params|status|symbols|auto_trade|position_size_pct|exchange|market_type|note|created_at|updated_at|is_default|default_for|
|--|-------------|------------|------|------|-------|----------|-----------------|--------|-----------|----|----------|----------|----------|-----------|
|32eba113-71ee-4718-b322-e2efb849ecc3|ma_trend_pullback|MA趋势回踩-BTC|{"stop_mode": "ma288", "min_angle_5m": 0, "bbw_threshold": 0, "hard_stop_pct": 1.5, "vol_threshold": 0, "  - fetch_klines_from_exchange 签名改动会牵连 hybrid_load/fill_gap_from_exchange/load_full_from_exchange/poll_single 四处调用点（都从传 tf.as_str() 改成传
tf），这个连带是预期内的。": 288, "slow_ma_period": 488, "entry_timeframe": "30m", "slope_threshold": 0, "take_profit_mode": "trailing", "use_5m_expanding": false, "use_30m_expanding": false, "trailing_activate_pct": 4.0, "trailing_callback_pct": 1.0}|active|{BTCUSDT}|false|10.00|binance|futures|30m穿越入场, 硬止损1.5%, 移动止盈4+1, 无过滤, 回测+68.66%|2026-07-24 02:17:43.078 +0800|2026-08-01 11:54:02.181 +0800|false||


- ETH:
|id|strategy_type|display_name|params|status|symbols|auto_trade|position_size_pct|exchange|market_type|note|created_at|updated_at|is_default|default_for|
|--|-------------|------------|------|------|-------|----------|-----------------|--------|-----------|----|----------|----------|----------|-----------|
|f56ad8cc-adb4-42b9-b141-990165e29b9c|ma_trend_pullback|MA趋势回踩-ETH|{"stop_mode": "ma288", "min_angle_5m": 0, "bbw_threshold": 0, "hard_stop_pct": 1.5, "vol_threshold": 0, "  - fetch_klines_from_exchange 签名改动会牵连 hybrid_load/fill_gap_from_exchange/load_full_from_exchange/poll_single 四处调用点（都从传 tf.as_str() 改成传
tf），这个连带是预期内的。": 288, "slow_ma_period": 488, "entry_timeframe": "30m", "slope_threshold": 0, "take_profit_mode": "trailing", "use_5m_expanding": false, "use_30m_expanding": false, "trailing_activate_pct": 5.0, "trailing_callback_pct": 1.0}|active|{ETHUSDT}|false|10.00|binance|futures|30m穿越入场, 硬止损1.5%, 移动止盈5+1, 无扩散, 回测+85.72%|2026-07-24 02:17:43.112 +0800|2026-08-01 11:20:13.603 +0800|false||


- SOL:
|id|strategy_type|display_name|params|status|symbols|auto_trade|position_size_pct|exchange|market_type|note|created_at|updated_at|is_default|default_for|
|--|-------------|------------|------|------|-------|----------|-----------------|--------|-----------|----|----------|----------|----------|-----------|
|6beb73a5-a532-4023-aef7-b7819cde33fb|ma_trend_pullback|MA趋势回踩-SOL|{"stop_mode": "ma288", "min_angle_5m": 0, "bbw_threshold": 0, "hard_stop_pct": 2.0, "vol_threshold": 0, "  - fetch_klines_from_exchange 签名改动会牵连 hybrid_load/fill_gap_from_exchange/load_full_from_exchange/poll_single 四处调用点（都从传 tf.as_str() 改成传
tf），这个连带是预期内的。": 288, "slow_ma_period": 488, "entry_timeframe": "30m", "slope_threshold": 0, "take_profit_mode": "trailing", "use_5m_expanding": false, "use_30m_expanding": false, "trailing_activate_pct": 4.0, "trailing_callback_pct": 1.0}|active|{SOLUSDT}|false|10.00|binance|futures|30m穿越入场, 硬止损2%, 移动止盈4+1, 无扩散, 回测+116%|2026-07-24 02:17:43.145 +0800|2026-08-01 10:19:47.729 +0800|false||


# 生产环境策略信号数据表
- F:\rust-projects\trade-test\rust-trade-prod-strategy_signals
