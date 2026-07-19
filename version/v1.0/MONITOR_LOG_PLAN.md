# 交易监控日志系统 - 实现方案

## 背景

当前交易管道的关键事件（成交、止损、风控）只写 stdout，进程重启即丢失。`trade_logs` 和 `risk_logs` 表已建但从未写入数据。WebSocket 只推送价格，不推送交易事件。前端无法看到完整的交易执行链路。

## 目标

- 全链路日志持久化（信号→下单→成交→止损/风控）
- 实时 WebSocket 事件推送
- 前端可查询完整交易时间线
- signal_id 贯穿全链路，可追溯

---

## 第一阶段：DB 日志写入（核心）

### 1.1 表结构增强

**trade_logs 新增字段**：

```sql
signal_id       UUID           -- 关联信号
exchange        VARCHAR(20)    -- 交易所
market_type     VARCHAR(10)    -- spot/futures
event_type      VARCHAR(30)    -- fill / stop_loss / take_profit / risk_close / risk_reduce
commission      DECIMAL(20,8)  -- 手续费
slippage        DECIMAL(20,8)  -- 滑点 (实际价 - 预期价)
details         JSONB          -- 扩展信息 (风控原因、止损类型等)
```

**risk_logs 新增字段**：

```sql
signal_id       UUID           -- 关联信号
exchange        VARCHAR(20)    -- 交易所
market_type     VARCHAR(10)    -- spot/futures
check_result    VARCHAR(20)    -- allow / reject / modify / action_triggered
current_equity  DECIMAL(20,8)  -- 当前权益
peak_equity     DECIMAL(20,8)  -- 峰值权益
daily_pnl       DECIMAL(20,8)  -- 当日盈亏
```

**新建文件**：`version/v1.0/sql/20260717_trading_monitor_logs.sql`

### 1.2 创建 EventRepository

**新建**：`trading-engine/src/storage/event_repository.rs`

```rust
pub struct EventRepository { pool: PgPool }

impl EventRepository {
    // 成交日志
    pub async fn log_trade(&self, signal_id, symbol, side, qty, price, 
                           order_id, exchange, event_type, commission, 
                           slippage, pnl, details) -> Result<()>
    
    // 风控日志
    pub async fn log_risk_event(&self, event_type, symbol, signal_id,
                                check_result, details, current_equity,
                                peak_equity, daily_pnl) -> Result<()>
    
    // 查询接口（给 API 用）
    pub async fn get_trade_logs(&self, symbol, signal_id, event_type, limit) -> Result<Vec>
    pub async fn get_risk_logs(&self, event_type, symbol, limit) -> Result<Vec>
}
```

### 1.3 传递 signal_id 到 OrderManager

**当前问题**：signal_id 在 signal_poller.rs 中可用，但传入 OrderManager.execute_signal() 时丢失。

**修复**：

1. `Signal` enum 新增 `signal_id: Option<Uuid>` 字段
2. `signal_poller.rs` 的 `convert_signal()` 填入 signal_id
3. `OrderManager` 的 `OrderInfo` 结构体新增 `signal_id: Option<Uuid>`
4. `place_and_track_order()` 保存 signal_id 到 active_orders
5. 成交时从 active_orders 取出 signal_id

**涉及文件**：
- `trading-common/src/backtest/strategy/mod.rs` — Signal enum
- `trading-engine/src/engine/signal_poller.rs` — convert_signal
- `trading-engine/src/order/manager.rs` — OrderInfo, place_and_track_order, handle_order_update

### 1.4 各节点写入位置

| 事件 | 写入表 | 代码位置 |
|------|--------|---------|
| **下单成交** | trade_logs | `order/manager.rs` → `handle_order_update()` 中 `Filled` 分支 |
| **止损触发** | trade_logs | `order/manager.rs` → `execute_stop_action()` 下单后 |
| **止盈触发** | trade_logs | 同上 |
| **风控平仓** | trade_logs | `order/manager.rs` → `execute_risk_action()` 下单后 |
| **风控检查** | risk_logs | `risk/engine.rs` → `check_order()` 每项检查后 |
| **持仓风控** | risk_logs | `risk/engine.rs` → `check_positions()` 触发 action 时 |
| **熔断触发** | risk_logs | `risk/engine.rs` → `trigger_circuit_breaker()` |

---

## 第二阶段：实时事件 WebSocket 推送

### 2.1 定义交易事件类型

**新建**：`trading-common/src/data/event_types.rs`

```rust
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum TradingEvent {
    // 信号产生
    SignalGenerated { 
        signal_id: Uuid, symbol: String, direction: String,
        strategy: String, confidence: Decimal, entry_price: Decimal 
    },
    // 订单下单
    OrderPlaced { 
        order_id: String, symbol: String, side: String,
        quantity: Decimal, order_type: String, signal_id: Option<Uuid> 
    },
    // 订单成交
    OrderFilled { 
        order_id: String, symbol: String, side: String,
        quantity: Decimal, avg_price: Decimal, pnl: Option<Decimal>,
        signal_id: Option<Uuid>, event_type: String 
    },
    // 止损止盈触发
    StopTriggered { 
        symbol: String, trigger_type: String, price: Decimal,
        entry_price: Decimal, pnl: Decimal 
    },
    // 风控动作
    RiskAction { 
        event_type: String, symbol: Option<String>,
        action: String, reason: String, details: serde_json::Value 
    },
    // 风控检查结果
    RiskCheck { 
        event_type: String, result: String,
        details: serde_json::Value 
    },
}
```

