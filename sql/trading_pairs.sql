-- public.trading_pairs 定义

-- Drop table

-- DROP TABLE public.trading_pairs;

CREATE TABLE public.trading_pairs (
	id serial4 NOT NULL,
	symbol varchar(20) NOT NULL,
	market_type varchar(10) NOT NULL,
	exchange varchar(20) DEFAULT 'binance'::character varying NOT NULL,
	status varchar(20) DEFAULT 'active'::character varying NOT NULL,
	note text NULL,
	created_at timestamptz DEFAULT now() NOT NULL,
	updated_at timestamptz DEFAULT now() NOT NULL,
	CONSTRAINT trading_pairs_market_type_check CHECK (((market_type)::text = ANY ((ARRAY['spot'::character varying, 'futures'::character varying])::text[]))),
	CONSTRAINT trading_pairs_pkey PRIMARY KEY (id),
	CONSTRAINT trading_pairs_status_check CHECK (((status)::text = ANY ((ARRAY['active'::character varying, 'paused'::character varying, 'archived'::character varying])::text[]))),
	CONSTRAINT trading_pairs_symbol_key UNIQUE (symbol)
);
CREATE INDEX idx_trading_pairs_market ON public.trading_pairs USING btree (market_type);
CREATE INDEX idx_trading_pairs_status ON public.trading_pairs USING btree (status);
CREATE INDEX idx_trading_pairs_symbol ON public.trading_pairs USING btree (symbol);

INSERT INTO trading_pairs
(id, symbol, market_type, exchange, status, note, created_at, updated_at)
VALUES(11, 'HYPEUSDT', 'futures', 'binance', 'active', NULL, '2026-07-07 00:55:46.518', '2026-07-07 00:55:46.518');
INSERT INTO trading_pairs
(id, symbol, market_type, exchange, status, note, created_at, updated_at)
VALUES(12, 'SPCXUSDT', 'futures', 'binance', 'active', NULL, '2026-07-07 00:55:47.767', '2026-07-07 00:55:47.767');
INSERT INTO trading_pairs
(id, symbol, market_type, exchange, status, note, created_at, updated_at)
VALUES(13, 'TSLAUSDT', 'futures', 'binance', 'active', NULL, '2026-07-07 00:55:48.250', '2026-07-07 00:55:48.250');
INSERT INTO trading_pairs
(id, symbol, market_type, exchange, status, note, created_at, updated_at)
VALUES(14, 'NVDAUSDT', 'futures', 'binance', 'active', NULL, '2026-07-07 00:55:49.087', '2026-07-07 00:55:49.087');
INSERT INTO trading_pairs
(id, symbol, market_type, exchange, status, note, created_at, updated_at)
VALUES(15, 'AAOIUSDT', 'futures', 'binance', 'active', NULL, '2026-07-07 00:55:49.578', '2026-07-07 00:55:49.578');
INSERT INTO trading_pairs
(id, symbol, market_type, exchange, status, note, created_at, updated_at)
VALUES(16, 'MUUSDT', 'futures', 'binance', 'active', NULL, '2026-07-07 00:55:49.997', '2026-07-07 00:55:49.997');
INSERT INTO trading_pairs
(id, symbol, market_type, exchange, status, note, created_at, updated_at)
VALUES(17, 'SKHYNIXUSDT', 'futures', 'binance', 'active', NULL, '2026-07-07 00:55:50.486', '2026-07-07 00:55:50.486');
INSERT INTO trading_pairs
(id, symbol, market_type, exchange, status, note, created_at, updated_at)
VALUES(1, 'BTCUSDT', 'spot', 'binance', 'active', NULL, '2026-07-06 12:55:02.739', '2026-07-06 12:55:02.739');
INSERT INTO trading_pairs
(id, symbol, market_type, exchange, status, note, created_at, updated_at)
VALUES(2, 'ETHUSDT', 'spot', 'binance', 'active', NULL, '2026-07-06 12:55:02.739', '2026-07-06 12:55:02.739');
INSERT INTO trading_pairs
(id, symbol, market_type, exchange, status, note, created_at, updated_at)
VALUES(3, 'SOLUSDT', 'spot', 'binance', 'active', NULL, '2026-07-06 12:55:02.739', '2026-07-06 12:55:02.739');
INSERT INTO trading_pairs
(id, symbol, market_type, exchange, status, note, created_at, updated_at)
VALUES(4, 'SUIUSDT', 'spot', 'binance', 'active', NULL, '2026-07-06 12:55:02.739', '2026-07-06 12:55:02.739');
INSERT INTO trading_pairs
(id, symbol, market_type, exchange, status, note, created_at, updated_at)
VALUES(5, 'BNBUSDT', 'spot', 'binance', 'active', NULL, '2026-07-06 12:55:02.739', '2026-07-06 12:55:02.739');
