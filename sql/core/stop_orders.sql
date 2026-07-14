-- public.stop_orders 定义
-- 止损止盈订单持久化表
-- 用于在引擎重启后恢复止损止盈状态

CREATE TABLE public.stop_orders (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    exchange varchar(32) NOT NULL,
    market_type varchar(16) NOT NULL DEFAULT 'futures',
    symbol varchar(32) NOT NULL,
    side varchar(8) NOT NULL,
    quantity numeric(20,8) NOT NULL,
    entry_price numeric(20,8) NOT NULL,
    stop_loss_price numeric(20,8) NULL,
    take_profit_price numeric(20,8) NULL,
    trailing_stop_pct numeric(10,6) NULL,
    exchange_sl_order_id varchar(128) NULL,
    exchange_tp_order_id varchar(128) NULL,
    status varchar(16) DEFAULT 'active' NOT NULL,
    triggered_at timestamptz NULL,
    triggered_reason varchar(32) NULL,
    created_at timestamptz DEFAULT now() NOT NULL,
    updated_at timestamptz DEFAULT now() NOT NULL,
    CONSTRAINT stop_orders_pkey PRIMARY KEY (id),
    CONSTRAINT chk_stop_orders_status CHECK (
        (status)::text = ANY (
            (ARRAY['active', 'triggered', 'cancelled', 'expired'])::text[]
        )
    )
);

CREATE INDEX idx_stop_orders_active ON public.stop_orders (exchange, symbol, status)
    WHERE (status)::text = 'active';
CREATE INDEX idx_stop_orders_symbol ON public.stop_orders (symbol, status);
CREATE INDEX idx_stop_orders_exchange ON public.stop_orders (exchange, market_type, status);

COMMENT ON TABLE stop_orders IS '止损止盈订单持久化表';
COMMENT ON COLUMN stop_orders.exchange_sl_order_id IS '交易所止损条件单订单ID';
COMMENT ON COLUMN stop_orders.exchange_tp_order_id IS '交易所止盈条件单订单ID';
COMMENT ON COLUMN stop_orders.trailing_stop_pct IS '追踪止损回撤百分比，如 0.01 = 1%';
COMMENT ON COLUMN stop_orders.triggered_reason IS '触发原因: stop_loss / take_profit / trailing_stop';
