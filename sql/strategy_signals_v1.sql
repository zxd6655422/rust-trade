-- public.strategy_signals_v1 定义

-- Drop table

-- DROP TABLE public.strategy_signals_v1;

CREATE TABLE public.strategy_signals_v1 (
	id uuid DEFAULT gen_random_uuid() NOT NULL,
	strategy_id varchar(50) NOT NULL,
	symbol varchar(20) NOT NULL,
	signal_time timestamptz NOT NULL,
	signal_type varchar(10) NOT NULL,
	signal_price numeric(20, 8) NOT NULL,
	signal_quantity numeric(20, 8) NULL,
	confidence numeric(5, 4) NULL,
	trend_direction varchar(20) NULL,
	timeframe_analysis jsonb NULL,
	created_at timestamptz DEFAULT now() NOT NULL,
	CONSTRAINT strategy_signals_pkey PRIMARY KEY (id),
	CONSTRAINT strategy_signals_signal_type_check CHECK (((signal_type)::text = ANY ((ARRAY['BUY'::character varying, 'SELL'::character varying, 'HOLD'::character varying])::text[])))
);
CREATE INDEX idx_signals_strategy_time ON public.strategy_signals_v1 USING btree (strategy_id, signal_time DESC);
CREATE INDEX idx_signals_symbol_time ON public.strategy_signals_v1 USING btree (symbol, signal_time DESC);
