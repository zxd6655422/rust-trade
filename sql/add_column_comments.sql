-- =================================================================
-- 数据库表字段注释文档
-- 为所有表和字段添加中文注释，方便数据库工具查看
-- 创建时间：2026-07-09
-- =================================================================

-- =================================================================
-- 1. 账户快照相关表 (account_snapshot.sql)
-- =================================================================

-- 账户快照表（账户级别汇总）
COMMENT ON TABLE account_snapshot IS '统一账户快照表 - 支持 Binance/OKX 等多交易所，定期采集账户余额和持仓汇总';
COMMENT ON COLUMN account_snapshot.id IS '主键ID';
COMMENT ON COLUMN account_snapshot.exchange IS '交易所名称：binance / okx';
COMMENT ON COLUMN account_snapshot.market_type IS '市场类型：spot(现货) / futures(合约) / swap(永续)';
COMMENT ON COLUMN account_snapshot.snapshot_at IS '快照采集时间';
COMMENT ON COLUMN account_snapshot.total_equity IS '总权益(USD)：账户总资产价值，包含未实现盈亏';
COMMENT ON COLUMN account_snapshot.total_balance IS '总余额(USD)：不含未实现盈亏的账户余额';
COMMENT ON COLUMN account_snapshot.available_balance IS '可用余额(USD)：可用于开新仓或提取的余额';
COMMENT ON COLUMN account_snapshot.frozen_balance IS '冻结余额(USD)：挂单占用或保证金冻结的金额';
COMMENT ON COLUMN account_snapshot.unrealized_pnl IS '未实现盈亏(USD)：当前持仓的浮动盈亏';
COMMENT ON COLUMN account_snapshot.initial_margin IS '初始保证金(USD)：开仓时占用的保证金（仅合约）';
COMMENT ON COLUMN account_snapshot.maint_margin IS '维持保证金(USD)：维持当前持仓所需的最低保证金（仅合约）';
COMMENT ON COLUMN account_snapshot.margin_ratio IS '保证金率：维持保证金/总权益，用于评估爆仓风险（仅合约）';
COMMENT ON COLUMN account_snapshot.position_count IS '持仓数量：当前持有的仓位数';
COMMENT ON COLUMN account_snapshot.raw_data IS '原始数据(JSON)：交易所API返回的原始响应，用于排查问题';

-- 资产余额详情表
COMMENT ON TABLE asset_balance IS '资产余额详情表 - 记录每个币种的详细余额信息';
COMMENT ON COLUMN asset_balance.id IS '主键ID';
COMMENT ON COLUMN asset_balance.exchange IS '交易所名称：binance / okx';
COMMENT ON COLUMN asset_balance.market_type IS '市场类型：spot(现货) / futures(合约) / swap(永续)';
COMMENT ON COLUMN asset_balance.asset IS '资产符号：如 USDT、BTC、ETH 等';
COMMENT ON COLUMN asset_balance.snapshot_at IS '快照采集时间';
COMMENT ON COLUMN asset_balance.total IS '总余额：该币种的总持有量';
COMMENT ON COLUMN asset_balance.available IS '可用余额：可自由支配的余额';
COMMENT ON COLUMN asset_balance.frozen IS '冻结余额：挂单或保证金冻结的数量';
COMMENT ON COLUMN asset_balance.unrealized_pnl IS '未实现盈亏：该币种持仓的浮动盈亏';
COMMENT ON COLUMN asset_balance.usd_value IS 'USD价值：该币种余额换算成美元的价值';

-- 持仓快照表
COMMENT ON TABLE position_snapshot IS '持仓快照表 - 记录每个持仓的详细状态';
COMMENT ON COLUMN position_snapshot.id IS '主键ID';
COMMENT ON COLUMN position_snapshot.exchange IS '交易所名称：binance / okx';
COMMENT ON COLUMN position_snapshot.symbol IS '统一交易对名称：系统内部使用的标准格式，如 BTCUSDT';
COMMENT ON COLUMN position_snapshot.raw_symbol IS '原始交易对名称：交易所实际使用的格式，如 Binance: BTCUSDT, OKX: BTC-USDT-SWAP';
COMMENT ON COLUMN position_snapshot.snapshot_at IS '快照采集时间';
COMMENT ON COLUMN position_snapshot.position_side IS '持仓方向：LONG(做多) / SHORT(做空) / BOTH(双向持仓) / NET(净持仓)';
COMMENT ON COLUMN position_snapshot.position_amt IS '持仓数量：正数表示多头，负数表示空头';
COMMENT ON COLUMN position_snapshot.entry_price IS '开仓均价：持仓的平均买入/卖出价格';
COMMENT ON COLUMN position_snapshot.mark_price IS '标记价格：交易所用于计算盈亏和强平的参考价格，通常基于指数';
COMMENT ON COLUMN position_snapshot.unrealized_pnl IS '未实现盈亏(USD)：当前持仓的浮动盈亏金额';
COMMENT ON COLUMN position_snapshot.leverage IS '杠杆倍数：如 10 表示 10 倍杠杆';
COMMENT ON COLUMN position_snapshot.margin_type IS '保证金模式：cross(全仓) / isolated(逐仓)';
COMMENT ON COLUMN position_snapshot.initial_margin IS '初始保证金(USD)：开仓占用的保证金';
COMMENT ON COLUMN position_snapshot.maint_margin IS '维持保证金(USD)：维持持仓所需的最低保证金';
COMMENT ON COLUMN position_snapshot.liquidation_price IS '强平价格：当标记价格达到此价格时将被强制平仓';
COMMENT ON COLUMN position_snapshot.notional IS '名义价值(USD)：持仓的总价值 = 持仓数量 × 标记价格';
COMMENT ON COLUMN position_snapshot.pnl_ratio IS '盈亏比例：未实现盈亏 / (开仓均价 × 持仓数量)，用于衡量收益率';
COMMENT ON COLUMN position_snapshot.raw_data IS '原始数据(JSON)：交易所API返回的原始响应';

