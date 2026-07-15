-- =====================================================
-- 策略管理优化 - 数据库表结构变更
-- 日期: 2025-07-15
-- 目的: 统一策略管理，支持回测/模拟交易绑定策略实例
-- =====================================================

-- 1. strategy_instances 表新增字段
-- =====================================================

-- 新增 is_default 字段：标记是否为默认策略
ALTER TABLE strategy_instances ADD COLUMN IF NOT EXISTS is_default bool DEFAULT false NOT NULL;

-- 新增 default_for 字段：标记作为哪个场景的默认策略
-- 可选值: 'dashboard', 'paper_trading', 'backtest', NULL
ALTER TABLE strategy_instances ADD COLUMN IF NOT EXISTS default_for varchar(50) NULL;

-- 添加注释
COMMENT ON COLUMN strategy_instances.is_default IS '是否为默认策略';
COMMENT ON COLUMN strategy_instances.default_for IS '作为哪个场景的默认策略: dashboard/paper_trading/backtest';

-- 添加索引
CREATE INDEX IF NOT EXISTS idx_strategy_instances_default ON public.strategy_instances USING btree (is_default, default_for) WHERE is_default = true;

-- 添加约束：default_for 只能是特定值或 NULL
ALTER TABLE strategy_instances ADD CONSTRAINT chk_default_for CHECK (
    default_for IS NULL OR default_for IN ('dashboard', 'paper_trading', 'backtest')
);

-- 2. backtest_results 表新增 instance_id 字段
-- =====================================================

-- 新增 instance_id 字段：关联策略实例
ALTER TABLE backtest_results ADD COLUMN IF NOT EXISTS instance_id uuid NULL;

-- 添加注释
COMMENT ON COLUMN backtest_results.instance_id IS '关联的策略实例ID';

-- 添加索引
CREATE INDEX IF NOT EXISTS idx_backtest_instance ON public.backtest_results USING btree (instance_id);

-- 3. live_strategy_log 表新增 instance_id 字段
-- =====================================================

-- 新增 instance_id 字段：关联策略实例
ALTER TABLE live_strategy_log ADD COLUMN IF NOT EXISTS instance_id uuid NULL;

-- 添加注释
COMMENT ON COLUMN live_strategy_log.instance_id IS '关联的策略实例ID';

-- 添加索引
CREATE INDEX IF NOT EXISTS idx_live_strategy_instance ON public.live_strategy_log USING btree (instance_id);

-- 4. 新增 paper_trading_sessions 表
-- =====================================================

CREATE TABLE IF NOT EXISTS paper_trading_sessions (
    id uuid DEFAULT gen_random_uuid() NOT NULL,
    instance_id uuid NOT NULL,                                      -- 关联策略实例
    strategy_type varchar(50) NOT NULL,                              -- 策略类型
    display_name varchar(100) NOT NULL,                              -- 策略显示名称
    initial_capital numeric(20, 8) NOT NULL,                         -- 初始资金
    status varchar(20) DEFAULT 'running' NOT NULL,                   -- running/stopped/completed
    started_at timestamptz DEFAULT now() NOT NULL,                   -- 开始时间
    stopped_at timestamptz NULL,                                     -- 停止时间
    total_pnl numeric(20, 8) DEFAULT 0,                             -- 总盈亏
    total_trades int4 DEFAULT 0,                                     -- 总交易次数
    winning_trades int4 DEFAULT 0,                                   -- 盈利交易次数
    losing_trades int4 DEFAULT 0,                                    -- 亏损交易次数
    win_rate numeric(5, 4) DEFAULT 0,                               -- 胜率
    max_drawdown numeric(10, 4) DEFAULT 0,                          -- 最大回撤
    notes text NULL,                                                 -- 备注
    created_at timestamptz DEFAULT now() NOT NULL,                   -- 创建时间
    updated_at timestamptz DEFAULT now() NOT NULL,                   -- 更新时间
    CONSTRAINT paper_trading_sessions_pkey PRIMARY KEY (id),
    CONSTRAINT paper_trading_sessions_status_check CHECK (
        status IN ('running', 'stopped', 'completed')
    )
);

