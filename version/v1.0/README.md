# v1.0 版本计划 - 真实交易系统

## 版本目标

实现完整的真实交易系统，包括：
- 多交易所支持（Binance、OKX）
- 高级风控系统
- 自动化交易
- Testnet 测试环境

---

## 架构设计 - 服务分离方案

### 设计理念

将**数据采集服务**与**交易引擎服务**分离，原因：

1. **风险隔离**：交易服务崩溃不影响数据采集
2. **安全隔离**：API Key 只存在于交易服务
3. **独立部署**：可独立更新、重启、扩展
4. **故障隔离**：数据采集是只读操作，交易涉及资金安全

### 整体架构

```
┌─────────────────────────────────────────────────────────────────┐
│                        云服务器 (systemd)                         │
├─────────────────────────────────────────────────────────────────┤
│                                                                  │
│  ┌───────────────────────────────────────────────────────────┐  │
│  │  Service 1: Data Collector (trading-core)                 │  │
│  │  ──────────────────────────────────────────────────────── │  │
│  │  • WebSocket 行情订阅 (Binance Public)                    │  │
│  │  • 数据写入 PostgreSQL (tick_data 表)                      │  │
│  │  • Redis 缓存行情数据                                      │  │
│  │  • 无需 API Key，只读操作                                  │  │
│  │  • systemd: trading-collector.service                     │  │
│  └───────────────────────────────────────────────────────────┘  │
│                              │                                   │
│                              │ (PostgreSQL + Redis)               │
│                              ▼                                   │
│  ┌───────────────────────────────────────────────────────────┐  │
│  │  Service 2: Trading Engine (trading-engine) [新增]        │  │
│  │  ──────────────────────────────────────────────────────── │  │
│  │  • 读取实时行情 (从 Redis 缓存)                            │  │
│  │  • 策略计算信号                                            │  │
│  │  • 风控检查 (Kelly 仓位、黑天鹅保护等)                      │  │
│  │  • 真实下单 (Binance/OKX REST API)                        │  │
│  │  • 订单状态追踪 (WebSocket 用户数据流)                     │  │
│  │  • 止损止盈管理                                            │  │
│  │  • 需要 API Key，涉及资金操作                              │  │
│  │  • systemd: trading-engine.service                        │  │
│  └───────────────────────────────────────────────────────────┘  │
│                              │                                   │
│                              │ (PostgreSQL)                      │
│                              ▼                                   │
│  ┌───────────────────────────────────────────────────────────┐  │
│  │  Service 3: PostgreSQL + Redis                            │  │
│  │  ──────────────────────────────────────────────────────── │  │
│  │  • tick_data (行情数据)                                    │  │
│  │  • orders (订单记录)                                       │  │
│  │  • positions (持仓记录)                                    │  │
│  │  • risk_logs (风控日志)                                    │  │
│  │  • live_strategy_log (策略日志)                            │  │
│  └───────────────────────────────────────────────────────────┘  │
│                                                                  │
│  ┌───────────────────────────────────────────────────────────┐  │
│  │  Service 4: Monitor Dashboard (可选)                      │  │
│  │  ──────────────────────────────────────────────────────── │  │
│  │  • src-tauri 桌面应用 (本地电脑)                           │  │
│  │  • 或 Web 仪表盘 (部署在服务器)                            │  │
│  └───────────────────────────────────────────────────────────┘  │
│                                                                  │
└─────────────────────────────────────────────────────────────────┘
```

### 服务间通信

```
Data Collector ──写入──▶ PostgreSQL (tick_data)
                              │
Trading Engine ◀──读取──┘
      │
      ├──写入──▶ PostgreSQL (orders, positions, risk_logs)
      │
      └──写入──▶ Redis (实时持仓状态)
```

**通信方式**：通过数据库（PostgreSQL）和缓存（Redis）解耦，不直接调用。

---

## 模块详细设计

### Module 1: Data Collector (现有 trading-core)

**职责**：
- 订阅 Binance 公共 WebSocket 行情流
- 将 tick 数据批量写入 PostgreSQL
- 维护 Redis 缓存

**状态**：已完成，无需大改

**文件**：
```
trading-core/src/
├── main.rs           # 保持现有
├── exchange/         # Binance WebSocket (只读)
├── service/          # MarketDataService
└── config.rs
```

**部署**：
```bash
# systemd service
[Service]
ExecStart=/opt/trading/trading-core live
Restart=always
```

