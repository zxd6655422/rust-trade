-- public.system_config 定义

-- Drop table

-- DROP TABLE public.system_config;

CREATE TABLE public.system_config (
	"key" varchar(50) NOT NULL,
	value text NOT NULL,
	updated_at timestamptz DEFAULT now() NOT NULL,
	CONSTRAINT system_config_pkey PRIMARY KEY (key)
);
