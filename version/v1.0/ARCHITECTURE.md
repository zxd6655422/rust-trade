# 架构设计 - 服务分离方案

## 核心架构约束（强制遵守）

### 约束1：信号路径唯一性

**信号只能从策略服务发出**，策略服务是系统中唯一的信号源。

```
┌─────────────────────────────────────────────────────────────────┐
│                     信号路径（唯一）                              │
│                                                                  │
│  行情数据 ──▶ 策略服务 ──▶ Buy/Sell 信号 ──▶ 交易执行引擎        │
│                  │                                               │
│                  │ 所有数据（K线、Tick、指标）                      │
│                  │ 都在策略服务内部消化处理                         │
│                  ▼                                               │
│            信号输出（唯一出口）                                    │
└─────────────────────────────────────────────────────────────────┘

❌ 禁止：交易引擎自行从 Tick 数据生成信号
❌ 禁止：任何模块绕过策略服务直接产生交易信号
✅ 唯一：策略服务处理所有数据并输出 Buy/Sell/Hold
```

- 即使今后引入 Tick 数据获取最近一段时间的实时数据，也**必须在策略服务内消化处理**
- 交易引擎只负责：接收信号 → 校验 → 执行 → 记录
- 信号去重由策略服务保证，交易引擎不承担去重职责

### 约束2：持仓风险计算 = 实时交易所数据

**持仓风险计算必须实时从交易所获取真实持仓数据**，不得使用任何缓存或数据库数据。

```
┌─────────────────────────────────────────────────────────────────┐
│                     数据用途分层                                  │
├─────────────────────────────────────────────────────────────────┤
│                                                                  │
│  实时数据源（风险计算）          存储数据源（前端展示）             │
│  ┌─────────────────┐           ┌─────────────────┐              │
│  │ 交易所 API       │           │ PostgreSQL       │              │
│  │ get_positions()  │           │ positions 表     │              │
│  │ get_account()    │           │ trades 表        │              │
│  └────────┬────────┘           └────────┬────────┘              │
│           │                              │                       │
│           ▼                              ▼                       │
│  ┌─────────────────┐           ┌─────────────────┐              │
│  │ RiskEngine       │           │ 监控桌面应用      │              │
│  │ 风控判断          │           │ 持仓/盈亏展示    │              │
│  │ 仓位计算          │           │ 历史回溯         │              │
│  └─────────────────┘           └─────────────────┘              │
│                                                                  │
│  ⚠️ 两套数据绝不混用                                             │
└─────────────────────────────────────────────────────────────────┘
```

- 数据库存储的持仓快照**仅用于前端展示和历史回溯**，不参与任何风险计算
- 风险计算的输入**只能是**：交易所 API 返回的实时持仓 + 实时账户余额
- PortfolioManager 的 `sync_positions()` 完成后必须同步到 RiskEngine

### 约束3：交易信号执行校验链

策略服务发出 Buy/Sell 信号后，交易执行前**必须依次经过以下校验**，任何一步失败则拒绝执行：

```
┌─────────────────────────────────────────────────────────────────┐
│                    信号执行校验链（必须全部通过）                   │
├─────────────────────────────────────────────────────────────────┤
│                                                                  │
│  Step 1: 交易类型校验                                            │
│  ├── 检查配置：是否启用现货交易？是否启用合约交易？                  │
│  ├── 信号标注的交易类型 vs 实际配置                                │
│  └── 不匹配 → Reject("trading type not enabled")                │
│           │                                                      │
│           ▼                                                      │
│  Step 2: 持仓检查                                                │
│  ├── 查询交易所实时持仓（get_position）                           │
│  ├── Buy 信号：是否已有该交易对持仓？                              │
│  │   └── 已有持仓 → Reject("position already exists")            │
│  ├── Sell 信号：是否有足够持仓可平？                               │
│  │   └── 持仓不足 → Reject("insufficient position")              │
│  └── 现货/合约分别走不同校验逻辑                                   │
│           │                                                      │
│           ▼                                                      │
│  Step 3: 未成交订单检查                                           │
│  ├── 查询交易所未成交订单（get_open_orders）                       │
│  ├── 是否有同方向挂单未成交？                                      │
│  │   └── 有冲突挂单 → Reject("pending order exists")             │
│  └── 避免资金被重复占用                                           │
│           │                                                      │
│           ▼                                                      │
│  Step 4: 仓位占比检查                                             │
│  ├── 获取账户总余额（get_account）                                │
│  ├── 计算：本次开仓价值 / 账户总余额                               │
│  ├── 计算：当前总持仓价值 / 账户总余额                             │
│  └── 超过配置阈值 → Reject("position limit exceeded")            │
│           │                                                      │
│           ▼                                                      │
│  ✅ 全部通过 → Allow → 执行下单                                   │
└─────────────────────────────────────────────────────────────────┘
```

