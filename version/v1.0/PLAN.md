# rust-trade v1.0 开发计划

## 当前进度 (2026-07-01 更新)

### ✅ 核心模块

| 模块 | 状态 | 说明 |
|------|------|------|
| BinanceAdapter (合约) | ✅ 完成 | `/fapi/v1/...` `/fapi/v2/...`，独立文件 |
| BinanceSpotAdapter (现货) | ✅ 完成 | `/api/v3/...`，独立文件 |
| OkxAdapter | ✅ 完成 | 6项修复已完成，可配置数据源 |
| Exchange trait | ✅ 完成 | 统一接口，覆盖现货/合约，含 fetch_klines / fetch_klines_with_time |
| 数据采集 (candle1m) | ✅ 完成 | REST 轮询拉取 + kline_1m 写入 + 历史数据回填 + 缺失补齐 |
| K线聚合器 | ✅ 完成 | 1m → 5m/15m/30m/1h/4h/1d |
| 多时间框架策略框架 | ✅ 完成 | MultiTimeframeStrategy trait + TrendStrategy |
| 数据库 Schema V2 | ✅ 完成 | kline_1m, backtest_results, strategy_signals 等表 |
| trading-core 服务化 | ✅ 完成 | HTTP API + WebSocket + 数据采集 |

### ✅ 回测引擎

| 模块 | 状态 | 说明 |
|------|------|------|
| 多时间框架回测引擎 | ✅ 完成 | MultiTimeframeBacktestEngine，逐 bar 模拟交易，支持做多做空 |
| Portfolio 做空支持 | ✅ 完成 | PositionSide 枚举，execute_short_open/close |
| 样本外测试 | ✅ 完成 | 70/30 单次划分，训练/测试分别回测 |
| 滚动前进测试 | ✅ 完成 | WalkForwardEngine，滚动窗口训练+测试 |
| 过拟合检测 | ✅ 完成 | 过拟合比率 = (train_sharpe - test_sharpe) / train_sharpe |
| 多交易对回测 | ✅ 完成 | MultiSymbolBacktestEngine，批量 symbol 回测汇总 |
| 市场状态分析 | ✅ 完成 | MarketStateAnalyzer，ATR/ADX 分析趋势/震荡/波动分布 |

### ✅ API 端点

| 端点 | 方法 | 功能 |
|------|------|------|
| `/health` | GET | 健康检查 |
| `/api/data/info` | GET | 数据信息 |
| `/api/strategies` | GET | 策略列表 |
| `/api/backtest` | POST | 单时间框架回测 |
| `/api/backtest/multi-timeframe` | POST | 多时间框架回测（逐 bar 模拟） |
| `/api/backtest/walk-forward` | POST | 滚动前进测试（抗过拟合） |
| `/api/backtest/out-of-sample` | POST | 样本外测试（抗过拟合） |
| `/api/backtest/multi-symbol` | POST | 多交易对回测 |
| `/api/analysis/market-state` | POST | 市场状态分析 |

### 📋 待开发

| 模块 | 状态 | 说明 |
|------|------|------|
| 监控桌面应用 | 📋 规划完成 | P8-P10，Tauri 桌面端 |
| Exchange trait 分层重构 | 📋 可选 | P11，MarketDataProvider / TradingOperations 分离 |

---

## 一、OkxAdapter 升级（✅ 已完成）

### 1.1 修复 subscribe_trades

**问题：** 当前使用 `tickers` channel（行情快照），不是实时成交数据。

**修复：** 改用 `trades` channel，接收逐笔成交。

```
当前: {"channel": "tickers", "instId": "BTC-USDT"}  → 行情快照
修改: {"channel": "trades", "instId": "BTC-USDT"}   → 逐笔成交
```

推送数据格式：`instId, tradeId, px, sz, side, ts, count`

### 1.2 实现 subscribe_user_data

**问题：** 当前返回 `warn!("not implemented yet")`。

**实现：**
- 连接 private WebSocket: `wss://ws.okx.com:8443/ws/v5/private`
- 发送 login 认证：`HMAC-SHA256-base64(timestamp + 'GET' + '/users/self/verify', secretKey)`
- 订阅 `orders` channel 接收订单更新
- 每 30 秒发送文本 `ping` 保持连接
- 解析推送数据转换为 `OrderUpdate`

### 1.3 使用原生批量接口

**问题：** 当前逐个调用 `place_order` / `cancel_order`。

**修复：**
- `batch_place_orders` → `POST /api/v5/trade/batch-orders`（最多 20 个）
- `batch_cancel_orders` → `POST /api/v5/trade/cancel-batch-orders`（最多 20 个）

