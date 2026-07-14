-- public.strategy_instances 定义

-- Drop table

-- DROP TABLE public.strategy_instances;

CREATE TABLE public.strategy_instances (
	id uuid DEFAULT gen_random_uuid() NOT NULL,
	strategy_type varchar(50) NOT NULL,
	display_name varchar(100) NOT NULL,
	params jsonb NOT NULL,
	status varchar(20) DEFAULT 'active'::character varying NOT NULL,
	symbols _text DEFAULT '{}'::text[] NOT NULL,
	auto_trade bool DEFAULT false NOT NULL,
	position_size_pct numeric(5, 2) DEFAULT 10.0 NOT NULL,
	exchange varchar(20) DEFAULT 'binance'::character varying NOT NULL,
	market_type varchar(10) DEFAULT 'futures'::character varying NOT NULL,
	note text NULL,
	created_at timestamptz DEFAULT now() NOT NULL,
	updated_at timestamptz DEFAULT now() NOT NULL,
	CONSTRAINT strategy_instances_market_type_check CHECK (((market_type)::text = ANY ((ARRAY['spot'::character varying, 'futures'::character varying])::text[]))),
	CONSTRAINT strategy_instances_pkey PRIMARY KEY (id),
	CONSTRAINT strategy_instances_status_check CHECK (((status)::text = ANY ((ARRAY['active'::character varying, 'paused'::character varying, 'archived'::character varying])::text[])))
);
CREATE INDEX idx_strategy_instances_exchange ON public.strategy_instances USING btree (exchange);
CREATE INDEX idx_strategy_instances_status ON public.strategy_instances USING btree (status);
CREATE INDEX idx_strategy_instances_type ON public.strategy_instances USING btree (strategy_type);
