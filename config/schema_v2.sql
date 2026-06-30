-- =================================================================
-- Trading System Database Schema V2
-- 新增多时间框架策略支持
-- =================================================================

-- =================================================================
-- 1. K线数据表 (kline_1m)
-- 用途：存储 1 分钟 K线数据，用于多时间框架聚合
-- 设计原则：
--   - 只存 1m K线，其他时间框架通过聚合器实时生成
--   - 存储成本：~100MB/年/交易对
--   - 支持高效的 OHLC 查询
-- =================================================================

CREATE TABLE IF NOT EXISTS kline_1m (
    -- 【时间戳】K线开始时间（UTC）
    timestamp TIMESTAMP WITH TIME ZONE NOT NULL,

    -- 【交易对】如 'BTCUSDT'
    symbol VARCHAR(20) NOT NULL,

    -- 【开盘价】K线周期内第一笔交易价格
    open DECIMAL(20, 8) NOT NULL,

    -- 【最高价】K线周期内最高交易价格
    high DECIMAL(20, 8) NOT NULL,

    -- 【最低价】K线周期内最低交易价格
    low DECIMAL(20, 8) NOT NULL,

    -- 【收盘价】K线周期内最后一笔交易价格
    close DECIMAL(20, 8) NOT NULL,

    -- 【成交量】K线周期内总成交量
    volume DECIMAL(20, 8) NOT NULL,

    -- 【成交笔数】K线周期内交易次数
    trade_count INTEGER NOT NULL DEFAULT 0,

    -- 主键：交易对 + 时间戳唯一
    PRIMARY KEY (symbol, timestamp)
);

-- K线索引
CREATE INDEX IF NOT EXISTS idx_kline_1m_symbol_time ON kline_1m(symbol, timestamp DESC);
CREATE INDEX IF NOT EXISTS idx_kline_1m_timestamp ON kline_1m(timestamp);

-- =================================================================
-- 2. 回测结果表 (backtest_results)
-- 用途：存储历史回测结果，便于比较和分析
-- =================================================================

CREATE TABLE IF NOT EXISTS backtest_results (
    -- 【回测ID】唯一标识
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),

    -- 【策略名称】如 'rsi', 'sma', 'trend'
    strategy_id VARCHAR(50) NOT NULL,

    -- 【交易对】如 'BTCUSDT'
    symbol VARCHAR(20) NOT NULL,

    -- 【初始资金】
    initial_capital DECIMAL(20, 8) NOT NULL,

    -- 【最终资金】
    final_capital DECIMAL(20, 8) NOT NULL,

    -- 【收益率】百分比
    return_pct DECIMAL(10, 4) NOT NULL,

    -- 【总交易次数】
    total_trades INTEGER NOT NULL,

    -- 【盈利交易次数】
    winning_trades INTEGER NOT NULL,

    -- 【亏损交易次数】
    losing_trades INTEGER NOT NULL,

    -- 【胜率】百分比
    win_rate DECIMAL(10, 4) NOT NULL,

    -- 【最大回撤】百分比
    max_drawdown DECIMAL(10, 4) NOT NULL,

    -- 【夏普比率】
    sharpe_ratio DECIMAL(10, 4) NOT NULL,

    -- 【盈亏比】
    profit_factor DECIMAL(10, 4) NOT NULL,

    -- 【数据点数量】
    data_points INTEGER NOT NULL,

    -- 【数据范围开始】
    data_start_time TIMESTAMP WITH TIME ZONE,

    -- 【数据范围结束】
    data_end_time TIMESTAMP WITH TIME ZONE,

    -- 【策略参数】JSON 格式
    strategy_params JSONB,

    -- 【创建时间】
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW()
);

-- 回测结果索引
CREATE INDEX IF NOT EXISTS idx_backtest_strategy ON backtest_results(strategy_id, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_backtest_symbol ON backtest_results(symbol, created_at DESC);

-- =================================================================
-- 3. 策略信号表 (strategy_signals)
-- 用途：记录策略生成的交易信号，用于分析和调试
-- =================================================================

CREATE TABLE IF NOT EXISTS strategy_signals (
    -- 【信号ID】唯一标识
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),

    -- 【策略名称】
    strategy_id VARCHAR(50) NOT NULL,

    -- 【交易对】
    symbol VARCHAR(20) NOT NULL,

    -- 【信号时间】
    signal_time TIMESTAMP WITH TIME ZONE NOT NULL,

    -- 【信号类型】BUY / SELL / HOLD
    signal_type VARCHAR(10) NOT NULL CHECK (signal_type IN ('BUY', 'SELL', 'HOLD')),

    -- 【信号价格】
    signal_price DECIMAL(20, 8) NOT NULL,

    -- 【信号数量】
    signal_quantity DECIMAL(20, 8),

    -- 【置信度】0.0 - 1.0
    confidence DECIMAL(5, 4),

    -- 【趋势方向】Bullish / Bearish / Neutral
    trend_direction VARCHAR(20),

    -- 【时间框架分析】JSON 格式
    timeframe_analysis JSONB,

    -- 【创建时间】
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW()
);

