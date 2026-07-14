-- public.kline_1m 定义

-- Drop table

-- DROP TABLE public.kline_1m;

CREATE TABLE public.kline_1m (
	"timestamp" timestamptz NOT NULL,
	symbol varchar(20) NOT NULL,
	"open" numeric(20, 8) NOT NULL,
	high numeric(20, 8) NOT NULL,
	low numeric(20, 8) NOT NULL,
	"close" numeric(20, 8) NOT NULL,
	volume numeric(20, 8) NOT NULL,
	trade_count int4 DEFAULT 0 NOT NULL,
	CONSTRAINT kline_1m_pkey PRIMARY KEY (symbol, "timestamp")
);
CREATE INDEX idx_kline_1m_symbol_latest ON public.kline_1m USING btree (symbol, "timestamp" DESC) INCLUDE (open, high, low, close, volume);
CREATE INDEX idx_kline_1m_symbol_time ON public.kline_1m USING btree (symbol, "timestamp" DESC);
CREATE INDEX idx_kline_1m_timestamp ON public.kline_1m USING btree ("timestamp");
