# Changelog

## 开发进度总结 (2026-07-01)

### ✅ 已完成

| 模块 | 状态 | 说明 |
|------|------|------|
| trading-engine | ✅ | 交易引擎核心功能 |
| trading-core service | ✅ | 数据采集 + HTTP API + WebSocket |
| 多时间框架策略框架 | ✅ | K线聚合器 + MultiTimeframeStrategy trait + TrendStrategy |
| 数据库 Schema V2 | ✅ | kline_1m, backtest_results, strategy_signals 等表 |
| candle1m REST 轮询采集 | ✅ | 每 10 秒拉取 Binance K线，写入 kline_1m 表 |
| 历史数据回填 (Backfill) | ✅ | 服务启动自动拉取历史数据 + 缺失 gap 检测补齐 |
| 多时间框架回测引擎 | ✅ | 逐 bar 模拟交易 + 做多做空 + 完整 BacktestResult |
| 样本外测试 + 滚动前进测试 | ✅ | WalkForwardEngine + 过拟合检测 |

### 🔄 进行中

无

### ⏳ 待完成

| 任务 | 优先级 | 说明 |
|------|--------|------|
| 多交易对回测 | 中 | 多交易对 + 市场状态分析 (P7) |
| 监控桌面应用 | 低 | Tauri 桌面端 (P8-P10) |

### 关键文件

```
trading-core/
├── src/main.rs                    # service 命令入口
├── src/api/handlers.rs            # HTTP API 处理器
├── src/api/websocket.rs           # WebSocket 处理器
└── src/api/server.rs              # Web 服务器

trading-common/
├── src/data/aggregator.rs         # K线聚合器
├── src/data/repository.rs         # 数据库操作
└── src/backtest/strategy/
    ├── multi_timeframe.rs         # 多时间框架策略 trait
    └── trend_strategy.rs          # 趋势策略实现

config/
├── schema.sql                     # 原始表 (tick_data)
└── schema_v2.sql                  # 新增表 (kline_1m 等)
```

---

## [2026-07-01] 样本外测试 + 滚动前进测试 (P6)

### 问题

- 回测只能在完整数据集上运行一次，无法检测过拟合
- 训练集表现好不代表实盘表现好，需要样本外验证
- 缺乏系统化的过拟合检测机制

### 实现

#### 1. WalkForwardEngine (`walk_forward.rs`)

滚动前进回测引擎，核心流程：

```
数据: [===========================================]
       |  train  | test |
       |    |  train  | test |
       |       |  train  | test |
              滚动窗口 →
每轮:
  1. 在 train 数据上运行 MultiTimeframeBacktestEngine
  2. 在 test 数据上运行 MultiTimeframeBacktestEngine
  3. 计算过拟合比率 = (train_sharpe - test_sharpe) / train_sharpe
```

#### 2. 样本外测试 (Out-of-Sample)

简化版：单次 70/30 划分，分别回测比较。

#### 3. 过拟合检测

- 过拟合比率 = (train_sharpe - test_sharpe) / max(train_sharpe, 0.01)
- 阈值默认 0.5，超过判定为过拟合
- 汇总指标：测试集累计收益率、平均 Sharpe、平均回撤、盈利轮次比例

#### 4. API 端点

- `POST /api/backtest/walk-forward` — 滚动前进测试
- `POST /api/backtest/out-of-sample` — 样本外测试

### 文件改动

| 文件 | 改动 |
|------|------|
| `trading-common/src/backtest/walk_forward.rs` | **新建** 滚动前进引擎 |
| `trading-common/src/backtest/mod.rs` | 导出新模块 |
| `trading-core/src/api/handlers.rs` | 新增 2 个 API handler |
| `trading-core/src/api/server.rs` | 注册新路由 |

### API 使用示例

```bash
# 滚动前进测试
curl -X POST http://localhost:8080/api/backtest/walk-forward \
  -H "Content-Type: application/json" \
  -d '{
    "strategy": "trend",
    "symbol": "BTCUSDT",
    "capital": 10000,
    "train_candles": 43200,
    "test_candles": 10080,
    "step_candles": 10080,
    "data_count": 100000
  }'

# 样本外测试
curl -X POST http://localhost:8080/api/backtest/out-of-sample \
  -H "Content-Type: application/json" \
  -d '{
    "strategy": "trend",
    "symbol": "BTCUSDT",
    "capital": 10000,
    "train_ratio": 0.7,
    "data_count": 50000
  }'
```

---

## [2026-07-01] 多时间框架回测引擎 (P5)

### 问题

