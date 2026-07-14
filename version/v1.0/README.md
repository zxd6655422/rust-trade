# v1.0 版本 - 量化交易系统

## 版本目标

构建一个**完整、易用、可验证**的量化交易系统，核心目标：

### 🎯 核心能力

| 能力 | 目标 | 状态 |
|------|------|------|
| **完整交易** | 支持多交易所（Binance、OKX）现货/合约交易 | ✅ |
| **数据采集** | 自动采集 K线数据，支持历史回填 | ✅ |
| **策略分析** | 7种内置策略，支持动态参数 | ✅ |
| **策略回测** | 多时间框架回测、抗过拟合验证 | ✅ |
| **交易统计** | 完整的盈亏分析、胜率统计、资金曲线 | ✅ |
| **便捷部署** | systemd 服务化，一键部署脚本 | ✅ |

### 🛠️ 开发体验

| 特性 | 说明 | 状态 |
|------|------|------|
| **策略易开发** | 统一的 Strategy trait，支持多时间框架 | ✅ |
| **策略易测试** | MockExchange 本地测试，不依赖网络 | ✅ |
| **回测易验证** | 样本外测试、滚动前进测试、过拟合检测 | ✅ |
| **代码质量** | 完整的单元测试覆盖核心逻辑 | ✅ |

### 📊 系统架构

```
┌─────────────────────────────────────────────────────────────────┐
│                      量化交易系统 v1.0                           │
├─────────────────────────────────────────────────────────────────┤
│                                                                  │
│  ┌─────────────────┐    ┌─────────────────┐    ┌─────────────┐ │
│  │  Layer 1         │    │  Layer 2         │    │  Layer 3     │ │
│  │  trading-core    │    │ strategy-service │    │trading-engine│ │
│  │                  │    │                  │    │             │ │
│  │  • K线数据采集   │    │  • 策略分析      │    │  • 交易执行  │ │
│  │  • 指标预计算    │    │  • 信号生成      │    │  • 订单管理  │ │
│  │  • Redis 缓存    │    │  • 指标计算      │    │  • 风控检查  │ │
│  │  • HTTP API      │    │  • WebSocket     │    │  • 止损止盈  │ │
│  └────────┬─────────┘    └────────┬─────────┘    └──────┬──────┘ │
│           │                       │                      │       │
│           └───────────────────────┼──────────────────────┘       │
│                                   │                              │
│                    ┌──────────────▼──────────────┐               │
│                    │     PostgreSQL + Redis       │               │
│                    │  (数据存储 + 缓存)           │               │
│                    └──────────────────────────────┘               │
│                                                                  │
│  ┌─────────────────────────────────────────────────────────────┐│
│  │  监控应用 (frontend + src-tauri)                             ││
│  │  • 行情图表  • 持仓监控  • 交易记录  • 统计分析              ││
│  └─────────────────────────────────────────────────────────────┘│
└─────────────────────────────────────────────────────────────────┘
```

---

## 职责边界

| 层级 | 服务 | 职责 | 不负责 |
|------|------|------|--------|
| Layer 1 | trading-core | 数据采集、指标预计算、Redis缓存 | 交易执行、信号生成 |
| Layer 2 | strategy-service | 策略分析、信号生成、指标动态计算 | 下单、订单同步、账户同步 |
| Layer 3 | trading-engine | 交易执行、订单管理、持仓同步、风控 | 策略分析、信号生成 |

**数据流：**
```
trading-core → Redis → strategy-service → strategy_signals表 → trading-engine
```

---

## 模块详细设计

### Module 1: Data Collector (trading-core)

**职责**：
- REST 轮询采集 K线数据
- 计算技术指标，写入 Redis 缓存
- 历史数据回填 + 缺失补齐
- HTTP API + WebSocket 服务

**文件**：
```
trading-core/src/
├── main.rs           # 入口
├── exchange/         # 交易所接口
├── service/          # 数据采集服务
├── api/              # HTTP API + WebSocket
├── redis_writer.rs   # Redis 写入
└── config.rs
```

