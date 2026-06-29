# rust-trade v1.0 开发计划

## 当前进度

| 模块 | 状态 | 说明 |
|------|------|------|
| BinanceAdapter (合约) | ✅ 完成 | `/fapi/v1/...` `/fapi/v2/...`，独立文件 |
| BinanceSpotAdapter (现货) | ✅ 完成 | `/api/v3/...`，独立文件 |
| OkxAdapter | ⚠️ 待升级 | 基础框架已有，6个问题待修复 |
| Exchange trait | ✅ 完成 | 统一接口，覆盖现货/合约 |
| 监控桌面应用 | 📋 规划完成 | 待开发 |

---

## 一、OkxAdapter 升级（下一步）

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

## 五、实施优先级

| 优先级 | 任务 | 预计工作量 | 说明 |
|--------|------|-----------|------|
| P0 | OkxAdapter 6 项修复 | 中 | subscribe_trades/user_data/batch/account/tdMode/ping |
| P1 | 数据源可配置（candle1m 优先） | 小 | 配置文件指定 trades/tickers/candle1m |
| P2 | 监控桌面应用 - 实时行情图表 | 大 | src-tauri 新增 WebSocket 直连行情 |
| P3 | 监控桌面应用 - 持仓/交易记录 | 中 | 读 trading_positions + trade_logs |
| P4 | 监控桌面应用 - 统计分析 | 中 | 复用 trading-common::backtest::metrics |
| P5 | Exchange trait 分层重构（可选） | 大 | MarketDataProvider / TradingOperations 分离 |

---

## 六、API 文档位置

| 文档 | 路径 |
|------|------|
| Binance 合约 | `version/v1.0/schema.yaml` |
| Binance 现货 | `version/v1.0/schema-Spot Trading.yaml` |
| OKX | `version/v1.0/okx_api.html` |
| 架构设计 | `version/v1.0/ARCHITECTURE.md` |
| 开发总结 | `version/v1.0/DEVELOPMENT_SUMMARY.md` |
