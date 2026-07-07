# trading-core

数据采集服务，负责连接交易所、采集K线数据、计算技术指标、写入数据库和缓存。

## 功能特性

- 🔄 REST 轮询采集 K线数据
- 📊 多时间框架 K线聚合（1m → 5m/15m/30m/1h/2h/4h/1d/3d/1w）
- 💾 PostgreSQL 存储
- 🚀 Redis 缓存（20000根/时间框架）
- 📈 技术指标计算
- 🔌 HTTP API 服务
- 📡 WebSocket 实时推送

## 模块结构

```
src/
├── main.rs                # 入口文件
├── config.rs              # 配置加载
├── exchange/              # 交易所适配器
│   ├── traits.rs          # 交易所 trait
│   ├── binance.rs         # Binance 适配器
│   └── okx.rs             # OKX 适配器
├── api/                   # HTTP API
│   ├── server.rs          # Web 服务器
│   ├── handlers.rs        # 请求处理
│   └── websocket.rs       # WebSocket 处理
├── service/               # 服务模块
│   ├── market_data.rs     # 市场数据服务
│   ├── backfill.rs        # 历史数据回填
│   └── strategy_scheduler.rs # 策略调度器
├── redis_writer.rs        # Redis 写入
└── lib.rs
```

## 使用方法

### 启动服务

```bash
# 完整服务（数据采集 + API + WebSocket）
cargo run -p trading-core service

# 仅数据采集
cargo run -p trading-core collector

# 回测模式
cargo run -p trading-core backtest
```

### 配置文件

```toml
# config/development.toml
[database]
url = "postgresql://localhost/trading_core"
max_connections = 10

[redis]
url = "redis://localhost:6379"

[collector]
mode = "candle1m"
poll_interval_secs = 30
backfill_enabled = true
backfill_start_date = "2024-01-01"

[symbols]
spot = ["BTCUSDT", "ETHUSDT", "SOLUSDT"]
futures = ["BTCUSDT", "ETHUSDT"]
```

### API 端点

| 端点 | 方法 | 功能 |
|------|------|------|
| `/health` | GET | 健康检查 |
| `/api/data/info` | GET | 数据统计信息 |
| `/api/strategies` | GET | 可用策略列表 |
| `/api/backtest` | POST | 执行回测 |
| `/api/backtest/multi-timeframe` | POST | 多时间框架回测 |
| `/api/backtest/walk-forward` | POST | 滚动前进测试 |
| `/api/backtest/out-of-sample` | POST | 样本外测试 |
| `/api/backtest/multi-symbol` | POST | 多交易对回测 |
| `/api/analysis/market-state` | POST | 市场状态分析 |
| `/ws` | WebSocket | 实时数据推送 |

### Redis 缓存结构

```
# K线数据（ZSET）
kline:{symbol}:{timeframe} → ZSET(score=timestamp, member=kline_json)

# 缓存数量
每个 timeframe: 20000 根

# TTL
1m: 10分钟
其他: 1小时
1d/3d/1w: 1天
```

## 依赖

```toml
[dependencies]
trading-common = { path = "../trading-common" }
tokio = { version = "1", features = ["full"] }
actix-web = "4"
sqlx = "0.7"
redis = "0.23"
reqwest = "0.11"
serde = "1.0"
tracing = "0.1"
```
