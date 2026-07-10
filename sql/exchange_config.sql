-- 交易所配置表
-- 前端可动态管理：启用/禁用、设置杠杆
-- API Key 配置在 .env 环境变量中
-- 交易对由策略服务控制，不在此配置

CREATE TABLE IF NOT EXISTS exchange_config (
    id              varchar(50) PRIMARY KEY,        -- 实例唯一标识，如 "binance-futures"
    exchange_id     varchar(20) NOT NULL,           -- 交易所 ID: "binance", "binance-spot", "okx", "okx-spot"
    market_type     varchar(10) NOT NULL,           -- 交易模式: "spot", "futures"
    testnet         bool DEFAULT true NOT NULL,     -- 是否测试网
    enabled         bool DEFAULT true NOT NULL,     -- 是否启用
    leverage        int DEFAULT 10 NOT NULL,        -- 杠杆倍数（仅合约有效）
    description     varchar(200),                   -- 备注说明
    created_at      timestamptz DEFAULT now() NOT NULL,
    updated_at      timestamptz DEFAULT now() NOT NULL
);

COMMENT ON TABLE exchange_config IS '交易所实例配置，前端可动态管理';
COMMENT ON COLUMN exchange_config.id IS '实例唯一标识，如 binance-futures';
COMMENT ON COLUMN exchange_config.exchange_id IS '交易所ID: binance/binance-spot/okx/okx-spot';
COMMENT ON COLUMN exchange_config.market_type IS '交易模式: spot/futures';
COMMENT ON COLUMN exchange_config.testnet IS '是否测试网';
COMMENT ON COLUMN exchange_config.enabled IS '是否启用';
COMMENT ON COLUMN exchange_config.leverage IS '杠杆倍数，仅合约有效';
COMMENT ON COLUMN exchange_config.description IS '备注说明';

-- API Key 通过环境变量配置：
-- BINANCE_FUTURES_API_KEY / BINANCE_FUTURES_API_SECRET
-- BINANCE_SPOT_API_KEY / BINANCE_SPOT_API_SECRET
-- OKX_FUTURES_API_KEY / OKX_FUTURES_API_SECRET / OKX_FUTURES_PASSPHRASE
-- OKX_SPOT_API_KEY / OKX_SPOT_API_SECRET / OKX_SPOT_PASSPHRASE

-- 默认配置
INSERT INTO exchange_config (id, exchange_id, market_type, testnet, enabled, leverage, description) VALUES
    ('binance-futures', 'binance', 'futures', true, true, 10, 'Binance USDⓈ-M 合约'),
    ('binance-spot', 'binance-spot', 'spot', true, true, 1, 'Binance 现货'),
    ('okx-futures', 'okx', 'futures', true, false, 5, 'OKX 合约 (SWAP)'),
    ('okx-spot', 'okx-spot', 'spot', true, false, 1, 'OKX 现货')
ON CONFLICT (id) DO NOTHING;