**现货 vs 合约校验差异：**

| 校验项 | 现货 (Spot) | 合约 (Futures) |
|--------|------------|----------------|
| 持仓查询 | 查 spot balance (base_asset) | 查 `get_position()` 合约持仓 |
| 余额查询 | 查 spot free balance | 查合约 available balance |
| 卖出校验 | base_asset free >= quantity | position.quantity >= quantity |
| 仓位占比 | 持仓价值 / 总资产 | 持仓价值 / 合约账户权益 |

---

## 信号生命周期管理

### 概述

信号服务（strategy-service）负责信号的完整生命周期管理，包括：
- **信号去重**：避免短时间内产生重复信号
- **方向反转检测**：当新信号与旧信号方向相反时，自动关闭旧信号
- **状态流转**：pending → executed/superseded/expired

### 信号状态定义

```
┌─────────────────────────────────────────────────────────────────┐
│                    信号状态流转图                                  │
├─────────────────────────────────────────────────────────────────┤
│                                                                  │
│  ┌─────────┐                                                     │
│  │ pending │ ──── 初始状态，等待执行                               │
│  └────┬────┘                                                     │
│       │                                                          │
│       ├─── 执行下单 ───▶ ┌───────────┐                           │
│       │                  │ executed  │ 已执行                     │
│       │                  └───────────┘                           │
│       │                                                          │
│       ├─── 方向反转 ───▶ ┌───────────┐                           │
│       │                  │superseded│ 被取代（记录收益率）         │
│       │                  └───────────┘                           │
│       │                                                          │
│       └─── 超时(1h) ───▶ ┌───────────┐                           │
│                          │  expired  │ 已过期                     │
│                          └───────────┘                           │
│                                                                  │
└─────────────────────────────────────────────────────────────────┘
```

### 信号去重策略

```
新信号产生
    │
    ▼
┌─────────────────────────────────────────┐
│         should_skip_signal() 检查        │
├─────────────────────────────────────────┤
│                                          │
│  1. 基础冷却期：5分钟内跳过所有信号        │
│     └── 无论方向，防止频繁交易             │
│                                          │
│  2. 同方向延长冷却：15分钟                 │
│     └── bullish → bullish: 15分钟内跳过   │
│     └── bearish → bearish: 15分钟内跳过   │
│                                          │
│  3. 反向信号：允许立即生成                 │
│     └── bullish → bearish: 5分钟后允许    │
│     └── bearish → bullish: 5分钟后允许    │
│                                          │
└─────────────────────────────────────────┘
```

### 方向反转处理流程

```
┌─────────────────────────────────────────────────────────────────┐
│                    方向反转处理流程                                │
├─────────────────────────────────────────────────────────────────┤
│                                                                  │
│  策略分析产生新信号                                                │
│         │                                                        │
│         ▼                                                        │
│  ┌─────────────────┐                                             │
│  │ 获取活跃信号      │  查询 status IN ('pending', 'executed')    │
│  │ get_active_signals│                                           │
│  └────────┬────────┘                                             │
│           │                                                      │
│           ▼                                                      │
│  ┌─────────────────┐                                             │
│  │ 检测方向反转      │  bullish ↔ bearish                         │
│  └────────┬────────┘                                             │
│           │                                                      │
│           ├─ 是 ──▶ ┌─────────────────────────────────────┐      │
│           │         │ 1. 计算旧信号收益率                    │      │
│           │         │    return_pct = calc_return_pct()    │      │
│           │         │                                      │      │
│           │         │ 2. 关闭旧信号为 superseded            │      │
│           │         │    supersede_signal(                  │      │
│           │         │      close_price = current_price,    │      │
│           │         │      actual_return_pct = return_pct  │      │
│           │         │    )                                  │      │
│           │         │                                      │      │
│           │         │ 3. 记录日志                           │      │
│           │         │    "🔄 Signal superseded: ..."       │      │
│           │         └─────────────────────────────────────┘      │
│           │                                                      │
│           └─ 否 ──▶ 继续生成新信号                                │
│                                                                  │
└─────────────────────────────────────────────────────────────────┘
```

