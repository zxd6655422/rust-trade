-- public.tick_data 定义

-- Drop table

-- DROP TABLE public.tick_data;

CREATE TABLE public.tick_data (
	"timestamp" timestamptz NOT NULL,
	symbol varchar(20) NOT NULL,
	price numeric(20, 8) NOT NULL,
	quantity numeric(20, 8) NOT NULL,
	side varchar(4) NOT NULL,
	trade_id varchar(50) NOT NULL,
	is_buyer_maker bool NOT NULL,
	CONSTRAINT tick_data_side_check CHECK (((side)::text = ANY ((ARRAY['BUY'::character varying, 'SELL'::character varying])::text[])))
);
CREATE INDEX idx_tick_data_symbol_time ON public.tick_data USING btree (symbol, "timestamp" DESC);
CREATE INDEX idx_tick_symbol_time ON public.tick_data USING btree (symbol, "timestamp" DESC);
CREATE INDEX idx_tick_timestamp ON public.tick_data USING btree ("timestamp");
CREATE UNIQUE INDEX idx_tick_unique ON public.tick_data USING btree (symbol, trade_id, "timestamp");