-- =================================================================
-- 2. K线数据表 (kline_1m.sql, kline_high_timeframe.sql, kline_multi_timeframe.sql)
-- =================================================================

-- 1分钟K线表
COMMENT ON TABLE kline_1m IS '1分钟K线表 - 基础时间框架，所有高时间框架K线由此聚合生成';
COMMENT ON COLUMN kline_1m.timestamp IS 'K线开始时间（UTC）';
COMMENT ON COLUMN kline_1m.symbol IS '交易对名称：如 BTCUSDT、ETHUSDT';
COMMENT ON COLUMN kline_1m.open IS '开盘价：该分钟第一笔成交价';
COMMENT ON COLUMN kline_1m.high IS '最高价：该分钟内的最高成交价';
COMMENT ON COLUMN kline_1m.low IS '最低价：该分钟内的最低成交价';
COMMENT ON COLUMN kline_1m.close IS '收盘价：该分钟最后一笔成交价';
COMMENT ON COLUMN kline_1m.volume IS '成交量：该分钟内的总成交数量（以币为单位）';
COMMENT ON COLUMN kline_1m.trade_count IS '成交笔数：该分钟内的成交次数';

-- 5分钟K线表
COMMENT ON TABLE kline_5m IS '5分钟K线表 - 由1分钟K线聚合生成';
COMMENT ON COLUMN kline_5m.symbol IS '交易对名称';
COMMENT ON COLUMN kline_5m.open_time IS 'K线开始时间（UTC）';
COMMENT ON COLUMN kline_5m.open IS '开盘价：该5分钟第一笔成交价';
COMMENT ON COLUMN kline_5m.high IS '最高价：该5分钟内的最高成交价';
COMMENT ON COLUMN kline_5m.low IS '最低价：该5分钟内的最低成交价';
COMMENT ON COLUMN kline_5m.close IS '收盘价：该5分钟最后一笔成交价';
COMMENT ON COLUMN kline_5m.volume IS '成交量：该5分钟内的总成交数量';
COMMENT ON COLUMN kline_5m.trade_count IS '成交笔数：该5分钟内的成交次数';

-- 15分钟K线表
COMMENT ON TABLE kline_15m IS '15分钟K线表 - 由1分钟K线聚合生成';
COMMENT ON COLUMN kline_15m.symbol IS '交易对名称';
COMMENT ON COLUMN kline_15m.open_time IS 'K线开始时间（UTC）';
COMMENT ON COLUMN kline_15m.open IS '开盘价';
COMMENT ON COLUMN kline_15m.high IS '最高价';
COMMENT ON COLUMN kline_15m.low IS '最低价';
COMMENT ON COLUMN kline_15m.close IS '收盘价';
COMMENT ON COLUMN kline_15m.volume IS '成交量';
COMMENT ON COLUMN kline_15m.trade_count IS '成交笔数';

-- 30分钟K线表
COMMENT ON TABLE kline_30m IS '30分钟K线表 - 由1分钟K线聚合生成';
COMMENT ON COLUMN kline_30m.symbol IS '交易对名称';
COMMENT ON COLUMN kline_30m.open_time IS 'K线开始时间（UTC）';
COMMENT ON COLUMN kline_30m.open IS '开盘价';
COMMENT ON COLUMN kline_30m.high IS '最高价';
COMMENT ON COLUMN kline_30m.low IS '最低价';
COMMENT ON COLUMN kline_30m.close IS '收盘价';
COMMENT ON COLUMN kline_30m.volume IS '成交量';
COMMENT ON COLUMN kline_30m.trade_count IS '成交笔数';

-- 1小时K线表
COMMENT ON TABLE kline_1h IS '1小时K线表 - 由1分钟K线聚合生成';
COMMENT ON COLUMN kline_1h.symbol IS '交易对名称';
COMMENT ON COLUMN kline_1h.open_time IS 'K线开始时间（UTC）';
COMMENT ON COLUMN kline_1h.open IS '开盘价';
COMMENT ON COLUMN kline_1h.high IS '最高价';
COMMENT ON COLUMN kline_1h.low IS '最低价';
COMMENT ON COLUMN kline_1h.close IS '收盘价';
COMMENT ON COLUMN kline_1h.volume IS '成交量';
COMMENT ON COLUMN kline_1h.trade_count IS '成交笔数';

-- 2小时K线表
COMMENT ON TABLE kline_2h IS '2小时K线表 - 由1分钟K线聚合生成';
COMMENT ON COLUMN kline_2h.symbol IS '交易对名称';
COMMENT ON COLUMN kline_2h.open_time IS 'K线开始时间（UTC）';
COMMENT ON COLUMN kline_2h.open IS '开盘价';
COMMENT ON COLUMN kline_2h.high IS '最高价';
COMMENT ON COLUMN kline_2h.low IS '最低价';
COMMENT ON COLUMN kline_2h.close IS '收盘价';
COMMENT ON COLUMN kline_2h.volume IS '成交量';
COMMENT ON COLUMN kline_2h.trade_count IS '成交笔数';