### 1.4 修复 get_account 解析

**问题：** 没有解析 `totalEq`（总权益）、`upl`（未实现盈亏）、`adjEq`（调整权益）。

**修复：**
```rust
// 正确解析 /api/v5/account/balance 响应
total_equity: data["data"][0]["totalEq"]      // 总权益 (USD)
available_balance: data["data"][0]["availEq"]  // 可用权益
unrealized_pnl: data["data"][0]["upl"]         // 未实现盈亏
margin_used: data["data"][0]["imr"]            // 初始保证金
```

### 1.5 修复 place_order 的 tdMode

**问题：** 硬编码 `tdMode: "cash"`，合约交易需要 `cross`/`isolated`。

**修复：** 根据交易对自动判断：
- 现货 (`BTC-USDT`) → `tdMode: "cash"`
- 合约 (`BTC-USDT-SWAP`) → `tdMode: "cross"` (默认全仓)
- 提供配置项可覆盖默认值

### 1.6 修复 WebSocket 心跳

**问题：** OKX 需要发送文本消息 `ping`，而非 WebSocket Ping frame。

**修复：** 在 WebSocket 循环中添加定时发送：
```rust
let mut ping_interval = tokio::time::interval(Duration::from_secs(30));
loop {
    tokio::select! {
        _ = ping_interval.tick() => {
            ws_stream.send(Message::Text("ping".to_string())).await?;
        }
        msg = ws_stream.next() => {
            if let Some(Ok(Message::Text(text))) = msg {
                if text == "pong" { continue; } // 心跳响应
                // 处理其他消息...
            }
        }
    }
}
```

---

## 二、数据源策略（低成本量化）

### 2.1 WebSocket Channel 选择

| Channel | 数据量 | 适用场景 | 资源消耗 |
|---------|--------|---------|---------|
| `trades` | ~5000条/分钟 | 高频策略、精确回测 | 高 |
| `tickers` | ~300条/分钟 | 中频策略、监控 | 中 |
| `candle1m` | 1条/分钟 | **低频量化首选** | 极低 |
| REST 轮询 | 按需 | 兜底、补数据 | 最低 |

### 2.2 推荐方案

**主数据源：** WebSocket `candle1m` channel
- 每分钟推送一根完成的 K 线（OHLCV）
- 资源消耗极低，适合服务器资源有限的场景

**兜底数据源：** REST `GET /api/v5/market/candles` / `GET /fapi/v1/klines`
- WebSocket 断线时补数据
- 策略初始化时拉取历史数据

**数据源可配置：** 后续在配置文件中指定
```toml
[trading]
data_source = "candle1m"  # trades / tickers / candle1m
```

---

## 三、监控桌面应用架构

### 3.1 现有基础

已有 `src-tauri` 桌面应用，功能为**回测工具**：

| 现有命令 | 功能 |
|---------|------|
| `get_data_info` | 查看历史数据统计 |
| `get_available_strategies` | 列出可用策略 |
| `run_backtest` | 运行回测 |
| `get_historical_data` | 获取历史 tick 数据 |
| `get_ohlc_preview` | 预览 K 线 |
| `get_strategy_capabilities` | 策略能力查询 |

**缺失功能：** 实时行情、持仓监控、交易记录、利润统计、胜率、夏普等。

### 3.2 扩展方案

在现有 `src-tauri` 基础上扩展，复用 Tauri 框架、数据库连接、策略引擎，新增监控 commands：

```
src-tauri/src/commands.rs
│
├── 现有 (回测)                    新增 (监控)
├── get_data_info                 ├── get_realtime_prices      ← WebSocket 直连
├── get_available_strategies      ├── get_positions            ← 读 trading_positions
├── run_backtest                  ├── get_trade_history        ← 读 trade_logs
├── get_historical_data           ├── get_pnl_summary          ← 汇总计算
├── get_ohlc_preview              ├── get_win_rate             ← 统计
├── get_strategy_capabilities     ├── get_sharpe_ratio         ← 复用 metrics
└── validate_backtest_config      ├── get_max_drawdown         ← 复用 metrics
                                  ├── get_equity_curve         ← 资金曲线
                                  └── get_exchange_status      ← 连接状态
```

### 3.3 整体架构

