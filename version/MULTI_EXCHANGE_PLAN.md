# 多交易所支持方案

> 状态：**待开发**
> 创建日期：2026-08-14
> 预估工作量：7-10 天

---

## 一、需求背景

当前 `trading-core` 仅支持 Binance 单一交易所。需要扩展为支持多交易所数据源，实现：

- 某些币种从 Binance 获取
- 某些币种从 Kraken 获取
- 某些币种从 Coinbase 获取
- 可按 symbol 粒度配置数据来源

---

## 二、架构设计

### 核心思路：ExchangeRouter 路由层

创建 `ExchangeRouter` 实现 `Exchange` trait，内部按 symbol 路由到不同交易所。现有所有服务（MarketDataService、BackfillService、MarketSentimentCollector）**零改动**。

```
┌─────────────────────────────────────────────────────────┐
│                    MarketDataService                     │
│                    BackfillService                       │
│                MarketSentimentCollector                  │
└──────────────────────┬──────────────────────────────────┘
                       │ Arc<dyn Exchange>
                       ▼
              ┌─────────────────┐
              │  ExchangeRouter │
              │  (路由 + 代理)   │
              └────────┬────────┘
                       │
          ┌────────────┼────────────┐
          ▼            ▼            ▼
    ┌──────────┐ ┌──────────┐ ┌──────────┐
    │ Binance  │ │  Kraken  │ │ Coinbase │
    │Exchange  │ │ Exchange │ │ Exchange │
    └──────────┘ └──────────┘ └──────────┘
```

### ExchangeRouter 实现

```rust
pub struct ExchangeRouter {
    /// symbol → 具体交易所实例
    routes: HashMap<String, Arc<dyn Exchange>>,
    /// 默认交易所（symbol 未配置时使用）
    default: Arc<dyn Exchange>,
}

impl ExchangeRouter {
    pub fn new(default: Arc<dyn Exchange>) -> Self { ... }
    pub fn add_route(&mut self, symbol: &str, exchange: Arc<dyn Exchange>) { ... }
    pub fn add_routes(&mut self, symbols: &[String], exchange: Arc<dyn Exchange>) { ... }
    fn get_exchange(&self, symbol: &str) -> &Arc<dyn Exchange> { ... }
}

#[async_trait]
impl Exchange for ExchangeRouter {
    async fn fetch_klines(&self, symbol: &str, interval: &str, limit: u32)
        -> Result<Vec<KlineData>, ExchangeError>
    {
        self.get_exchange(symbol).fetch_klines(symbol, interval, limit).await
    }

    async fn subscribe_trades(&self, symbols: &[String], callback: ..., shutdown_rx: ...)
        -> Result<(), ExchangeError>
    {
        // 按交易所分组 symbols，分别建立 WebSocket 连阅
        // 统一回调
    }

    // ... 其他方法类似代理
}
```

---

## 三、各交易所接口差异

### Kraken

| 接口 | 端点 | 说明 |
|------|------|------|
| K线数据 | `GET /public/OHLC` | 最多 720 条，支持 interval: 1/5/15/30/60/240/1440/10080/21600 |
| 订单簿 | `GET /public/Depth` | L2 深度 |
| 最新成交 | `GET /public/Trades` | 最近 1000 笔 |
| WebSocket | `wss://ws.kraken.com` | trade channel |

**符号映射**：`BTCUSDT` → `XXBTZUSD`，`ETHUSDT` → `XETHZUSD`

**不支持**：资金费率、持仓量、多空比（现货无此概念）

### Coinbase

| 接口 | 端点 | 说明 |
|------|------|------|
| K线数据 | `GET /products/{id}/candles` | 最多 300 条 |
| 订单簿 | `GET /products/{id}/book` | L2/L3 深度 |
| 最新成交 | `GET /products/{id}/trades` | 分页 |
| WebSocket | `wss://ws-feed.exchange.coinbase.com` | matches channel |

**符号映射**：`BTCUSDT` → `BTC-USD`，`ETHUSDT` → `ETH-USD`

**不支持**：资金费率、持仓量、多空比

---

## 四、改动清单

### 新增文件

| 文件 | 说明 | 工作量 |
|------|------|--------|
| `trading-core/src/exchange/router.rs` | ExchangeRouter 路由实现 | 1 天 |
| `trading-core/src/exchange/kraken.rs` | Kraken 交易所实现 | 2-3 天 |
| `trading-core/src/exchange/coinbase.rs` | Coinbase 交易所实现 | 2-3 天 |

### 修改文件

