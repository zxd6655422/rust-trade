-- public.symbol_config 定义

-- Drop table

-- DROP TABLE public.symbol_config;

CREATE TABLE public.symbol_config (
	symbol varchar(20) NOT NULL,
	enabled bool DEFAULT true NOT NULL,
	added_at timestamptz DEFAULT now() NOT NULL,
	CONSTRAINT symbol_config_pkey PRIMARY KEY (symbol)
);

INSERT INTO symbol_config
(symbol, enabled, added_at)
VALUES('BTCUSDT', true, '2026-07-06 02:31:02.087');
INSERT INTO symbol_config
(symbol, enabled, added_at)
VALUES('ETHUSDT', true, '2026-07-06 02:31:02.087');
INSERT INTO symbol_config
(symbol, enabled, added_at)
VALUES('SOLUSDT', true, '2026-07-06 02:31:02.087');
INSERT INTO symbol_config
(symbol, enabled, added_at)
VALUES('SPCXUSDT', true, '2026-07-06 03:02:47.458');
