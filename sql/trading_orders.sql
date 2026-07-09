-- public.trading_orders 定义

-- Drop table

-- DROP TABLE public.trading_orders;

CREATE TABLE public.trading_orders (
	id uuid DEFAULT gen_random_uuid() NOT NULL,
	order_id varchar(50) NOT NULL,
	exchange varchar(20) NOT NULL,
	symbol varchar(20) NOT NULL,
	side varchar(4) NOT NULL,
	order_type varchar(20) NOT NULL,
	quantity numeric(20, 8) NOT NULL,
	price numeric(20, 8) NULL,
	status varchar(20) NOT NULL,
	filled_quantity numeric(20, 8) DEFAULT 0 NULL,
	avg_price numeric(20, 8) NULL,
	commission numeric(20, 8) NULL,
	commission_asset varchar(10) NULL,
	client_order_id varchar(50) NULL,
	created_at timestamptz DEFAULT now() NOT NULL,
	updated_at timestamptz DEFAULT now() NOT NULL,
	CONSTRAINT trading_orders_order_id_exchange_key UNIQUE (order_id, exchange),
	CONSTRAINT trading_orders_pkey PRIMARY KEY (id)
);
CREATE INDEX idx_orders_status ON public.trading_orders USING btree (status);
CREATE INDEX idx_orders_symbol ON public.trading_orders USING btree (symbol);
