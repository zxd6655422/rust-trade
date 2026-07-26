# Strategy Service 数据层重构 - 实现方案

## 背景

当前 strategy-service 依赖 trading-core → PostgreSQL → Redis 这条长链路获取K线数据。链路中任何一环出问题（采集挂了、聚合 bug、DB 慢、Redis 同步失败），策略就会因为"数据过旧"而停止执行。

2026-07-27 凌晨再次出现此问题：3 个币种（BTC/ETH/SOL）的 5m 数据延迟从 10 分钟持续增长到 13 分钟，策略完全瘫痪。根因是 trading-core 的数据采集管道断流。

## 核心问题

1. **策略服务对数据管道强依赖** — 采集、聚合、DB、Redis 任一环节故障 → 策略停摆
2. **聚合 volume 累加 bug** — ON CONFLICT 用 `volume = existing + new` 导致每次增量聚合 volume 翻倍（已修复）
3. **5m 数据依赖聚合** — 不是直接从交易所获取，多一层故障点
4. **无实时数据源** — 依赖轮询，延迟高

## 设计目标

- strategy-service 自主管理K线数据，不依赖 trading-core/Redis
- 启动时快速加载足够历史数据
- 运行时通过 WebSocket 实时更新
- 断连自动恢复，保证内存数据完整
- 支持不同策略的不同数据量需求

---

## 架构设计

### 数据流对比

```
【旧架构】
Binance API → trading-core → PostgreSQL → 聚合 → PostgreSQL → Redis → strategy-service
                                ↑ 任何一环断了，策略就废了

【新架构】
Binance REST (启动时) ──┐
                        ├─→ KlineStore (内存) → 策略引擎
Binance WS (实时)  ────┘
                        ↑ 策略服务自给自足
```

### 模块拆分

```
strategy-service/
├── src/
│   ├── main.rs
│   ├── engine.rs              # 策略引擎（现有，改造）
│   ├── kline_store.rs         # 【新建】内存K线存储
│   ├── kline_loader.rs        # 【新建】混合加载器（DB + 交易所）
│   ├── ws_feed.rs             # 【新建】WebSocket 实时数据源
│   ├── gap_detector.rs        # 【新建】间隙检测与恢复
│   └── redis_reader.rs        # 【保留】DB 查询辅助
```

---

## KlineStore 设计

### 核心结构

```rust
pub struct KlineStore {
    symbol: String,
    timeframe: Timeframe,
    bars: VecDeque<KlineBar>,     // 滚动窗口
    max_size: usize,              // 最大容量（默认 1000）
    last_update: DateTime<Utc>,   // 最后更新时间
}

pub struct KlineBar {
    pub open_time: i64,           // 毫秒时间戳
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
    pub volume: f64,
    pub closed: bool,             // 是否已完成
}
```

### 操作

```rust
impl KlineStore {
    /// 追加已完成的K线（前端截断）
    pub fn push_closed(&mut self, bar: KlineBar) {
        debug_assert!(bar.closed);
        self.bars.push_back(bar);
        if self.bars.len() > self.max_size {
            self.bars.pop_front();
        }
    }

    /// 更新当前未完成的K线（最后一根）
    pub fn update_current(&mut self, bar: KlineBar) {
        debug_assert!(!bar.closed);
        if let Some(last) = self.bars.back_mut() {
            if !last.closed {
                *last = bar;  // 覆盖未完成的
            } else {
                self.bars.push_back(bar);  // 新的一根未完成
            }
        }
    }

    /// 获取最近 N 根已完成K线（供策略计算）
    pub fn closed_bars(&self, n: usize) -> Vec<&KlineBar> {
        self.bars.iter()
            .filter(|b| b.closed)
            .rev()
            .take(n)
            .rev()
            .collect()
    }

    /// 检查是否有足够数据
    pub fn has_enough(&self, required: usize) -> bool {
        self.bars.iter().filter(|b| b.closed).count() >= required
    }

    /// 最新已完成K线的时间戳
    pub fn latest_closed_time(&self) -> Option<i64> {
        self.bars.iter().rev().find(|b| b.closed).map(|b| b.open_time)
    }
}
```