-- 4小时K线表
COMMENT ON TABLE kline_4h IS '4小时K线表 - 由1分钟K线聚合生成';
COMMENT ON COLUMN kline_4h.symbol IS '交易对名称';
COMMENT ON COLUMN kline_4h.open_time IS 'K线开始时间（UTC）';
COMMENT ON COLUMN kline_4h.open IS '开盘价';
COMMENT ON COLUMN kline_4h.high IS '最高价';
COMMENT ON COLUMN kline_4h.low IS '最低价';
COMMENT ON COLUMN kline_4h.close IS '收盘价';
COMMENT ON COLUMN kline_4h.volume IS '成交量';
COMMENT ON COLUMN kline_4h.trade_count IS '成交笔数';

-- 日K线表
COMMENT ON TABLE kline_1d IS '日K线表 - 由1分钟K线聚合生成，按UTC 0点分割';
COMMENT ON COLUMN kline_1d.symbol IS '交易对名称';
COMMENT ON COLUMN kline_1d.open_time IS 'K线开始时间（UTC 00:00）';
COMMENT ON COLUMN kline_1d.open IS '开盘价';
COMMENT ON COLUMN kline_1d.high IS '最高价';
COMMENT ON COLUMN kline_1d.low IS '最低价';
COMMENT ON COLUMN kline_1d.close IS '收盘价';
COMMENT ON COLUMN kline_1d.volume IS '成交量';
COMMENT ON COLUMN kline_1d.trade_count IS '成交笔数';

-- 3日K线表
COMMENT ON TABLE kline_3d IS '3日K线表 - 由1分钟K线聚合生成';
COMMENT ON COLUMN kline_3d.symbol IS '交易对名称';
COMMENT ON COLUMN kline_3d.open_time IS 'K线开始时间（UTC）';
COMMENT ON COLUMN kline_3d.open IS '开盘价';
COMMENT ON COLUMN kline_3d.high IS '最高价';
COMMENT ON COLUMN kline_3d.low IS '最低价';
COMMENT ON COLUMN kline_3d.close IS '收盘价';
COMMENT ON COLUMN kline_3d.volume IS '成交量';
COMMENT ON COLUMN kline_3d.trade_count IS '成交笔数';

-- 周K线表
COMMENT ON TABLE kline_1w IS '周K线表 - 由1分钟K线聚合生成，按UTC周一0点分割';
COMMENT ON COLUMN kline_1w.symbol IS '交易对名称';
COMMENT ON COLUMN kline_1w.open_time IS 'K线开始时间（UTC 周一 00:00）';
COMMENT ON COLUMN kline_1w.open IS '开盘价';
COMMENT ON COLUMN kline_1w.high IS '最高价';
COMMENT ON COLUMN kline_1w.low IS '最低价';
COMMENT ON COLUMN kline_1w.close IS '收盘价';
COMMENT ON COLUMN kline_1w.volume IS '成交量';
COMMENT ON COLUMN kline_1w.trade_count IS '成交笔数';

-- =================================================================
-- 3. 市场情绪数据表 (market_sentiment.sql)
-- =================================================================

-- 资金费率表
COMMENT ON TABLE funding_rate IS '资金费率表 - 合约资金费率每8小时结算一次，反映多空力量对比';
COMMENT ON COLUMN funding_rate.symbol IS '交易对名称：如 BTCUSDT';
COMMENT ON COLUMN funding_rate.funding_rate IS '资金费率：正数表示多头付费给空头（市场偏多），负数反之。如 0.0001 表示 0.01%';
COMMENT ON COLUMN funding_rate.funding_time IS '结算时间：资金费率结算的UTC时间点，通常为 00:00、08:00、16:00';
COMMENT ON COLUMN funding_rate.mark_price IS '标记价格：结算时的标记价格，用于计算资金费用';

-- 持仓量表
COMMENT ON TABLE open_interest IS '持仓量表 - 记录未平仓合约数量，每分钟采集一次';
COMMENT ON COLUMN open_interest.symbol IS '交易对名称：如 BTCUSDT';
COMMENT ON COLUMN open_interest.open_interest IS '未平仓合约数量：当前市场上所有未平仓合约的总数量（以币为单位）';
COMMENT ON COLUMN open_interest.open_value IS '未平仓合约价值(USDT)：未平仓合约的总美元价值';
COMMENT ON COLUMN open_interest.timestamp IS '采集时间';

-- 多空比表
COMMENT ON TABLE long_short_ratio IS '多空比表 - 记录账户级别的多空比例，每5分钟采集一次';
COMMENT ON COLUMN long_short_ratio.symbol IS '交易对名称：如 BTCUSDT';
COMMENT ON COLUMN long_short_ratio.long_ratio IS '多头账户比例：持有多头仓位的账户占比（0-1）';
COMMENT ON COLUMN long_short_ratio.short_ratio IS '空头账户比例：持有空头仓位的账户占比（0-1）';
COMMENT ON COLUMN long_short_ratio.ratio IS '多空比：多头账户数/空头账户数，>1 表示多头占优';

-- =================================================================
-- 4. 交易对配置表 (symbol_config.sql, symbol_mapping.sql, trading_pairs.sql)
-- =================================================================

-- 交易对配置表
COMMENT ON TABLE symbol_config IS '交易对配置表 - 管理系统监控的交易对列表';
COMMENT ON COLUMN symbol_config.symbol IS '交易对名称：统一格式，如 BTCUSDT';
COMMENT ON COLUMN symbol_config.enabled IS '是否启用：true=正在监控，false=暂停监控';
COMMENT ON COLUMN symbol_config.added_at IS '添加时间';