- `BacktestEngine` 只支持单时间框架的 `Strategy` trait
- `MultiTimeframeStrategy` 是独立 trait，不被回测引擎消费
- `/api/backtest/multi-timeframe` 只做一次性分析，不执行模拟交易
- Portfolio 不支持做空（`EntryDirection::Short` 无法执行）

### 实现

#### 1. Portfolio 做空支持

- 新增 `PositionSide` 枚举：`Long` / `Short`
- `Position` 增加 `side` 字段
- 新增 `execute_short_open()` — 开空仓（借入卖出，获得 proceeds）
- 新增 `execute_short_close()` — 平空仓（买入归还，计算盈亏）
- `update_price()` 正确计算空头 `unrealized_pnl`
- `total_value()` 正确处理空头持仓
- 新增 `has_long_position()` / `has_short_position()` / `get_position_side()` 辅助方法

#### 2. MultiTimeframeBacktestEngine

逐 1m bar 模拟交易的核心引擎：

```
for each 1m kline:
  1. aggregator.update(kline)          // 更新聚合器
  2. portfolio.update_price(close)      // 更新价格
  3. check has_sufficient_data()        // 检查数据充足性
  4. get all_timeframes                 // 获取多时间框架快照
  5. strategy.analyze(&all_klines)      // 策略分析
  6. should_enter / should_exit         // 信号判断
  7. execute buy/sell/short             // 执行交易
```

#### 3. API 更新

- `POST /api/backtest/multi-timeframe` 现在返回完整回测结果
- 新增 `strategy_params` 请求字段
- 优先从 `kline_1m` 表读取数据，回退到 tick 数据生成

### 文件改动

| 文件 | 改动 |
|------|------|
| `trading-common/src/backtest/portfolio.rs` | 做空支持 |
| `trading-common/src/backtest/multi_timeframe_engine.rs` | **新建** 回测引擎 |
| `trading-common/src/backtest/mod.rs` | 导出新模块 |
| `trading-core/src/api/handlers.rs` | 更新 API handler |
| `trading-common/src/backtest/strategy/multi_timeframe.rs` | 修复测试浮点数问题 |

### API 使用示例

```bash
curl -X POST http://localhost:8080/api/backtest/multi-timeframe \
  -H "Content-Type: application/json" \
  -d '{
    "strategy": "trend",
    "symbol": "BTCUSDT",
    "capital": 10000,
    "data_count": 50000,
    "commission_rate": 0.1
  }'
```

### 响应示例（完整回测结果）

```json
{
  "success": true,
  "message": "Multi-timeframe backtest completed successfully",
  "data": {
    "strategy": "trend",
    "symbol": "BTCUSDT",
    "initial_capital": "$10000",
    "final_capital": "$10234.56",
    "total_return_pct": "2.35%",
    "total_trades": 12,
    "winning_trades": 7,
    "losing_trades": 5,
    "win_rate": "58.33%",
    "max_drawdown": "3.21%",
    "sharpe_ratio": "1.45",
    "profit_factor": "1.67"
  }
}
```

---

## [2026-06-30] candle1m 数据采集 + 历史回填

### 问题

candle1m 模式和 tick 模式走的是完全相同的 WebSocket 代码路径，`kline_1m` 表从未被写入。

### 修复

#### 1. candle1m REST 轮询

- `Exchange` trait 新增 `fetch_klines()` 和 `fetch_klines_with_time()` 方法
- `BinanceExchange` 实现调用 `GET /api/v3/klines` REST API
- `main.rs` candle1m 分支改为定时轮询（默认 10 秒），每次拉取最新 100 条 K 线
- `TickDataRepository` 新增 `insert_kline()` / `batch_insert_klines()` / `get_klines()` 写入 `kline_1m` 表
- 使用 `ON CONFLICT DO UPDATE`（upsert），同一根 K 线在未完成前会随时间更新

#### 2. 历史数据回填 (Backfill)

- 新增 `service/backfill.rs` — `BackfillService`
- 服务启动时自动执行：
  - 查询数据库已有数据的最早/最新时间
  - 从配置起始日期（如 2024-01-01）拉取到已有数据开始时间
  - 分页拉取（每次 1000 条），限速 100ms/请求
  - 检测已有数据中的缺失时间段（gap > 2 分钟），逐段补齐

#### 3. 新增 Repository 方法

- `get_kline_earliest(symbol)` — 获取某 symbol 最早的 kline 时间戳
- `get_kline_latest(symbol)` — 获取某 symbol 最新的 kline 时间戳
- `find_kline_gaps(symbol, start, end)` — 查找指定时间范围内的缺失时间段