### 价格记录策略

| 场景 | 价格来源 | 说明 |
|------|----------|------|
| 关闭旧信号 | `market_data.current_price` | 最新K线收盘价（对应时间框架） |
| 新信号入场 | `signal.entry_price` | 策略分析时的价格 |
| 收益率计算 | `calc_return_pct()` | 基于入场价和当前价 |

**价格链路：**
```
Redis K线数据 → market_data.current_price → 关闭旧信号（close_price）
```

### 收益率计算

```rust
fn calc_return_pct(direction: &str, entry_price: Decimal, current_price: Decimal) -> Decimal {
    let pct = (current_price - entry_price) / entry_price * 100;
    match direction {
        "bullish" => pct,   // 多头：涨=正收益
        "bearish" => -pct,  // 空头：跌=正收益
        _ => Decimal::ZERO,
    }
}
```

### 代码位置

| 文件 | 函数 | 职责 |
|------|------|------|
| `strategy-service/src/engine.rs` | `should_skip_signal()` | 信号去重逻辑 |
| `strategy-service/src/engine.rs` | `process_strategy()` | 方向反转检测 |
| `strategy-service/src/engine.rs` | `calc_return_pct()` | 收益率计算 |
| `strategy-service/src/db/signals.rs` | `get_active_signals()` | 查询活跃信号 |
| `strategy-service/src/db/signals.rs` | `get_last_signal()` | 获取最近信号 |
| `strategy-service/src/db/signals.rs` | `supersede_signal()` | 关闭旧信号 |

### 数据库验证

```sql
-- 查看信号状态分布
SELECT status, COUNT(*) FROM strategy_signals GROUP BY status;

-- 查看被取代的信号及收益率
SELECT 
    id, symbol, direction, 
    entry_price, close_price, 
    actual_return_pct, closed_at,
    closed_reason
FROM strategy_signals 
WHERE status = 'superseded' 
ORDER BY closed_at DESC;

-- 查看某个交易对的信号历史
SELECT 
    id, direction, status,
    entry_price, close_price,
    actual_return_pct, created_at, closed_at
FROM strategy_signals 
WHERE symbol = 'BTCUSDT'
ORDER BY created_at DESC;
```

---

## 核心设计原则

### 1. 职责分离
```
Data Collector: 只做数据采集（只读）
Trading Engine: 只做交易执行（读写）
```

### 2. 风险隔离
```
Data Collector 崩溃 → 不影响资金安全
Trading Engine 崩溃 → 数据采集继续运行
```

### 3. 安全分层
```
Data Collector: 无需 API Key
Trading Engine: 存储 API Key，权限受限
```

---

## 服务交互流程

### 交易引擎主循环（SignalPoller）

交易引擎职责最小化，只有一个主循环：**SignalPoller**

```
┌─────────────────────────────────────────────────────────────┐
│                    SignalPoller 主循环                        │
├─────────────────────────────────────────────────────────────┤
│                                                              │
│  定时任务：                                                   │
│  ├── 每 5s   轮询 strategy_signals 表 → 执行交易              │
│  ├── 每 5s   检查止损止盈（交易所获取实时价格）                  │
│  ├── 每 5min 同步持仓到 RiskEngine                            │
│  └── 每 1h   清理过期信号                                     │
│                                                              │
│  信号执行流程：                                                │
│  ┌─────────────┐    ┌─────────────┐    ┌─────────────┐       │
│  │ 读取信号     │───▶│ 查询市场数据 │───▶│ 校验执行     │       │
│  │ (DB 轮询)   │    │ (交易所API)  │    │ (OrderMgr)  │       │
│  └─────────────┘    └─────────────┘    └─────────────┘       │
│                                                              │
│  不做的事：                                                   │
│  ✗ 不订阅行情数据（策略服务负责）                               │
│  ✗ 不缓存价格到 Redis（策略服务负责）                           │
│  ✗ 不做持仓对账（监控功能，独立服务）                            │
└─────────────────────────────────────────────────────────────┘
```

