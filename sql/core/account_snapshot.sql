-- =================================================================
-- 统一账户快照表
-- 支持 Binance / OKX 等多交易所
-- 优化：移除未使用的 raw_data JSONB 字段，减小存储空间
-- 创建时间：2026-07-08
-- 更新时间：2026-07-16
-- =================================================================

-- 账户快照表（账户级别汇总）
CREATE TABLE IF NOT EXISTS account_snapshot (
    id BIGSERIAL PRIMARY KEY,
    exchange VARCHAR(20) NOT NULL,          -- 'binance' / 'okx'
    market_type VARCHAR(20) NOT NULL,       -- 'spot' / 'futures' / 'swap'
    uid VARCHAR(20),                        -- 用户标识（API Key 前缀）
    snapshot_at TIMESTAMPTZ NOT NULL,

    -- ============ 余额相关 ============
    total_equity DECIMAL(20,8) NOT NULL DEFAULT 0,      -- 总权益（USD）
    total_balance DECIMAL(20,8) NOT NULL DEFAULT 0,      -- 总余额（不含未实现盈亏）
    available_balance DECIMAL(20,8) NOT NULL DEFAULT 0,  -- 可用余额
    frozen_balance DECIMAL(20,8) NOT NULL DEFAULT 0,     -- 冻结余额

    -- ============ 盈亏相关 ============
    unrealized_pnl DECIMAL(20,8) NOT NULL DEFAULT 0,     -- 未实现盈亏

    -- ============ 保证金相关（仅合约） ============
    initial_margin DECIMAL(20,8),           -- 初始保证金
    maint_margin DECIMAL(20,8),             -- 维持保证金
    margin_ratio DECIMAL(10,8),             -- 保证金率

    -- ============ 持仓相关 ============
    position_count INTEGER NOT NULL DEFAULT 0,

    UNIQUE(exchange, market_type, uid, snapshot_at)
);

CREATE INDEX IF NOT EXISTS idx_account_snapshot_uid
    ON account_snapshot(uid, exchange, market_type, snapshot_at DESC);

-- 资产余额详情表
CREATE TABLE IF NOT EXISTS asset_balance (
    id BIGSERIAL PRIMARY KEY,
    exchange VARCHAR(20) NOT NULL,
    market_type VARCHAR(20) NOT NULL,
    uid VARCHAR(20),                        -- 用户标识
    asset VARCHAR(20) NOT NULL,             -- 'USDT' / 'BTC'
    snapshot_at TIMESTAMPTZ NOT NULL,

    total DECIMAL(20,8) NOT NULL DEFAULT 0,         -- 总余额
    available DECIMAL(20,8) NOT NULL DEFAULT 0,      -- 可用余额
    frozen DECIMAL(20,8) NOT NULL DEFAULT 0,         -- 冻结余额
    unrealized_pnl DECIMAL(20,8) NOT NULL DEFAULT 0, -- 未实现盈亏
    usd_value DECIMAL(20,8),                         -- USD价值

    UNIQUE(exchange, market_type, uid, asset, snapshot_at)
);

CREATE INDEX IF NOT EXISTS idx_asset_balance_uid
    ON asset_balance(uid, exchange, market_type, snapshot_at DESC);

-- 持仓快照表
CREATE TABLE IF NOT EXISTS position_snapshot (
    id BIGSERIAL PRIMARY KEY,
    exchange VARCHAR(20) NOT NULL,
    market_type VARCHAR(20) NOT NULL DEFAULT 'futures',  -- 'futures' / 'swap' / 'spot'
    uid VARCHAR(20),                        -- 用户标识
    symbol VARCHAR(30) NOT NULL,            -- 统一格式: 'BTCUSDT'
    raw_symbol VARCHAR(50) NOT NULL,        -- 原始格式: 'BTCUSDT' / 'BTC-USDT-SWAP'
    snapshot_at TIMESTAMPTZ NOT NULL,

    -- ============ 持仓基本信息 ============
    position_side VARCHAR(10) NOT NULL,     -- 'LONG' / 'SHORT' / 'BOTH' / 'NET'
    position_amt DECIMAL(20,8) NOT NULL,    -- 持仓数量
    entry_price DECIMAL(20,8) NOT NULL,     -- 开仓均价
    mark_price DECIMAL(20,8),               -- 标记价格（/fapi/v2/account 不返回此字段）
    unrealized_pnl DECIMAL(20,8) NOT NULL,  -- 未实现盈亏

    -- ============ 杠杆和保证金 ============
    leverage INTEGER NOT NULL DEFAULT 1,
    margin_type VARCHAR(10) NOT NULL DEFAULT 'cross',  -- 'cross' / 'isolated'
    initial_margin DECIMAL(20,8) NOT NULL DEFAULT 0,
    maint_margin DECIMAL(20,8) NOT NULL DEFAULT 0,

    -- ============ 风控相关 ============
    liquidation_price DECIMAL(20,8),        -- 强平价格
    notional DECIMAL(20,8) NOT NULL DEFAULT 0,  -- 名义价值

    -- ============ 盈亏平衡 ============
    break_even_price DECIMAL(20,8),         -- 盈亏平衡价
    isolated_wallet DECIMAL(20,8),          -- 逐仓钱包余额

    -- ============ 盈亏计算 ============
    pnl_ratio DECIMAL(10,8),                -- 盈亏比例 = unrealized_pnl / (entry_price * position_amt)

    UNIQUE(exchange, symbol, position_side, uid, snapshot_at)
);

CREATE INDEX IF NOT EXISTS idx_position_snapshot_uid
    ON position_snapshot(uid, exchange, snapshot_at DESC);

-- 注意：清理旧快照的逻辑已移至应用层实现（Rust 代码）
