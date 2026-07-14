-- =================================================================
-- Trading System Database Schema (Latest)
-- 统一完整版本 - 2026-07-14
--
-- 包含所有表的最新结构：
--   1. kline_1m (K线数据)
--   2. trading_pairs (交易对配置)
--   3. symbol_config (监控列表)
--   4. strategy_instances (策略实例)
--   5. strategy_signals (策略信号)
--   6. strategy_analysis_log (分析日志)
--   7. strategy_performance (策略性能)
--   8. trades (交易记录)
--   9. positions (持仓)
--   10. backtest_results (回测结果)
--   11. price_cache (价格缓存)
--   12. system_config (系统配置)
--
-- 执行方式：
--   psql -U postgres -d trading_core -f schema_latest.sql
-- =================================================================

-- =================================================================
-- 1. K线数据表 (kline_1m)
-- =================================================================

CREATE TABLE IF NOT EXISTS kline_1m (
    symbol VARCHAR(20) NOT NULL,
    open_time TIMESTAMPTZ NOT NULL,
    open DECIMAL(20,8) NOT NULL,
    high DECIMAL(20,8) NOT NULL,
    low DECIMAL(20,8) NOT NULL,
    close DECIMAL(20,8) NOT NULL,
    volume DECIMAL(20,8) NOT NULL,
    close_time TIMESTAMPTZ NOT NULL,
    quote_volume DECIMAL(20,8) NOT NULL DEFAULT 0,
    trades INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY (symbol, open_time)
);

CREATE INDEX IF NOT EXISTS idx_kline_1m_symbol_time ON kline_1m(symbol, open_time DESC);
CREATE INDEX IF NOT EXISTS idx_kline_1m_time ON kline_1m(open_time);


-- =================================================================
-- 2. 交易对配置表 (trading_pairs)
-- =================================================================