-- 交易对映射表
COMMENT ON TABLE symbol_mapping IS '交易对映射表 - 解决不同交易所交易对名称不一致的问题';
COMMENT ON COLUMN symbol_mapping.id IS '主键ID';
COMMENT ON COLUMN symbol_mapping.unified_symbol IS '统一交易对名称：系统内部使用的标准格式，策略层使用此名称';
COMMENT ON COLUMN symbol_mapping.exchange IS '交易所名称：binance / okx';
COMMENT ON COLUMN symbol_mapping.exchange_symbol IS '交易所实际交易对名称：如 Binance: BTCUSDT, OKX: BTC-USDT-SWAP';
COMMENT ON COLUMN symbol_mapping.market_type IS '市场类型：spot(现货) / futures(合约)';
COMMENT ON COLUMN symbol_mapping.status IS '状态：active(可用) / inactive(不可用)';
COMMENT ON COLUMN symbol_mapping.note IS '备注：如 "Binance USDⓈ-M 合约"、"OKX 永续合约"';
COMMENT ON COLUMN symbol_mapping.created_at IS '创建时间';

-- 交易对表
COMMENT ON TABLE trading_pairs IS '交易对表 - 管理可交易的交易对及其状态';
COMMENT ON COLUMN trading_pairs.id IS '主键ID';
COMMENT ON COLUMN trading_pairs.symbol IS '交易对名称：统一格式，如 BTCUSDT';
COMMENT ON COLUMN trading_pairs.market_type IS '市场类型：spot(现货) / futures(合约)';
COMMENT ON COLUMN trading_pairs.exchange IS '交易所名称：默认 binance';
COMMENT ON COLUMN trading_pairs.status IS '状态：active(活跃) / paused(暂停) / archived(归档)';
COMMENT ON COLUMN trading_pairs.note IS '备注';
COMMENT ON COLUMN trading_pairs.created_at IS '创建时间';
COMMENT ON COLUMN trading_pairs.updated_at IS '更新时间';

-- =================================================================
-- 5. 策略相关表 (strategy_instances.sql, strategy_signals.sql, etc.)
-- =================================================================

-- 策略实例表
COMMENT ON TABLE strategy_instances IS '策略实例表 - 管理运行中的策略实例配置';
COMMENT ON COLUMN strategy_instances.id IS '主键ID (UUID)';
COMMENT ON COLUMN strategy_instances.strategy_type IS '策略类型：如 rsi_macd、multi_tf_trend 等';
COMMENT ON COLUMN strategy_instances.display_name IS '显示名称：用户自定义的策略名称，便于识别';
COMMENT ON COLUMN strategy_instances.params IS '策略参数(JSON)：包含技术指标参数、阈值等配置';
COMMENT ON COLUMN strategy_instances.status IS '状态：active(运行中) / paused(已暂停) / archived(已归档)';
COMMENT ON COLUMN strategy_instances.symbols IS '监控交易对数组：该策略关注的交易对列表，如 {BTCUSDT, ETHUSDT}';
COMMENT ON COLUMN strategy_instances.auto_trade IS '是否自动交易：true=信号触发自动下单，false=仅生成信号';
COMMENT ON COLUMN strategy_instances.position_size_pct IS '仓位比例(%)：每次开仓使用的资金占总资金的百分比，如 10.0 表示 10%';
COMMENT ON COLUMN strategy_instances.exchange IS '交易所名称：该策略使用的交易所';
COMMENT ON COLUMN strategy_instances.market_type IS '市场类型：spot(现货) / futures(合约)';
COMMENT ON COLUMN strategy_instances.note IS '备注：用户添加的备注信息';
COMMENT ON COLUMN strategy_instances.created_at IS '创建时间';
COMMENT ON COLUMN strategy_instances.updated_at IS '更新时间';