### 订单执行流程

```
SignalPoller 读到 Buy/Sell 信号
       │
       ▼
┌─────────────────┐
│  OrderManager    │
│  execute_signal()│
└────────┬────────┘
         │
         ├── 1. 从交易所查询实时市场数据
         │   ├── get_ticker()   → 最新价格、买一/卖一
         │   ├── get_depth()    → 盘口深度
         │   └── get_account()  → 账户余额
         │
         ├── 2. 校验链
         │   ├── 交易类型是否启用（现货/合约）
         │   ├── 持仓检查（交易所实时数据）
         │   ├── 未成交订单检查
         │   └── 仓位占比检查
         │
         ├── 3. RiskEngine.check_order()
         │   ├── 熔断检查
         │   ├── 日亏损限制
         │   ├── 最大回撤
         │   ├── 单笔仓位大小
         │   ├── 总曝光度
         │   ├── 黑天鹅检测
         │   └── Kelly 仓位调整
         │
         ├── 4. 交易所 API 下单
         │
         └── 5. 更新交易记录
```

### 异常处理流程

```
┌─────────────────────────────────────────────────────────────┐
│ 2. 风控触发流程                                                │
└─────────────────────────────────────────────────────────────┘

Trading Engine 检测到异常
       │
       ▼
┌─────────────────┐
│  Risk Engine     │
│  (触发风控规则)   │
└────────┬────────┘
         │
         ├──▶ 拒绝订单 (RiskDecision::Reject)
         │
         ├──▶ 修改仓位 (RiskDecision::Modify)
         │
         └──▶ 触发熔断 (Circuit Breaker)
                │
                ▼
┌─────────────────┐
│  Risk Log        │
│  (记录到数据库)   │
└────────┬────────┘
         │
         └──▶ 告警通知 (可选)
```

---

## 数据流设计

### 实时数据流

```
Binance WebSocket (公共流)
       │
       │ TickData { symbol, price, quantity, timestamp }
       │
       ▼
┌──────────────────────────────────────────────────────────────┐
│                    Data Collector                             │
│  ┌─────────────┐    ┌─────────────┐    ┌─────────────┐       │
│  │  接收数据    │───▶│  写入 Batch  │───▶│  批量写入    │       │
│  │  (WebSocket) │    │  (内存缓冲)  │    │  PostgreSQL  │       │
│  └─────────────┘    └─────────────┘    └─────────────┘       │
│                           │                                  │
│                           ▼                                  │
│                    ┌─────────────┐                            │
│                    │  Redis 缓存  │                            │
│                    │  (实时数据)  │                            │
│                    └─────────────┘                            │
└──────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌──────────────────────────────────────────────────────────────┐
│                    Trading Engine                              │
│  ┌─────────────┐    ┌─────────────┐    ┌─────────────┐       │
│  │  读取行情    │───▶│  策略计算    │───▶│  风控检查    │       │
│  │  (Redis)    │    │  (Signal)   │    │  (Decision) │       │
│  └─────────────┘    └─────────────┘    └─────────────┘       │
│                                                │              │
│                                                ▼              │
│                                         ┌─────────────┐       │
│                                         │  下单执行    │       │
│                                         │  (REST API) │       │
│                                         └─────────────┘       │
└──────────────────────────────────────────────────────────────┘
```

### 订单状态流

```
┌─────────────────────────────────────────────────────────────┐
│ 3. 订单生命周期                                                │
└─────────────────────────────────────────────────────────────┘

Signal::Buy { symbol: "BTCUSDT", quantity: 0.01 }
       │
       ▼
┌─────────────────┐
│  RiskEngine      │
│  check_order()   │──▶ RiskDecision::Allow
└────────┬────────┘
         │
         ▼
┌─────────────────┐
│  OrderManager    │
│  execute_signal()│
└────────┬────────┘
         │
         ▼
┌─────────────────┐
│  Exchange API    │
│  place_order()   │──▶ OrderResult { order_id, status: "Pending" }
└────────┬────────┘
         │
         │ WebSocket 用户数据流
         │
         ▼
┌─────────────────┐
│  OrderTracker    │
│  handle_update() │──▶ OrderStatus::PartiallyFilled (50%)
└────────┬────────┘
         │
         ▼
┌─────────────────┐
│  OrderTracker    │
│  handle_update() │──▶ OrderStatus::Filled (100%)
└────────┬────────┘
         │
         ▼
┌─────────────────┐
│  StopLossManager │
│  setup_stop()    │──▶ 设置止损止盈价格
└─────────────────┘
```