---

### Module 2: Strategy Service (strategy-service)

**职责**：
- 从 PostgreSQL 加载策略实例配置
- 从 Redis 读取指标数据（毫秒级）
- 运行策略逻辑，生成交易信号
- 信号写入 `strategy_signals` 表
- WebSocket 推送 + 告警通知

**文件**：
```
strategy-service/src/
├── main.rs           # 入口
├── config.rs         # 配置
├── engine.rs         # 策略执行引擎
├── strategies/       # 7个策略实现
├── indicators.rs     # 指标计算
├── redis_reader.rs   # Redis 数据读取
├── exchange.rs       # 公开 API（实时价格）
├── websocket.rs      # WebSocket 推送
├── alert.rs          # 告警系统
└── api.rs            # HTTP API
```

---

### Module 3: Trading Engine (trading-engine)

**职责**：
- 从 `strategy_signals` 表轮询待执行信号
- 风控检查（持仓数量/金额/杠杆）
- 调用交易所 API 下单（支持多交易所）
- 订单状态同步
- 止损止盈管理
- 持仓同步与对账

**文件**：
```
trading-engine/src/
├── main.rs                    # 入口
├── config.rs                  # 配置
├── engine/
│   ├── signal_poller.rs       # 信号轮询器（主循环）
│   └── trading_unit.rs        # 交易单元
├── exchange/
│   ├── traits.rs              # Exchange trait
│   └── adapters/              # 交易所适配器
├── risk/
│   ├── engine.rs              # 风控引擎
│   └── stop_loss.rs           # 止损止盈
├── order/
│   └── manager.rs             # 订单管理器
└── portfolio/
    ├── manager.rs             # 持仓管理
    └── reconciler.rs          # 持仓对账
```

---

## 数据库表结构

### 核心表

| 表名 | 说明 | 所属服务 |
|------|------|----------|
| `kline_1m` | 1分钟K线数据 | trading-core |
| `trading_pairs` | 交易对配置 | trading-core |
| `symbol_config` | 监控列表 | trading-core |
| `strategy_instances` | 策略实例配置 | strategy-service |
| `strategy_signals` | 策略信号 | strategy-service → trading-engine |
| `strategy_analysis_log` | 分析日志（前端用） | strategy-service |
| `strategy_performance` | 策略性能统计 | strategy-service |
| `trades` | 交易记录 | trading-engine |
| `positions` | 当前持仓 | trading-engine |
| `system_config` | 系统配置 | 共享 |

### 交易引擎相关表

| 表名 | 说明 |
|------|------|
| `exchange_config` | 交易所实例配置（前端管理） |
| `trading_orders` | 交易订单详情 |
| `trading_positions` | 交易持仓详情 |
| `stop_orders` | 止损止盈订单 |
| `account_snapshot` | 账户余额快照 |
| `risk_logs` | 风控日志 |
| `trade_logs` | 交易日志 |

---

## API 端点

### trading-core (端口 8080)

| 端点 | 方法 | 功能 |
|------|------|------|
| `/health` | GET | 健康检查 |
| `/api/data/info` | GET | 数据信息 |
| `/api/strategies` | GET | 策略列表 |
| `/api/backtest` | POST | 单时间框架回测 |
| `/api/backtest/multi-timeframe` | POST | 多时间框架回测 |
| `/api/backtest/walk-forward` | POST | 滚动前进测试 |
| `/api/backtest/out-of-sample` | POST | 样本外测试 |
| `/api/backtest/multi-symbol` | POST | 多交易对回测 |
| `/api/analysis/market-state` | POST | 市场状态分析 |

### strategy-service (端口 8082)

