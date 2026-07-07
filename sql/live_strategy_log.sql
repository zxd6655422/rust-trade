-- public.live_strategy_log 定义

-- Drop table

-- DROP TABLE public.live_strategy_log;

CREATE TABLE public.live_strategy_log (
	id uuid DEFAULT gen_random_uuid() NOT NULL,
	"timestamp" timestamptz DEFAULT now() NOT NULL,
	strategy_id varchar(50) NOT NULL,
	symbol varchar(20) NOT NULL,
	current_price numeric(18, 8) NOT NULL,
	signal_type varchar(10) NOT NULL,
	portfolio_value numeric(18, 8) NOT NULL,
	total_pnl numeric(18, 8) DEFAULT 0 NOT NULL,
	cache_hit bool DEFAULT true NULL,
	processing_time_us int4 NULL,
	CONSTRAINT live_strategy_log_pkey PRIMARY KEY (id)
);
CREATE INDEX idx_live_strategy_symbol ON public.live_strategy_log USING btree (strategy_id, symbol);
CREATE INDEX idx_live_strategy_time ON public.live_strategy_log USING btree ("timestamp" DESC);
