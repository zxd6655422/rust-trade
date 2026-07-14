-- trading_positions 表扩展
-- 新增 market_type 字段，支持同一交易所的现货/合约独立持仓

-- 新增 market_type 字段
ALTER TABLE public.trading_positions
    ADD COLUMN IF NOT EXISTS market_type varchar(16) DEFAULT 'futures' NOT NULL;

-- 删除旧的唯一约束
ALTER TABLE public.trading_positions
    DROP CONSTRAINT IF EXISTS trading_positions_exchange_symbol_key;

-- 新增包含 market_type 的唯一约束
ALTER TABLE public.trading_positions
    ADD CONSTRAINT trading_positions_exchange_market_symbol_key
    UNIQUE (exchange, market_type, symbol);

-- 新增标记价格和清算价格字段（如果不存在）
ALTER TABLE public.trading_positions
    ADD COLUMN IF NOT EXISTS mark_price numeric(20,8) NULL;
ALTER TABLE public.trading_positions
    ADD COLUMN IF NOT EXISTS liquidation_price numeric(20,8) NULL;
ALTER TABLE public.trading_positions
    ADD COLUMN IF NOT EXISTS margin_type varchar(16) DEFAULT 'ISOLATED';
ALTER TABLE public.trading_positions
    ADD COLUMN IF NOT EXISTS notional numeric(20,8) DEFAULT 0;
ALTER TABLE public.trading_positions
    ADD COLUMN IF NOT EXISTS break_even_price numeric(20,8) NULL;

COMMENT ON COLUMN trading_positions.market_type IS '交易模式: spot / futures';
COMMENT ON COLUMN trading_positions.margin_type IS '保证金模式: ISOLATED / CROSSED';
