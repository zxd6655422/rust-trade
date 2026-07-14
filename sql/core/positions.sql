-- public.positions 定义

-- Drop table

-- DROP TABLE public.positions;

CREATE TABLE public.positions (
	id uuid DEFAULT gen_random_uuid() NOT NULL,
	symbol varchar(20) NOT NULL,
	side varchar(10) NOT NULL,
	quantity numeric(20, 8) NOT NULL,
	avg_entry_price numeric(20, 8) NOT NULL,
	current_price numeric(20, 8) NULL,
	unrealized_pnl numeric(20, 8) NULL,
	realized_pnl numeric(20, 8) DEFAULT 0 NOT NULL,
	opened_at timestamptz NOT NULL,
	updated_at timestamptz DEFAULT now() NOT NULL,
	exchange varchar(20) DEFAULT 'binance'::character varying NOT NULL,
	market_type varchar(10) DEFAULT 'futures'::character varying NOT NULL,
	CONSTRAINT positions_market_type_check CHECK (((market_type)::text = ANY ((ARRAY['spot'::character varying, 'futures'::character varying])::text[]))),
	CONSTRAINT positions_pkey PRIMARY KEY (id),
	CONSTRAINT positions_side_check CHECK (((side)::text = ANY ((ARRAY['LONG'::character varying, 'SHORT'::character varying])::text[]))),
	CONSTRAINT positions_symbol_key UNIQUE (symbol)
);
CREATE INDEX idx_positions_exchange ON public.positions USING btree (exchange);
CREATE INDEX idx_positions_market_type ON public.positions USING btree (market_type);