---

### Module 2: Trading Engine (新增 trading-engine)

**职责**：
- 从 Redis 读取实时行情
- 运行策略计算交易信号
- 执行风控检查
- 调用交易所 API 下单
- 管理订单状态
- 执行止损止盈

**新增文件**：
```
trading-engine/src/
├── main.rs                    # 入口
├── config.rs                  # 配置（含 API Key）
├── engine/
│   ├── mod.rs
│   ├── strategy_engine.rs     # 策略引擎
│   └── trading_loop.rs        # 主交易循环
├── exchange/
│   ├── mod.rs
│   ├── traits.rs              # 扩展的 Exchange trait
│   ├── adapters/
│   │   ├── mod.rs
│   │   ├── binance_adapter.rs # Binance REST + WebSocket
│   │   └── okx_adapter.rs     # OKX REST + WebSocket
│   ├── order_types.rs         # 订单类型定义
│   └── factory.rs             # 交易所工厂
├── risk/
│   ├── mod.rs
│   ├── config.rs              # 风控配置
│   ├── engine.rs              # 风控引擎
│   ├── kelly.rs               # Kelly 仓位计算
│   ├── volatility.rs          # 波动率计算
│   ├── stop_loss.rs           # 止损止盈管理
│   └── circuit_breaker.rs     # 熔断机制
├── order/
│   ├── mod.rs
│   ├── manager.rs             # 订单管理器
│   ├── executor.rs            # 订单执行器
│   └── tracker.rs             # 订单状态追踪
├── portfolio/
│   ├── mod.rs
│   ├── manager.rs             # 持仓管理
│   └── reconciler.rs          # 持仓对账
└── utils/
    ├── mod.rs
    ├── logger.rs              # 日志工具
    └── metrics.rs             # 指标收集
```

---

### Module 3: Shared Library (现有 trading-common)

**职责**：
- 共享数据类型
- 共享策略逻辑
- 共享工具函数

**扩展内容**：
```
trading-common/src/
├── data/
│   ├── types.rs          # 添加 Order, Position 类型
│   ├── repository.rs     # 添加订单、持仓查询方法
│   └── cache.rs          # 保持现有
├── backtest/
│   └── strategy/         # 策略逻辑 (两服务共用)
└── exchange/             # 新增：通用交易所类型
    ├── types.rs          # OrderRequest, OrderResult, AccountInfo
    └── traits.rs         # Exchange trait 定义
```

---

## 配置文件设计

### Data Collector 配置
**文件**: `config/collector-production.toml`

```toml
[database]
url = ""  # 从环境变量 DATABASE_URL
max_connections = 5
min_connections = 2
max_lifetime = 1800

[cache.redis]
url = ""  # 从环境变量 REDIS_URL
ttl_seconds = 300
max_ticks_per_symbol = 1000

[symbols]
list = ["BTCUSDT", "ETHUSDT"]
```

### Trading Engine 配置
**文件**: `config/engine-production.toml`

```toml
[exchange]
id = "binance"
testnet = false

[exchange.api]
# 从环境变量加载，不写入配置文件
# BINANCE_API_KEY
# BINANCE_API_SECRET

[trading]
mode = "live"                    # testnet / live
strategy = "rsi"
symbols = ["BTCUSDT", "ETHUSDT"]
poll_interval_ms = 100           # 行情轮询间隔

[risk_control]
# 基础风控
max_position_size = 1000.0
max_order_size = 0.01
stop_loss_pct = 0.02
take_profit_pct = 0.04

# 中级风控
max_daily_loss = 200.0
max_drawdown_pct = 0.15
max_exposure_pct = 0.8

# 高级风控
kelly_fraction = 0.25
volatility_lookback = 20
volatility_target = 0.15
black_swan_threshold = 0.05
circuit_breaker_cooldown = 3600
```

### 环境变量
**文件**: `.env`

```bash
# 数据库
DATABASE_URL=postgresql://mydb:password@localhost:5432/trading_core
REDIS_URL=redis://:password@localhost:6379

# Binance API
BINANCE_API_KEY=your_api_key_here
BINANCE_API_SECRET=your_api_secret_here
BINANCE_TESTNET=true

# OKX API (可选)
OKX_API_KEY=your_api_key_here
OKX_API_SECRET=your_api_secret_here
OKX_PASSPHRASE=your_passphrase_here
OKX_SIMULATED=true
```

