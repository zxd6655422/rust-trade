-- =================================================================
-- 数据库初始化脚本
-- 按正确顺序创建所有表结构
-- 执行方式: psql -U your_user -d your_db -f sql/init_database.sql
-- 创建时间：2026-07-16
-- =================================================================

-- 开始事务
BEGIN;

-- =================================================================
-- 第1层：基础配置表（无依赖）
-- =================================================================

-- 系统配置表
CREATE TABLE IF NOT EXISTS system_config (
    key VARCHAR(50) PRIMARY KEY,
    value TEXT NOT NULL,
    description VARCHAR(200),
    updated_at TIMESTAMPTZ DEFAULT NOW()
);

-- 交易所配置表
CREATE TABLE IF NOT EXISTS exchange_config (
    id VARCHAR(30) PRIMARY KEY,             -- 'binance-futures' / 'okx-futures'
    exchange_id VARCHAR(20) NOT NULL,       -- 'binance' / 'okx'
    market_type VARCHAR(10) NOT NULL,       -- 'spot' / 'futures'
    testnet BOOLEAN DEFAULT false,
    enabled BOOLEAN DEFAULT true,
    leverage INTEGER DEFAULT 10,
    created_at TIMESTAMPTZ DEFAULT NOW(),
    updated_at TIMESTAMPTZ DEFAULT NOW(),
    CONSTRAINT chk_exchange_market_type CHECK (market_type IN ('spot', 'futures'))
);

-- 交易对配置表
CREATE TABLE IF NOT EXISTS trading_pairs (
    id SERIAL PRIMARY KEY,
    symbol VARCHAR(20) NOT NULL,
    base_asset VARCHAR(10) NOT NULL,
    quote_asset VARCHAR(10) NOT NULL,
    exchange VARCHAR(20) NOT NULL,
    market_type VARCHAR(10) NOT NULL DEFAULT 'futures',
    price_precision INTEGER DEFAULT 8,
    quantity_precision INTEGER DEFAULT 8,
    min_quantity DECIMAL(20, 8),
    min_notional DECIMAL(20, 8),
    enabled BOOLEAN DEFAULT true,
    created_at TIMESTAMPTZ DEFAULT NOW(),
    UNIQUE(symbol, exchange, market_type)
);

-- 交易对映射表（统一格式与交易所原始格式）
CREATE TABLE IF NOT EXISTS symbol_mapping (
    id SERIAL PRIMARY KEY,
    exchange VARCHAR(20) NOT NULL,
    market_type VARCHAR(10) NOT NULL DEFAULT 'futures',
    unified_symbol VARCHAR(20) NOT NULL,    -- 统一格式: 'BTCUSDT'
    raw_symbol VARCHAR(50) NOT NULL,        -- 交易所格式: 'BTCUSDT' / 'BTC-USDT-SWAP'
    base_asset VARCHAR(10) NOT NULL,
    quote_asset VARCHAR(10) NOT NULL,
    price_precision INTEGER DEFAULT 8,
    quantity_precision INTEGER DEFAULT 8,
    min_quantity DECIMAL(20, 8),
    max_quantity DECIMAL(20, 8),
    min_notional DECIMAL(20, 8),
    tick_size DECIMAL(20, 8),
    step_size DECIMAL(20, 8),
    enabled BOOLEAN DEFAULT true,
    created_at TIMESTAMPTZ DEFAULT NOW(),
    updated_at TIMESTAMPTZ DEFAULT NOW(),
    UNIQUE(exchange, unified_symbol, market_type)
);

-- 交易对配置表（每个交易所实例的交易对列表）
CREATE TABLE IF NOT EXISTS symbol_config (
    id SERIAL PRIMARY KEY,
    exchange_config_id VARCHAR(30) NOT NULL REFERENCES exchange_config(id),
    symbol VARCHAR(20) NOT NULL,
    enabled BOOLEAN DEFAULT true,
    created_at TIMESTAMPTZ DEFAULT NOW(),
    UNIQUE(exchange_config_id, symbol)
);

-- =================================================================
-- 第2层：行情数据表
-- =================================================================

