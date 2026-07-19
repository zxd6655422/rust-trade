-- 交易监控日志系统 - 表结构迁移
-- 日期: 2026-07-18
-- 说明: trade_logs 和 risk_logs 新增字段，支持全链路日志追踪

-- ============================================================
-- trade_logs 新增字段
-- ============================================================

ALTER TABLE trade_logs ADD COLUMN IF NOT EXISTS signal_id UUID;
ALTER TABLE trade_logs ADD COLUMN IF NOT EXISTS exchange VARCHAR(20);
ALTER TABLE trade_logs ADD COLUMN IF NOT EXISTS market_type VARCHAR(10);
ALTER TABLE trade_logs ADD COLUMN IF NOT EXISTS event_type VARCHAR(30);
ALTER TABLE trade_logs ADD COLUMN IF NOT EXISTS commission DECIMAL(20,8);
ALTER TABLE trade_logs ADD COLUMN IF NOT EXISTS slippage DECIMAL(20,8);
ALTER TABLE trade_logs ADD COLUMN IF NOT EXISTS details JSONB;
ALTER TABLE trade_logs ADD COLUMN IF NOT EXISTS source VARCHAR(20) DEFAULT 'live';
-- source: 'live' = 实盘, 'paper' = 模拟交易, 'backtest' = 回测

CREATE INDEX IF NOT EXISTS idx_trade_logs_signal ON trade_logs(signal_id);
CREATE INDEX IF NOT EXISTS idx_trade_logs_event_type ON trade_logs(event_type);

COMMENT ON COLUMN trade_logs.signal_id IS '关联策略信号ID，贯穿全链路';
COMMENT ON COLUMN trade_logs.exchange IS '交易所: binance / okx';
COMMENT ON COLUMN trade_logs.market_type IS '交易模式: spot / futures';
COMMENT ON COLUMN trade_logs.event_type IS '事件类型: fill / stop_loss / take_profit / risk_close / risk_reduce';
COMMENT ON COLUMN trade_logs.commission IS '手续费';
COMMENT ON COLUMN trade_logs.slippage IS '滑点 (实际价 - 预期价)';
COMMENT ON COLUMN trade_logs.details IS '扩展信息 (风控原因、止损类型等)';

-- ============================================================
-- risk_logs 新增字段
-- ============================================================

ALTER TABLE risk_logs ADD COLUMN IF NOT EXISTS signal_id UUID;
ALTER TABLE risk_logs ADD COLUMN IF NOT EXISTS exchange VARCHAR(20);
ALTER TABLE risk_logs ADD COLUMN IF NOT EXISTS market_type VARCHAR(10);
ALTER TABLE risk_logs ADD COLUMN IF NOT EXISTS check_result VARCHAR(20);
ALTER TABLE risk_logs ADD COLUMN IF NOT EXISTS current_equity DECIMAL(20,8);
ALTER TABLE risk_logs ADD COLUMN IF NOT EXISTS peak_equity DECIMAL(20,8);
ALTER TABLE risk_logs ADD COLUMN IF NOT EXISTS daily_pnl DECIMAL(20,8);
ALTER TABLE risk_logs ADD COLUMN IF NOT EXISTS source VARCHAR(20) DEFAULT 'live';
-- source: 'live' = 实盘, 'paper' = 模拟交易

CREATE INDEX IF NOT EXISTS idx_risk_logs_signal ON risk_logs(signal_id);

COMMENT ON COLUMN risk_logs.signal_id IS '关联策略信号ID';
COMMENT ON COLUMN risk_logs.exchange IS '交易所: binance / okx';
COMMENT ON COLUMN risk_logs.market_type IS '交易模式: spot / futures';
COMMENT ON COLUMN risk_logs.check_result IS '检查结果: allow / reject / modify / action_triggered';
COMMENT ON COLUMN risk_logs.current_equity IS '当前权益';
COMMENT ON COLUMN risk_logs.peak_equity IS '峰值权益';
COMMENT ON COLUMN risk_logs.daily_pnl IS '当日盈亏';