-- 策略信号表（当前版本）
COMMENT ON TABLE strategy_signals IS '策略信号表 - 记录策略生成的交易信号及执行状态';
COMMENT ON COLUMN strategy_signals.id IS '主键ID (UUID)';
COMMENT ON COLUMN strategy_signals.symbol IS '交易对名称';
COMMENT ON COLUMN strategy_signals.strategy_id IS '策略类型标识：如 rsi_macd、multi_tf_trend';
COMMENT ON COLUMN strategy_signals.direction IS '市场方向：bullish(看多) / bearish(看空) / neutral(中性)';
COMMENT ON COLUMN strategy_signals.entry_price IS '建议入场价：策略计算的理想入场价格';
COMMENT ON COLUMN strategy_signals.overall_confidence IS '综合置信度(0-1)：多时间框架分析的综合得分，越高越可靠';
COMMENT ON COLUMN strategy_signals.entry_allowed IS '是否允许入场：true=满足入场条件，false=仅分析不入场';
COMMENT ON COLUMN strategy_signals.entry_direction IS '入场方向：long(做多) / short(做空)，null 表示不入场';
COMMENT ON COLUMN strategy_signals.timeframe_details IS '时间框架详情(JSON)：各时间框架的分析结果，如 1h/4h/1d 的趋势和指标';
COMMENT ON COLUMN strategy_signals.order_id IS '订单ID：实际下单后交易所返回的订单编号';
COMMENT ON COLUMN strategy_signals.executed IS '是否已执行：true=已下单，false=未下单';
COMMENT ON COLUMN strategy_signals.status IS '信号状态：pending(待确认) / confirmed(已确认) / invalidated(已失效) / expired(已过期) / superseded(已被取代)';
COMMENT ON COLUMN strategy_signals.closed_reason IS '关闭原因：如 stop_loss(止损)、take_profit(止盈)、manual(手动)、expired(超时)';
COMMENT ON COLUMN strategy_signals.evaluated_at IS '最近评估时间：信号最后一次被评估的时间';
COMMENT ON COLUMN strategy_signals.best_price IS '最佳价格：信号生成后的最优价格（多头为最高价，空头为最低价）';
COMMENT ON COLUMN strategy_signals.worst_price IS '最差价格：信号生成后的最差价格（多头为最低价，空头为最高价）';
COMMENT ON COLUMN strategy_signals.eval_count IS '评估次数：信号被评估的总次数';
COMMENT ON COLUMN strategy_signals.closed_at IS '关闭时间：信号关闭的时间戳';
COMMENT ON COLUMN strategy_signals.close_price IS '平仓价格：实际平仓的价格';
COMMENT ON COLUMN strategy_signals.actual_return_pct IS '实际收益率(%)：平仓后的实际盈亏百分比';
COMMENT ON COLUMN strategy_signals.created_at IS '创建时间：信号生成的时间';
COMMENT ON COLUMN strategy_signals.instance_id IS '策略实例ID：关联到 strategy_instances 表';
COMMENT ON COLUMN strategy_signals.signal_strength IS '信号强度(0-1)：原始信号的强度，不考虑市场环境';
COMMENT ON COLUMN strategy_signals.market_context IS '市场上下文(JSON)：信号生成时的市场状态，如趋势、波动率等';
COMMENT ON COLUMN strategy_signals.stop_loss IS '止损价格：建议的止损触发价格';
COMMENT ON COLUMN strategy_signals.take_profit IS '止盈价格：建议的止盈触发价格';

-- 策略信号表（V1版本，历史保留）
COMMENT ON TABLE strategy_signals_v1 IS '策略信号表V1 - 旧版本信号表，已废弃，保留用于历史数据查询';
COMMENT ON COLUMN strategy_signals_v1.id IS '主键ID (UUID)';
COMMENT ON COLUMN strategy_signals_v1.strategy_id IS '策略标识';
COMMENT ON COLUMN strategy_signals_v1.symbol IS '交易对名称';
COMMENT ON COLUMN strategy_signals_v1.signal_time IS '信号时间';
COMMENT ON COLUMN strategy_signals_v1.signal_type IS '信号类型：BUY(买入) / SELL(卖出) / HOLD(持有)';
COMMENT ON COLUMN strategy_signals_v1.signal_price IS '信号价格';
COMMENT ON COLUMN strategy_signals_v1.signal_quantity IS '建议数量';
COMMENT ON COLUMN strategy_signals_v1.confidence IS '置信度(0-1)';
COMMENT ON COLUMN strategy_signals_v1.trend_direction IS '趋势方向';
COMMENT ON COLUMN strategy_signals_v1.timeframe_analysis IS '时间框架分析(JSON)';
COMMENT ON COLUMN strategy_signals_v1.created_at IS '创建时间';

-- 策略分析日志表
COMMENT ON TABLE strategy_analysis_log IS '策略分析日志表 - 记录策略分析过程及结果跟踪';
COMMENT ON COLUMN strategy_analysis_log.id IS '主键ID (UUID)';
COMMENT ON COLUMN strategy_analysis_log.symbol IS '交易对名称';
COMMENT ON COLUMN strategy_analysis_log.strategy_id IS '策略类型标识';
COMMENT ON COLUMN strategy_analysis_log.direction IS '分析方向：bullish(看多) / bearish(看空) / neutral(中性)';
COMMENT ON COLUMN strategy_analysis_log.entry_price IS '分析时的价格';
COMMENT ON COLUMN strategy_analysis_log.overall_confidence IS '综合置信度(0-1)';
COMMENT ON COLUMN strategy_analysis_log.entry_allowed IS '是否允许入场';
COMMENT ON COLUMN strategy_analysis_log.entry_direction IS '入场方向：long / short';
COMMENT ON COLUMN strategy_analysis_log.timeframe_details IS '时间框架详情(JSON)';
COMMENT ON COLUMN strategy_analysis_log.status IS '状态：pending(待确认) / confirmed(已确认) / invalidated(已失效) / expired(已过期) / superseded(已被取代)';
COMMENT ON COLUMN strategy_analysis_log.closed_reason IS '关闭原因';
COMMENT ON COLUMN strategy_analysis_log.evaluated_at IS '评估时间';
COMMENT ON COLUMN strategy_analysis_log.best_price IS '最佳价格';
COMMENT ON COLUMN strategy_analysis_log.worst_price IS '最差价格';
COMMENT ON COLUMN strategy_analysis_log.eval_count IS '评估次数';
COMMENT ON COLUMN strategy_analysis_log.closed_at IS '关闭时间';
COMMENT ON COLUMN strategy_analysis_log.close_price IS '平仓价格';
COMMENT ON COLUMN strategy_analysis_log.actual_return_pct IS '实际收益率(%)';
COMMENT ON COLUMN strategy_analysis_log.created_at IS '创建时间';