-- 1分钟K线表
CREATE TABLE IF NOT EXISTS kline_1m (
    symbol VARCHAR(20) NOT NULL,
    open_time TIMESTAMPTZ NOT NULL,
    open DECIMAL(20, 8) NOT NULL,
    high DECIMAL(20, 8) NOT NULL,
    low DECIMAL(20, 8) NOT NULL,
    close DECIMAL(20, 8) NOT NULL,
    volume DECIMAL(20, 8) NOT NULL,
    close_time TIMESTAMPTZ NOT NULL,
    quote_volume DECIMAL(20, 8) NOT NULL,
    trades_count INTEGER DEFAULT 0,
    PRIMARY KEY (symbol, open_time)
);

-- 高时间框架K线表
CREATE TABLE IF NOT EXISTS kline_high_timeframe (
    symbol VARCHAR(20) NOT NULL,
    timeframe VARCHAR(5) NOT NULL,          -- '5m' / '15m' / '30m' / '1h' / '2h' / '4h' / '1d' / '3d' / '1w'
    open_time TIMESTAMPTZ NOT NULL,
    open DECIMAL(20, 8) NOT NULL,
    high DECIMAL(20, 8) NOT NULL,
    low DECIMAL(20, 8) NOT NULL,
    close DECIMAL(20, 8) NOT NULL,
    volume DECIMAL(20, 8) NOT NULL,
    close_time TIMESTAMPTZ NOT NULL,
    quote_volume DECIMAL(20, 8) NOT NULL,
    trades_count INTEGER DEFAULT 0,
    source VARCHAR(10) DEFAULT '1m',        -- 数据来源: '1m' / 'exchange'
    PRIMARY KEY (symbol, timeframe, open_time)
);

-- 多时间框架K线表
CREATE TABLE IF NOT EXISTS kline_multi_timeframe (
    symbol VARCHAR(20) NOT NULL,
    timeframe VARCHAR(5) NOT NULL,
    open_time TIMESTAMPTZ NOT NULL,
    open DECIMAL(20, 8) NOT NULL,
    high DECIMAL(20, 8) NOT NULL,
    low DECIMAL(20, 8) NOT NULL,
    close DECIMAL(20, 8) NOT NULL,
    volume DECIMAL(20, 8) NOT NULL,
    close_time TIMESTAMPTZ NOT NULL,
    quote_volume DECIMAL(20, 8) NOT NULL,
    trades_count INTEGER DEFAULT 0,
    PRIMARY KEY (symbol, timeframe, open_time)
);

-- 逐笔成交数据表
CREATE TABLE IF NOT EXISTS tick_data (
    id BIGSERIAL,
    symbol VARCHAR(20) NOT NULL,
    price DECIMAL(20, 8) NOT NULL,
    quantity DECIMAL(20, 8) NOT NULL,
    side VARCHAR(4) NOT NULL,               -- 'BUY' / 'SELL'
    trade_id VARCHAR(50),
    timestamp TIMESTAMPTZ NOT NULL,
    PRIMARY KEY (symbol, timestamp, id)
);

-- 市场情绪数据表
CREATE TABLE IF NOT EXISTS market_sentiment (
    id BIGSERIAL PRIMARY KEY,
    symbol VARCHAR(20) NOT NULL,
    data_type VARCHAR(20) NOT NULL,         -- 'funding_rate' / 'open_interest' / 'long_short_ratio'
    value DECIMAL(20, 8) NOT NULL,
    timestamp TIMESTAMPTZ NOT NULL,
    raw_data JSONB,
    UNIQUE(symbol, data_type, timestamp)
);

-- 价格缓存表
CREATE TABLE IF NOT EXISTS price_cache (
    symbol VARCHAR(20) PRIMARY KEY,
    price DECIMAL(20, 8) NOT NULL,
    updated_at TIMESTAMPTZ DEFAULT NOW()
);

-- =================================================================
-- 第3层：策略相关表
-- =================================================================

-- 策略实例表
CREATE TABLE IF NOT EXISTS strategy_instances (
    id VARCHAR(50) PRIMARY KEY,
    name VARCHAR(100) NOT NULL,
    strategy_type VARCHAR(50) NOT NULL,
    description TEXT,
    parameters JSONB,
    enabled BOOLEAN DEFAULT true,
    is_default BOOLEAN DEFAULT false,
    default_for VARCHAR(20),                -- 'backtest' / 'paper' / 'live'
    created_at TIMESTAMPTZ DEFAULT NOW(),
    updated_at TIMESTAMPTZ DEFAULT NOW()
);