-- 信号索引
CREATE INDEX IF NOT EXISTS idx_signals_strategy_time ON strategy_signals(strategy_id, signal_time DESC);
CREATE INDEX IF NOT EXISTS idx_signals_symbol_time ON strategy_signals(symbol, signal_time DESC);

-- =================================================================
-- 4. 持仓表 (positions)
-- 用途：记录当前持仓状态
-- =================================================================

CREATE TABLE IF NOT EXISTS positions (
    -- 【持仓ID】唯一标识
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),

    -- 【交易对】
    symbol VARCHAR(20) NOT NULL UNIQUE,

    -- 【持仓方向】LONG / SHORT
    side VARCHAR(10) NOT NULL CHECK (side IN ('LONG', 'SHORT')),

    -- 【持仓数量】
    quantity DECIMAL(20, 8) NOT NULL,

    -- 【平均入场价格】
    avg_entry_price DECIMAL(20, 8) NOT NULL,

    -- 【当前价格】
    current_price DECIMAL(20, 8),

    -- 【未实现盈亏】
    unrealized_pnl DECIMAL(20, 8),

    -- 【实现盈亏】
    realized_pnl DECIMAL(20, 8) NOT NULL DEFAULT 0,

    -- 【开仓时间】
    opened_at TIMESTAMP WITH TIME ZONE NOT NULL,

    -- 【更新时间】
    updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW()
);

-- =================================================================
-- 5. 交易记录表 (trades)
-- 用途：记录所有已执行的交易
-- =================================================================

CREATE TABLE IF NOT EXISTS trades (
    -- 【交易ID】唯一标识
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),

    -- 【交易所订单ID】
    order_id VARCHAR(100),

    -- 【交易对】
    symbol VARCHAR(20) NOT NULL,

    -- 【交易方向】BUY / SELL
    side VARCHAR(10) NOT NULL CHECK (side IN ('BUY', 'SELL')),

    -- 【交易价格】
    price DECIMAL(20, 8) NOT NULL,

    -- 【交易数量】
    quantity DECIMAL(20, 8) NOT NULL,

    -- 【手续费】
    commission DECIMAL(20, 8) NOT NULL DEFAULT 0,

    -- 【实现盈亏】（仅平仓时有值）
    realized_pnl DECIMAL(20, 8),

    -- 【策略名称】
    strategy_id VARCHAR(50),

    -- 【交易时间】
    trade_time TIMESTAMP WITH TIME ZONE NOT NULL,

    -- 【创建时间】
    created_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW()
);

-- 交易索引
CREATE INDEX IF NOT EXISTS idx_trades_symbol_time ON trades(symbol, trade_time DESC);
CREATE INDEX IF NOT EXISTS idx_trades_strategy ON trades(strategy_id, trade_time DESC);

-- =================================================================
-- 6. 实时价格缓存表 (price_cache)
-- 用途：缓存最新价格，减少数据库查询
-- 注意：也可以使用 Redis 替代
-- =================================================================

CREATE TABLE IF NOT EXISTS price_cache (
    -- 【交易对】主键
    symbol VARCHAR(20) PRIMARY KEY,

    -- 【最新价格】
    price DECIMAL(20, 8) NOT NULL,

    -- 【24h 涨跌幅】
    change_24h DECIMAL(10, 4),

    -- 【24h 成交量】
    volume_24h DECIMAL(20, 8),

    -- 【更新时间】
    updated_at TIMESTAMP WITH TIME ZONE NOT NULL DEFAULT NOW()
);

-- =================================================================
-- 说明：所有聚合查询在 Rust 代码中实现（KlineAggregator）
-- 不使用存储过程，便于数据库迁移和版本管理
-- =================================================================
