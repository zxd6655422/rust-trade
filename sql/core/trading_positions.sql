-- public.trading_positions 定义

-- Drop table

-- DROP TABLE public.trading_positions;

CREATE TABLE public.trading_positions (
	id uuid DEFAULT gen_random_uuid() NOT NULL,
	exchange varchar(20) NOT NULL,
	symbol varchar(20) NOT NULL,
	side varchar(10) NOT NULL,
	quantity numeric(20, 8) NOT NULL,
	avg_entry_price numeric(20, 8) NOT NULL,
	unrealized_pnl numeric(20, 8) DEFAULT 0 NULL,
	stop_loss_price numeric(20, 8) NULL,
	take_profit_price numeric(20, 8) NULL,
	leverage int4 DEFAULT 1 NULL,
	margin numeric(20, 8) DEFAULT 0 NULL,
	created_at timestamptz DEFAULT now() NOT NULL,
	updated_at timestamptz DEFAULT now() NOT NULL,
	CONSTRAINT trading_positions_exchange_symbol_key UNIQUE (exchange, symbol),
	CONSTRAINT trading_positions_pkey PRIMARY KEY (id)
);
CREATE INDEX idx_positions_symbol ON public.trading_positions USING btree (symbol);