-- 策略信号表
CREATE TABLE IF NOT EXISTS strategy_signals (
    id UUID DEFAULT gen_random_uuid() PRIMARY KEY,
    strategy_id VARCHAR(50) NOT NULL,
    instance_id VARCHAR(50),
    symbol VARCHAR(20) NOT NULL,
    signal_type VARCHAR(10) NOT NULL,       -- 'BUY' / 'SELL' / 'HOLD'
    price DECIMAL(20, 8),
    quantity DECIMAL(20, 8),
    confidence DECIMAL(5, 4),
    reason TEXT,
    metadata JSONB,
    created_at TIMESTAMPTZ DEFAULT NOW()
);

-- 策略分析日志表
CREATE TABLE IF NOT EXISTS strategy_analysis_log (
    id BIGSERIAL PRIMARY KEY,
    strategy_id VARCHAR(50) NOT NULL,
    instance_id VARCHAR(50),
    symbol VARCHAR(20) NOT NULL,
    analysis_type VARCHAR(30) NOT NULL,
    result JSONB,
    created_at TIMESTAMPTZ DEFAULT NOW()
);

-- 策略绩效表
CREATE TABLE IF NOT EXISTS strategy_performance (
    id BIGSERIAL PRIMARY KEY,
    strategy_id VARCHAR(50) NOT NULL,
    instance_id VARCHAR(50),
    symbol VARCHAR(20),
    period VARCHAR(10) NOT NULL,            -- '1d' / '7d' / '30d'
    total_trades INTEGER DEFAULT 0,
    winning_trades INTEGER DEFAULT 0,
    losing_trades INTEGER DEFAULT 0,
    total_pnl DECIMAL(20, 8) DEFAULT 0,
    max_drawdown DECIMAL(20, 8) DEFAULT 0,
    win_rate DECIMAL(5, 4) DEFAULT 0,
    sharpe_ratio DECIMAL(10, 4),
    updated_at TIMESTAMPTZ DEFAULT NOW(),
    UNIQUE(strategy_id, instance_id, symbol, period)
);

-- 实时策略日志表
CREATE TABLE IF NOT EXISTS live_strategy_log (
    id BIGSERIAL PRIMARY KEY,
    strategy_id VARCHAR(50) NOT NULL,
    instance_id VARCHAR(50),
    symbol VARCHAR(20) NOT NULL,
    action VARCHAR(20) NOT NULL,
    details JSONB,
    created_at TIMESTAMPTZ DEFAULT NOW()
);

-- =================================================================
-- 第4层：交易相关表
-- =================================================================

-- 交易订单表
CREATE TABLE IF NOT EXISTS trading_orders (
    id UUID DEFAULT gen_random_uuid() PRIMARY KEY,
    order_id VARCHAR(50) NOT NULL,
    exchange VARCHAR(20) NOT NULL,
    market_type VARCHAR(10) NOT NULL DEFAULT 'futures',
    uid VARCHAR(20),
    symbol VARCHAR(20) NOT NULL,
    side VARCHAR(4) NOT NULL,
    order_type VARCHAR(20) NOT NULL,
    position_side VARCHAR(10) DEFAULT 'BOTH',
    quantity DECIMAL(20, 8) NOT NULL,
    price DECIMAL(20, 8),
    stop_price DECIMAL(20, 8),
    status VARCHAR(20) NOT NULL,
    filled_quantity DECIMAL(20, 8) DEFAULT 0,
    avg_price DECIMAL(20, 8),
    commission DECIMAL(20, 8),
    commission_asset VARCHAR(10),
    client_order_id VARCHAR(50),
    time_in_force VARCHAR(10) DEFAULT 'GTC',
    source VARCHAR(20) NOT NULL DEFAULT 'unknown',
    signal_id UUID,
    strategy_id VARCHAR(50),
    created_at TIMESTAMPTZ DEFAULT NOW(),
    updated_at TIMESTAMPTZ DEFAULT NOW(),
    CONSTRAINT trading_orders_order_id_exchange_key UNIQUE (order_id, exchange)
);

