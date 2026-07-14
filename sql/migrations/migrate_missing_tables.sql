-- =================================================================
-- 增量迁移脚本：只创建缺失的表
-- 执行方式：psql -U postgres -d trading_core -f migrate_missing_tables.sql
-- =================================================================

-- 检查并创建 strategy_performance 表
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.tables
        WHERE table_schema = 'public'
        AND table_name = 'strategy_performance'
    ) THEN
        CREATE TABLE strategy_performance (
            id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
            instance_id UUID NOT NULL REFERENCES strategy_instances(id) ON DELETE CASCADE,
            period_start TIMESTAMPTZ NOT NULL,
            period_end TIMESTAMPTZ NOT NULL,
            total_signals INTEGER NOT NULL DEFAULT 0,
            buy_signals INTEGER NOT NULL DEFAULT 0,
            sell_signals INTEGER NOT NULL DEFAULT 0,
            total_trades INTEGER NOT NULL DEFAULT 0,
            winning_trades INTEGER NOT NULL DEFAULT 0,
            losing_trades INTEGER NOT NULL DEFAULT 0,
            total_pnl DECIMAL(20,8) NOT NULL DEFAULT 0,
            win_rate DECIMAL(5,4),
            avg_win DECIMAL(20,8),
            avg_loss DECIMAL(20,8),
            profit_factor DECIMAL(10,4),
            max_drawdown DECIMAL(10,4),
            updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
            UNIQUE(instance_id, period_start, period_end)
        );

        CREATE INDEX idx_performance_instance ON strategy_performance(instance_id, period_start DESC);

        RAISE NOTICE '✅ 创建 strategy_performance 表';
    ELSE
        RAISE NOTICE '⏭️ strategy_performance 表已存在，跳过';
    END IF;
END $$;


-- 检查并创建 system_config 表
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.tables
        WHERE table_schema = 'public'
        AND table_name = 'system_config'
    ) THEN
        CREATE TABLE system_config (
            key VARCHAR(50) PRIMARY KEY,
            value TEXT NOT NULL,
            updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
        );

        -- 初始化调度器状态
        INSERT INTO system_config (key, value) VALUES ('scheduler_paused', 'false');

        RAISE NOTICE '✅ 创建 system_config 表';
    ELSE
        RAISE NOTICE '⏭️ system_config 表已存在，跳过';
    END IF;
END $$;


-- 完成提示
DO $$
BEGIN
    RAISE NOTICE '========================================';
    RAISE NOTICE '增量迁移完成！';
    RAISE NOTICE '========================================';
END $$;