CREATE TABLE IF NOT EXISTS trading_pairs (
    id SERIAL PRIMARY KEY,
    symbol VARCHAR(20) NOT NULL UNIQUE,
    market_type VARCHAR(10) NOT NULL CHECK (market_type IN ('spot', 'futures')),
    exchange VARCHAR(20) NOT NULL DEFAULT 'binance',
    status VARCHAR(20) NOT NULL DEFAULT 'active'
        CHECK (status IN ('active', 'paused', 'archived')),
    note TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_trading_pairs_symbol ON trading_pairs(symbol);
CREATE INDEX IF NOT EXISTS idx_trading_pairs_status ON trading_pairs(status);
CREATE INDEX IF NOT EXISTS idx_trading_pairs_market ON trading_pairs(market_type);


-- =================================================================
-- 3. 监控列表 (symbol_config)
-- =================================================================

CREATE TABLE IF NOT EXISTS symbol_config (
    symbol VARCHAR(20) PRIMARY KEY,
    enabled BOOLEAN NOT NULL DEFAULT true,
    added_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);


-- =================================================================
-- 4. 策略实例表 (strategy_instances)
-- =================================================================

CREATE TABLE IF NOT EXISTS strategy_instances (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    strategy_type VARCHAR(50) NOT NULL,
    display_name VARCHAR(100) NOT NULL,
    params JSONB NOT NULL,
    status VARCHAR(20) NOT NULL DEFAULT 'active'
        CHECK (status IN ('active', 'paused', 'archived')),
    symbols TEXT[] NOT NULL DEFAULT '{}',
    auto_trade BOOLEAN NOT NULL DEFAULT false,
    position_size_pct DECIMAL(5,2) NOT NULL DEFAULT 10.0,
    exchange VARCHAR(20) NOT NULL DEFAULT 'binance',
    market_type VARCHAR(10) NOT NULL DEFAULT 'futures'
        CHECK (market_type IN ('spot', 'futures')),
    note TEXT,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_strategy_instances_type ON strategy_instances(strategy_type);
CREATE INDEX IF NOT EXISTS idx_strategy_instances_status ON strategy_instances(status);
CREATE INDEX IF NOT EXISTS idx_strategy_instances_exchange ON strategy_instances(exchange);


-- =================================================================
-- 5. 策略信号表 (strategy_signals)
-- =================================================================

CREATE TABLE IF NOT EXISTS strategy_signals (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    symbol VARCHAR(20) NOT NULL,
    strategy_id VARCHAR(50) NOT NULL,

    -- 信号内容
    direction VARCHAR(10) NOT NULL,
    entry_price DECIMAL(20,8) NOT NULL,
    overall_confidence DECIMAL(5,4) NOT NULL,
    entry_allowed BOOLEAN NOT NULL DEFAULT false,
    entry_direction VARCHAR(10),
    timeframe_details JSONB NOT NULL DEFAULT '{}',

    -- V6 新增：策略实例关联（无外键，应用层保证完整性）
    instance_id UUID,
    signal_strength DECIMAL(5,4),
    market_context JSONB,
    stop_loss DECIMAL(20,8),
    take_profit DECIMAL(20,8),

    -- 交易关联
    order_id VARCHAR(100),
    executed BOOLEAN NOT NULL DEFAULT false,

    -- 生命周期
    status VARCHAR(20) NOT NULL DEFAULT 'pending',
    closed_reason VARCHAR(50),

    -- 验证追踪
    evaluated_at TIMESTAMPTZ,
    best_price DECIMAL(20,8),
    worst_price DECIMAL(20,8),
    eval_count INTEGER NOT NULL DEFAULT 0,

    -- 闭环结果
    closed_at TIMESTAMPTZ,
    close_price DECIMAL(20,8),
    actual_return_pct DECIMAL(10,4),

    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_signals_instance ON strategy_signals(instance_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_signals_pending ON strategy_signals(symbol, strategy_id) WHERE status = 'pending';
CREATE INDEX IF NOT EXISTS idx_signals_symbol_time ON strategy_signals(symbol, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_signals_status ON strategy_signals(status, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_signals_order ON strategy_signals(order_id) WHERE order_id IS NOT NULL;


-- =================================================================
-- 6. 策略分析日志表 (strategy_analysis_log)
-- =================================================================

CREATE TABLE IF NOT EXISTS strategy_analysis_log (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    symbol VARCHAR(20) NOT NULL,
    strategy_id VARCHAR(50) NOT NULL,
    direction VARCHAR(10) NOT NULL,
    entry_price DECIMAL(20,8) NOT NULL,
    overall_confidence DECIMAL(5,4) NOT NULL,
    entry_allowed BOOLEAN NOT NULL DEFAULT false,
    entry_direction VARCHAR(10),
    timeframe_details JSONB NOT NULL DEFAULT '{}',
    status VARCHAR(20) NOT NULL DEFAULT 'pending',
    closed_reason VARCHAR(50),
    evaluated_at TIMESTAMPTZ,
    best_price DECIMAL(20,8),
    worst_price DECIMAL(20,8),
    eval_count INTEGER NOT NULL DEFAULT 0,
    closed_at TIMESTAMPTZ,
    close_price DECIMAL(20,8),
    actual_return_pct DECIMAL(10,4),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_analysis_pending ON strategy_analysis_log(symbol, strategy_id) WHERE status = 'pending';
CREATE INDEX IF NOT EXISTS idx_analysis_symbol_time ON strategy_analysis_log(symbol, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_analysis_status ON strategy_analysis_log(status, created_at DESC);


-- =================================================================
-- 7. 策略性能统计表 (strategy_performance)
-- =================================================================

CREATE TABLE IF NOT EXISTS strategy_performance (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    instance_id UUID NOT NULL,
    period_start TIMESTAMPTZ NOT NULL,
    period_end TIMESTAMPTZ NOT NULL,
    total_signals INTEGER NOT NULL DEFAULT 0,
    buy_signals INTEGER NOT NULL DEFAULT 0,
    sell_signals INTEGER NOT NULL DEFAULT 0,
    total_trades INTEGER NOT NULL DEFAULT 0,
    winning_trades INTEGER NOT NULL DEFAULT 0,
    losing_trades INTEGER NOT NULL DEFAULT 0,
    total_pnl DECIMAL(20,8) NOT NULL DEFAULT 0,
    win_rate DECIMAL(5,4),
    avg_win DECIMAL(20,8),
    avg_loss DECIMAL(20,8),
    profit_factor DECIMAL(10,4),
    max_drawdown DECIMAL(10,4),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(instance_id, period_start, period_end)
);

CREATE INDEX IF NOT EXISTS idx_performance_instance ON strategy_performance(instance_id, period_start DESC);


-- =================================================================
-- 8. 交易记录表 (trades)
-- =================================================================

CREATE TABLE IF NOT EXISTS trades (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    order_id VARCHAR(100),
    symbol VARCHAR(20) NOT NULL,
    side VARCHAR(10) NOT NULL CHECK (side IN ('BUY', 'SELL')),
    price DECIMAL(20,8) NOT NULL,
    quantity DECIMAL(20,8) NOT NULL,
    commission DECIMAL(20,8) NOT NULL DEFAULT 0,
    realized_pnl DECIMAL(20,8),
    strategy_id VARCHAR(50),
    trade_time TIMESTAMPTZ NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    -- V5 新增
    exchange VARCHAR(20) NOT NULL DEFAULT 'binance',
    market_type VARCHAR(10) NOT NULL DEFAULT 'futures'
        CHECK (market_type IN ('spot', 'futures')),

    -- V6 新增（无外键，应用层保证完整性）
    signal_id UUID,
    order_status VARCHAR(20) DEFAULT 'filled',
    order_type VARCHAR(20) DEFAULT 'market',
    leverage INTEGER DEFAULT 1,
    slippage DECIMAL(10,6),
    metadata JSONB
);

CREATE INDEX IF NOT EXISTS idx_trades_symbol_time ON trades(symbol, trade_time DESC);
CREATE INDEX IF NOT EXISTS idx_trades_strategy ON trades(strategy_id, trade_time DESC);
CREATE INDEX IF NOT EXISTS idx_trades_exchange ON trades(exchange);
CREATE INDEX IF NOT EXISTS idx_trades_market_type ON trades(market_type);
CREATE INDEX IF NOT EXISTS idx_trades_exchange_symbol ON trades(exchange, symbol);
CREATE INDEX IF NOT EXISTS idx_trades_signal ON trades(signal_id);


-- =================================================================
-- 9. 持仓表 (positions)
-- =================================================================

CREATE TABLE IF NOT EXISTS positions (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    symbol VARCHAR(20) NOT NULL UNIQUE,
    side VARCHAR(10) NOT NULL CHECK (side IN ('LONG', 'SHORT')),
    quantity DECIMAL(20,8) NOT NULL,
    avg_entry_price DECIMAL(20,8) NOT NULL,
    current_price DECIMAL(20,8),
    unrealized_pnl DECIMAL(20,8),
    realized_pnl DECIMAL(20,8) NOT NULL DEFAULT 0,
    opened_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    -- V5 新增
    exchange VARCHAR(20) NOT NULL DEFAULT 'binance',
    market_type VARCHAR(10) NOT NULL DEFAULT 'futures'
        CHECK (market_type IN ('spot', 'futures'))
);

CREATE INDEX IF NOT EXISTS idx_positions_exchange ON positions(exchange);
CREATE INDEX IF NOT EXISTS idx_positions_market_type ON positions(market_type);


-- =================================================================
-- 10. 回测结果表 (backtest_results)
-- =================================================================

CREATE TABLE IF NOT EXISTS backtest_results (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    strategy_id VARCHAR(50) NOT NULL,
    symbol VARCHAR(20) NOT NULL,
    initial_capital DECIMAL(20,8) NOT NULL,
    final_capital DECIMAL(20,8) NOT NULL,
    return_pct DECIMAL(10,4) NOT NULL,
    total_trades INTEGER NOT NULL,
    winning_trades INTEGER NOT NULL,
    losing_trades INTEGER NOT NULL,
    win_rate DECIMAL(10,4) NOT NULL,
    max_drawdown DECIMAL(10,4) NOT NULL,
    sharpe_ratio DECIMAL(10,4) NOT NULL,
    profit_factor DECIMAL(10,4) NOT NULL,
    data_points INTEGER NOT NULL,
    data_start_time TIMESTAMPTZ,
    data_end_time TIMESTAMPTZ,
    strategy_params JSONB,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX IF NOT EXISTS idx_backtest_strategy ON backtest_results(strategy_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_backtest_symbol ON backtest_results(symbol, created_at DESC);


-- =================================================================
-- 11. 价格缓存表 (price_cache)
-- =================================================================

CREATE TABLE IF NOT EXISTS price_cache (
    symbol VARCHAR(20) PRIMARY KEY,
    price DECIMAL(20,8) NOT NULL,
    change_24h DECIMAL(10,4),
    volume_24h DECIMAL(20,8),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);


-- =================================================================
-- 12. 系统配置表 (system_config)
-- =================================================================

CREATE TABLE IF NOT EXISTS system_config (
    key VARCHAR(50) PRIMARY KEY,
    value TEXT NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- 初始化调度器状态
INSERT INTO system_config (key, value) VALUES ('scheduler_paused', 'false')
ON CONFLICT (key) DO NOTHING;


-- =================================================================
-- 13. 初始化数据
-- =================================================================

-- 默认交易对
INSERT INTO trading_pairs (symbol, market_type, exchange, status) VALUES
    ('BTCUSDT', 'spot', 'binance', 'active'),
    ('ETHUSDT', 'spot', 'binance', 'active'),
    ('SOLUSDT', 'spot', 'binance', 'active'),
    ('SUIUSDT', 'spot', 'binance', 'active'),
    ('BNBUSDT', 'spot', 'binance', 'active')
ON CONFLICT (symbol) DO NOTHING;

-- 默认监控列表
INSERT INTO symbol_config (symbol) VALUES ('BTCUSDT'), ('ETHUSDT'), ('SOLUSDT')
ON CONFLICT (symbol) DO NOTHING;

-- 默认策略实例
INSERT INTO strategy_instances (strategy_type, display_name, params, symbols, auto_trade, note) VALUES
    ('rsi', 'RSI-BTC-默认', '{"period": 14, "overbought": 70, "oversold": 30, "confirm_candles": 2}', '{BTCUSDT}', false, 'RSI 默认配置'),
    ('macd', 'MACD-ETH-默认', '{"fast_period": 12, "slow_period": 26, "signal_period": 9, "histogram_threshold": 0}', '{ETHUSDT}', false, 'MACD 默认配置'),
    ('bollinger', '布林带-SOL-默认', '{"period": 20, "std_dev": 2.0, "squeeze_threshold": 0.02}', '{SOLUSDT}', false, '布林带默认配置'),
    ('trend', '趋势-BTC-默认', '{"fast_ma": 7, "slow_ma": 25, "trend_ma": 99, "adx_threshold": 25}', '{BTCUSDT}', false, '趋势策略默认配置'),
    ('multi_tf', '多时间框架-BTC-默认', '{"timeframes": ["1h", "4h", "1d"], "min_agreement": 2}', '{BTCUSDT}', false, '多时间框架默认配置')
ON CONFLICT DO NOTHING;


-- =================================================================
-- 完成
-- =================================================================

DO $$
BEGIN
    RAISE NOTICE '========================================';
    RAISE NOTICE 'Schema Latest 迁移完成';
    RAISE NOTICE '  - kline_1m (K线数据)';
    RAISE NOTICE '  - trading_pairs (交易对配置)';
    RAISE NOTICE '  - symbol_config (监控列表)';
    RAISE NOTICE '  - strategy_instances (策略实例)';
    RAISE NOTICE '  - strategy_signals (策略信号)';
    RAISE NOTICE '  - strategy_analysis_log (分析日志)';
    RAISE NOTICE '  - strategy_performance (策略性能)';
    RAISE NOTICE '  - trades (交易记录)';
    RAISE NOTICE '  - positions (持仓)';
    RAISE NOTICE '  - backtest_results (回测结果)';
    RAISE NOTICE '  - price_cache (价格缓存)';
    RAISE NOTICE '  - system_config (系统配置)';
    RAISE NOTICE '========================================';
END $$;