---

## 关键接口设计

### Exchange Trait (扩展版)

```rust
#[async_trait]
pub trait Exchange: Send + Sync {
    // ===== 行情接口 (只读) =====

    /// 订阅实时行情 (WebSocket)
    async fn subscribe_trades(
        &self,
        symbols: &[String],
        callback: Box<dyn Fn(TickData) + Send + Sync>,
        shutdown_rx: broadcast::Receiver<()>,
    ) -> Result<(), ExchangeError>;

    // ===== 账户接口 =====

    /// 获取账户信息
    async fn get_account(&self) -> Result<AccountInfo, ExchangeError>;

    /// 获取持仓信息
    async fn get_position(&self, symbol: &str) -> Result<PositionInfo, ExchangeError>;

    /// 获取所有持仓
    async fn get_positions(&self) -> Result<Vec<PositionInfo>, ExchangeError>;

    // ===== 订单接口 =====

    /// 下单
    async fn place_order(&self, order: OrderRequest) -> Result<OrderResult, ExchangeError>;

    /// 撤单
    async fn cancel_order(&self, symbol: &str, order_id: &str) -> Result<(), ExchangeError>;

    /// 获取未成交订单
    async fn get_open_orders(&self, symbol: Option<&str>) -> Result<Vec<OrderInfo>, ExchangeError>;

    /// 获取订单详情
    async fn get_order(&self, symbol: &str, order_id: &str) -> Result<OrderInfo, ExchangeError>;

    // ===== 元信息 =====

    /// 交易所 ID
    fn exchange_id(&self) -> &str;

    /// 是否测试网
    fn is_testnet(&self) -> bool;

    /// 获取时间戳
    async fn get_server_time(&self) -> Result<DateTime<Utc>, ExchangeError>;
}
```

### RiskEngine 接口

```rust
pub struct RiskEngine {
    config: RiskConfig,
    state: Arc<Mutex<RiskState>>,
}

impl RiskEngine {
    /// 核心方法：检查订单是否允许执行
    pub async fn check_order(
        &self,
        order: &OrderRequest,
        account: &AccountInfo,
    ) -> Result<RiskDecision, RiskError>;

    /// 更新市场数据（用于波动率计算）
    pub async fn update_market_data(&self, symbol: &str, price: Decimal);

    /// 更新交易结果（用于 Kelly 公式）
    pub async fn record_trade_result(&self, trade: &TradeResult);

    /// 获取风控状态
    pub async fn get_status(&self) -> RiskStatus;

    /// 手动触发熔断
    pub async fn trigger_circuit_breaker(&self, reason: &str);

    /// 重置日统计
    pub async fn reset_daily_stats(&self);
}

pub enum RiskDecision {
    /// 允许执行
    Allow,
    /// 拒绝执行，附带原因
    Reject(String),
    /// 允许但修改数量
    Modify(Decimal),
}
```

### OrderManager 接口

```rust
pub struct OrderManager {
    exchange: Arc<dyn Exchange>,
    risk_engine: Arc<RiskEngine>,
    repository: Arc<TickDataRepository>,
    active_orders: Arc<Mutex<HashMap<String, OrderInfo>>>,
    stop_loss_manager: Arc<StopLossManager>,
}

impl OrderManager {
    /// 执行交易信号
    pub async fn execute_signal(&self, signal: Signal) -> Result<OrderResult, OrderError>;

    /// 处理订单状态更新
    pub async fn handle_order_update(&self, update: OrderUpdate);

    /// 获取活动订单
    pub async fn get_active_orders(&self) -> Vec<OrderInfo>;

    /// 取消所有订单
    pub async fn cancel_all_orders(&self) -> Result<(), OrderError>;

    /// 紧急停止
    pub async fn emergency_stop(&self) -> Result<(), OrderError>;
}
```

---

## 部署拓扑

### 单机部署 (推荐初期)