-- 持仓表
CREATE TABLE IF NOT EXISTS positions (
    id UUID DEFAULT gen_random_uuid() PRIMARY KEY,
    exchange VARCHAR(20) NOT NULL DEFAULT 'binance',
    market_type VARCHAR(10) NOT NULL DEFAULT 'futures',
    uid VARCHAR(20),
    symbol VARCHAR(20) NOT NULL,
    side VARCHAR(10) NOT NULL,
    quantity DECIMAL(20, 8) NOT NULL,
    avg_entry_price DECIMAL(20, 8) NOT NULL,
    current_price DECIMAL(20, 8),
    unrealized_pnl DECIMAL(20, 8),
    realized_pnl DECIMAL(20, 8) DEFAULT 0 NOT NULL,
    leverage INTEGER DEFAULT 1,
    margin_type VARCHAR(10) DEFAULT 'cross',
    liquidation_price DECIMAL(20, 8),
    opened_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ DEFAULT NOW(),
    CONSTRAINT positions_market_type_check CHECK (market_type IN ('spot', 'futures')),
    CONSTRAINT positions_side_check CHECK (side IN ('LONG', 'SHORT'))
);

-- 止损止盈订单表
CREATE TABLE IF NOT EXISTS stop_orders (
    id UUID DEFAULT gen_random_uuid() PRIMARY KEY,
    exchange VARCHAR(32) NOT NULL,
    market_type VARCHAR(16) NOT NULL DEFAULT 'futures',
    uid VARCHAR(20),
    symbol VARCHAR(32) NOT NULL,
    side VARCHAR(8) NOT NULL,
    quantity DECIMAL(20, 8) NOT NULL,
    entry_price DECIMAL(20, 8) NOT NULL,
    stop_loss_price DECIMAL(20, 8),
    take_profit_price DECIMAL(20, 8),
    trailing_stop_pct DECIMAL(10, 6),
    exchange_sl_order_id VARCHAR(128),
    exchange_tp_order_id VARCHAR(128),
    status VARCHAR(16) DEFAULT 'active' NOT NULL,
    triggered_at TIMESTAMPTZ,
    triggered_reason VARCHAR(32),
    created_at TIMESTAMPTZ DEFAULT NOW(),
    updated_at TIMESTAMPTZ DEFAULT NOW(),
    CONSTRAINT chk_stop_orders_status CHECK (
        status IN ('active', 'triggered', 'cancelled', 'expired')
    )
);

-- 交易记录表
CREATE TABLE IF NOT EXISTS trades (
    id BIGSERIAL PRIMARY KEY,
    trade_id VARCHAR(50),
    order_id VARCHAR(50) NOT NULL,
    exchange VARCHAR(20) NOT NULL,
    symbol VARCHAR(20) NOT NULL,
    side VARCHAR(4) NOT NULL,
    price DECIMAL(20, 8) NOT NULL,
    quantity DECIMAL(20, 8) NOT NULL,
    quote_quantity DECIMAL(20, 8),
    commission DECIMAL(20, 8),
    commission_asset VARCHAR(10),
    realized_pnl DECIMAL(20, 8),
    is_maker BOOLEAN DEFAULT false,
    timestamp TIMESTAMPTZ NOT NULL,
    created_at TIMESTAMPTZ DEFAULT NOW()
);

-- 交易日志表
CREATE TABLE IF NOT EXISTS trade_logs (
    id BIGSERIAL PRIMARY KEY,
    exchange VARCHAR(20) NOT NULL,
    symbol VARCHAR(20) NOT NULL,
    action VARCHAR(20) NOT NULL,
    details JSONB,
    created_at TIMESTAMPTZ DEFAULT NOW()
);

-- =================================================================
-- 第5层：账户快照表
-- =================================================================

-- 账户快照表
CREATE TABLE IF NOT EXISTS account_snapshot (
    id BIGSERIAL PRIMARY KEY,
    exchange VARCHAR(20) NOT NULL,
    market_type VARCHAR(20) NOT NULL,
    uid VARCHAR(20),
    snapshot_at TIMESTAMPTZ NOT NULL,
    total_equity DECIMAL(20, 8) NOT NULL DEFAULT 0,
    total_balance DECIMAL(20, 8) NOT NULL DEFAULT 0,
    available_balance DECIMAL(20, 8) NOT NULL DEFAULT 0,
    frozen_balance DECIMAL(20, 8) NOT NULL DEFAULT 0,
    unrealized_pnl DECIMAL(20, 8) NOT NULL DEFAULT 0,
    initial_margin DECIMAL(20, 8),
    maint_margin DECIMAL(20, 8),
    margin_ratio DECIMAL(10, 8),
    position_count INTEGER NOT NULL DEFAULT 0,
    UNIQUE(exchange, market_type, uid, snapshot_at)
);