### 全局管理

```rust
pub struct KlineManager {
    stores: HashMap<(String, Timeframe), KlineStore>,
    max_bars: usize,  // 全局配置，默认 1000
}

impl KlineManager {
    /// 获取指定 symbol+timeframe 的 store
    pub fn get(&self, symbol: &str, tf: Timeframe) -> Option<&KlineStore> {
        self.stores.get(&(symbol.to_string(), tf))
    }

    /// 启动时创建所有需要的 store
    pub fn init_stores(&mut self, pairs: Vec<(String, Timeframe)>) {
        for (symbol, tf) in pairs {
            self.stores.insert(
                (symbol.clone(), tf),
                KlineStore::new(symbol, tf, self.max_bars),
            );
        }
    }
}
```

---

## 混合加载策略

### 流程

```
启动
  │
  ├─ 1. 查询 DB 获取所有 active 策略
  ├─ 2. 收集所有 (symbol, timeframe) 对，去重
  │     → {(BTC, 30m), (BTC, 5m), (ETH, 30m), (ETH, 5m), (SOL, 30m), (SOL, 5m)}
  ├─ 3. 计算每个对需要的K线数（取所有策略最大值）
  │     → max_bars = max(498, ...) = 498，向上取整到 1000
  │
  ├─ 4. 对每个 (symbol, timeframe):
  │     ├─ 从 DB 加载 max_bars 根历史K线
  │     ├─ 检查 DB 最新时间 vs 交易所最新时间
  │     ├─ 如果有缺口：从交易所补拉缺口部分
  │     └─ 如果 DB 无数据：从交易所全量加载
  │
  └─ 5. 建立 WebSocket 订阅
```

### DB 加载

```rust
async fn load_from_db(
    pool: &PgPool,
    symbol: &str,
    tf: &str,
    limit: usize,
) -> Result<Vec<KlineBar>> {
    // 对应表：kline_5m, kline_30m 等
    let table = format!("kline_{}", tf);
    sqlx::query_as::<_, KlineBar>(&format!(
        "SELECT open_time, open, high, low, close, volume
         FROM {table}
         WHERE symbol = $1
         ORDER BY open_time DESC
         LIMIT $2"
    ))
    .bind(symbol)
    .bind(limit as i64)
    .fetch_all(pool)
    .await
    .map(|mut rows| { rows.reverse(); rows })  // 按时间正序
}
```

### 交易所补拉

```rust
async fn fill_gap_from_exchange(
    client: &BinanceClient,
    symbol: &str,
    interval: &str,
    after_time: i64,        // DB 最新时间之后
    needed: usize,          // 需要补多少根
) -> Result<Vec<KlineBar>> {
    client.fetch_klines(symbol, interval, needed.min(1000), Some(after_time + 1)).await
}
```

### 无 DB 数据时的全量加载

```rust
async fn load_full_from_exchange(
    client: &BinanceClient,
    symbol: &str,
    interval: &str,
    required: usize,
) -> Result<Vec<KlineBar>> {
    let mut all = Vec::new();
    let mut end_time: Option<i64> = None;

    while all.len() < required {
        let limit = (required - all.len()).min(1000);
        let batch = client.fetch_klines(symbol, interval, limit, end_time).await;
        if batch.is_empty() { break; }
        end_time = Some(batch.first().unwrap().open_time - 1);
        all.splice(0..0, batch);
    }

    if all.len() > required {
        all.drain(0..all.len() - required);
    }
    Ok(all)
}
```

---

## WebSocket 实时数据源

### 订阅管理

```rust
pub struct WsFeed {
    subscriptions: Vec<(String, Timeframe)>,
    sender: broadcast::Sender<KlineEvent>,
}

pub struct KlineEvent {
    pub symbol: String,
    pub timeframe: Timeframe,
    pub bar: KlineBar,
}
```

### 连接策略