---

## 开发计划

### Phase 1: 基础框架搭建 (第 1 周)

**任务**：
- [ ] 创建 trading-engine crate
- [ ] 设计并实现 Exchange trait (扩展版)
- [ ] 实现 Binance Adapter (REST API)
- [ ] 实现 API Key 管理
- [ ] 基础配置系统

**产出**：
- trading-engine 可编译
- 能连接 Binance Testnet
- 能查询账户余额

---

### Phase 2: 订单管理系统 (第 2 周)

**任务**：
- [ ] 实现 OrderManager
- [ ] 实现订单状态机
- [ ] Binance 下单/撤单 API
- [ ] OKX Adapter 实现
- [ ] WebSocket 用户数据流（订单状态更新）

**产出**：
- 能在 Testnet 下单
- 能追踪订单状态
- 支持 Binance + OKX

---

### Phase 3: 风控系统 (第 3 周)

**任务**：
- [ ] 实现 RiskEngine 基础框架
- [ ] 基础风控规则（单笔限额、止损止盈）
- [ ] 中级风控（日亏损、最大回撤）
- [ ] 高级风控（Kelly 公式、波动率自适应）
- [ ] 黑天鹅检测 + 熔断机制

**产出**：
- 完整风控系统
- 风控日志记录

---

### Phase 4: 策略集成 + 实盘对接 (第 4 周)

**任务**：
- [ ] 策略引擎与交易引擎集成
- [ ] 从 Redis 读取实时行情
- [ ] 信号 → 风控 → 下单 完整流程
- [ ] 止损止盈自动执行
- [ ] 持仓管理 + 对账

**产出**：
- 完整自动交易流程
- Testnet 24 小时稳定运行

---

### Phase 5: 部署 + 监控 (第 5 周)

**任务**：
- [ ] systemd 服务配置
- [ ] 日志系统完善
- [ ] 告警机制（交易失败、风控触发）
- [ ] 生产环境部署
- [ ] 文档编写

**产出**：
- 生产环境可用
- 完整运维文档

---

## 部署方案

### 目录结构
```
/opt/trading/
├── bin/
│   ├── trading-collector    # 数据采集服务
│   └── trading-engine       # 交易引擎服务
├── config/
│   ├── collector-production.toml
│   └── engine-production.toml
├── logs/
│   ├── collector.log
│   └── engine.log
└── .env                      # API Key (权限 600)
```

### systemd 服务

**trading-collector.service**
```ini
[Unit]
Description=Trading Data Collector
After=network.target postgresql.service redis.service

[Service]
Type=simple
User=trading
WorkingDirectory=/opt/trading
ExecStart=/opt/trading/bin/trading-collector live
Restart=always
RestartSec=10

[Install]
WantedBy=multi-user.target
```

**trading-engine.service**
```ini
[Unit]
Description=Trading Engine
After=network.target trading-collector.service

[Service]
Type=simple
User=trading
WorkingDirectory=/opt/trading
ExecStart=/opt/trading/bin/trading-engine live
Restart=always
RestartSec=10
EnvironmentFile=/opt/trading/.env

[Install]
WantedBy=multi-user.target
```

---

## 风险控制

### 资金安全
1. API Key 只在交易服务中使用
2. API Key 权限限制：只允许交易，不允许提币
3. IP 白名单限制
4. 所有交易记录到数据库

### 故障恢复
1. 服务崩溃自动重启 (systemd)
2. 订单状态与交易所对账
3. 持仓状态定期同步
4. 异常交易自动停止

### 监控告警
1. 交易失败告警
2. 风控触发告警
3. 服务异常告警
4. 每日交易汇总报告

---

## 验证清单

### 功能验证
- [ ] Testnet 下单成功
- [ ] Testnet 撤单成功
- [ ] 订单状态实时更新
- [ ] 止损止盈自动触发
- [ ] 风控规则正确执行
- [ ] Kelly 仓位计算正确
- [ ] 黑天鹅检测触发熔断
- [ ] 持仓对账正确

### 性能验证
- [ ] 行情延迟 < 100ms
- [ ] 下单延迟 < 500ms
- [ ] 风控检查 < 10ms
- [ ] 内存占用稳定

### 稳定性验证
- [ ] Testnet 运行 7 天无崩溃
- [ ] 网络断线自动重连
- [ ] 交易所 API 限流处理
- [ ] 异常数据容错处理