#### 4. 配置变更

```toml
[collector]
mode = "candle1m"
poll_interval_secs = 10          # 轮询间隔（秒）
backfill_enabled = true          # 是否启用历史回填
backfill_start_date = "2024-01-01"  # 回填起始日期
```

### 文件变更

| 文件 | 改动 |
|------|------|
| `trading-core/src/exchange/traits.rs` | 新增 `fetch_klines` / `fetch_klines_with_time` |
| `trading-core/src/exchange/binance.rs` | 实现 REST K 线拉取，重构为 `do_fetch_klines` |
| `trading-core/src/exchange/types.rs` | 新增 `KlineData` 结构体 |
| `trading-common/src/data/repository.rs` | 新增 kline 写入 + 时间查询 + gap 检测方法 |
| `trading-core/src/service/backfill.rs` | 新建历史数据回填服务 |
| `trading-core/src/service/mod.rs` | 导出 `BackfillService` |
| `trading-core/src/config.rs` | 新增 `backfill_enabled` / `backfill_start_date` |
| `trading-core/src/main.rs` | candle1m 先 backfill 再轮询 |
| `config/development.toml` | 新增 backfill 配置 |
| `config/production.toml` | 新增 backfill 配置 |

---

## [2026-06-30] 数据库 Schema V2

### 新增表结构

| 表名 | 用途 | 说明 |
|------|------|------|
| `kline_1m` | K线数据 | 存储 1m K线，用于多时间框架聚合 |
| `backtest_results` | 回测结果 | 存储历史回测结果，便于比较分析 |
| `strategy_signals` | 策略信号 | 记录策略生成的交易信号 |
| `positions` | 持仓状态 | 记录当前持仓 |
| `trades` | 交易记录 | 记录所有已执行的交易 |
| `price_cache` | 价格缓存 | 缓存最新价格 |

### 文件位置

- `config/schema_v2.sql` - 完整的数据库 Schema V2

### 初始化命令

```bash
# 连接到 PostgreSQL
psql -U postgres -d trading_core

# 执行 Schema V2
\i config/schema_v2.sql
```

### 存储估算

| 表 | 数据量 | 存储空间 |
|------|------|------|
| `kline_1m` | ~525,600 条/年/交易对 | ~100MB/年/交易对 |
| `backtest_results` | 每次回测 1 条 | ~1KB/次 |
| `strategy_signals` | ~1,440 条/天 | ~1MB/天 |

### 设计说明

- **不使用存储过程**：所有聚合查询在 Rust 代码中实现（KlineAggregator），便于数据库迁移
- **不使用触发器**：应用层处理数据一致性，避免数据库层复杂性

---

## [2026-06-30] 多时间框架策略

### 新增功能

#### 1. K线聚合器 (`trading-common/src/data/aggregator.rs`)
- 将 1m K线聚合为其他时间框架（5m, 15m, 30m, 1h, 4h, 1d）
- 支持实时更新和批量聚合
- 自动处理时间窗口对齐

#### 2. 多时间框架策略框架
- `MultiTimeframeStrategy` trait - 多时间框架策略接口
- `TrendDirection` - 趋势方向枚举（Bullish/Bearish/Neutral）
- `MultiTimeframeAnalysis` - 多时间框架分析结果

#### 3. 趋势策略实现 (`trend_strategy.rs`)
- 4h 时间框架：使用 EMA20/EMA50 判断大趋势
- 1h 时间框架：使用 MACD 确认趋势
- 15m 时间框架：使用 RSI 寻找入场点
- 综合评分：加权计算整体置信度

#### 4. 新增 API 端点
- `POST /api/backtest/multi-timeframe` - 多时间框架策略分析

### 文件改动

#### 新增文件
- `trading-common/src/data/aggregator.rs` - K线聚合器
- `trading-common/src/backtest/strategy/multi_timeframe.rs` - 多时间框架策略 trait
- `trading-common/src/backtest/strategy/trend_strategy.rs` - 趋势策略实现

#### 修改文件
- `trading-common/src/data/types.rs` - Timeframe 添加 Hash trait
- `trading-common/src/data/mod.rs` - 导出 aggregator
- `trading-common/src/backtest/strategy/mod.rs` - 导出新策略
- `trading-core/src/api/handlers.rs` - 添加多时间框架回测 API
- `trading-core/src/api/server.rs` - 添加新路由

### API 使用示例

