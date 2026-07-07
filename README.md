# Rust Trade - 高性能量化交易系统

[English](README.md) | [中文](README_CN.md)

一个用 Rust 构建的高性能量化交易系统，支持多交易所、多策略、自动化交易。

## ✨ 核心特性

- 🚀 **高性能** - Rust 异步运行时，毫秒级响应
- 📊 **多策略** - 内置 7 种策略（RSI/MACD/布林带/趋势/多时间框架/大周期/成交量）
- 🔄 **自动化** - 信号触发自动下单，支持止损止盈
- 📈 **大周期分析** - 支持周K/3日K 分析，识别历史支撑阻力位
- 🌐 **多交易所** - 支持 Binance、OKX
- 💹 **多市场** - 支持现货、合约交易
- 📱 **桌面应用** - Tauri 桌面端，实时监控
- 🌍 **国际化** - 支持中英文切换

## 🏗️ 系统架构

```
┌─────────────────────────────────────────────────────────────────┐
│  Tauri 桌面应用 (src-tauri)                                       │
│  ├── 实时行情展示                                                  │
│  ├── 持仓/交易监控                                                │
│  ├── 策略管理                                                    │
│  └── 回测分析                                                    │
└─────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│  trading-core (数据采集 + 指标计算)                                │
│  ├── 连接交易所（Binance/OKX）                                    │
│  ├── REST 轮询采集 K线数据                                       │
│  ├── 计算技术指标                                                │
│  ├── 写入 PostgreSQL                                             │
│  └── 写入 Redis 缓存（20000根/时间框架）                          │
└─────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│  strategy-service (策略分析 + 信号生成)                            │
│  ├── 从 PostgreSQL 加载策略实例配置                               │
│  ├── 从 Redis 读取指标数据                                       │
│  ├── 运行策略逻辑产生信号                                        │
│  ├── 信号写入 PostgreSQL                                         │
│  ├── WebSocket 实时推送信号                                      │
│  └── 自动交易执行                                                │
└─────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│  trading-engine (交易执行)                                       │
│  ├── 调用交易所 API 下单                                        │
│  ├── 订单状态同步                                                │
│  ├── 持仓管理                                                    │
│  └── 风险控制                                                    │
└─────────────────────────────────────────────────────────────────┘
```

## 📦 项目结构

```
rust-trade/
├── trading-common/          # 共享库（数据类型、指标计算、回测引擎）
├── trading-core/            # 数据采集服务
├── trading-engine/          # 交易执行服务
├── strategy-service/        # 策略分析服务
├── src-tauri/               # Tauri 桌面应用
├── frontend/                # Next.js 前端
├── config/                  # 配置文件
├── sql/                     # 数据库表结构
├── deploy/                  # 部署脚本
└── version/                 # 版本文档和SQL脚本
```

## 🚀 快速开始

### 环境要求

- Rust 1.70+
- Node.js 18+
- PostgreSQL 14+
- Redis 6+

### 安装

```bash
# 克隆项目
git clone https://github.com/yourusername/rust-trade.git
cd rust-trade

# 安装依赖
cargo build
cd frontend && npm install

# 配置环境变量
cp .env.example .env
# 编辑 .env 文件，填入数据库和交易所配置

# 初始化数据库
psql -U postgres -d trading_core -f sql/schema_latest.sql

# 启动服务
cargo run -p trading-core service          # 数据采集
cargo run -p strategy-service              # 策略分析
cargo run -p trading-engine                # 交易执行

# 启动前端
cd frontend && npm run dev
```

### 环境变量

```bash
# 数据库
DATABASE_URL=postgresql://user:password@localhost/trading_core
REDIS_URL=redis://localhost:6379

# Binance
BINANCE_API_KEY=your_api_key
BINANCE_API_SECRET=your_api_secret
BINANCE_TESTNET=false

# OKX（可选）
OKX_API_KEY=your_api_key
OKX_API_SECRET=your_api_secret
OKX_PASSPHRASE=your_passphrase
```

## 📊 内置策略

| 策略 | 说明 | 参数 |
|------|------|------|
| RSI | 相对强弱指数 | period, overbought, oversold |
| MACD | 指数平滑异同移动平均线 | fast, slow, signal |
| 布林带 | 波动率通道 | period, std_dev |
| 趋势 | 多均线趋势跟踪 | fast_ma, slow_ma, trend_ma |
| 多时间框架 | 多周期共振 | timeframes, min_agreement |
| 大周期 | 历史支撑阻力分析 | lookback_periods, proximity_threshold |
| 成交量 | 量价关系分析 | volume_ma_period, spike_threshold |

## 🔧 API 端点

### trading-core (端口 8080)

| 端点 | 方法 | 功能 |
|------|------|------|
| `/health` | GET | 健康检查 |
| `/api/data/info` | GET | 数据信息 |
| `/api/strategies` | GET | 策略列表 |
| `/api/backtest` | POST | 回测执行 |

### strategy-service (端口 8082)

| 端点 | 方法 | 功能 |
|------|------|------|
| `/api/strategies` | GET/POST | 策略管理 |
| `/api/signals` | GET | 信号查询 |
| `/api/trades` | GET | 交易记录 |
| `/ws/signals` | WebSocket | 实时信号推送 |

## 📈 Redis 缓存

```
kline:{symbol}:1m    → 20000根 K线（约14天）
kline:{symbol}:5m    → 20000根 K线（约69天）
kline:{symbol}:1h    → 20000根 K线（约2.3年）
kline:{symbol}:4h    → 20000根 K线（约9年）
kline:{symbol}:1d    → 20000根 K线（约54年）
kline:{symbol}:1w    → 20000根 K线（约384年）
```

## 📚 文档

- [开发计划](version/v1.0/PLAN.md)
- [更新日志](version/CHANGELOG.md)
- [部署指南](deploy/README.md)
- [API 文档](api-docs/)

## 📄 License

MIT License
