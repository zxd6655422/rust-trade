-- public.backtest_results 定义

-- Drop table

-- DROP TABLE public.backtest_results;

CREATE TABLE public.backtest_results (
	id uuid DEFAULT gen_random_uuid() NOT NULL,
	strategy_id varchar(50) NOT NULL,
	symbol varchar(20) NOT NULL,
	initial_capital numeric(20, 8) NOT NULL,
	final_capital numeric(20, 8) NOT NULL,
	return_pct numeric(10, 4) NOT NULL,
	total_trades int4 NOT NULL,
	winning_trades int4 NOT NULL,
	losing_trades int4 NOT NULL,
	win_rate numeric(10, 4) NOT NULL,
	max_drawdown numeric(10, 4) NOT NULL,
	sharpe_ratio numeric(10, 4) NOT NULL,
	profit_factor numeric(10, 4) NOT NULL,
	data_points int4 NOT NULL,
	data_start_time timestamptz NULL,
	data_end_time timestamptz NULL,
	strategy_params jsonb NULL,
	created_at timestamptz DEFAULT now() NOT NULL,
	CONSTRAINT backtest_results_pkey PRIMARY KEY (id)
);
CREATE INDEX idx_backtest_results_created ON public.backtest_results USING btree (created_at DESC);
CREATE INDEX idx_backtest_results_strategy_symbol ON public.backtest_results USING btree (strategy_id, symbol);
CREATE INDEX idx_backtest_strategy ON public.backtest_results USING btree (strategy_id, created_at DESC);
CREATE INDEX idx_backtest_symbol ON public.backtest_results USING btree (symbol, created_at DESC);