```bash
# 获取策略列表（包含多时间框架策略）
curl http://localhost:8080/api/strategies

# 执行多时间框架分析
curl -X POST http://localhost:8080/api/backtest/multi-timeframe \
  -H "Content-Type: application/json" \
  -d '{"strategy": "trend", "symbol": "BTCUSDT", "capital": 10000, "data_count": 10000}'
```

### 响应示例

```json
{
  "success": true,
  "message": "Multi-timeframe analysis completed",
  "data": {
    "strategy": "Multi-Timeframe Trend",
    "symbol": "BTCUSDT",
    "overall_direction": "Bullish",
    "overall_confidence": "0.75",
    "entry_allowed": true,
    "entry_direction": "Long",
    "timeframe_analyses": [
      {
        "timeframe": "4h",
        "direction": "Bullish",
        "confidence": "0.8",
        "description": "4h EMA20 > EMA50 by 2.50%"
      },
      {
        "timeframe": "1h",
        "direction": "Bullish",
        "confidence": "0.7",
        "description": "1h MACD histogram positive"
      },
      {
        "timeframe": "15m",
        "direction": "Bullish",
        "confidence": "0.6",
        "description": "15m RSI oversold at 28.50"
      }
    ],
    "data_points": 1000
  }
}
```

---

## [2026-06-30] trading-core 服务化改造

### 背景
- trading-engine 停机维护时数据采集不应中断
- 回测功能应随时可用，不需要重启服务
- 支持同时启用多种数据采集模式

### 新增功能

#### 1. `service` 命令
```bash
cargo run service        # 完整服务（数据采集 + API + 回测）
cargo run collector      # 仅数据采集
cargo run backtest       # CLI 回测（保留）
cargo run live           # 旧模式（保留）
```

#### 2. HTTP REST API (端口 8080)
- `GET /health` - 健康检查
- `GET /api/data/info` - 数据信息
- `GET /api/strategies` - 策略列表
- `POST /api/backtest` - 执行回测

#### 3. WebSocket 实时数据
- `ws://0.0.0.0:8080/ws` - 实时数据推送
- 支持订阅/取消订阅交易对
- 心跳检测

#### 4. 数据采集配置
```toml
[collector]
mode = "candle1m"           # disabled / tick / candle1m
enable_tick = false         # 是否同时启用 tick 采集
poll_interval_secs = 60     # 采集间隔
```

### 文件改动

#### 新增文件
- `trading-core/src/api/mod.rs` - API 模块入口
- `trading-core/src/api/handlers.rs` - HTTP 处理器
- `trading-core/src/api/websocket.rs` - WebSocket 处理器
- `trading-core/src/api/server.rs` - Web 服务器

#### 修改文件
- `trading-core/Cargo.toml` - 添加 actix-web, actix-cors, actix-ws 依赖
- `trading-core/src/config.rs` - 添加 CollectorConfig, CollectorMode
- `trading-core/src/main.rs` - 添加 service 命令和 run_service_mode
- `config/development.toml` - 添加 collector 配置
- `config/production.toml` - 添加 collector 配置

### 依赖新增
```toml
actix-web = "4"
actix-cors = "0.7"
actix-ws = "0.2"
actix-web-actors = "4"
actix = "0.13"
```

### 测试结果
- ✅ 服务启动成功
- ✅ 数据库/Redis 连接正常
- ✅ 交易所 WebSocket 连接正常
- ✅ API 端点全部响应正常
- ✅ 回测 API 可用

### 架构优势
| 场景 | 旧方案 | 新方案 |
|------|--------|--------|
| 停机维护 | 数据断档 | 数据采集继续 |
| 执行回测 | 需要重启 | HTTP API 随时调用 |
| 实时监控 | 无 | WebSocket 推送 |
| 多模式采集 | 不支持 | 配置开关控制 |

### 使用示例
```bash
# 启动完整服务
cd trading-core
cargo run --release -- service

# 测试 API
curl http://localhost:8080/health
curl http://localhost:8080/api/strategies

# 执行回测
curl -X POST http://localhost:8080/api/backtest \
  -H "Content-Type: application/json" \
  -d '{"strategy": "rsi", "symbol": "BTCUSDT", "capital": 10000, "data_count": 10000}'
```

---

## [2026-06-29] OkxAdapter 修复与数据源可配置

### 已完成
- OkxAdapter 6项修复
- 数据源可配置 (trades/tickers/candle1m)
- 数据积累方案确定

---

## [2026-06-28] 交易引擎核心功能

### 已完成
- Phase 1-4: 交易引擎核心功能
- BinanceAdapter: USDⓈ-M 合约
- BinanceSpotAdapter: 现货
- OkxAdapter: 基础实现
- Exchange trait: 统一接口