```
WebSocket 连接
  │
  ├─ 订阅格式: btcusdt@kline_30m, btcusdt@kline_5m, ...
  ├─ 心跳: 每 30 秒检查连接状态
  │
  ├─ 断连处理:
  │   ├─ 自动重连（指数退避: 1s → 2s → 4s → ... → 30s max）
  │   ├─ 重连成功后：
  │   │   ├─ REST 拉最近 1000 根替换内存（简单可靠）
  │   │   └─ 或只填补间隙（省 API 配额）
  │   └─ 重连失败超过阈值 → 告警
  │
  └─ 消息处理:
      ├─ k.x = true (K线关闭) → store.push_closed(bar)
      └─ k.x = false (进行中) → store.update_current(bar)
```

### 间隙检测

```rust
pub fn detect_gap(store: &KlineStore, new_bar: &KlineBar) -> Option<GapInfo> {
    let last_time = store.latest_closed_time()?;
    let expected_next = last_time + store.timeframe.duration_ms();

    if new_bar.open_time > expected_next + store.timeframe.duration_ms() {
        Some(GapInfo {
            from: expected_next,
            to: new_bar.open_time,
            missing_bars: ((new_bar.open_time - expected_next) / store.timeframe.duration_ms()) as usize,
        })
    } else {
        None
    }
}
```

发现间隙时，触发 REST 补拉：

```rust
async fn fill_gap(client: &BinanceClient, store: &mut KlineStore, gap: GapInfo) {
    let bars = client.fetch_klines(
        &store.symbol,
        &store.timeframe.as_str(),
        gap.missing_bars + 10,  // 多拉几根保险
        Some(gap.from),
    ).await;

    for bar in bars {
        store.push_closed(bar);
    }
}
```

---

## 策略执行改造

### engine.rs 改造要点

```rust
// 旧：从 Redis 读取
// let klines = redis_reader.get_klines(symbol, timeframe, 500).await?;

// 新：从内存 Store 读取
let store = kline_manager.get(symbol, timeframe).unwrap();
let bars = store.closed_bars(required_bars);

if bars.len() < required_bars {
    warn!("[{}] 数据不足: 需要 {} 根，实际 {} 根", symbol, required_bars, bars.len());
    return;
}

let market_data = MarketData {
    klines: bars,
    current_price: store.current_price(),
    symbol: symbol.to_string(),
    timeframe,
    klines_5m: store_5m.map(|s| s.closed_bars(500)),
};

let signal = strategy.analyze(&market_data);
```

### 未完成K线处理

```rust
// WebSocket 推送未完成K线时，不触发策略
// 只更新 Store，等K线完成（closed=true）后再触发

fn on_ws_message(&mut self, event: KlineEvent) {
    let store = self.kline_manager.get_mut(&event.symbol, &event.timeframe).unwrap();

    if event.bar.closed {
        store.push_closed(event.bar);

        // 检查间隙
        if let Some(gap) = self.detect_gap(&event) {
            self.fill_gap(store, gap).await;
        }

        // 通知引擎：新K线完成，可以执行策略
        self.engine.notify_new_bar(&event.symbol, &event.timeframe);
    } else {
        store.update_current(event.bar);
        // 不触发策略
    }
}
```

---

## 配置化设计

### max_bars 配置

```toml
# config.toml
[kline]
# 默认加载和保持的K线数量
default_max_bars = 1000

# 特殊策略需要更多数据时，单独配置
# 例如 ML 策略需要 50000 根
[[kline.overrides]]
symbol = "BTCUSDT"
timeframe = "30m"
max_bars = 50000
```

### 启动时计算

```rust
fn resolve_max_bars(strategies: &[StrategyInstance], config: &KlineConfig) -> usize {
    // 策略最小需求
    let strategy_min = strategies.iter()
        .flat_map(|s| s.required_timeframes())
        .map(|(_, _, min_bars)| min_bars)
        .max()
        .unwrap_or(500);

    // 配置覆盖
    let configured = config.default_max_bars.max(
        config.overrides.iter().map(|o| o.max_bars).max().unwrap_or(0)
    );

    // 取最大值，确保所有策略都够用
    strategy_min.max(configured).max(1000)
}
```

