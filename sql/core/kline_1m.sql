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

-- 覆盖索引：用于 SELECT * FROM kline_1m WHERE symbol = $1 ORDER BY timestamp DESC LIMIT $2
-- INCLUDE 避免回表，实现 Index Only Scan
CREATE INDEX idx_kline_1m_cover ON public.kline_1m USING btree (symbol, "timestamp" DESC)
INCLUDE (open, high, low, close, volume, trade_count);

-- 单列时间索引：用于 MIN/MAX(timestamp) 查询
CREATE INDEX idx_kline_1m_timestamp ON public.kline_1m USING btree ("timestamp");

-- 注意：不需要单独的 (symbol, timestamp DESC) 索引，因为主键已经是这个结构
