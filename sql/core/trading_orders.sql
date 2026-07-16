-- =================================================================
-- trading_orders 表结构
-- 记录所有交易订单信息
-- 更新时间：2026-07-16
-- =================================================================

CREATE TABLE IF NOT EXISTS trading_orders (
    id UUID DEFAULT gen_random_uuid() PRIMARY KEY,
    order_id VARCHAR(50) NOT NULL,              -- 交易所订单ID
    exchange VARCHAR(20) NOT NULL,              -- 'binance' / 'okx'
    market_type VARCHAR(10) NOT NULL DEFAULT 'futures',  -- 'spot' / 'futures'
    uid VARCHAR(20),                            -- 用户标识（API Key 前缀）
    symbol VARCHAR(20) NOT NULL,                -- 交易对
    side VARCHAR(4) NOT NULL,                   -- 'BUY' / 'SELL'
    order_type VARCHAR(20) NOT NULL,            -- 'MARKET' / 'LIMIT' / 'STOP_MARKET' 等
    position_side VARCHAR(10) DEFAULT 'BOTH',   -- 'LONG' / 'SHORT' / 'BOTH'
    quantity DECIMAL(20, 8) NOT NULL,           -- 下单数量
    price DECIMAL(20, 8),                       -- 下单价格（市价单为NULL）
    stop_price DECIMAL(20, 8),                  -- 止损/止盈触发价格
    status VARCHAR(20) NOT NULL,                -- 'NEW' / 'FILLED' / 'CANCELED' / 'REJECTED'
    filled_quantity DECIMAL(20, 8) DEFAULT 0,   -- 已成交数量
    avg_price DECIMAL(20, 8),                   -- 成交均价
    commission DECIMAL(20, 8),                  -- 手续费
    commission_asset VARCHAR(10),               -- 手续费资产
    client_order_id VARCHAR(50),                -- 客户端订单ID
    time_in_force VARCHAR(10) DEFAULT 'GTC',    -- 'GTC' / 'IOC' / 'FOK'
    source VARCHAR(20) NOT NULL DEFAULT 'unknown',  -- 'auto' / 'manual' / 'unknown'
    signal_id UUID,                             -- 关联策略信号ID
    strategy_id VARCHAR(50),                    -- 关联策略实例ID
    created_at TIMESTAMPTZ DEFAULT NOW() NOT NULL,
    updated_at TIMESTAMPTZ DEFAULT NOW() NOT NULL,

    CONSTRAINT trading_orders_order_id_exchange_key UNIQUE (order_id, exchange)
);

-- 查询索引
CREATE INDEX IF NOT EXISTS idx_orders_status ON trading_orders(status);
CREATE INDEX IF NOT EXISTS idx_orders_symbol ON trading_orders(symbol);
CREATE INDEX IF NOT EXISTS idx_orders_market_type ON trading_orders(market_type);
CREATE INDEX IF NOT EXISTS idx_orders_uid ON trading_orders(uid);
CREATE INDEX IF NOT EXISTS idx_orders_source ON trading_orders(source);
CREATE INDEX IF NOT EXISTS idx_orders_signal ON trading_orders(signal_id);
CREATE INDEX IF NOT EXISTS idx_orders_strategy ON trading_orders(strategy_id);
CREATE INDEX IF NOT EXISTS idx_orders_created ON trading_orders(created_at DESC);
