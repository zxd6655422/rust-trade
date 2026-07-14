# 量化交易系统开发计划

## 架构概述

```
┌─────────────────────────────────────────────────────────────────┐
│  Layer 1: trading-core (数据采集 + 指标计算 + Redis 写入)        │
│  ├── 连接交易所（Binance/OKX）                                    │
│  ├── REST 轮询采集 K线数据                                       │
│  ├── 计算技术指标 (MA/RSI/MACD)                                  │
│  ├── 写入 PostgreSQL (kline_1m)                                 │
│  └── 写入 Redis 缓存 (kline + 指标)                              │
└─────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│  Redis 缓存                                                      │
│  ├── kline:{symbol}:1m → 最新100根K线                            │
│  ├── indicator:{symbol}:ma → {ma7, ma25, ma99}                  │
│  ├── indicator:{symbol}:rsi → 62.5                              │
│  └── indicator:{symbol}:macd → {macd, signal, hist}             │
└─────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│  Layer 2: strategy-service (策略分析 + 信号生成)                  │
│  ├── 从 PostgreSQL 加载策略实例配置                               │
│  ├── 从 Redis 读取指标数据（毫秒级）                              │
│  ├── 运行策略逻辑（RSI/MACD/布林/趋势/多时间框架）                │
│  ├── 信号写入 PostgreSQL（strategy_signals 表）                   │
│  └── WebSocket 推送 + 告警通知                                   │
└─────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│  Layer 3: trading-engine (交易执行)                               │
│  ├── 从 strategy_signals 表轮询待执行信号                        │
│  ├── 风控检查（持仓数量/金额/杠杆）                               │
│  ├── 调用交易所 API 下单（支持多交易所）                          │
│  ├── 订单状态同步                                                │
│  ├── 止损止盈管理                                                │
│  └── 持仓同步与对账                                              │
└─────────────────────────────────────────────────────────────────┘
```

---

## 职责边界（重要）

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

## 服务列表

| 服务 | 端口 | 说明 |
|------|------|------|
| trading-core | 8080 | 数据采集 + 指标计算 + HTTP API + WebSocket |
| strategy-service | 8082 | 策略分析 + 信号生成 |
| trading-engine | - | 交易执行（无 HTTP API，从数据库轮询） |
| PostgreSQL | 5432 | 数据存储 |
| Redis | 6379 | 指标缓存 |

---

## 已完成功能

### ✅ Layer 1: 数据采集层 (trading-core)

| 模块 | 状态 | 说明 |
|------|------|------|
| REST 轮询采集 | ✅ | 每 10 秒拉取 Binance/OKX K线 |
| K线聚合器 | ✅ | 1m → 5m/15m/30m/1h/4h/1d |
| 历史数据回填 | ✅ | 服务启动自动拉取历史数据 + 缺失 gap 检测补齐 |
| Redis 写入器 | ✅ | 写入 K线和指标到 Redis 缓存 |
| HTTP API | ✅ | 健康检查、数据查询、回测接口 |
| WebSocket | ✅ | 实时价格推送 |

### ✅ Layer 2: 策略分析层 (strategy-service)

| 模块 | 状态 | 说明 |
|------|------|------|
| 策略实例管理 | ✅ | PostgreSQL CRUD，支持多策略实例 |
| Redis 读取器 | ✅ | 从 Redis 读取 K线和指标数据 |
| 7 个策略实现 | ✅ | RSI/MACD/布林带/成交量/趋势/多时间框架/大周期 |
| 策略执行引擎 | ✅ | 定时轮询 PostgreSQL + Redis |
| 信号生成 | ✅ | 信号写入 PostgreSQL（含完整上下文） |
| HTTP API | ✅ | 策略管理/信号查询/交易记录/统计 |
| 告警系统 | ✅ | 信号触发告警通知 |

### ✅ Layer 3: 交易执行层 (trading-engine)

| 模块 | 状态 | 说明 |
|------|------|------|
| 信号轮询器 | ✅ | 从 strategy_signals 表轮询信号执行 |
| 多交易所支持 | ✅ | Binance (现货+合约) / OKX / Bybit |
| TradingUnit | ✅ | 独立的交易所+模式交易实例 |
| 订单管理器 | ✅ | 下单、撤单、订单状态查询 |
| 止损止盈 | ✅ | StopLossManager，支持追踪止损 |
| 风控引擎 | ✅ | RiskEngine，持仓数量/金额/杠杆检查 |
| 持仓同步 | ✅ | 每5分钟同步交易所持仓 |
| 持仓对账 | ✅ | PositionReconciler，对比系统与交易所持仓 |
| 多交易所模式 | ✅ | 支持同时运行多个交易所实例 |

### ✅ 回测引擎

| 模块 | 状态 | 说明 |
|------|------|------|
| 多时间框架回测 | ✅ | MultiTimeframeBacktestEngine，逐 bar 模拟 |
| 样本外测试 | ✅ | 70/30 单次划分，过拟合检测 |
| 滚动前进测试 | ✅ | WalkForwardEngine，滚动窗口训练+测试 |
| 多交易对回测 | ✅ | MultiSymbolBacktestEngine，批量 symbol 回测 |
| 市场状态分析 | ✅ | MarketStateAnalyzer，ATR/ADX 分析 |

### ✅ 监控桌面应用 (Tauri)