| 端点 | 方法 | 功能 |
|------|------|------|
| `/api/strategies` | GET | 列出所有策略实例 |
| `/api/strategies/{id}` | GET | 获取策略详情 |
| `/api/strategies` | POST | 创建策略实例 |
| `/api/strategies/{id}` | PUT | 更新策略参数 |
| `/api/strategies/{id}` | DELETE | 删除策略 |
| `/api/signals` | GET | 查询信号 |
| `/api/trades` | GET | 查询交易记录 |
| `/ws/signals` | WebSocket | 实时信号推送 |

### trading-engine

| 接口 | 说明 |
|------|------|
| 无 HTTP API | 从数据库轮询执行 |
| `strategy_signals` 表 | 信号输入 |
| `exchange_config` 表 | 交易所配置（前端管理） |

---

## 部署方案

### systemd 服务

```bash
# 启动服务
sudo systemctl start trading-collector
sudo systemctl start trading-engine

# 查看状态
sudo systemctl status trading-collector
sudo systemctl status trading-engine

# 查看日志
journalctl -u trading-collector -f
journalctl -u trading-engine -f
```

### 目录结构

```
~/apps/
├── trading-core/           # 数据采集服务
│   ├── trading-core
│   ├── config/
│   └── logs/
└── trading-engine/         # 交易引擎
    ├── trading-engine
    ├── config/
    └── logs/
```

---

## 验证清单

### 功能验证
- [x] Testnet 下单成功
- [x] Testnet 撤单成功
- [x] 订单状态实时更新
- [x] 止损止盈自动触发
- [x] 风控规则正确执行
- [x] 持仓对账正确
- [x] 策略信号生成正确
- [x] 信号生命周期管理正确

### 性能验证
- [x] 行情延迟 < 100ms
- [x] 下单延迟 < 500ms
- [x] 风控检查 < 10ms
- [x] 内存占用稳定

### 监控 API 验证
- [x] 实时价格查询正常
- [x] K线历史数据正确
- [x] 持仓数据查询正常
- [x] 交易历史分页正常
- [x] 统计指标准确
- [x] 资金曲线数据正确

---

## 快速开始

### 1. 环境准备

```bash
# 编译
cargo build --release

# 配置环境变量
cp config/.env.example config/.env.development
# 编辑填入数据库和 API 配置
```

### 2. 数据库初始化

```bash
# 创建数据库
psql -U postgres -c "CREATE DATABASE trading_core;"

# 初始化表结构
psql -U postgres -d trading_core -f config/schema_v6.sql
```

### 3. 启动服务

```bash
# 启动数据采集服务
cargo run -p trading-core --release -- service

# 启动策略服务（另一个终端）
cargo run -p strategy-service --release

# 启动交易引擎（另一个终端）
cargo run -p trading-engine --release
```

---

## 项目结构

```
rust-trade/
├── trading-common/          # 共享库
│   └── src/
│       ├── backtest/        # 回测引擎
│       ├── data/            # 数据类型和聚合器
│       ├── pricing/         # 期权定价
│       └── simulation/      # 蒙特卡洛模拟
│
├── trading-core/            # Layer 1: 数据采集服务
│   └── src/
│       ├── api/             # HTTP API + WebSocket
│       ├── exchange/        # 交易所接口
│       └── service/         # 数据采集服务
│
├── strategy-service/        # Layer 2: 策略分析服务
│   └── src/
│       ├── strategies/      # 策略实现
│       ├── engine.rs        # 策略执行引擎
│       └── api.rs           # HTTP API
│
├── trading-engine/          # Layer 3: 交易引擎
│   └── src/
│       ├── engine/          # 交易循环
│       ├── exchange/        # 交易所适配器
│       ├── risk/            # 风控系统
│       ├── order/           # 订单管理
│       └── portfolio/       # 持仓管理
│
├── frontend/                # 前端应用
├── src-tauri/               # Tauri 桌面应用
│
└── config/                  # 配置文件
    ├── schema*.sql          # 数据库表结构
    └── *.toml               # 应用配置
```
