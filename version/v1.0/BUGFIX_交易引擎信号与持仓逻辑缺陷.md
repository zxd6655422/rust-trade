# 交易引擎信号去重与持仓逻辑缺陷

## 问题概述

交易引擎 `trading-engine` 存在 **信号重复执行**、**持仓状态不一致**、**Sell校验不适配合约** 三类问题。

---

## 问题1：信号重复执行（P0）

### 问题描述

交易引擎存在两条并行信号路径，且均无有效去重机制：

```
路径A: DB(strategy_signals表) → SignalPoller → OrderManager → 交易所
路径B: WebSocket/Tick数据 → TradingLoop.Strategy.on_tick() → OrderManager → 交易所
```

`main.rs:167-177` 中两者同时启动，共享同一个 `OrderManager`。

### 影响

- **TradingLoop 路径**：每个 tick 调用 `strategy.on_tick(tick)`，连续 tick 返回相同信号时会重复下单
- **路径冲突**：SignalPoller 和 TradingLoop 可能同时对同一 symbol 发出方向相反的指令，无互斥机制
- 可能导致重复开仓、超额持仓、资金快速消耗

### 代码位置

**TradingLoop 无去重：**

`trading-engine/src/engine/trading_loop.rs:406-449`

```rust
// 每个tick都执行，无任何去重逻辑
let signal = self.strategy.borrow_mut().on_tick(tick);
match &signal {
    Signal::Buy { .. } => {
        self.order_manager.execute_signal(signal).await // 直接执行
    }
    Signal::Sell { .. } => {
        self.order_manager.execute_signal(signal).await // 直接执行
    }
    Signal::Hold => {}
}
```

**SignalPoller 路径（有去重，无问题）：**

`trading-engine/src/engine/signal_poller.rs:144-149`

```rust
// 数据库层面有 status='pending' 过滤，执行后更新为 executed/rejected
"SELECT ... FROM strategy_signals WHERE status='pending' AND entry_allowed=true ..."
```

### 修复方案

**方案A（推荐）：TradingLoop 移除策略执行，统一走 SignalPoller**

TradingLoop 只负责：
1. 行情数据推送与缓存
2. 持仓价格更新
3. 止损止盈检查

策略信号统一由 SignalPoller 从数据库读取执行，避免两套路径冲突。

**方案B：TradingLoop 增加去重**

在 `process_tick` 中记录每个 symbol 最后执行的方向和时间，短时间内相同方向不重复执行：

```rust
last_signals: Arc<Mutex<HashMap<String, (SignalDirection, Instant)>>>
```

---

## 问题2：RiskEngine 与 PortfolioManager 持仓状态不一致（P1）

### 问题描述

两套系统各自独立维护持仓数据，互不同步：

- **RiskEngine**：通过 `record_trade_result()` 在内存中维护 `positions`（`risk/engine.rs:268-295`）
- **PortfolioManager**：通过 `sync_positions()` 从交易所同步持仓（`portfolio/manager.rs:55-115`）

### 影响

- RiskEngine 的持仓可能与交易所实际持仓不一致
- 风控判断（最大持仓、总曝光度等）基于不准确的数据
- 可能误拒合法订单或放行超限订单

### 代码位置

`trading-engine/src/risk/engine.rs:268-295` — RiskEngine 独立维护持仓：

```rust
pub async fn record_trade_result(&self, symbol: &str, side: &str, quantity: Decimal, price: Decimal) {
    // 独立更新 RiskEngine 内部的 positions，与 PortfolioManager 无关联
    if side == "BUY" {
        let position = state.positions.entry(symbol.to_string()).or_insert_with(|| ...);
        position.quantity += quantity;
    } else if side == "SELL" {
        if let Some(position) = state.positions.get_mut(symbol) {
            position.quantity -= quantity;
        }
    }
}
```

`trading-engine/src/portfolio/manager.rs:55-115` — PortfolioManager 从交易所同步：

```rust
pub async fn sync_positions(&self) -> Result<usize, PortfolioError> {
    let exchange_positions = self.exchange.get_positions().await...;
    // 独立维护，不通知 RiskEngine
}
```

### 修复方案

在 `PortfolioManager.sync_positions()` 完成后，将持仓数据同步到 RiskEngine：

```rust
// portfolio/manager.rs — sync_positions 增加回调
pub async fn sync_positions(&self, risk_engine: &RiskEngine) -> Result<usize, PortfolioError> {
    // ... 现有同步逻辑 ...

    // 同步到风控引擎
    risk_engine.sync_positions_from_exchange(&positions).await;

    Ok(exchange_positions.len())
}
```

RiskEngine 增加接收外部持仓同步的方法：

```rust
// risk/engine.rs
pub async fn sync_positions_from_exchange(&self, positions: &HashMap<String, PositionSnapshot>) {
    let mut state = self.state.lock().await;
    state.positions.clear();
    for (symbol, pos) in positions {
        state.positions.insert(symbol.clone(), PositionSnapshot {
            symbol: pos.symbol.clone(),
            quantity: pos.quantity,
            avg_entry_price: pos.avg_entry_price,
            current_price: pos.current_price,
            unrealized_pnl: pos.unrealized_pnl,
        });
    }
}
```

---

## 问题3：Sell 信号持仓校验不适配合约交易（P2）

### 问题描述

`OrderManager.build_order_request()` 中 Sell 信号的持仓校验从 symbol 提取 base_asset 查 spot 余额，不适用于合约交易。

### 影响

- 合约持仓不在 spot balance 中，校验会误报 `InsufficientPosition`
- Sell 信号被错误拒绝，无法平仓

### 代码位置

`trading-engine/src/order/manager.rs:369-389`

```rust
Signal::Sell { symbol, quantity, entry_price } => {
    // 从 symbol 提取 base_asset（如 BTCUSDT -> BTC），查 spot 余额
    let base_asset = if symbol.ends_with("USDT") {
        symbol.strip_suffix("USDT").unwrap_or(symbol)
    } else { ... };

    let balance = account.balances.iter()
        .find(|b| b.asset == base_asset)
        .map(|b| b.free)
        .unwrap_or(Decimal::ZERO);

    if *quantity > balance {
        return Err(OrderError::InsufficientPosition(...)); // 合约场景下必然失败
    }
}
```

### 修复方案

根据交易模式区分校验逻辑：

- **Spot 模式**：保持现有逻辑，查 spot balance
- **Futures 模式**：从 `get_positions()` 查询合约持仓数量进行校验

```rust
Signal::Sell { symbol, quantity, entry_price } => {
    if self.is_futures_mode {
        // 合约模式：从持仓中校验
        let position = self.exchange.get_position(symbol).await?;
        if quantity > position.quantity {
            return Err(OrderError::InsufficientPosition(...));
        }
    } else {
        // 现有 spot 逻辑
    }
}
```

---

## 问题优先级

| 编号 | 问题 | 优先级 | 影响范围 |
|------|------|--------|----------|
| 1 | 信号重复执行 | P0 | 资金安全，可能造成超额持仓 |
| 2 | 持仓状态不一致 | P1 | 风控准确性 |
| 3 | Sell 校验不适配合约 | P2 | 合约平仓功能 |

---

## 相关文件

- `trading-engine/src/engine/trading_loop.rs` — 交易循环
- `trading-engine/src/engine/signal_poller.rs` — 信号轮询器
- `trading-engine/src/order/manager.rs` — 订单管理器
- `trading-engine/src/risk/engine.rs` — 风控引擎
- `trading-engine/src/portfolio/manager.rs` — 持仓管理器
- `trading-engine/src/main.rs` — 入口文件
