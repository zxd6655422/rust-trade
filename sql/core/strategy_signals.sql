-- public.strategy_signals 定义

-- Drop table

-- DROP TABLE public.strategy_signals;

CREATE TABLE public.strategy_signals (
	id uuid DEFAULT gen_random_uuid() NOT NULL,
	symbol varchar(20) NOT NULL,
	strategy_id varchar(50) NOT NULL,
	direction varchar(10) NOT NULL,
	entry_price numeric(20, 8) NOT NULL,
	overall_confidence numeric(5, 4) NOT NULL,
	entry_allowed bool DEFAULT false NOT NULL,
	entry_direction varchar(10) NULL,
	timeframe_details jsonb DEFAULT '{}'::jsonb NOT NULL,
	order_id varchar(100) NULL,
	executed bool DEFAULT false NOT NULL,
	status varchar(20) DEFAULT 'pending'::character varying NOT NULL,
	closed_reason varchar(50) NULL,
	evaluated_at timestamptz NULL,
	best_price numeric(20, 8) NULL,
	worst_price numeric(20, 8) NULL,
	eval_count int4 DEFAULT 0 NOT NULL,
	closed_at timestamptz NULL,
	close_price numeric(20, 8) NULL,
	actual_return_pct numeric(10, 4) NULL,
	created_at timestamptz DEFAULT now() NOT NULL,
	instance_id uuid NULL,
	signal_strength numeric(5, 4) NULL,
	market_context jsonb NULL,
	stop_loss numeric(20, 8) NULL,
	take_profit numeric(20, 8) NULL,
	CONSTRAINT chk_engine_direction CHECK ((((direction)::text = ANY ((ARRAY['bullish'::character varying, 'bearish'::character varying, 'neutral'::character varying])::text[])))),
	CONSTRAINT chk_engine_entry_dir CHECK ((((entry_direction IS NULL) OR ((entry_direction)::text = ANY ((ARRAY['long'::character varying, 'short'::character varying])::text[]))))),
	CONSTRAINT chk_engine_status CHECK (
        (status)::text = ANY (ARRAY['pending', 'confirmed', 'invalidated', 'expired', 'superseded', 'executed', 'failed', 'rejected']::text[])
    ),
	CONSTRAINT strategy_signals_pkey1 PRIMARY KEY (id)
	-- 外键已移除，应用层保证数据完整性
);
CREATE INDEX idx_engine_signals_order ON public.strategy_signals (order_id);
CREATE INDEX idx_engine_signals_pending ON public.strategy_signals (symbol,strategy_id);
CREATE INDEX idx_engine_signals_status ON public.strategy_signals (status,created_at DESC);
CREATE INDEX idx_engine_signals_symbol_time ON public.strategy_signals (symbol,created_at DESC);
CREATE INDEX idx_signals_instance ON public.strategy_signals (instance_id,created_at DESC);
