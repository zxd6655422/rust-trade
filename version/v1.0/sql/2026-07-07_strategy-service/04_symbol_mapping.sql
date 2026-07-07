-- =================================================================
-- 交易对映射表
-- 解决不同交易所交易对名称不一致的问题
--
-- 示例：
--   统一名称: BTCUSDT
--   Binance: BTCUSDT (合约)
--   OKX: BTC-USDT-SWAP (合约)
--   Binance Spot: BTCUSDT (现货)
--   OKX Spot: BTC-USDT (现货)
-- =================================================================

CREATE TABLE IF NOT EXISTS symbol_mapping (
    id SERIAL PRIMARY KEY,

    -- 内部统一交易对名称（策略使用）
    unified_symbol VARCHAR(20) NOT NULL,

    -- 交易所名称（binance, okx, etc.）
    exchange VARCHAR(20) NOT NULL,

    -- 该交易所的实际交易对名称
    exchange_symbol VARCHAR(50) NOT NULL,

    -- 市场类型（spot, futures）
    market_type VARCHAR(10) NOT NULL CHECK (market_type IN ('spot', 'futures')),

    -- 交易对状态（active, inactive）
    status VARCHAR(20) NOT NULL DEFAULT 'active' CHECK (status IN ('active', 'inactive')),

    -- 备注
    note TEXT,

    -- 创建时间
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),

    -- 唯一约束：同一交易所、同一市场类型下，交易所交易对名称唯一
    UNIQUE(exchange, exchange_symbol, market_type)
);

-- 索引
CREATE INDEX IF NOT EXISTS idx_symbol_mapping_unified ON symbol_mapping(unified_symbol);
CREATE INDEX IF NOT EXISTS idx_symbol_mapping_exchange ON symbol_mapping(exchange, market_type);

-- =================================================================
-- 初始化数据
-- =================================================================

-- Binance 合约
INSERT INTO symbol_mapping (unified_symbol, exchange, exchange_symbol, market_type, note) VALUES
    ('BTCUSDT', 'binance', 'BTCUSDT', 'futures', 'Binance USDⓈ-M 合约'),
    ('ETHUSDT', 'binance', 'ETHUSDT', 'futures', 'Binance USDⓈ-M 合约'),
    ('SOLUSDT', 'binance', 'SOLUSDT', 'futures', 'Binance USDⓈ-M 合约'),
    ('BNBUSDT', 'binance', 'BNBUSDT', 'futures', 'Binance USDⓈ-M 合约'),
    ('SUIUSDT', 'binance', 'SUIUSDT', 'futures', 'Binance USDⓈ-M 合约')
ON CONFLICT (exchange, exchange_symbol, market_type) DO NOTHING;

-- Binance 现货
INSERT INTO symbol_mapping (unified_symbol, exchange, exchange_symbol, market_type, note) VALUES
    ('BTCUSDT', 'binance', 'BTCUSDT', 'spot', 'Binance 现货'),
    ('ETHUSDT', 'binance', 'ETHUSDT', 'spot', 'Binance 现货'),
    ('SOLUSDT', 'binance', 'SOLUSDT', 'spot', 'Binance 现货'),
    ('BNBUSDT', 'binance', 'BNBUSDT', 'spot', 'Binance 现货'),
    ('SUIUSDT', 'binance', 'SUIUSDT', 'spot', 'Binance 现货')
ON CONFLICT (exchange, exchange_symbol, market_type) DO NOTHING;

-- OKX 合约
INSERT INTO symbol_mapping (unified_symbol, exchange, exchange_symbol, market_type, note) VALUES
    ('BTCUSDT', 'okx', 'BTC-USDT-SWAP', 'futures', 'OKX 永续合约'),
    ('ETHUSDT', 'okx', 'ETH-USDT-SWAP', 'futures', 'OKX 永续合约'),
    ('SOLUSDT', 'okx', 'SOL-USDT-SWAP', 'futures', 'OKX 永续合约'),
    ('BNBUSDT', 'okx', 'BNB-USDT-SWAP', 'futures', 'OKX 永续合约'),
    ('SUIUSDT', 'okx', 'SUI-USDT-SWAP', 'futures', 'OKX 永续合约')
ON CONFLICT (exchange, exchange_symbol, market_type) DO NOTHING;

-- OKX 现货
INSERT INTO symbol_mapping (unified_symbol, exchange, exchange_symbol, market_type, note) VALUES
    ('BTCUSDT', 'okx', 'BTC-USDT', 'spot', 'OKX 现货'),
    ('ETHUSDT', 'okx', 'ETH-USDT', 'spot', 'OKX 现货'),
    ('SOLUSDT', 'okx', 'SOL-USDT', 'spot', 'OKX 现货'),
    ('BNBUSDT', 'okx', 'BNB-USDT', 'spot', 'OKX 现货'),
    ('SUIUSDT', 'okx', 'SUI-USDT', 'spot', 'OKX 现货')
ON CONFLICT (exchange, exchange_symbol, market_type) DO NOTHING;


-- =================================================================
-- 查询函数
-- =================================================================

-- 获取指定交易所的交易对名称
CREATE OR REPLACE FUNCTION get_exchange_symbol(
    p_unified_symbol VARCHAR(20),
    p_exchange VARCHAR(20),
    p_market_type VARCHAR(10)
) RETURNS VARCHAR(50) AS $$
DECLARE
    v_exchange_symbol VARCHAR(50);
BEGIN
    SELECT exchange_symbol INTO v_exchange_symbol
    FROM symbol_mapping
    WHERE unified_symbol = p_unified_symbol
      AND exchange = p_exchange
      AND market_type = p_market_type
      AND status = 'active';

    -- 如果没有映射，返回原始名称
    RETURN COALESCE(v_exchange_symbol, p_unified_symbol);
END;
$$ LANGUAGE plpgsql;

-- 获取所有交易所的交易对映射
CREATE OR REPLACE FUNCTION get_all_exchange_symbols(
    p_unified_symbol VARCHAR(20),
    p_market_type VARCHAR(10)
) RETURNS TABLE(exchange VARCHAR, exchange_symbol VARCHAR) AS $$
BEGIN
    RETURN QUERY
    SELECT sm.exchange, sm.exchange_symbol
    FROM symbol_mapping sm
    WHERE sm.unified_symbol = p_unified_symbol
      AND sm.market_type = p_market_type
      AND sm.status = 'active';
END;
$$ LANGUAGE plpgsql;