| 模块 | 状态 | 说明 |
|------|------|------|
| Dashboard | ✅ | 系统概览、策略矩阵、快速回测 |
| Trading Center | ✅ | 实时交易、回测、模拟、高级回测 |
| 实时行情 | ✅ | PriceTicker + KlineChart |
| 持仓/交易 | ✅ | PositionTable + TradeHistory |
| 统计分析 | ✅ | 性能指标、手续费、策略胜率 |
| 自动交易状态 | ✅ | AutoTradingStatus 组件 |
| i18n | ✅ | 中英文切换 |

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

### Schema 文件

| 文件 | 说明 |
|------|------|
| `config/schema_v1.sql` | 原始表 (tick_data) |
| `config/schema_v2.sql` | kline_1m, backtest_results 等 |
| `config/schema_v3.sql` | 策略信号表（分离） |
| `config/schema_v4.sql` | 交易对配置表 |
| `config/schema_v5.sql` | 交易所/市场类型字段扩展 |
| `config/schema_v6.sql` | 策略服务相关表 |

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
| `/api/strategies/{id}/status` | PUT | 启用/暂停策略 |
| `/api/strategies/{id}` | DELETE | 删除策略 |
| `/api/signals` | GET | 查询信号（按策略/交易对/时间） |
| `/api/trades` | GET | 查询交易记录 |
| `/api/strategies/{id}/performance` | GET | 策略收益统计 |
| `/ws/signals` | WebSocket | 实时信号推送 |

### trading-engine

| 接口 | 说明 |
|------|------|
| 无 HTTP API | 从数据库轮询执行 |
| `strategy_signals` 表 | 信号输入 |
| `exchange_config` 表 | 交易所配置（前端管理） |

---

## Redis 缓存结构

```
kline:{symbol}:1m                    → 最新100根K线 JSON 数组
indicator:{symbol}:ma                → {ma7, ma25, ma99}
indicator:{symbol}:rsi               → 62.5
indicator:{symbol}:macd              → {macd, signal, histogram}
indicator:{symbol}:bollinger         → {upper, middle, lower}
```

---

## 策略参数结构

```json
// RSI 策略
{
  "period": 14,
  "overbought": 70,
  "oversold": 30,
  "confirm_candles": 2
}

// MACD 策略
{
  "fast_period": 12,
  "slow_period": 26,
  "signal_period": 9,
  "histogram_threshold": 0
}

// 布林带策略
{
  "period": 20,
  "std_dev": 2.0,
  "squeeze_threshold": 0.02
}

// 成交量策略
{
  "volume_ma_period": 20,
  "volume_spike_threshold": 2.0,
  "price_change_threshold": 0.01
}

// 趋势策略
{
  "fast_ma": 7,
  "slow_ma": 25,
  "trend_ma": 99,
  "adx_threshold": 25
}

// 多时间框架策略
{
  "timeframes": ["1h", "4h", "1d"],
  "min_agreement": 2,
  "weight_h4": 0.5,
  "weight_d1": 0.3,
  "weight_h1": 0.2
}

// 大周期策略
{
  "primary_timeframe": "1w",
  "ma_periods": [20, 50, 100, 200],
  "proximity_threshold": 5.0,
  "adx_threshold": 25.0,
  "lookback_periods": 52
}
```

---

## 技术栈

| 组件 | 技术 | 用途 |
|------|------|------|
| 后端 | Rust + Tokio | 高性能异步 |
| 数据库 | PostgreSQL | 持久化存储 |
| 缓存 | Redis | 指标缓存 |
| Web 框架 | Actix-web (trading-core) / Axum (strategy-service) | HTTP API |
| 前端 | React + Next.js + Tauri | 桌面应用 |
| UI 组件 | Tailwind + shadcn/ui | 样式 |
| 图表 | recharts | 数据可视化 |

---

## 部署架构

```
┌─────────────────────────────────────────────────────────────┐
│                    轻量级架构（2核4G）                        │
│                                                             │
│  ┌──────────────────┐         ┌──────────────────┐         │
│  │  轻量云服务器      │         │  独立服务器        │         │
│  │  2核4GB           │         │  Redis            │         │
│  │                  │         │  PostgreSQL       │         │
│  │  - trading-core  │ ◄──────►│                  │         │
│  │  - strategy-svc  │   网络  │  - 数据存储       │         │
│  │  - trading-engine│         │  - 缓存           │         │
│  │                  │         │                  │         │
│  └──────────────────┘         └──────────────────┘         │
│                                                             │
│  资源使用：                                                  │
│  - 内存：1-2 GB（应用）✅                                    │
│  - CPU：10-30%（正常）✅                                     │
│  - 网络：低（只传数据）✅                                     │
│                                                             │
└─────────────────────────────────────────────────────────────┘
```

---

## 进度跟踪

| 阶段 | 状态 | 完成时间 |
|------|------|----------|
| 数据采集层 (trading-core) | ✅ 已完成 | 2026-07-06 |
| 策略分析层 (strategy-service) | ✅ 已完成 | 2026-07-07 |
| 交易执行层 (trading-engine) | ✅ 已完成 | 2026-06-28 |
| 回测引擎 | ✅ 已完成 | 2026-07-01 |
| 监控桌面应用 | ✅ 已完成 | 2026-07-06 |
| i18n 国际化 | ✅ 已完成 | 2026-07-07 |
| 职责边界重构 | ✅ 已完成 | 2026-07-14 |
