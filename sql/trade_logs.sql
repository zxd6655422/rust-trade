-- public.trade_logs 定义

-- Drop table

-- DROP TABLE public.trade_logs;

CREATE TABLE public.trade_logs (
	id uuid DEFAULT gen_random_uuid() NOT NULL,
	"timestamp" timestamptz DEFAULT now() NOT NULL,
	strategy_id varchar(50) NULL,
	symbol varchar(20) NOT NULL,
	side varchar(4) NOT NULL,
	quantity numeric(20, 8) NOT NULL,
	price numeric(20, 8) NOT NULL,
	order_id varchar(50) NULL,
	pnl numeric(20, 8) NULL,
	notes text NULL,
	CONSTRAINT trade_logs_pkey PRIMARY KEY (id)
);
CREATE INDEX idx_trade_logs_timestamp ON public.trade_logs USING btree ("timestamp");
