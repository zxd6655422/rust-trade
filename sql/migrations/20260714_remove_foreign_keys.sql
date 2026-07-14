-- =================================================================
-- 移除外键约束迁移脚本
-- 执行时间：2026-07-14
-- 目的：提升性能，支持分库分表，应用层控制数据完整性
--
-- 执行方式：
--   psql -U postgres -d trading_core -f 20260714_remove_foreign_keys.sql
--
-- 注意：此脚本幂等，可重复执行
-- =================================================================

DO $$
BEGIN
    RAISE NOTICE '========================================';
    RAISE NOTICE '开始移除外键约束...';
    RAISE NOTICE '========================================';
END $$;


-- =================================================================
-- 1. strategy_signals 表
-- 移除: instance_id → strategy_instances(id)
-- =================================================================

DO $$
DECLARE
    constraint_name TEXT;
BEGIN
    -- 查找外键约束名称
    SELECT tc.constraint_name INTO constraint_name
    FROM information_schema.table_constraints tc
    JOIN information_schema.key_column_usage kcu
        ON tc.constraint_name = kcu.constraint_name
    WHERE tc.table_name = 'strategy_signals'
        AND tc.constraint_type = 'FOREIGN KEY'
        AND kcu.column_name = 'instance_id';

    IF constraint_name IS NOT NULL THEN
        EXECUTE 'ALTER TABLE strategy_signals DROP CONSTRAINT ' || constraint_name;
        RAISE NOTICE '✅ strategy_signals: 已移除外键 %', constraint_name;
    ELSE
        RAISE NOTICE '⏭️ strategy_signals: 无外键约束';
    END IF;

    -- 确保索引存在
    CREATE INDEX IF NOT EXISTS idx_signals_instance ON strategy_signals(instance_id, created_at DESC);
    RAISE NOTICE '✅ strategy_signals: 索引 idx_signals_instance 已就绪';
END $$;


-- =================================================================
-- 2. strategy_performance 表
-- 移除: instance_id → strategy_instances(id) ON DELETE CASCADE
-- =================================================================

DO $$
DECLARE
    constraint_name TEXT;
BEGIN
    -- 查找外键约束名称
    SELECT tc.constraint_name INTO constraint_name
    FROM information_schema.table_constraints tc
    JOIN information_schema.key_column_usage kcu
        ON tc.constraint_name = kcu.constraint_name
    WHERE tc.table_name = 'strategy_performance'
        AND tc.constraint_type = 'FOREIGN KEY'
        AND kcu.column_name = 'instance_id';

    IF constraint_name IS NOT NULL THEN
        EXECUTE 'ALTER TABLE strategy_performance DROP CONSTRAINT ' || constraint_name;
        RAISE NOTICE '✅ strategy_performance: 已移除外键 %', constraint_name;
    ELSE
        RAISE NOTICE '⏭️ strategy_performance: 无外键约束';
    END IF;

    -- 确保索引存在
    CREATE INDEX IF NOT EXISTS idx_performance_instance ON strategy_performance(instance_id, period_start DESC);
    RAISE NOTICE '✅ strategy_performance: 索引 idx_performance_instance 已就绪';
END $$;


-- =================================================================
-- 3. trades 表
-- 移除: signal_id → strategy_signals(id)
-- =================================================================

DO $$
DECLARE
    constraint_name TEXT;
BEGIN
    -- 查找外键约束名称
    SELECT tc.constraint_name INTO constraint_name
    FROM information_schema.table_constraints tc
    JOIN information_schema.key_column_usage kcu
        ON tc.constraint_name = kcu.constraint_name
    WHERE tc.table_name = 'trades'
        AND tc.constraint_type = 'FOREIGN KEY'
        AND kcu.column_name = 'signal_id';

    IF constraint_name IS NOT NULL THEN
        EXECUTE 'ALTER TABLE trades DROP CONSTRAINT ' || constraint_name;
        RAISE NOTICE '✅ trades: 已移除外键 %', constraint_name;
    ELSE
        RAISE NOTICE '⏭️ trades: 无外键约束';
    END IF;

    -- 确保索引存在
    CREATE INDEX IF NOT EXISTS idx_trades_signal ON trades(signal_id);
    RAISE NOTICE '✅ trades: 索引 idx_trades_signal 已就绪';
END $$;


-- =================================================================
-- 完成
-- =================================================================

DO $$
BEGIN
    RAISE NOTICE '========================================';
    RAISE NOTICE '外键约束移除完成！';
    RAISE NOTICE '';
    RAISE NOTICE '变更摘要：';
    RAISE NOTICE '  - strategy_signals.instance_id: 移除外键，保留索引';
    RAISE NOTICE '  - strategy_performance.instance_id: 移除外键+级联删除，保留索引';
    RAISE NOTICE '  - trades.signal_id: 移除外键，保留索引';
    RAISE NOTICE '';
    RAISE NOTICE '注意事项：';
    RAISE NOTICE '  1. 数据完整性现由应用层保证';
    RAISE NOTICE '  2. 删除策略实例时不再自动删除关联数据';
    RAISE NOTICE '  3. 建议使用软删除代替物理删除';
    RAISE NOTICE '========================================';
END $$;


-- =================================================================
-- 验证：检查是否还有外键
-- =================================================================

DO $$
DECLARE
    fk_count INTEGER;
BEGIN
    SELECT COUNT(*) INTO fk_count
    FROM information_schema.table_constraints
    WHERE table_name IN ('strategy_signals', 'strategy_performance', 'trades')
        AND constraint_type = 'FOREIGN KEY';

    IF fk_count = 0 THEN
        RAISE NOTICE '✅ 验证通过：目标表已无外键约束';
    ELSE
        RAISE WARNING '⚠️ 验证失败：仍有 % 个外键约束', fk_count;
    END IF;
END $$;
