-- public.risk_config 定义
-- 风控参数配置表，支持运行时热更新

CREATE TABLE IF NOT EXISTS public.risk_config (
    key VARCHAR(50) NOT NULL,
    value DECIMAL(20,8) NOT NULL,
    description TEXT,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CONSTRAINT risk_config_pkey PRIMARY KEY (key)
);

COMMENT ON TABLE public.risk_config IS '风控参数配置表，支持运行时热更新';
COMMENT ON COLUMN public.risk_config.key IS '参数名';
COMMENT ON COLUMN public.risk_config.value IS '参数值';
COMMENT ON COLUMN public.risk_config.description IS '参数说明';
COMMENT ON COLUMN public.risk_config.updated_at IS '最后更新时间';

-- 默认值（从 config.toml [risk_control] 提取）
INSERT INTO public.risk_config (key, value, description) VALUES
    -- 基础风控
    ('max_position_pct',     0.30,        '单笔最大仓位占权益百分比 (0.30 = 30%)'),
    ('stop_loss_pct',        0.02,        '止损百分比 (0.02 = 2%)'),
    ('take_profit_pct',      0.04,        '止盈百分比 (0.04 = 4%)'),
    ('risk_per_trade_pct',   0.02,        '单笔风险占权益百分比 (0.02 = 2%)'),
    -- 中级风控
    ('max_daily_loss',       500.0,       '日最大亏损 (USDT)'),
    ('max_drawdown_pct',     0.15,        '最大回撤百分比 (0.15 = 15%)'),
    ('max_exposure_pct',     0.8,         '最大总曝光度百分比 (0.8 = 80%)'),
    -- 高级风控
    ('kelly_fraction',       0.25,        'Kelly 公式分数 (0.25 = 1/4 Kelly)'),
    ('volatility_lookback',  20,          '波动率计算回溯 tick 数量'),
    ('volatility_target',    0.15,        '目标波动率 (0.15 = 15%)'),
    ('black_swan_threshold', 0.05,        '黑天鹅检测阈值 (0.05 = 5% 瞬间波动)'),
    ('circuit_breaker_cooldown', 3600,    '熔断冷却时间 (秒)'),
    -- 每日重置
    ('daily_reset_hour',     0,           '每日重置小时 (UTC 0-23, 0=午夜)')
ON CONFLICT (key) DO NOTHING;