---

## 实现阶段

### Phase 1: KlineStore + 混合加载（核心）

**目标**：策略服务从 DB 加载历史数据，从交易所补最新缺口

- [ ] 新建 `kline_store.rs` — VecDeque 滚动窗口，push/update/query
- [ ] 新建 `kline_loader.rs` — DB 加载 + 交易所补拉逻辑
- [ ] 改造 `engine.rs` — 从 KlineStore 读取数据替代 Redis
- [ ] 启动流程改造 — 查询 active 策略 → 计算数据需求 → 混合加载
- [ ] 配置支持 — `default_max_bars` + overrides

**验收**：strategy-service 启动后从 DB 加载数据，策略可正常执行

### Phase 2: WebSocket 实时更新

**目标**：运行时通过 WS 保持数据实时

- [ ] 新建 `ws_feed.rs` — Binance WS 连接、订阅、消息分发
- [ ] KlineManager 集成 WS 事件 — push_closed / update_current
- [ ] 未完成K线不触发策略 — 只在 closed=true 时通知引擎
- [ ] 订阅管理 — 启动时按 (symbol, timeframe) 对建立订阅

**验收**：策略服务运行中，内存数据实时更新，策略按新K线触发

### Phase 3: 断连恢复 + 间隙检测

**目标**：WS 断连后自动恢复，数据不丢

- [ ] WS 自动重连 — 指数退避，最大 30s
- [ ] 间隙检测 — 对比时间戳发现缺失
- [ ] REST 补拉 — 重连后填补间隙
- [ ] 健康检查 — 定期验证内存数据完整性

**验收**：模拟 WS 断连 5 分钟，重连后数据完整，策略无感知

### Phase 4: 多策略共享 + 动态管理

**目标**：支持策略热加载，共享数据源

- [ ] 多策略共享同一 (symbol, timeframe) 的 Store
- [ ] 新增策略时自动创建缺失的 Store（如果 WS 未订阅则新增订阅）
- [ ] 策略停用时检查 Store 是否还有其他策略使用，无则清理
- [ ] 定期清理过期数据（超过 max_bars 的旧K线）

---

## 已完成的修复

### volume 累加 bug（2026-07-27 已修复）

**文件**：`sql/core/kline_aggregation_all.sql`

**问题**：ON CONFLICT 中 `volume = kline_xx.volume + EXCLUDED.volume` 导致增量聚合时 volume 成倍膨胀。

**修复**：改为 `volume = EXCLUDED.volume`，`trade_count = EXCLUDED.trade_count`。9 个聚合函数（5m/15m/30m/1h/2h/4h/1d/3d/1w）全部修复。

**部署**：在服务器重新执行 SQL 文件即可。历史被污染的 volume 数据需要全量重跑聚合修正。

---

## 数据量参考

| 策略参数 | 30m 需要 | 5m 需要 | 时间跨度 |
|----------|---------|---------|---------|
| MA288/488（当前配置） | 498 根 | 498 根 | 30m: 10.4天, 5m: 1.7天 |
| 交易所单次上限 | 1000 根 | 1000 根 | 30m: 20.8天, 5m: 3.5天 |
| ML 策略（预估） | 10000+ 根 | - | 30m: 208天+ |

**内存开销**：每个 KlineBar ≈ 48 bytes，1000 根 ≈ 48KB，3 币种 × 2 周期 ≈ 288KB。

---

## 风险与注意事项

1. **DB 数据准确性** — 聚合 bug 修复后，历史 volume 数据可能已被污染。首次加载时可选择从交易所全量拉取作为兜底。

2. **API 配额** — 全量加载 10000+ 根需要多次分页请求。Binance 限速 2400 权重/分钟，klines 权重 5，理论上 480 次/分钟，足够。

3. **时钟同步** — WS 推送的时间戳是交易所时间，需确保本地时钟不偏差太大，否则间隙检测可能误判。

4. **多交易所支持** — 当前设计针对 Binance，OKX 的 WS 格式不同，需要适配层。
