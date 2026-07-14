-- public.trades 定义

-- Drop table

-- DROP TABLE public.trades;

CREATE TABLE public.trades (
	id uuid DEFAULT gen_random_uuid() NOT NULL,
	order_id varchar(100) NULL,
	symbol varchar(20) NOT NULL,
	side varchar(10) NOT NULL,
	price numeric(20, 8) NOT NULL,
	quantity numeric(20, 8) NOT NULL,
	commission numeric(20, 8) DEFAULT 0 NOT NULL,
	realized_pnl numeric(20, 8) NULL,
	strategy_id varchar(50) NULL,
	trade_time timestamptz NOT NULL,
	created_at timestamptz DEFAULT now() NOT NULL,
	exchange varchar(20) DEFAULT 'binance'::character varying NOT NULL,
	market_type varchar(10) DEFAULT 'futures'::character varying NOT NULL,
	signal_id uuid NULL,
	order_status varchar(20) DEFAULT 'filled'::character varying NULL,
	order_type varchar(20) DEFAULT 'market'::character varying NULL,
	leverage int4 DEFAULT 1 NULL,
	slippage numeric(10, 6) NULL,
	metadata jsonb NULL,
	CONSTRAINT trades_market_type_check CHECK (((market_type)::text = ANY ((ARRAY['spot'::character varying, 'futures'::character varying])::text[]))),
	CONSTRAINT trades_pkey PRIMARY KEY (id),
	CONSTRAINT trades_side_check CHECK (((side)::text = ANY ((ARRAY['BUY'::character varying, 'SELL'::character varying])::text[])))
	-- 外键已移除，应用层保证数据完整性
);
CREATE INDEX idx_trades_exchange ON public.trades USING btree (exchange);
CREATE INDEX idx_trades_exchange_symbol ON public.trades USING btree (exchange, symbol);
CREATE INDEX idx_trades_market_type ON public.trades USING btree (market_type);
CREATE INDEX idx_trades_signal ON public.trades USING btree (signal_id);
CREATE INDEX idx_trades_strategy ON public.trades USING btree (strategy_id, trade_time DESC);
CREATE INDEX idx_trades_symbol_time ON public.trades USING btree (symbol, trade_time DESC);
