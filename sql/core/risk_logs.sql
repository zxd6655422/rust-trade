-- public.risk_logs 定义

-- Drop table

-- DROP TABLE public.risk_logs;

CREATE TABLE public.risk_logs (
	id uuid DEFAULT gen_random_uuid() NOT NULL,
	"timestamp" timestamptz DEFAULT now() NOT NULL,
	event_type varchar(50) NOT NULL,
	symbol varchar(20) NULL,
	details jsonb NULL,
	decision varchar(20) NOT NULL,
	CONSTRAINT risk_logs_pkey PRIMARY KEY (id)
);
CREATE INDEX idx_risk_logs_timestamp ON public.risk_logs USING btree ("timestamp");