-- 策略性能统计表
COMMENT ON TABLE strategy_performance IS '策略性能统计表 - 定期汇总每个策略实例的运行指标';
COMMENT ON COLUMN strategy_performance.id IS '主键ID (UUID)';
COMMENT ON COLUMN strategy_performance.instance_id IS '策略实例ID：关联到 strategy_instances 表';
COMMENT ON COLUMN strategy_performance.period_start IS '统计周期开始时间';
COMMENT ON COLUMN strategy_performance.period_end IS '统计周期结束时间';
COMMENT ON COLUMN strategy_performance.total_signals IS '总信号数：该周期内生成的信号总数';
COMMENT ON COLUMN strategy_performance.buy_signals IS '买入信号数';
COMMENT ON COLUMN strategy_performance.sell_signals IS '卖出信号数';
COMMENT ON COLUMN strategy_performance.total_trades IS '总成交笔数';
COMMENT ON COLUMN strategy_performance.winning_trades IS '盈利笔数';
COMMENT ON COLUMN strategy_performance.losing_trades IS '亏损笔数';
COMMENT ON COLUMN strategy_performance.total_pnl IS '总盈亏(USD)：该周期内的累计盈亏';
COMMENT ON COLUMN strategy_performance.win_rate IS '胜率(0-1)：盈利笔数 / 总成交笔数';
COMMENT ON COLUMN strategy_performance.avg_win IS '平均盈利(USD)：盈利交易的平均收益';
COMMENT ON COLUMN strategy_performance.avg_loss IS '平均亏损(USD)：亏损交易的平均损失';
COMMENT ON COLUMN strategy_performance.profit_factor IS '盈亏比：总盈利 / 总亏损，>1 表示盈利大于亏损';
COMMENT ON COLUMN strategy_performance.max_drawdown IS '最大回撤(%)：从最高点到最低点的最大跌幅';
COMMENT ON COLUMN strategy_performance.updated_at IS '更新时间';

-- 实时策略日志表
COMMENT ON TABLE live_strategy_log IS '实时策略日志表 - 记录策略实时运行的日志';
COMMENT ON COLUMN live_strategy_log.id IS '主键ID (UUID)';
COMMENT ON COLUMN live_strategy_log.timestamp IS '日志时间';
COMMENT ON COLUMN live_strategy_log.strategy_id IS '策略类型标识';
COMMENT ON COLUMN live_strategy_log.symbol IS '交易对名称';
COMMENT ON COLUMN live_strategy_log.current_price IS '当前价格：日志记录时的市场价格';
COMMENT ON COLUMN live_strategy_log.signal_type IS '信号类型：BUY / SELL / HOLD';
COMMENT ON COLUMN live_strategy_log.portfolio_value IS '组合价值(USD)：当前账户总价值';
COMMENT ON COLUMN live_strategy_log.total_pnl IS '累计盈亏(USD)';
COMMENT ON COLUMN live_strategy_log.cache_hit IS '缓存命中：true=使用了缓存数据，false=实时获取';
COMMENT ON COLUMN live_strategy_log.processing_time_us IS '处理耗时(微秒)：策略计算所花费的时间';

-- =================================================================
-- 6. 交易记录表 (trades.sql, tick_data.sql)
-- =================================================================

-- 交易记录表
COMMENT ON TABLE trades IS '交易记录表 - 记录所有实际成交的交易';
COMMENT ON COLUMN trades.id IS '主键ID (UUID)';
COMMENT ON COLUMN trades.order_id IS '订单ID：交易所返回的订单编号';
COMMENT ON COLUMN trades.symbol IS '交易对名称';
COMMENT ON COLUMN trades.side IS '交易方向：BUY(买入) / SELL(卖出)';
COMMENT ON COLUMN trades.price IS '成交价格';
COMMENT ON COLUMN trades.quantity IS '成交数量';
COMMENT ON COLUMN trades.commission IS '手续费(USD)：交易所收取的手续费';
COMMENT ON COLUMN trades.realized_pnl IS '已实现盈亏(USD)：平仓时的实际盈亏';
COMMENT ON COLUMN trades.strategy_id IS '策略标识：产生该交易的策略';
COMMENT ON COLUMN trades.trade_time IS '成交时间';
COMMENT ON COLUMN trades.created_at IS '记录创建时间';
COMMENT ON COLUMN trades.exchange IS '交易所名称';
COMMENT ON COLUMN trades.market_type IS '市场类型：spot(现货) / futures(合约)';
COMMENT ON COLUMN trades.signal_id IS '信号ID：关联到 strategy_signals 表，表示该交易由哪个信号触发';
COMMENT ON COLUMN trades.order_status IS '订单状态：filled(已成交) / partial(部分成交) / cancelled(已取消)';
COMMENT ON COLUMN trades.order_type IS '订单类型：market(市价单) / limit(限价单)';
COMMENT ON COLUMN trades.leverage IS '杠杆倍数：合约交易使用的杠杆';
COMMENT ON COLUMN trades.slippage IS '滑点(%)：实际成交价与信号价的偏差百分比';
COMMENT ON COLUMN trades.metadata IS '扩展数据(JSON)：预留字段，存储额外交易信息';

-- 逐笔成交数据表
COMMENT ON TABLE tick_data IS '逐笔成交数据表 - 记录每笔成交的详细信息，数据量大';
COMMENT ON COLUMN tick_data.timestamp IS '成交时间（UTC）';
COMMENT ON COLUMN tick_data.symbol IS '交易对名称';
COMMENT ON COLUMN tick_data.price IS '成交价格';
COMMENT ON COLUMN tick_data.quantity IS '成交数量';
COMMENT ON COLUMN tick_data.side IS '成交方向：BUY(主动买入) / SELL(主动卖出)';
COMMENT ON COLUMN tick_data.trade_id IS '成交ID：交易所返回的唯一成交编号';
COMMENT ON COLUMN tick_data.is_buyer_maker IS '是否买方挂单成交：true=卖方主动吃单(maker)，false=买方主动吃单(taker)';