```
┌─────────────────────────────────────────────────────────────┐
│                    云服务器 (4 核 8G)                         │
├─────────────────────────────────────────────────────────────┤
│                                                              │
│  Port 5432: PostgreSQL                                       │
│  Port 6379: Redis                                            │
│                                                              │
│  systemd services:                                           │
│  ├── trading-collector.service   (数据采集)                   │
│  ├── trading-engine.service      (交易引擎)                   │
│  ├── postgresql.service          (数据库)                     │
│  └── redis-server.service        (缓存)                      │
│                                                              │
└─────────────────────────────────────────────────────────────┘
```

### 分布式部署 (未来扩展)

```
┌──────────────────┐     ┌──────────────────┐
│   Server 1       │     │   Server 2       │
│   (数据采集)      │     │   (交易引擎)      │
├──────────────────┤     ├──────────────────┤
│ trading-collector│     │ trading-engine   │
│ PostgreSQL       │◀───▶│ Redis            │
│ Redis            │     │                  │
└──────────────────┘     └──────────────────┘
         │                        │
         └────────┬───────────────┘
                  │
                  ▼
         ┌──────────────────┐
         │   Server 3       │
         │   (监控仪表盘)    │
         ├──────────────────┤
         │ Web Dashboard    │
         │ Grafana          │
         └──────────────────┘
```

---

## 安全设计

### API Key 安全

```
.env 文件 (权限 600，仅 owner 可读)
├── BINANCE_API_KEY=xxx
├── BINANCE_API_SECRET=xxx
└── BINANCE_TESTNET=true

API Key 限制：
├── 只允许交易 (Spot Trading)
├── 禁止提币 (Withdraw)
├── IP 白名单 (可选)
└── 仅限 Testnet (初期)
```

### 服务隔离

```
Data Collector:
├── 无需 API Key
├── 只读操作
├── 独立进程
└── 独立日志

Trading Engine:
├── 需要 API Key
├── 读写操作
├── 独立进程
├── 独立日志
└── 独立用户运行 (非 root)
```

### 故障隔离

```
场景 1: Data Collector 崩溃
├── Trading Engine 继续运行
├── 使用 Redis 缓存的最后数据
├── systemd 自动重启 Data Collector
└── 数据恢复后自动追上

场景 2: Trading Engine 崩溃
├── Data Collector 继续运行
├── 订单状态与交易所对账
├── systemd 自动重启 Trading Engine
└── 未完成订单继续追踪

场景 3: PostgreSQL 崩溃
├── 两个服务都暂停
├── Redis 缓存继续工作
├── 数据库恢复后自动重连
└── 补写丢失的数据

场景 4: Redis 崩溃
├── Trading Engine 降级运行
├── 直接查询 PostgreSQL
├── 性能下降但功能正常
└── Redis 恢复后自动重连
```

---

## 监控设计

### 关键指标

```
Data Collector:
├── 行情接收延迟 (ms)
├── 数据写入 QPS
├── 批次大小分布
├── 重试次数
└── 连接状态

Trading Engine:
├── 策略计算延迟 (ms)
├── 风控检查延迟 (ms)
├── 下单延迟 (ms)
├── 成功率 (%)
├── 当前持仓价值
├── 未实现盈亏
├── 日交易次数
└── 风控触发次数
```

### 告警规则

```
Critical (立即处理):
├── 交易执行失败
├── 风控熔断触发
├── 服务连续崩溃
└── API Key 无效

Warning (关注):
├── 下单延迟 > 1s
├── 风控规则频繁触发
├── 日亏损接近限制
└── 持仓接近上限

Info (记录):
├── 服务重启
├── 策略切换
├── 配置变更
└── 日交易汇总
```

---

## 总结

### 服务分离的优势

1. **安全性**：API Key 只在交易服务中
2. **稳定性**：数据采集不受交易影响
3. **可维护性**：职责清晰，易于调试
4. **可扩展性**：可独立扩展、部署
5. **故障隔离**：单点故障不影响整体

### 开发顺序

1. 先完成 Data Collector (已有)
2. 再开发 Trading Engine (核心)
3. 最后集成测试 + 部署

### 关键里程碑

- Week 1: Trading Engine 基础框架
- Week 2: 订单管理系统
- Week 3: 风控系统
- Week 4: 策略集成
- Week 5: 部署上线