-- 添加注释
COMMENT ON TABLE paper_trading_sessions IS '模拟交易会话表';
COMMENT ON COLUMN paper_trading_sessions.id IS '会话ID';
COMMENT ON COLUMN paper_trading_sessions.instance_id IS '关联的策略实例ID';
COMMENT ON COLUMN paper_trading_sessions.strategy_type IS '策略类型';
COMMENT ON COLUMN paper_trading_sessions.display_name IS '策略显示名称';
COMMENT ON COLUMN paper_trading_sessions.initial_capital IS '初始资金';
COMMENT ON COLUMN paper_trading_sessions.status IS '状态: running/stopped/completed';
COMMENT ON COLUMN paper_trading_sessions.started_at IS '开始时间';
COMMENT ON COLUMN paper_trading_sessions.stopped_at IS '停止时间';
COMMENT ON COLUMN paper_trading_sessions.total_pnl IS '总盈亏';
COMMENT ON COLUMN paper_trading_sessions.total_trades IS '总交易次数';
COMMENT ON COLUMN paper_trading_sessions.winning_trades IS '盈利交易次数';
COMMENT ON COLUMN paper_trading_sessions.losing_trades IS '亏损交易次数';
COMMENT ON COLUMN paper_trading_sessions.win_rate IS '胜率';
COMMENT ON COLUMN paper_trading_sessions.max_drawdown IS '最大回撤';
COMMENT ON COLUMN paper_trading_sessions.notes IS '备注';

-- 添加索引
CREATE INDEX IF NOT EXISTS idx_paper_sessions_instance ON public.paper_trading_sessions USING btree (instance_id);
CREATE INDEX IF NOT EXISTS idx_paper_sessions_status ON public.paper_trading_sessions USING btree (status, started_at DESC);
CREATE INDEX IF NOT EXISTS idx_paper_sessions_time ON public.paper_trading_sessions USING btree (started_at DESC);

-- 5. 插入默认策略配置示例
-- =====================================================

-- 注意：以下为示例数据，实际使用时需要根据实际情况调整
-- 如果已有策略实例，可以更新为默认策略

-- 示例：将某个 RSI 策略设置为 Dashboard 默认策略
-- UPDATE strategy_instances
-- SET is_default = true, default_for = 'dashboard'
-- WHERE strategy_type = 'rsi' AND status = 'active'
-- LIMIT 1;

-- 示例：将某个趋势策略设置为模拟交易默认策略
-- UPDATE strategy_instances
-- SET is_default = true, default_for = 'paper_trading'
-- WHERE strategy_type = 'trend' AND status = 'active'
-- LIMIT 1;

-- 6. 验证变更
-- =====================================================

-- 验证字段添加成功
DO $$
BEGIN
    -- 检查 strategy_instances 表
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_name = 'strategy_instances' AND column_name = 'is_default'
    ) THEN
        RAISE EXCEPTION 'strategy_instances.is_default field not added';
    END IF;

    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_name = 'strategy_instances' AND column_name = 'default_for'
    ) THEN
        RAISE EXCEPTION 'strategy_instances.default_for field not added';
    END IF;

    -- 检查 backtest_results 表
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_name = 'backtest_results' AND column_name = 'instance_id'
    ) THEN
        RAISE EXCEPTION 'backtest_results.instance_id field not added';
    END IF;

    -- 检查 live_strategy_log 表
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_name = 'live_strategy_log' AND column_name = 'instance_id'
    ) THEN
        RAISE EXCEPTION 'live_strategy_log.instance_id field not added';
    END IF;

    -- 检查 paper_trading_sessions 表
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.tables
        WHERE table_name = 'paper_trading_sessions'
    ) THEN
        RAISE EXCEPTION 'paper_trading_sessions table not created';
    END IF;

    RAISE NOTICE 'All schema changes applied successfully!';
END $$;
