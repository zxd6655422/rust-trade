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


-- 检查并创建未来 K 线分区的函数（可选，为将来准备）
CREATE OR REPLACE FUNCTION create_kline_partition(start_date DATE)
RETURNS VOID AS $$
DECLARE
    partition_name TEXT;
    end_date DATE;
BEGIN
    partition_name := 'kline_1m_' || to_char(start_date, 'YYYY_MM');
    end_date := start_date + INTERVAL '1 month';

    -- 检查分区是否已存在
    IF NOT EXISTS (
        SELECT 1 FROM pg_class
        WHERE relname = partition_name
    ) THEN
        -- 注意：当前 kline_1m 是普通表，不是分区表
        -- 如果未来需要分区，需要先迁移表结构
        RAISE NOTICE '⚠️ 当前 kline_1m 是普通表，不支持直接创建分区';
        RAISE NOTICE '💡 如需分区，请参考分区迁移指南';
    END IF;
END;
$$ LANGUAGE plpgsql;


-- 完成提示
DO $$
BEGIN
    RAISE NOTICE '========================================';
    RAISE NOTICE '增量迁移完成！';
    RAISE NOTICE '========================================';
END $$;