-- 资产余额表
CREATE TABLE IF NOT EXISTS asset_balance (
    id BIGSERIAL PRIMARY KEY,
    exchange VARCHAR(20) NOT NULL,
    market_type VARCHAR(20) NOT NULL,
    uid VARCHAR(20),
    asset VARCHAR(20) NOT NULL,
    snapshot_at TIMESTAMPTZ NOT NULL,
    total DECIMAL(20, 8) NOT NULL DEFAULT 0,
    available DECIMAL(20, 8) NOT NULL DEFAULT 0,
    frozen DECIMAL(20, 8) NOT NULL DEFAULT 0,
    unrealized_pnl DECIMAL(20, 8) NOT NULL DEFAULT 0,
    usd_value DECIMAL(20, 8),
    UNIQUE(exchange, market_type, uid, asset, snapshot_at)
);

-- 持仓快照表
CREATE TABLE IF NOT EXISTS position_snapshot (
    id BIGSERIAL PRIMARY KEY,
    exchange VARCHAR(20) NOT NULL,
    market_type VARCHAR(20) NOT NULL DEFAULT 'futures',
    uid VARCHAR(20),
    symbol VARCHAR(30) NOT NULL,
    raw_symbol VARCHAR(50) NOT NULL,
    snapshot_at TIMESTAMPTZ NOT NULL,
    position_side VARCHAR(10) NOT NULL,
    position_amt DECIMAL(20, 8) NOT NULL,
    entry_price DECIMAL(20, 8) NOT NULL,
    mark_price DECIMAL(20, 8) NOT NULL,
    unrealized_pnl DECIMAL(20, 8) NOT NULL,
    leverage INTEGER NOT NULL DEFAULT 1,
    margin_type VARCHAR(10) NOT NULL DEFAULT 'cross',
    initial_margin DECIMAL(20, 8) NOT NULL DEFAULT 0,
    maint_margin DECIMAL(20, 8) NOT NULL DEFAULT 0,
    liquidation_price DECIMAL(20, 8),
    notional DECIMAL(20, 8) NOT NULL DEFAULT 0,
    break_even_price DECIMAL(20, 8),
    isolated_wallet DECIMAL(20, 8),
    pnl_ratio DECIMAL(10, 8),
    UNIQUE(exchange, symbol, position_side, uid, snapshot_at)
);

-- =================================================================
-- 第6层：回测和风控表
-- =================================================================

-- 回测结果表
CREATE TABLE IF NOT EXISTS backtest_results (
    id VARCHAR(50) PRIMARY KEY,
    strategy_id VARCHAR(50) NOT NULL,
    instance_id VARCHAR(50),
    symbol VARCHAR(20) NOT NULL,
    start_date TIMESTAMPTZ NOT NULL,
    end_date TIMESTAMPTZ NOT NULL,
    initial_capital DECIMAL(20, 8) NOT NULL,
    final_capital DECIMAL(20, 8) NOT NULL,
    total_return DECIMAL(10, 6),
    max_drawdown DECIMAL(10, 6),
    sharpe_ratio DECIMAL(10, 4),
    win_rate DECIMAL(5, 4),
    total_trades INTEGER DEFAULT 0,
    parameters JSONB,
    metadata JSONB,
    created_at TIMESTAMPTZ DEFAULT NOW()
);

-- 风控日志表
CREATE TABLE IF NOT EXISTS risk_logs (
    id BIGSERIAL PRIMARY KEY,
    event_type VARCHAR(30) NOT NULL,
    symbol VARCHAR(20),
    details JSONB,
    created_at TIMESTAMPTZ DEFAULT NOW()
);

-- 风控参数配置表（支持运行时热更新）
CREATE TABLE IF NOT EXISTS risk_config (
    key VARCHAR(50) NOT NULL,
    value DECIMAL(20,8) NOT NULL,
    description TEXT,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT risk_config_pkey PRIMARY KEY (key)
);

