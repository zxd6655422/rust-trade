-- =================================================================
-- Trading System Database Schema V3
-- 策略信号生命周期管理（两张表完全分离）
--
-- 设计原则：
--   strategy_signals        → 交易引擎专属，与订单关联
--   strategy_analysis_log   → 前端分析专属，纯观察记录
--   两表结构相似但职责隔离，互不干扰
--
-- 信号生命周期（两表共用）：
--   pending → confirmed（价格顺向确认）
--   pending → invalidated（价格逆向失效）
--   pending → expired（超时过期）
--   pending → superseded（被新方向信号取代）
--
-- 统计查询在 Rust 仓储层实现，不使用数据库视图
--
-- 执行方式：
--   psql -U postgres -d mydb -f schema_v3.sql
-- =================================================================

-- 清理旧版本（如果存在）
DROP TABLE IF EXISTS strategy_signals CASCADE;
DROP TABLE IF EXISTS strategy_analysis_log CASCADE;


-- =================================================================
-- 1. 策略信号表（交易引擎专属）
-- =================================================================
-- 写入方：trading-engine TradingLoop
-- 用途：记录引擎每次策略判断的信号，支持重启后闭环
-- 特点：可关联 order_id，追踪信号→交易的完整链路
-- =================================================================

CREATE TABLE strategy_signals (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    symbol VARCHAR(20) NOT NULL,
    strategy_id VARCHAR(50) NOT NULL,

    -- 信号内容
    direction VARCHAR(10) NOT NULL,           -- bullish/bearish/neutral
    entry_price DECIMAL(20, 8) NOT NULL,
    overall_confidence DECIMAL(5, 4) NOT NULL,
    entry_allowed BOOLEAN NOT NULL DEFAULT false,
    entry_direction VARCHAR(10),              -- long/short/null
    timeframe_details JSONB NOT NULL DEFAULT '{}',

    -- 交易关联
    order_id VARCHAR(100),                    -- 关联 trades 表
    executed BOOLEAN NOT NULL DEFAULT false,

    -- 生命周期
    status VARCHAR(20) NOT NULL DEFAULT 'pending',
    closed_reason VARCHAR(50),

    -- 验证追踪
    evaluated_at TIMESTAMPTZ,
    best_price DECIMAL(20, 8),
    worst_price DECIMAL(20, 8),
    eval_count INTEGER NOT NULL DEFAULT 0,

    -- 闭环结果
    closed_at TIMESTAMPTZ,
    close_price DECIMAL(20, 8),
    actual_return_pct DECIMAL(10, 4),

    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_engine_signals_pending
    ON strategy_signals(symbol, strategy_id) WHERE status = 'pending';
CREATE INDEX idx_engine_signals_symbol_time
    ON strategy_signals(symbol, created_at DESC);
CREATE INDEX idx_engine_signals_status
    ON strategy_signals(status, created_at DESC);
CREATE INDEX idx_engine_signals_order
    ON strategy_signals(order_id) WHERE order_id IS NOT NULL;


-- =================================================================
-- 2. 策略分析日志表（前端分析专属）
-- =================================================================
-- 写入方：前端 Tauri get_strategy_analysis 命令
-- 用途：记录用户每次触发的策略分析，支持前端重启后闭环
-- 特点：纯观察记录，不关联交易，有完整生命周期
-- =================================================================

CREATE TABLE strategy_analysis_log (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    symbol VARCHAR(20) NOT NULL,
    strategy_id VARCHAR(50) NOT NULL,

    -- 分析内容
    direction VARCHAR(10) NOT NULL,           -- bullish/bearish/neutral
    entry_price DECIMAL(20, 8) NOT NULL,
    overall_confidence DECIMAL(5, 4) NOT NULL,
    entry_allowed BOOLEAN NOT NULL DEFAULT false,
    entry_direction VARCHAR(10),              -- long/short/null
    timeframe_details JSONB NOT NULL DEFAULT '{}',

    -- 生命周期
    status VARCHAR(20) NOT NULL DEFAULT 'pending',
    closed_reason VARCHAR(50),

    -- 验证追踪
    evaluated_at TIMESTAMPTZ,
    best_price DECIMAL(20, 8),
    worst_price DECIMAL(20, 8),
    eval_count INTEGER NOT NULL DEFAULT 0,

    -- 闭环结果
    closed_at TIMESTAMPTZ,
    close_price DECIMAL(20, 8),
    actual_return_pct DECIMAL(10, 4),

    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX idx_analysis_pending
    ON strategy_analysis_log(symbol, strategy_id) WHERE status = 'pending';
CREATE INDEX idx_analysis_symbol_time
    ON strategy_analysis_log(symbol, created_at DESC);
CREATE INDEX idx_analysis_status
    ON strategy_analysis_log(status, created_at DESC);


-- =================================================================
-- 3. 约束
-- =================================================================

-- strategy_signals
ALTER TABLE strategy_signals ADD CONSTRAINT chk_engine_direction
    CHECK (direction IN ('bullish', 'bearish', 'neutral'));
ALTER TABLE strategy_signals ADD CONSTRAINT chk_engine_status
    CHECK (status IN ('pending', 'confirmed', 'invalidated', 'expired', 'superseded'));
ALTER TABLE strategy_signals ADD CONSTRAINT chk_engine_entry_dir
    CHECK (entry_direction IS NULL OR entry_direction IN ('long', 'short'));

-- strategy_analysis_log
ALTER TABLE strategy_analysis_log ADD CONSTRAINT chk_analysis_direction
    CHECK (direction IN ('bullish', 'bearish', 'neutral'));
ALTER TABLE strategy_analysis_log ADD CONSTRAINT chk_analysis_status
    CHECK (status IN ('pending', 'confirmed', 'invalidated', 'expired', 'superseded'));
ALTER TABLE strategy_analysis_log ADD CONSTRAINT chk_analysis_entry_dir
    CHECK (entry_direction IS NULL OR entry_direction IN ('long', 'short'));


-- =================================================================
-- 4. 交易对配置表
-- =================================================================

CREATE TABLE IF NOT EXISTS symbol_config (
    symbol VARCHAR(20) PRIMARY KEY,
    enabled BOOLEAN NOT NULL DEFAULT true,
    added_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

-- 默认交易对
INSERT INTO symbol_config (symbol) VALUES ('BTCUSDT'), ('ETHUSDT'), ('SOLUSDT')
ON CONFLICT (symbol) DO NOTHING;


-- 完成
DO $$
BEGIN
    RAISE NOTICE '========================================';
    RAISE NOTICE 'Schema V3 迁移完成';
    RAISE NOTICE '  strategy_signals (引擎信号)';
    RAISE NOTICE '  strategy_analysis_log (前端分析)';
    RAISE NOTICE '  symbol_config (交易对配置)';
    RAISE NOTICE '========================================';
END $$;
