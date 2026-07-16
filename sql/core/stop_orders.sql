-- =================================================================
-- stop_orders 表结构
-- 止损止盈订单持久化表
-- 用于在引擎重启后恢复止损止盈状态
-- 更新时间：2026-07-16
-- =================================================================

CREATE TABLE IF NOT EXISTS stop_orders (
    id UUID DEFAULT gen_random_uuid() PRIMARY KEY,
    exchange VARCHAR(32) NOT NULL,              -- 'binance' / 'okx'
    market_type VARCHAR(16) NOT NULL DEFAULT 'futures',  -- 'spot' / 'futures'
    uid VARCHAR(20),                            -- 用户标识（API Key 前缀）
    symbol VARCHAR(32) NOT NULL,                -- 交易对
    side VARCHAR(8) NOT NULL,                   -- 'LONG' / 'SHORT'
    quantity DECIMAL(20, 8) NOT NULL,           -- 数量
    entry_price DECIMAL(20, 8) NOT NULL,        -- 开仓价格
    stop_loss_price DECIMAL(20, 8),             -- 止损触发价格
    take_profit_price DECIMAL(20, 8),           -- 止盈触发价格
    trailing_stop_pct DECIMAL(10, 6),           -- 追踪止损回撤百分比，如 0.01 = 1%
    exchange_sl_order_id VARCHAR(128),          -- 交易所止损条件单订单ID
    exchange_tp_order_id VARCHAR(128),          -- 交易所止盈条件单订单ID
    status VARCHAR(16) DEFAULT 'active' NOT NULL,  -- 'active' / 'triggered' / 'cancelled' / 'expired'
    triggered_at TIMESTAMPTZ,                   -- 触发时间
    triggered_reason VARCHAR(32),               -- 触发原因: 'stop_loss' / 'take_profit' / 'trailing_stop'
    created_at TIMESTAMPTZ DEFAULT NOW() NOT NULL,
    updated_at TIMESTAMPTZ DEFAULT NOW() NOT NULL,

    CONSTRAINT chk_stop_orders_status CHECK (
        status IN ('active', 'triggered', 'cancelled', 'expired')
    )
);

-- 查询索引
CREATE INDEX IF NOT EXISTS idx_stop_orders_active ON stop_orders(exchange, symbol, status)
    WHERE status = 'active';
CREATE INDEX IF NOT EXISTS idx_stop_orders_symbol ON stop_orders(symbol, status);
CREATE INDEX IF NOT EXISTS idx_stop_orders_exchange ON stop_orders(exchange, market_type, status);
CREATE INDEX IF NOT EXISTS idx_stop_orders_uid ON stop_orders(uid);

COMMENT ON TABLE stop_orders IS '止损止盈订单持久化表';
COMMENT ON COLUMN stop_orders.exchange_sl_order_id IS '交易所止损条件单订单ID';
COMMENT ON COLUMN stop_orders.exchange_tp_order_id IS '交易所止盈条件单订单ID';
COMMENT ON COLUMN stop_orders.trailing_stop_pct IS '追踪止损回撤百分比，如 0.01 = 1%';
COMMENT ON COLUMN stop_orders.triggered_reason IS '触发原因: stop_loss / take_profit / trailing_stop';