```
┌──────────────────────────────────────────────────────────────┐
│  Tauri 桌面应用 (src-tauri)                                    │
│                                                              │
│  ┌────────────────────┐    ┌────────────────────────────────┐│
│  │ 回测模块 (已有)      │    │ 监控模块 (新增)                 ││
│  │ - 策略选择           │    │ - 实时行情图表 (WS 直连)        ││
│  │ - 回测运行           │    │ - 持仓/交易记录 (读数据库)      ││
│  │ - 结果展示           │    │ - 利润/胜率/夏普 (统计计算)     ││
│  └────────┬───────────┘    └──────────┬─────────────────────┘│
│           │                           │                      │
└───────────┼───────────────────────────┼──────────────────────┘
            │                           │
            ▼                           ▼
   ┌──────────────────┐    ┌──────────────────────────┐
   │  PostgreSQL       │    │  Binance / OKX            │
   │  Redis            │    │  WebSocket                │
   │  (交易引擎写入)    │    │  (行情直连)               │
   └──────────────────┘    └──────────────────────────┘
```

### 3.4 数据来源

| 展示内容 | 数据来源 | 说明 |
|---------|---------|------|
| 实时行情图表 | WebSocket 直连交易所 | 低延迟，不依赖服务器 |
| 当前持仓 | PostgreSQL `trading_positions` + Redis | 交易引擎实时写入 |
| 交易记录 | PostgreSQL `trading_orders` + `trade_logs` | 完整历史 |
| 利润曲线 | PostgreSQL `trade_logs` 汇总计算 | 按时间聚合 |
| 交易胜率 | `trade_logs` 中 pnl > 0 的比例 | |
| 夏普比率 | 复用 `trading-common::backtest::metrics` | 接入实盘收益率序列 |
| 最大回撤 | 同上 | |
| 资金费率 | REST 按需拉取 | |

### 3.5 功能模块

```
监控桌面应用 (扩展自 src-tauri)
│
├── 回测模块 (已有)
│   ├── 策略选择与参数配置
│   ├── 回测运行与结果展示
│   └── OHLC 数据预览
│
├── 行情模块 (新增)
│   ├── 实时 K 线图（WebSocket 直连交易所）
│   ├── 多交易对切换
│   └── 多交易所支持（Binance/OKX）
│
├── 持仓模块 (新增)
│   ├── 当前持仓列表
│   ├── 持仓盈亏实时更新
│   └── 杠杆/保证金信息
│
├── 交易模块 (新增)
│   ├── 交易历史记录
│   ├── 订单状态追踪
│   └── 手动下单（可选）
│
└── 统计模块 (新增)
    ├── 利润曲线（日/周/月）
    ├── 胜率统计
    ├── 夏普比率
    ├── 最大回撤
    ├── 资金费率收益
    └── 手续费统计
```

---

## 四、交易所接口差异备忘

### 4.1 Binance 现货 vs 合约

| 维度 | 现货 (api.binance.com) | 合约 (fapi.binance.com) |
|------|----------------------|------------------------|
| 路径 | `/api/v3/...` | `/fapi/v1/...` `/fapi/v2/...` |
| WebSocket | `stream.binance.com:9443` | `fstream.binance.com` |
| 持仓 | 无（只有余额） | `/fapi/v2/positionRisk` |
| 杠杆 | 不支持 | `/fapi/v1/leverage` |
| 保证金模式 | 不支持 | `/fapi/v1/marginType` |
| 订单类型 | + `LIMIT_MAKER` | + `TRAILING_STOP_MARKET` |

### 4.2 OKX vs Binance

| 维度 | Binance | OKX |
|------|---------|-----|
| 现货/合约分离 | 完全分开（不同域名、端点） | 统一 API，`instType` 参数区分 |
| 签名方式 | HMAC-SHA256 → hex，放 query | HMAC-SHA256 → base64，放 header |
| Passphrase | 无 | 必须 |
| 数值类型 | 混合 | 全部字符串 |
| 保证金模式 | 持久化设置 | 每笔订单 `tdMode` 参数 |
| 批量下单 | 现货不支持，合约最多 5 个 | 统一最多 20 个 |
| K 线格式 | 12 元素数组 | 9 元素数组 + `confirm` |
| WebSocket 心跳 | Ping/Pong frame | 文本 `ping`/`pong` |
| 订单状态 | `NEW`/`FILLED`/`CANCELED` | `live`/`filled`/`canceled` |
| InstId 格式 | `BTCUSDT` | 现货 `BTC-USDT`，合约 `BTC-USDT-SWAP` |

---

## 五、数据积累与回测抗过拟合

### 5.1 数据积累方案

trading-engine (candle1m 模式) 边跑边存，自然积累：