INSERT INTO risk_config (key, value, description) VALUES
    ('max_position_pct',     0.30,        '单笔最大仓位占权益百分比 (0.30 = 30%)'),
    ('stop_loss_pct',        0.02,        '止损百分比 (0.02 = 2%)'),
    ('take_profit_pct',      0.04,        '止盈百分比 (0.04 = 4%)'),
    ('risk_per_trade_pct',   0.02,        '单笔风险占权益百分比 (0.02 = 2%)'),
    ('max_daily_loss',       500.0,       '日最大亏损 (USDT)'),
    ('max_drawdown_pct',     0.15,        '最大回撤百分比 (0.15 = 15%)'),
    ('max_exposure_pct',     0.8,         '最大总曝光度百分比 (0.8 = 80%)'),
    ('kelly_fraction',       0.25,        'Kelly 公式分数 (0.25 = 1/4 Kelly)'),
    ('volatility_lookback',  20,          '波动率计算回溯 tick 数量'),
    ('volatility_target',    0.15,        '目标波动率 (0.15 = 15%)'),
    ('black_swan_threshold', 0.05,        '黑天鹅检测阈值 (0.05 = 5% 瞬间波动)'),
    ('circuit_breaker_cooldown', 3600,    '熔断冷却时间 (秒)'),
    ('daily_reset_hour',     0,           '每日重置小时 (UTC 0-23, 0=午夜)')
ON CONFLICT (key) DO NOTHING;

-- =================================================================
-- 创建索引
-- =================================================================

-- K线索引
CREATE INDEX IF NOT EXISTS idx_kline_1m_time ON kline_1m(open_time DESC);
CREATE INDEX IF NOT EXISTS idx_kline_htf_time ON kline_high_timeframe(symbol, timeframe, open_time DESC);
CREATE INDEX IF NOT EXISTS idx_kline_mtf_time ON kline_multi_timeframe(symbol, timeframe, open_time DESC);

-- Tick数据索引
CREATE INDEX IF NOT EXISTS idx_tick_data_time ON tick_data(symbol, timestamp DESC);

-- 市场情绪索引
CREATE INDEX IF NOT EXISTS idx_sentiment_symbol_time ON market_sentiment(symbol, data_type, timestamp DESC);

