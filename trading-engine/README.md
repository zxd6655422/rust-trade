# Trading Engine - 自动化交易系统

一个用 Rust 编写的高性能、多交易所自动化交易系统。

## 特性

- 🔄 **多交易所支持** - Binance, OKX
- 🛡️ **高级风控** - Kelly 仓位、黑天鹅检测、熔断机制
- 💾 **数据持久化** - PostgreSQL + Redis
- 📊 **实时行情** - WebSocket 订阅
- ⚡ **高性能** - 异步架构，低延迟
- 🔒 **安全可靠** - 模拟盘优先，签名认证

## 快速开始

### 环境要求

- Rust 1.75+
- PostgreSQL 14+
- Redis 7+

### 安装

```bash
# 克隆项目
git clone <repo_url>
cd rust-trade

# 编译
cargo build -p trading-engine
```

### 配置

```bash
# 复制环境变量文件
cp .env.example .env.development

# 编辑环境变量
vim .env.development
```

环境变量配置：
```bash
# 数据库
DATABASE_URL=postgresql://user:pass@host:5432/db
REDIS_URL=redis://:pass@host:6379

# Binance API
BINANCE_API_KEY=your_api_key
BINANCE_API_SECRET=your_api_secret
BINANCE_TESTNET=true

# OKX API (可选)
OKX_API_KEY=your_api_key
OKX_API_SECRET=your_api_secret
OKX_PASSPHRASE=your_passphrase
OKX_SIMULATED=true
```

### 运行

```bash
# 测试数据库连接
cargo run --bin test_db

# 运行交易引擎
cargo run -p trading-engine
```

## 架构

```
┌─────────────────────────────────────────────────────────────┐
│                      Trading Engine                          │
├─────────────────────────────────────────────────────────────┤
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────────────┐ │
│  │  Strategy    │  │    Risk     │  │   Order Manager     │ │
│  │  Engine      │──│  Controller │──│  (下单/撤单/状态)     │ │
│  └─────────────┘  └─────────────┘  └─────────────────────┘ │
│         │                │                      │           │
│         ▼                ▼                      ▼           │
│  ┌─────────────────────────────────────────────────────┐   │
│  │                Exchange Adapter Layer               │   │
│  │  ┌─────────────┐  ┌─────────────┐                  │   │
│  │  │  Binance    │  │    OKX      │                  │   │
│  │  │  Adapter    │  │  Adapter    │                  │   │
│  │  └─────────────┘  └─────────────┘                  │   │
│  └─────────────────────────────────────────────────────┘   │
│         │                │                      │           │
│         ▼                ▼                      ▼           │
│  ┌─────────────┐  ┌─────────────┐  ┌─────────────────────┐ │
│  │  PostgreSQL  │  │   Redis     │  │   Local State       │ │
│  │  (持久化)    │  │  (缓存)     │  │  (内存状态)          │ │
│  └─────────────┘  └─────────────┘  └─────────────────────┘ │
└─────────────────────────────────────────────────────────────┘
```

## 模块说明

### Exchange Adapter
- 统一的交易所接口
- 支持 Binance 和 OKX
- REST API + WebSocket

### Risk Engine
- 单笔限额
- 止损止盈
- 日亏损限制
- 最大回撤保护
- Kelly 仓位管理
- 黑天鹅检测
- 熔断机制

### Order Manager
- 订单执行
- 状态追踪
- 持仓管理
- 紧急停止

### Storage
- PostgreSQL 持久化
- Redis 缓存
- 订单/持仓仓储

## 配置文件

### 交易配置
```toml
# config/engine-development.toml
[exchange]
id = "binance"
testnet = true

[trading]
mode = "testnet"
strategy = "rsi"
symbols = ["BTCUSDT", "ETHUSDT"]
poll_interval_ms = 100
```

### 风控配置
```toml
[risk_control]
max_position_size = 500.0
max_order_size = 0.001
stop_loss_pct = 0.02
take_profit_pct = 0.04
max_daily_loss = 100.0
max_drawdown_pct = 0.10
kelly_fraction = 0.25
black_swan_threshold = 0.05
```

## 开发

### 编译
```bash
cargo build -p trading-engine
```

### 测试
```bash
# 数据库测试
cargo run --bin test_db

# 完整测试
cargo run --bin test_full
```

### 运行
```bash
cargo run -p trading-engine
```

## 文档

详细文档请查看 `version/v1.0/` 目录：

- [README.md](version/v1.0/README.md) - 版本计划
- [ARCHITECTURE.md](version/v1.0/ARCHITECTURE.md) - 架构设计
- [QUICKSTART.md](version/v1.0/QUICKSTART.md) - 快速开始
- [DEVELOPMENT_SUMMARY.md](version/v1.0/DEVELOPMENT_SUMMARY.md) - 开发总结

## 许可证

MIT License