```
trading-engine 每分钟:
  ├── 拉取 K 线 → 策略执行
  └── 写入 PostgreSQL → 积累历史数据

第 1 天:    1,440 根 K 线 → 策略预热
第 7 天:    10,080 根 K 线 → 短期回测
第 30 天:   43,200 根 K 线 → 中期回测
第 365 天:  525,600 根 K 线 → 完整年回测

存储成本: ~100 MB/年/交易对，3 个交易对 ≈ 300 MB/年
```

trading-core 改造为按需工具：批量拉取历史 K 线补充数据。

### 5.2 抗过拟合措施

**核心原则：数据多样性 > 数据数量**

#### 5.2.1 样本外测试 (Out-of-Sample)

```
数据划分:
  训练集 (70%): 用于策略参数优化
  测试集 (30%): 用于验证，不参与优化

|████████████████████████░░░░░░░░░|
|←── 训练集 (优化) ──→|←─ 测试集 ─→|
                       从未被策略"看过"

验证标准:
  训练集表现好 + 测试集表现好 → 策略可能有效
  训练集表现好 + 测试集表现差 → 过拟合，策略无效
```

#### 5.2.2 滚动前进测试 (Walk-Forward Analysis)

```
第 1 轮: 训练 [1月-3月] → 测试 [4月]
第 2 轮: 训练 [2月-4月] → 测试 [5月]
第 3 轮: 训练 [3月-5月] → 测试 [6月]
...

优点:
  - 更接近真实运行方式
  - 检测策略在不同市场状态下的适应性
  - 避免单次划分的偶然性
```

#### 5.2.3 多交易对验证

```
同一策略在多个交易对上测试:
  BTC-USDT:  回测结果
  ETH-USDT:  回测结果
  SOL-USDT:  回测结果

如果都有效 → 更可能是真规律
如果只对一个有效 → 可能是过拟合
```

#### 5.2.4 市场状态覆盖检查

```
好的数据:                          差的数据:
├── 上涨趋势 (牛市)                ├── 只有横盘震荡
├── 下跌趋势 (熊市)                ├── 市场状态单一
├── 横盘震荡
├── 剧烈波动 (黑天鹅)
└── 快速反弹

检查方法: 回测前分析数据中的波动率分布、趋势分布
```

### 5.3 回测模块增强计划

在 `trading-common::backtest::engine` 中新增：

| 功能 | 说明 | 状态 |
|------|------|------|
| 样本外测试 | 自动划分训练集/测试集，分别回测 | ✅ 已完成 |
| 滚动前进测试 | 滚动窗口训练+测试，输出每轮结果 | ✅ 已完成 |
| 多交易对回测 | 批量运行多个 symbol，汇总统计 | ✅ 已完成 |
| 市场状态分析 | 分析数据中的趋势/震荡/波动分布 | ✅ 已完成 |
| 过拟合检测 | 训练集 vs 测试集表现差异告警 | ✅ 已完成 |
| 多时间框架回测 | 支持 1m→4h 综合分析策略的回测 | ✅ 已完成 |

---

## 六、多时间框架趋势策略

### 6.1 策略设计

```
分析链: 1m → 5m → 15m → 30m → 1h → 2h → 4h

各时间框架职责:
  4h:  判断大趋势方向 (主趋势)
  2h:  确认中期趋势
  1h:  趋势强度判断
  30m: 寻找回调/反弹区域
  15m: 确认入场区域
  5m:  精确入场信号
  1m:  执行入场/出场

示例逻辑:
  4h 趋势向上 + 1h 回调到支撑位 + 15m 出现反转信号 → 做多
  4h 趋势向下 + 1h 反弹到阻力位 + 15m 出现反转信号 → 做空
```

### 6.2 数据架构

**只存储 1m K 线，其他时间框架实时聚合生成：**

```
存储层: PostgreSQL
  kline_data 表 (只存 1m K 线)
    symbol, timestamp, open, high, low, close, volume

聚合层: trading-common::data::aggregator (新增)
  输入: Vec<OHLCData> (1m K线)
  输出: HashMap<Timeframe, Vec<OHLCData>>
    5m  = 5 根 1m 聚合
    15m = 15 根 1m 聚合
    30m = 30 根 1m 聚合
    1h  = 60 根 1m 聚合
    2h  = 120 根 1m 聚合
    4h  = 240 根 1m 聚合

缓存层: Redis
  缓存最近 N 根各时间框架 K 线 (避免重复聚合)
```

### 6.3 需要新增的模块

#### 6.3.1 K 线聚合器 (trading-common)