-- =================================================================
-- 7. 持仓表 (positions.sql)
-- =================================================================

COMMENT ON TABLE positions IS '持仓表 - 记录当前持有的仓位';
COMMENT ON COLUMN positions.id IS '主键ID (UUID)';
COMMENT ON COLUMN positions.symbol IS '交易对名称：唯一约束，每个交易对同时只能有一个仓位';
COMMENT ON COLUMN positions.side IS '持仓方向：LONG(做多) / SHORT(做空)';
COMMENT ON COLUMN positions.quantity IS '持仓数量：正数';
COMMENT ON COLUMN positions.avg_entry_price IS '开仓均价：所有成交的加权平均价格';
COMMENT ON COLUMN positions.current_price IS '当前价格：最新的市场价格';
COMMENT ON COLUMN positions.unrealized_pnl IS '未实现盈亏(USD)：当前持仓的浮动盈亏';
COMMENT ON COLUMN positions.realized_pnl IS '已实现盈亏(USD)：该仓位历史已平仓部分的累计盈亏';
COMMENT ON COLUMN positions.opened_at IS '开仓时间';
COMMENT ON COLUMN positions.updated_at IS '更新时间';
COMMENT ON COLUMN positions.exchange IS '交易所名称';
COMMENT ON COLUMN positions.market_type IS '市场类型：spot(现货) / futures(合约)';

-- =================================================================
-- 8. 价格缓存表 (price_cache.sql)
-- =================================================================

COMMENT ON TABLE price_cache IS '价格缓存表 - 缓存最新价格和24小时行情数据';
COMMENT ON COLUMN price_cache.symbol IS '交易对名称：主键';
COMMENT ON COLUMN price_cache.price IS '最新价格';
COMMENT ON COLUMN price_cache.change_24h IS '24小时涨跌幅(%)：如 5.25 表示上涨 5.25%';
COMMENT ON COLUMN price_cache.volume_24h IS '24小时成交量';
COMMENT ON COLUMN price_cache.updated_at IS '更新时间';

-- =================================================================
-- 9. 回测结果表 (backtest_results.sql)
-- =================================================================

COMMENT ON TABLE backtest_results IS '回测结果表 - 存储策略历史回测的性能指标';
COMMENT ON COLUMN backtest_results.id IS '主键ID (UUID)';
COMMENT ON COLUMN backtest_results.strategy_id IS '策略标识：如 rsi_macd';
COMMENT ON COLUMN backtest_results.symbol IS '交易对名称';
COMMENT ON COLUMN backtest_results.initial_capital IS '初始资金(USD)：回测开始时的资金';
COMMENT ON COLUMN backtest_results.final_capital IS '最终资金(USD)：回测结束时的资金';
COMMENT ON COLUMN backtest_results.return_pct IS '总收益率(%)：(最终资金-初始资金)/初始资金 × 100';
COMMENT ON COLUMN backtest_results.total_trades IS '总成交笔数';
COMMENT ON COLUMN backtest_results.winning_trades IS '盈利笔数';
COMMENT ON COLUMN backtest_results.losing_trades IS '亏损笔数';
COMMENT ON COLUMN backtest_results.win_rate IS '胜率(0-1)：盈利笔数/总笔数';
COMMENT ON COLUMN backtest_results.max_drawdown IS '最大回撤(%)：从最高点到最低点的最大跌幅';
COMMENT ON COLUMN backtest_results.sharpe_ratio IS '夏普比率：风险调整后收益，越高越好，>1 为良好';
COMMENT ON COLUMN backtest_results.profit_factor IS '盈亏比：总盈利/总亏损，>1 表示盈利大于亏损';
COMMENT ON COLUMN backtest_results.data_points IS '数据点数：回测使用的K线数量';
COMMENT ON COLUMN backtest_results.data_start_time IS '数据开始时间：回测数据的起始时间';
COMMENT ON COLUMN backtest_results.data_end_time IS '数据结束时间：回测数据的截止时间';
COMMENT ON COLUMN backtest_results.strategy_params IS '策略参数(JSON)：回测使用的参数配置';
COMMENT ON COLUMN backtest_results.created_at IS '创建时间：回测执行的时间';

-- =================================================================
-- 10. 交易持仓表 (trading_positions.sql)
-- =================================================================

COMMENT ON TABLE trading_positions IS '交易持仓表 - 记录当前活跃的持仓信息，与交易所同步';
COMMENT ON COLUMN trading_positions.id IS '主键ID (UUID)';
COMMENT ON COLUMN trading_positions.exchange IS '交易所名称：binance / okx';
COMMENT ON COLUMN trading_positions.symbol IS '交易对名称：如 BTCUSDT';
COMMENT ON COLUMN trading_positions.side IS '持仓方向：LONG(做多) / SHORT(做空)';
COMMENT ON COLUMN trading_positions.quantity IS '持仓数量：当前持有的合约/币数量';
COMMENT ON COLUMN trading_positions.avg_entry_price IS '开仓均价：所有成交的加权平均价格';
COMMENT ON COLUMN trading_positions.unrealized_pnl IS '未实现盈亏(USD)：当前持仓的浮动盈亏';
COMMENT ON COLUMN trading_positions.stop_loss_price IS '止损价格：设置的止损触发价，达到此价格自动平仓';
COMMENT ON COLUMN trading_positions.take_profit_price IS '止盈价格：设置的止盈触发价，达到此价格自动平仓';
COMMENT ON COLUMN trading_positions.leverage IS '杠杆倍数：合约交易使用的杠杆，如 10 表示 10 倍';
COMMENT ON COLUMN trading_positions.margin IS '保证金(USD)：该持仓占用的保证金金额';
COMMENT ON COLUMN trading_positions.created_at IS '创建时间：持仓建立的时间';
COMMENT ON COLUMN trading_positions.updated_at IS '更新时间：最后同步或变更的时间';

