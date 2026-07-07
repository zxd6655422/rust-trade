# trading-engine

交易执行服务，负责调用交易所 API 执行交易、订单管理、持仓管理、风险控制。

## 功能特性

- 🔄 订单执行（市价单、限价单、止损单、止盈单）
- 📊 持仓管理（实时同步、盈亏计算）
- 🛡️ 风险控制（止损止盈、仓位限制）
- 🔌 多交易所支持（Binance、OKX）
- 📈 订单状态同步
- 🔐 API 签名认证

## 模块结构

```
src/
├── main.rs                # 入口文件
├── config.rs              # 配置加载
├── exchange/              # 交易所适配器
│   ├── traits.rs          # 交易所 trait
│   ├── types.rs           # 类型定义
│   ├── errors.rs          # 错误类型
│   ├── factory.rs         # 交易所工厂
│   └── adapters/          # 交易所实现
│       ├── binance_adapter.rs     # Binance 合约
│       ├── binance_spot_adapter.rs # Binance 现货
│       ├── okx_adapter.rs         # OKX
│       └── mock_exchange.rs       # Mock（测试用）
├── engine/                # 交易引擎
│   └── trading_loop.rs    # 交易循环
├── order/                 # 订单管理
│   ├── manager.rs         # 订单管理器
│   └── types.rs           # 订单类型
├── portfolio/             # 投资组合
│   ├── manager.rs         # 持仓管理器
│   └── reconciler.rs      # 对账器
├── risk/                  # 风险控制
│   ├── engine.rs          # 风控引擎
│   └── stop_loss.rs       # 止损止盈
├── storage/               # 存储层
│   ├── database.rs        # 数据库连接
│   ├── order_repo.rs      # 订单仓储
│   ├── position_repo.rs   # 持仓仓储
│   └── redis_cache.rs     # Redis 缓存
└── lib.rs
```

## 使用方法

### 启动服务

```bash
# 实盘模式
cargo run -p trading-engine

# 测试网模式
BINANCE_TESTNET=true cargo run -p trading-engine
```

### 环境变量

```bash
# Binance
BINANCE_API_KEY=your_api_key
BINANCE_API_SECRET=your_api_secret
BINANCE_TESTNET=false

# OKX
OKX_API_KEY=your_api_key
OKX_API_SECRET=your_api_secret
OKX_PASSPHRASE=your_passphrase

# 数据库
DATABASE_URL=postgresql://localhost/trading_core
REDIS_URL=redis://localhost:6379
```

### 配置文件

```toml
# config/production.toml
[exchange]
id = "binance"
testnet = false

[trading]
strategy = "trend"
symbol = "BTCUSDT"

[risk_control]
max_position_pct = 20.0
stop_loss_pct = 5.0
take_profit_pct = 10.0
max_daily_trades = 50
```

## 交易所接口

### MarketDataProvider（公开数据）

```rust
async fn get_ticker(&self, symbol: &str) -> Result<Ticker>;
async fn get_klines(&self, symbol: &str, interval: &str, limit: u32) -> Result<Vec<Kline>>;
async fn get_order_book(&self, symbol: &str, limit: u32) -> Result<OrderBook>;
async fn get_symbol_precision(&self, symbol: &str) -> Result<SymbolPrecision>;
```

### TradingOperations（交易操作）

```rust
async fn get_account(&self) -> Result<AccountInfo>;
async fn get_futures_account(&self) -> Result<FuturesAccountInfo>;
async fn get_positions(&self) -> Result<Vec<PositionInfo>>;
async fn place_order(&self, order: OrderRequest) -> Result<OrderResult>;
async fn cancel_order(&self, symbol: &str, order_id: &str) -> Result<()>;
async fn get_open_orders(&self, symbol: Option<&str>) -> Result<Vec<OrderInfo>>;
```

## 风险控制

| 规则 | 说明 | 默认值 |
|------|------|--------|
| 最大持仓比例 | 单个持仓占总资金比例 | 20% |
| 止损比例 | 最大亏损比例 | 5% |
| 止盈比例 | 目标盈利比例 | 10% |
| 最大每日交易次数 | 防止过度交易 | 50 |

## 依赖

```toml
[dependencies]
trading-common = { path = "../trading-common" }
tokio = { version = "1", features = ["full"] }
reqwest = "0.11"
sqlx = "0.7"
redis = "0.23"
hmac = "0.12"
sha2 = "0.10"
serde = "1.0"
tracing = "0.1"
```