| 文件 | 改动 | 工作量 |
|------|------|--------|
| `trading-core/src/exchange/mod.rs` | 注册 router/kraken/coinbase 模块 | 10 分钟 |
| `trading-core/src/config.rs` | 增加 exchange 配置结构体 | 半天 |
| `trading-core/src/main.rs` | 工厂逻辑：根据配置构建 ExchangeRouter | 半天 |
| `config/production.toml` | 增加交易所路由配置 | 10 分钟 |
| `sql/core/symbol_mapping.sql` | 插入 Kraken/Coinbase 符号映射数据 | 1 小时 |

### 不需要改动的文件（得益于 Exchange trait 抽象）

- `trading-core/src/service/market_data.rs` ✅
- `trading-core/src/service/backfill.rs` ✅
- `trading-core/src/service/market_sentiment.rs` ✅（需处理不支持的接口）
- `trading-core/src/exchange/traits.rs` ✅
- `trading-core/src/exchange/types.rs` ✅

---

## 五、配置设计

### production.toml

```toml
# 默认交易所
[exchange]
default = "binance"

# 按 symbol 指定交易所（可选，未配置的使用 default）
[exchange.routes]
"BTCUSDT" = "binance"
"ETHUSDT" = "kraken"
"SOLUSDT" = "coinbase"
"BNBUSDT" = "binance"
"SUIUSDT" = "binance"

# 各交易所配置
[exchanges.binance]
spot_url = "https://api.binance.com"
futures_url = "https://fapi.binance.com"
ws_url = "wss://stream.binance.com:9443/stream"
futures_symbols = ["BTCUSDT", "ETHUSDT", "SOLUSDT", "BNBUSDT", "SUIUSDT"]

[exchanges.kraken]
rest_url = "https://api.kraken.com/0"
ws_url = "wss://ws.kraken.com"

[exchanges.coinbase]
rest_url = "https://api.exchange.coinbase.com"
ws_url = "wss://ws-feed.exchange.coinbase.com"
```

### Settings 结构体扩展

```rust
#[derive(Debug, Deserialize)]
pub struct ExchangeConfig {
    /// 默认交易所
    pub default: String,
    /// symbol → 交易所名称映射
    #[serde(default)]
    pub routes: HashMap<String, String>,
}

#[derive(Debug, Deserialize)]
pub struct ExchangesConfig {
    #[serde(default)]
    pub binance: BinanceConfig,
    #[serde(default)]
    pub kraken: KrakenConfig,
    #[serde(default)]
    pub coinbase: CoinbaseConfig,
}
```

---

## 六、注意事项

### 1. subscribe_trades 的多交易所 WebSocket

`ExchangeRouter::subscribe_trades` 需要：
- 按 symbol 分组到各交易所
- 为每个交易所建立独立的 WebSocket 连接
- 统一通过 callback 回调 TickData
- 任一交易所断连不影响其他交易所

### 2. 市场情绪数据兼容

Kraken/Coinbase 不支持资金费率等接口。`MarketSentimentCollector` 需要：
- 捕获 `ExchangeError::NotSupported` 错误
- 跳过不支持的 symbol，记录 warn 日志
- 或在 Exchange trait 中增加 `supports_sentiment()` 方法

### 3. 符号映射表

数据库 `symbol_mapping` 表已支持多交易所，直接插入数据：

```sql
INSERT INTO symbol_mapping (unified_symbol, exchange, exchange_symbol, market_type) VALUES
    ('BTCUSDT', 'kraken', 'XXBTZUSD', 'spot'),
    ('ETHUSDT', 'kraken', 'XETHZUSD', 'spot'),
    ('SOLUSDT', 'kraken', 'SOLUSD', 'spot'),
    ('BTCUSDT', 'coinbase', 'BTC-USD', 'spot'),
    ('ETHUSDT', 'coinbase', 'ETH-USD', 'spot'),
    ('SOLUSDT', 'coinbase', 'SOL-USD', 'spot');
```

### 4. rate limit 差异

| 交易所 | REST 限流 | WebSocket 限流 |
|--------|-----------|----------------|
| Binance | 1200 req/min | 5 streams/connection |
| Kraken | 15-20 req/min（公开接口） | 无明确限制 |
| Coinbase | 10 req/s | 无明确限制 |

ExchangeRouter 或各交易所实现中需要加入对应的 rate limit 逻辑。

---

## 七、测试计划

1. **单元测试**：ExchangeRouter 路由逻辑
2. **集成测试**：各交易所 fetch_klines 真实调用
3. **多交易所联调**：同时从 Binance + Kraken 拉取数据
4. **断连测试**：单个交易所断连不影响其他交易所
5. **符号映射测试**：验证 unified_symbol → exchange_symbol 转换正确

---

## 八、后续扩展

- 支持更多交易所（OKX、Bybit、Bitget 等）
- 交易所健康检查 + 自动降级（主交易所不可用时切换备用）
- 按延迟/深度智能路由
- 交易所间价格套利监控