-- 策略信号索引
CREATE INDEX IF NOT EXISTS idx_signals_strategy ON strategy_signals(strategy_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_signals_symbol ON strategy_signals(symbol, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_signals_instance ON strategy_signals(instance_id, created_at DESC);

-- 订单索引
CREATE INDEX IF NOT EXISTS idx_orders_status ON trading_orders(status);
CREATE INDEX IF NOT EXISTS idx_orders_symbol ON trading_orders(symbol);
CREATE INDEX IF NOT EXISTS idx_orders_market_type ON trading_orders(market_type);
CREATE INDEX IF NOT EXISTS idx_orders_uid ON trading_orders(uid);
CREATE INDEX IF NOT EXISTS idx_orders_source ON trading_orders(source);
CREATE INDEX IF NOT EXISTS idx_orders_signal ON trading_orders(signal_id);
CREATE INDEX IF NOT EXISTS idx_orders_strategy ON trading_orders(strategy_id);
CREATE INDEX IF NOT EXISTS idx_orders_created ON trading_orders(created_at DESC);

-- 持仓索引
CREATE INDEX IF NOT EXISTS idx_positions_exchange ON positions(exchange);
CREATE INDEX IF NOT EXISTS idx_positions_market_type ON positions(market_type);
CREATE INDEX IF NOT EXISTS idx_positions_uid ON positions(uid);
CREATE INDEX IF NOT EXISTS idx_positions_symbol ON positions(symbol);

-- 止损单索引
CREATE INDEX IF NOT EXISTS idx_stop_orders_active ON stop_orders(exchange, symbol, status)
    WHERE status = 'active';
CREATE INDEX IF NOT EXISTS idx_stop_orders_symbol ON stop_orders(symbol, status);
CREATE INDEX IF NOT EXISTS idx_stop_orders_exchange ON stop_orders(exchange, market_type, status);
CREATE INDEX IF NOT EXISTS idx_stop_orders_uid ON stop_orders(uid);

-- 交易记录索引
CREATE INDEX IF NOT EXISTS idx_trades_symbol ON trades(symbol, timestamp DESC);
CREATE INDEX IF NOT EXISTS idx_trades_order ON trades(order_id);

-- 账户快照索引
CREATE INDEX IF NOT EXISTS idx_account_snapshot_uid ON account_snapshot(uid, exchange, market_type, snapshot_at DESC);
CREATE INDEX IF NOT EXISTS idx_asset_balance_uid ON asset_balance(uid, exchange, market_type, snapshot_at DESC);
CREATE INDEX IF NOT EXISTS idx_position_snapshot_uid ON position_snapshot(uid, exchange, snapshot_at DESC);

-- 交易对映射索引
CREATE INDEX IF NOT EXISTS idx_symbol_mapping_unified ON symbol_mapping(exchange, unified_symbol);
CREATE INDEX IF NOT EXISTS idx_symbol_mapping_raw ON symbol_mapping(exchange, raw_symbol);

-- 符号配置索引
CREATE INDEX IF NOT EXISTS idx_symbol_config_exchange ON symbol_config(exchange_config_id);

-- 策略绩效索引
CREATE INDEX IF NOT EXISTS idx_performance_strategy ON strategy_performance(strategy_id, period);

-- 策略分析日志索引
CREATE INDEX IF NOT EXISTS idx_analysis_strategy ON strategy_analysis_log(strategy_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_analysis_instance ON strategy_analysis_log(instance_id, created_at DESC);

-- 实时策略日志索引
CREATE INDEX IF NOT EXISTS idx_live_log_strategy ON live_strategy_log(strategy_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_live_log_instance ON live_strategy_log(instance_id, created_at DESC);

-- 回测结果索引
CREATE INDEX IF NOT EXISTS idx_backtest_strategy ON backtest_results(strategy_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_backtest_instance ON backtest_results(instance_id, created_at DESC);

-- 风控日志索引
CREATE INDEX IF NOT EXISTS idx_risk_logs_type ON risk_logs(event_type, created_at DESC);

-- =================================================================
-- K线聚合函数
-- =================================================================

-- 从1分钟K线聚合生成高时间框架K线的函数
CREATE OR REPLACE FUNCTION aggregate_kline(
    p_symbol VARCHAR,
    p_timeframe VARCHAR,
    p_start_time TIMESTAMPTZ,
    p_end_time TIMESTAMPTZ
) RETURNS TABLE (
    open_time TIMESTAMPTZ,
    open DECIMAL,
    high DECIMAL,
    low DECIMAL,
    close DECIMAL,
    volume DECIMAL,
    close_time TIMESTAMPTZ,
    quote_volume DECIMAL,
    trades_count INTEGER
) AS $$
DECLARE
    interval_minutes INTEGER;
BEGIN
    -- 根据时间框架计算间隔分钟数
    interval_minutes := CASE p_timeframe
        WHEN '5m' THEN 5
        WHEN '15m' THEN 15
        WHEN '30m' THEN 30
        WHEN '1h' THEN 60
        WHEN '2h' THEN 120
        WHEN '4h' THEN 240
        WHEN '1d' THEN 1440
        WHEN '3d' THEN 4320
        WHEN '1w' THEN 10080
        ELSE 60
    END;

    RETURN QUERY
    SELECT
        date_trunc('hour', k.open_time) +
            (EXTRACT(MINUTE FROM k.open_time)::INTEGER / interval_minutes) * interval_minutes * INTERVAL '1 minute' AS agg_open_time,
        (ARRAY_AGG(k.open ORDER BY k.open_time))[1] AS agg_open,
        MAX(k.high) AS agg_high,
        MIN(k.low) AS agg_low,
        (ARRAY_AGG(k.close ORDER BY k.open_time DESC))[1] AS agg_close,
        SUM(k.volume) AS agg_volume,
        date_trunc('hour', k.open_time) +
            (EXTRACT(MINUTE FROM k.open_time)::INTEGER / interval_minutes) * interval_minutes * INTERVAL '1 minute' +
            (interval_minutes - 1) * INTERVAL '1 minute' AS agg_close_time,
        SUM(k.quote_volume) AS agg_quote_volume,
        SUM(k.trades_count)::INTEGER AS agg_trades_count
    FROM kline_1m k
    WHERE k.symbol = p_symbol
      AND k.open_time >= p_start_time
      AND k.open_time < p_end_time
    GROUP BY agg_open_time
    ORDER BY agg_open_time;
END;
$$ LANGUAGE plpgsql;

-- 提交事务
COMMIT;

-- =================================================================
-- 完成提示
-- =================================================================
\echo 'Database initialization completed successfully!'
\echo 'Tables created: 25'
\echo 'Indexes created: 35+'
\echo 'Functions created: 1 (aggregate_kline)'
