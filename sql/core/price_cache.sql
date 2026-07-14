-- public.price_cache 定义

-- Drop table

-- DROP TABLE public.price_cache;

CREATE TABLE public.price_cache (
	symbol varchar(20) NOT NULL,
	price numeric(20, 8) NOT NULL,
	change_24h numeric(10, 4) NULL,
	volume_24h numeric(20, 8) NULL,
	updated_at timestamptz DEFAULT now() NOT NULL,
	CONSTRAINT price_cache_pkey PRIMARY KEY (symbol)
);
