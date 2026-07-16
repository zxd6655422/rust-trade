-- =================================================================
-- 删除所有表脚本
-- 用于重建数据库前清理
-- 执行方式: psql -U your_user -d your_db -f sql/drop_all_tables.sql
-- 警告：此操作不可逆，请确保已备份重要数据！
-- =================================================================

-- 开始事务
BEGIN;

-- 删除所有表（按依赖关系反序）
DROP TABLE IF EXISTS risk_logs CASCADE;
DROP TABLE IF EXISTS backtest_results CASCADE;
DROP TABLE IF EXISTS position_snapshot CASCADE;
DROP TABLE IF EXISTS asset_balance CASCADE;
DROP TABLE IF EXISTS account_snapshot CASCADE;
DROP TABLE IF EXISTS trade_logs CASCADE;
DROP TABLE IF EXISTS trades CASCADE;
DROP TABLE IF EXISTS stop_orders CASCADE;
DROP TABLE IF EXISTS positions CASCADE;
DROP TABLE IF EXISTS trading_orders CASCADE;
DROP TABLE IF EXISTS live_strategy_log CASCADE;
DROP TABLE IF EXISTS strategy_performance CASCADE;
DROP TABLE IF EXISTS strategy_analysis_log CASCADE;
DROP TABLE IF EXISTS strategy_signals CASCADE;
DROP TABLE IF EXISTS strategy_instances CASCADE;
DROP TABLE IF EXISTS price_cache CASCADE;
DROP TABLE IF EXISTS market_sentiment CASCADE;
DROP TABLE IF EXISTS tick_data CASCADE;
DROP TABLE IF EXISTS kline_multi_timeframe CASCADE;
DROP TABLE IF EXISTS kline_high_timeframe CASCADE;
DROP TABLE IF EXISTS kline_1m CASCADE;
DROP TABLE IF EXISTS symbol_config CASCADE;
DROP TABLE IF EXISTS symbol_mapping CASCADE;
DROP TABLE IF EXISTS trading_pairs CASCADE;
DROP TABLE IF EXISTS exchange_config CASCADE;
DROP TABLE IF EXISTS system_config CASCADE;

-- 删除函数
DROP FUNCTION IF EXISTS aggregate_kline CASCADE;

-- 提交事务
COMMIT;

\echo 'All tables dropped successfully!'
