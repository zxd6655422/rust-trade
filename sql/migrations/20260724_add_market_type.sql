-- =================================================================
-- 迁移脚本: 添加 market_type 字段到 strategy_signals 表
-- 执行方式: psql -U your_user -d your_db -f sql/migrations/20260724_add_market_type.sql
-- 创建时间：2026-07-24
-- =================================================================

BEGIN;

-- =================================================================
-- 1. 更新 strategy_signals 表结构
-- =================================================================

-- 添加 market_type 字段
-- "futures" = 只在合约执行 (默认)
-- "spot" = 只在现货执行
-- "both" = 同时在合约和现货执行
ALTER TABLE strategy_signals
ADD COLUMN IF NOT EXISTS market_type VARCHAR(20) DEFAULT 'futures';

-- 添加约束
ALTER TABLE strategy_signals
ADD CONSTRAINT chk_signal_market_type
CHECK (market_type IN ('futures', 'spot', 'both'));

-- 添加注释
COMMENT ON COLUMN strategy_signals.market_type IS '目标市场类型: futures(合约), spot(现货), both(两者)';

-- =================================================================
-- 2. 更新 init_database.sql 中的表定义 (如果需要重建)
-- =================================================================

-- 注意: 以下是完整的 strategy_signals 表定义，用于参考
-- 如果需要重建表，取消注释以下代码
/*
CREATE TABLE IF NOT EXISTS strategy_signals (
    id UUID DEFAULT gen_random_uuid() PRIMARY KEY,
    symbol VARCHAR(20) NOT NULL,
    strategy_id VARCHAR(50) NOT NULL,
    direction VARCHAR(20) NOT NULL,           -- bullish/bearish/neutral
    entry_price DECIMAL(20, 8) NOT NULL,
    overall_confidence DECIMAL(5, 4) DEFAULT 0,
    entry_allowed BOOLEAN DEFAULT true,
    entry_direction VARCHAR(10),              -- long/short
    timeframe_details JSONB DEFAULT '{}',
    order_id VARCHAR(100),
    executed BOOLEAN DEFAULT false,
    status VARCHAR(20) DEFAULT 'pending',     -- pending/confirmed/invalidated/expired/superseded/executed/failed
    closed_reason TEXT,
    evaluated_at TIMESTAMPTZ,
    best_price DECIMAL(20, 8),
    worst_price DECIMAL(20, 8),
    eval_count INTEGER DEFAULT 0,
    closed_at TIMESTAMPTZ,
    close_price DECIMAL(20, 8),
    actual_return_pct DECIMAL(10, 4),
    created_at TIMESTAMPTZ DEFAULT NOW(),
    -- V6 新增字段
    instance_id UUID,
    signal_strength DECIMAL(5, 4),
    market_context JSONB,
    stop_loss DECIMAL(20, 8),
    take_profit DECIMAL(20, 8),
    -- V7 新增字段：策略分析详情
    market_structure JSONB,
    key_levels JSONB,
    trade_setup JSONB,
    -- V8 新增字段：目标市场类型
    market_type VARCHAR(20) DEFAULT 'futures',
    CONSTRAINT chk_signal_market_type CHECK (market_type IN ('futures', 'spot', 'both'))
);
*/

-- =================================================================
-- 3. 创建索引 (如果不存在)
-- =================================================================

CREATE INDEX IF NOT EXISTS idx_signals_market_type ON strategy_signals(market_type);

-- =================================================================
-- 4. 更新现有数据 (可选)
-- =================================================================

-- 将所有现有信号标记为 futures (向后兼容)
UPDATE strategy_signals
SET market_type = 'futures'
WHERE market_type IS NULL;

COMMIT;

-- =================================================================
-- 验证
-- =================================================================

-- 检查字段是否添加成功
SELECT column_name, data_type, column_default
FROM information_schema.columns
WHERE table_name = 'strategy_signals'
  AND column_name = 'market_type';

-- 检查数据分布
SELECT market_type, COUNT(*) as count
FROM strategy_signals
GROUP BY market_type;