### 2.2 trading-engine 侧广播

**修改**：`trading-engine/src/main.rs`

```rust
let (event_tx, _) = tokio::sync::broadcast::channel::<TradingEvent>(10000);
```

将 `event_tx` 注入：
- `RiskEngine::new(config_repo, event_tx)` — 风控事件
- `OrderManager::new(exchange, risk_engine, event_tx)` — 下单/成交事件
- `SignalPoller::new(pool, risk_engine, units, config, event_tx)` — 信号事件

### 2.3 trading-core WebSocket 扩展

**修改**：`trading-core/src/api/websocket.rs`

- `WsResponse` 新增 `TradingEvent(TradingEvent)` 变体
- `WsRequest` 新增 `SubscribeEvents` 类型
- `WsSession` 新增 `event_rx: broadcast::Receiver<TradingEvent>`
- `ws_handler` 增加 `event_tx` 参数

### 2.4 跨进程传递

trading-engine 和 trading-core 是独立进程。event_tx 无法跨进程共享。

**方案**：通过 Redis Pub/Sub 传递交易事件。

- trading-engine 发布事件到 Redis channel `trading:events`
- trading-core 订阅该 channel，转发到 WebSocket 客户端

**新建**：`trading-engine/src/storage/event_publisher.rs`

```rust
pub struct EventPublisher { redis: redis::aio::ConnectionManager }

impl EventPublisher {
    pub async fn publish(&self, event: &TradingEvent) -> Result<()>
}
```

---

## 第三阶段：前端查询 API

### 3.1 REST 端点

在 trading-engine 中新增：

| 端点 | 方法 | 说明 |
|------|------|------|
| `/api/events/trades` | GET | 查询成交日志（支持 symbol/signal_id/event_type/limit 过滤） |
| `/api/events/risk` | GET | 查询风控日志（支持 event_type/symbol/limit 过滤） |
| `/api/events/timeline` | GET | 给定 signal_id，返回完整链路时间线 |

### 3.2 全链路时间线

`/api/events/timeline?signal_id=xxx` 返回：

```json
{
  "signal_id": "uuid",
  "timeline": [
    {"time": "...", "event": "signal_generated", "data": {...}},
    {"time": "...", "event": "risk_check", "result": "allow", "data": {...}},
    {"time": "...", "event": "order_placed", "data": {...}},
    {"time": "...", "event": "order_filled", "data": {...}},
    {"time": "...", "event": "stop_triggered", "data": {...}}
  ]
}
```

---

## 涉及文件汇总

| 文件 | 操作 |
|------|------|
| `version/v1.0/sql/20260717_trading_monitor_logs.sql` | 新建 |
| `sql/core/trade_logs.sql` | 更新 |
| `sql/core/risk_logs.sql` | 更新 |
| `sql/init_database.sql` | 更新 |
| `trading-engine/src/storage/event_repository.rs` | 新建 |
| `trading-engine/src/storage/event_publisher.rs` | 新建（Redis Pub/Sub） |
| `trading-engine/src/storage/mod.rs` | 注册模块 |
| `trading-common/src/data/event_types.rs` | 新建：TradingEvent 定义 |
| `trading-common/src/data/mod.rs` | 注册模块 |
| `trading-common/src/backtest/strategy/mod.rs` | Signal 增加 signal_id |
| `trading-engine/src/engine/signal_poller.rs` | convert_signal 填 signal_id |
| `trading-engine/src/order/manager.rs` | 成交/止损/风控时写日志+广播 |
| `trading-engine/src/risk/engine.rs` | check_order/check_positions 写日志+广播 |
| `trading-engine/src/main.rs` | 创建 event channel，注入各组件 |
| `trading-core/src/api/websocket.rs` | 支持交易事件推送 |
| `trading-core/src/api/server.rs` | 注入 Redis 订阅 |
| `trading-core/src/api/handlers.rs` | 新增 /api/events 端点 |

## 验证方式

```bash
# 1. 运行 SQL 迁移
psql -f version/v1.0/sql/20260717_trading_monitor_logs.sql

# 2. 编译验证
cargo check

# 3. 启动引擎后验证
# 执行一笔交易后：
SELECT * FROM trade_logs ORDER BY timestamp DESC LIMIT 10;
SELECT * FROM risk_logs ORDER BY timestamp DESC LIMIT 10;

# 4. WebSocket 验证
# 连接 ws://localhost:8080/ws，发送 SubscribeEvents，观察事件推送

# 5. API 验证
curl http://localhost:8080/api/events/trades?limit=10
curl http://localhost:8080/api/events/risk?limit=10
curl http://localhost:8080/api/events/timeline?signal_id=xxx
```