```
文件: trading-common/src/data/aggregator.rs (新增)

功能:
  pub struct CandleAggregator;
  impl CandleAggregator {
      /// 从 1m K 线聚合生成指定时间框架
      pub fn aggregate(candles_1m: &[OHLCData], target: Timeframe) -> Vec<OHLCData>;

      /// 从 1m K 线生成所有时间框架
      pub fn aggregate_all(candles_1m: &[OHLCData]) -> HashMap<Timeframe, Vec<OHLCData>>;
  }

聚合规则:
  - open = 第一根的 open
  - high = 所有中的最高价
  - low = 所有中的最低价
  - close = 最后一根的 close
  - volume = 所有之和
  - timestamp = 第一根的 open_time
```

#### 6.3.2 多时间框架策略接口 (trading-common)

```
文件: trading-common/src/backtest/strategy/base.rs (扩展)

新增 trait:
  pub trait MultiTimeframeStrategy: Strategy {
      /// 策略需要哪些时间框架
      fn required_timeframes(&self) -> Vec<Timeframe>;

      /// 多时间框架数据回调
      fn on_multi_timeframe(
          &mut self,
          data: &HashMap<Timeframe, Vec<OHLCData>>,
      ) -> Signal;
  }
```

#### 6.3.3 K 线存储 (trading-engine)

```
文件: trading-engine/src/storage/ (扩展)

新增: kline_repository.rs
  - store_kline(symbol, timeframe, ohlc) → INSERT
  - get_klines(symbol, timeframe, limit) → SELECT
  - get_latest_kline(symbol, timeframe) → SELECT
```

#### 6.3.4 trading-engine 数据流改造

```
candle1m 数据源:
  交易所 REST → 1m K线
      │
      ├──▶ 存储 PostgreSQL (kline_data 表)
      │
      ├──▶ 聚合器 → 5m/15m/30m/1h/2h/4h
      │       │
      │       ▼
      │   多时间框架策略分析
      │       │
      │       ▼
      │   信号生成 → 下单
      │
      └──▶ Redis 缓存 (各时间框架最新 K 线)
```

### 6.4 回测支持

```
回测流程:
  1. 从 PostgreSQL 加载 1m K 线
  2. 聚合器生成所有时间框架
  3. 逐根遍历，每到一根完整的 1m K 线:
     a. 更新对应时间框架的 K 线
     b. 如果有新的 5m/15m/30m/1h/2h/4h K 线完成
     c. 调用策略 on_multi_timeframe()
     d. 根据信号模拟成交
  4. 输出回测结果

数据库需求:
  - kline_1m 表存储所有 1m K 线
  - 回测时按时间范围加载
  - 内存中聚合生成高时间框架
```

---

## 七、实施优先级

---

## 六、实施优先级

| 优先级 | 任务 | 状态 | 说明 |
|--------|------|------|------|
| P0 | OkxAdapter 6 项修复 | ✅ 已完成 | |
| P1 | 数据源可配置（candle1m 优先） | ✅ 已完成 | |
| P2 | K 线聚合器 | ✅ 已完成 | |
| P3 | K 线存储 + 自动积累 + 历史回填 | ✅ 已完成 | REST 轮询 + backfill + gap 检测补齐 |
| P4 | 多时间框架策略接口 | ✅ 已完成 | MultiTimeframeStrategy trait + TrendStrategy |
| P5 | 多时间框架回测支持 | ✅ 已完成 | MultiTimeframeBacktestEngine + 做空支持 |
| P6 | 回测增强 - 样本外测试 + 滚动前进测试 | ✅ 已完成 | WalkForwardEngine + 过拟合检测 |
| P7 | 回测增强 - 多交易对 + 市场状态分析 | ✅ 已完成 | MultiSymbolBacktestEngine + MarketStateAnalyzer |
| P8 | 监控桌面应用 - 实时行情图表 | ⏳ 待开发 | src-tauri 新增 WebSocket 直连行情 |
| P9 | 监控桌面应用 - 持仓/交易记录 | ⏳ 待开发 | 读 trading_positions + trade_logs |
| P10 | 监控桌面应用 - 统计分析 | ⏳ 待开发 | 复用 trading-common::backtest::metrics |
| P11 | Exchange trait 分层重构（可选） | 📋 可选 | MarketDataProvider / TradingOperations 分离 |

**完成进度：P0-P7 ✅ (8/11)，P8-P10 待开发，P11 可选**

---

## 八、API 文档位置

| 文档 | 路径 |
|------|------|
| Binance 合约 | `version/v1.0/schema.yaml` |
| Binance 现货 | `version/v1.0/schema-Spot Trading.yaml` |
| OKX | `version/v1.0/okx_api.html` |
| 架构设计 | `version/v1.0/ARCHITECTURE.md` |
| 开发总结 | `version/v1.0/DEVELOPMENT_SUMMARY.md` |
