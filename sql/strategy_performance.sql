-- public.strategy_performance 定义

-- 策略性能统计表
-- 定期汇总每个策略实例的运行指标

CREATE TABLE public.strategy_performance (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    instance_id uuid NOT NULL,
    period_start timestamptz NOT NULL,
    period_end timestamptz NOT NULL,
    total_signals int4 DEFAULT 0 NOT NULL,
    buy_signals int4 DEFAULT 0 NOT NULL,
    sell_signals int4 DEFAULT 0 NOT NULL,
    total_trades int4 DEFAULT 0 NOT NULL,
    winning_trades int4 DEFAULT 0 NOT NULL,
    losing_trades int4 DEFAULT 0 NOT NULL,
    total_pnl numeric(20, 8) DEFAULT 0 NOT NULL,
    win_rate numeric(5, 4) NULL,
    avg_win numeric(20, 8) NULL,
    avg_loss numeric(20, 8) NULL,
    profit_factor numeric(10, 4) NULL,
    max_drawdown numeric(10, 4) NULL,
    updated_at timestamptz DEFAULT now() NOT NULL,
    CONSTRAINT strategy_performance_pkey PRIMARY KEY (id),
    CONSTRAINT strategy_performance_instance_id_period_start_period_end_key UNIQUE (instance_id, period_start, period_end),
    CONSTRAINT strategy_performance_instance_id_fkey FOREIGN KEY (instance_id) REFERENCES public.strategy_instances(id) ON DELETE CASCADE
);

CREATE INDEX idx_performance_instance ON public.strategy_performance USING btree (instance_id, period_start DESC);
