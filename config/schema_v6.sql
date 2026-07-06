-- =================================================================
-- Trading System Database Schema V6
-- 策略服务：策略实例、信号溯源、交易关联、性能统计
-- =================================================================

-- =================================================================
-- 1. 策略实例表 (strategy_instances)
-- 用途：存储策略实例配置，每个实例是某个策略类型的独立配置
-- =================================================================

CREATE TABLE IF NOT EXISTS strategy_instances (
    -- 【ID】主键
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),

    -- 【策略类型】rsi, macd, bollinger, volume, trend, multi_tf
    strategy_type VARCHAR(50) NOT NULL,

    -- 【显示名称】如 "RSI-BTC-激进版"
    display_name VARCHAR(100) NOT NULL,

    -- 【策略参数】JSON 格式，不同策略类型有不同参数结构
    params JSONB NOT NULL,

    -- 【运行状态】active(运行中) / paused(暂停) / archived(归档)
    status VARCHAR(20) NOT NULL DEFAULT 'active'
        CHECK (status IN ('active', 'paused', 'archived')),

    -- 【适用交易对】数组类型，支持多交易对
    symbols TEXT[] NOT NULL DEFAULT '{}',

    -- 【是否启用自动交易】true=信号触发自动下单
    auto_trade BOOLEAN NOT NULL DEFAULT false,

    -- 【仓位大小】相对于总资金的百分比
    position_size_pct DECIMAL(5,2) NOT NULL DEFAULT 10.0,

    -- 【交易所】binance / okx
    exchange VARCHAR(20) NOT NULL DEFAULT 'binance',

    -- 【市场类型】spot / futures
    market_type VARCHAR(10) NOT NULL DEFAULT 'futures'
        CHECK (market_type IN ('spot', 'futures')),

    -- 【备注】
    note TEXT,

    -- 【创建时间】
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    -- 【更新时间】
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- 索引
CREATE INDEX IF NOT EXISTS idx_strategy_instances_type ON strategy_instances(strategy_type);
CREATE INDEX IF NOT EXISTS idx_strategy_instances_status ON strategy_instances(status);
CREATE INDEX IF NOT EXISTS idx_strategy_instances_exchange ON strategy_instances(exchange);

-- =================================================================
-- 2. 策略信号表 (strategy_signals) 扩展
-- 用途：记录策略生成的交易信号，关联策略实例
-- =================================================================

-- 新增字段
ALTER TABLE strategy_signals ADD COLUMN IF NOT EXISTS
    instance_id UUID REFERENCES strategy_instances(id);

ALTER TABLE strategy_signals ADD COLUMN IF NOT EXISTS
    signal_strength DECIMAL(5,4);

ALTER TABLE strategy_signals ADD COLUMN IF NOT EXISTS
    market_context JSONB;

ALTER TABLE strategy_signals ADD COLUMN IF NOT EXISTS
    entry_price DECIMAL(20,8);

ALTER TABLE strategy_signals ADD COLUMN IF NOT EXISTS
    stop_loss DECIMAL(20,8);

ALTER TABLE strategy_signals ADD COLUMN IF NOT EXISTS
    take_profit DECIMAL(20,8);

ALTER TABLE strategy_signals ADD COLUMN IF NOT EXISTS
    exchange VARCHAR(20) NOT NULL DEFAULT 'binance';

ALTER TABLE strategy_signals ADD COLUMN IF NOT EXISTS
    market_type VARCHAR(10) NOT NULL DEFAULT 'futures';

-- 新增索引
CREATE INDEX IF NOT EXISTS idx_signals_instance ON strategy_signals(instance_id, signal_time DESC);
CREATE INDEX IF NOT EXISTS idx_signals_exchange ON strategy_signals(exchange);

-- =================================================================
-- 3. 交易记录表 (trades) 扩展
-- 用途：确保每笔交易都能追溯到信号和策略
-- =================================================================

-- 新增字段
ALTER TABLE trades ADD COLUMN IF NOT EXISTS
    signal_id UUID REFERENCES strategy_signals(id);

ALTER TABLE trades ADD COLUMN IF NOT EXISTS
    order_status VARCHAR(20) DEFAULT 'filled'
        CHECK (order_status IN ('pending', 'filled', 'cancelled', 'rejected'));

ALTER TABLE trades ADD COLUMN IF NOT EXISTS
    order_type VARCHAR(20) DEFAULT 'market'
        CHECK (order_type IN ('market', 'limit', 'stop'));

ALTER TABLE trades ADD COLUMN IF NOT EXISTS
    leverage INTEGER DEFAULT 1;

ALTER TABLE trades ADD COLUMN IF NOT EXISTS
    slippage DECIMAL(10,6);

ALTER TABLE trades ADD COLUMN IF NOT EXISTS
    metadata JSONB;

-- 新增索引
CREATE INDEX IF NOT EXISTS idx_trades_signal ON trades(signal_id);
CREATE INDEX IF NOT EXISTS idx_trades_exchange ON trades(exchange);

-- =================================================================
-- 4. 策略性能统计表 (strategy_performance)
-- 用途：定期汇总每个策略实例的运行指标
-- =================================================================

CREATE TABLE IF NOT EXISTS strategy_performance (
    -- 【ID】主键
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),

    -- 【策略实例ID】
    instance_id UUID NOT NULL REFERENCES strategy_instances(id) ON DELETE CASCADE,

    -- 【统计周期开始】
    period_start TIMESTAMPTZ NOT NULL,

    -- 【统计周期结束】
    period_end TIMESTAMPTZ NOT NULL,

    -- 【信号统计】
    total_signals INTEGER NOT NULL DEFAULT 0,
    buy_signals INTEGER NOT NULL DEFAULT 0,
    sell_signals INTEGER NOT NULL DEFAULT 0,

    -- 【交易统计】
    total_trades INTEGER NOT NULL DEFAULT 0,
    winning_trades INTEGER NOT NULL DEFAULT 0,
    losing_trades INTEGER NOT NULL DEFAULT 0,

    -- 【盈亏统计】
    total_pnl DECIMAL(20,8) NOT NULL DEFAULT 0,
    win_rate DECIMAL(5,4),
    avg_win DECIMAL(20,8),
    avg_loss DECIMAL(20,8),
    profit_factor DECIMAL(10,4),
    max_drawdown DECIMAL(10,4),

    -- 【更新时间】
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    -- 唯一约束
    UNIQUE(instance_id, period_start, period_end)
);

-- 索引
CREATE INDEX IF NOT EXISTS idx_performance_instance ON strategy_performance(instance_id, period_start DESC);

-- =================================================================
-- 5. 初始化默认策略实例
-- =================================================================

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
    RAISE NOTICE 'Schema V6 迁移完成';
    RAISE NOTICE '  - strategy_instances (策略实例表)';
    RAISE NOTICE '  - strategy_signals 扩展 (instance_id, market_context)';
    RAISE NOTICE '  - trades 扩展 (signal_id, order_status)';
    RAISE NOTICE '  - strategy_performance (策略性能统计)';
    RAISE NOTICE '========================================';
END $$;
