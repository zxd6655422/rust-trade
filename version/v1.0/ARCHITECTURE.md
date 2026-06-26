# 架构设计 - 服务分离方案

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

### 正常交易流程

```
┌─────────────────────────────────────────────────────────────┐
│ 1. 行情数据流                                                │
└─────────────────────────────────────────────────────────────┘

Binance WebSocket
       │
       ▼
┌─────────────────┐
│  Data Collector  │
│  (trading-core)  │
└────────┬────────┘
         │
         ├──▶ PostgreSQL (tick_data 表)
         │
         └──▶ Redis (实时缓存)
                │
                ▼
┌─────────────────┐
│  Trading Engine  │
│  (trading-engine)│
└────────┬────────┘
         │
         │ 读取 Redis 缓存的实时行情
         │
         ▼
┌─────────────────┐
│  Strategy Engine │
│  (策略计算)       │
└────────┬────────┘
         │
         │ 生成 Signal (Buy/Sell/Hold)
         │
         ▼
┌─────────────────┐
│  Risk Engine     │
│  (风控检查)       │
└────────┬────────┘
         │
         │ RiskDecision (Allow/Reject/Modify)
         │
         ▼
┌─────────────────┐
│  Order Manager   │
│  (订单管理)       │
└────────┬────────┘
         │
         ├──▶ Binance REST API (下单)
         │
         └──▶ PostgreSQL (orders 表)
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