-- =================================================================
-- 11. 交易订单表 (trading_orders.sql)
-- =================================================================

COMMENT ON TABLE trading_orders IS '交易订单表 - 记录所有提交到交易所的订单';
COMMENT ON COLUMN trading_orders.id IS '主键ID (UUID)';
COMMENT ON COLUMN trading_orders.order_id IS '交易所订单ID：交易所返回的订单编号';
COMMENT ON COLUMN trading_orders.exchange IS '交易所名称：binance / okx';
COMMENT ON COLUMN trading_orders.symbol IS '交易对名称：如 BTCUSDT';
COMMENT ON COLUMN trading_orders.side IS '订单方向：BUY(买入) / SELL(卖出)';
COMMENT ON COLUMN trading_orders.order_type IS '订单类型：market(市价单) / limit(限价单) / stop(止损单)';
COMMENT ON COLUMN trading_orders.quantity IS '委托数量：订单的总数量';
COMMENT ON COLUMN trading_orders.price IS '委托价格：限价单的价格，市价单为 null';
COMMENT ON COLUMN trading_orders.status IS '订单状态：new(新建) / partially_filled(部分成交) / filled(已成交) / cancelled(已取消) / rejected(已拒绝)';
COMMENT ON COLUMN trading_orders.filled_quantity IS '已成交数量：实际成交的数量';
COMMENT ON COLUMN trading_orders.avg_price IS '成交均价：实际成交的加权平均价格';
COMMENT ON COLUMN trading_orders.commission IS '手续费：交易所收取的手续费金额';
COMMENT ON COLUMN trading_orders.commission_asset IS '手续费币种：手续费以哪种币种收取，如 USDT、BNB';
COMMENT ON COLUMN trading_orders.client_order_id IS '客户端订单ID：系统自定义的订单标识，用于去重和关联';
COMMENT ON COLUMN trading_orders.created_at IS '创建时间：订单提交时间';
COMMENT ON COLUMN trading_orders.updated_at IS '更新时间：订单最后状态变更时间';

-- =================================================================
-- 12. 交易日志表 (trade_logs.sql)
-- =================================================================

COMMENT ON TABLE trade_logs IS '交易日志表 - 记录交易执行的详细日志，用于审计和问题排查';
COMMENT ON COLUMN trade_logs.id IS '主键ID (UUID)';
COMMENT ON COLUMN trade_logs.timestamp IS '日志时间';
COMMENT ON COLUMN trade_logs.strategy_id IS '策略标识：产生该交易的策略类型';
COMMENT ON COLUMN trade_logs.symbol IS '交易对名称';
COMMENT ON COLUMN trade_logs.side IS '交易方向：BUY(买入) / SELL(卖出)';
COMMENT ON COLUMN trade_logs.quantity IS '成交数量';
COMMENT ON COLUMN trade_logs.price IS '成交价格';
COMMENT ON COLUMN trade_logs.order_id IS '订单ID：关联的交易所订单编号';
COMMENT ON COLUMN trade_logs.pnl IS '盈亏(USD)：该笔交易的盈亏金额，平仓时计算';
COMMENT ON COLUMN trade_logs.notes IS '备注：交易相关的额外说明或特殊情况记录';

-- =================================================================
-- 13. 系统配置表 (system_config.sql)
-- =================================================================

COMMENT ON TABLE system_config IS '系统配置表 - 存储系统级别的键值对配置';
COMMENT ON COLUMN system_config.key IS '配置键：唯一标识，如 scheduler_paused(调度器暂停)、max_position_size(最大仓位)';
COMMENT ON COLUMN system_config.value IS '配置值：JSON 或纯文本格式的配置内容';
COMMENT ON COLUMN system_config.updated_at IS '更新时间：配置最后修改的时间';

-- =================================================================
-- 14. 风控日志表 (risk_logs.sql)
-- =================================================================

COMMENT ON TABLE risk_logs IS '风控日志表 - 记录风险控制事件和决策';
COMMENT ON COLUMN risk_logs.id IS '主键ID (UUID)';
COMMENT ON COLUMN risk_logs.timestamp IS '事件时间';
COMMENT ON COLUMN risk_logs.event_type IS '事件类型：如 position_limit(仓位限制)、drawdown_alert(回撤警告)、margin_warning(保证金警告)、liquidation_risk(爆仓风险)';
COMMENT ON COLUMN risk_logs.symbol IS '交易对名称：涉及的交易对，全局事件为 null';
COMMENT ON COLUMN risk_logs.details IS '详情(JSON)：事件的详细信息，如触发条件、当前值、阈值等';
COMMENT ON COLUMN risk_logs.decision IS '风控决策：allow(允许) / reject(拒绝) / reduce(减仓) / close(强制平仓)';
