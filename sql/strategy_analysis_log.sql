-- public.strategy_analysis_log 定义

-- Drop table

-- DROP TABLE public.strategy_analysis_log;

CREATE TABLE public.strategy_analysis_log (
	id uuid DEFAULT gen_random_uuid() NOT NULL,
	symbol varchar(20) NOT NULL,
	strategy_id varchar(50) NOT NULL,
	direction varchar(10) NOT NULL,
	entry_price numeric(20, 8) NOT NULL,
	overall_confidence numeric(5, 4) NOT NULL,
	entry_allowed bool DEFAULT false NOT NULL,
	entry_direction varchar(10) NULL,
	timeframe_details jsonb DEFAULT '{}'::jsonb NOT NULL,
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
	CONSTRAINT chk_analysis_direction CHECK (((direction)::text = ANY ((ARRAY['bullish'::character varying, 'bearish'::character varying, 'neutral'::character varying])::text[]))),
	CONSTRAINT chk_analysis_entry_dir CHECK (((entry_direction IS NULL) OR ((entry_direction)::text = ANY ((ARRAY['long'::character varying, 'short'::character varying])::text[])))),
	CONSTRAINT chk_analysis_status CHECK (((status)::text = ANY ((ARRAY['pending'::character varying, 'confirmed'::character varying, 'invalidated'::character varying, 'expired'::character varying, 'superseded'::character varying])::text[]))),
	CONSTRAINT strategy_analysis_log_pkey PRIMARY KEY (id)
);
CREATE INDEX idx_analysis_pending ON public.strategy_analysis_log USING btree (symbol, strategy_id) WHERE ((status)::text = 'pending'::text);
CREATE INDEX idx_analysis_status ON public.strategy_analysis_log USING btree (status, created_at DESC);
CREATE INDEX idx_analysis_symbol_time ON public.strategy_analysis_log USING btree (symbol, created_at DESC);
