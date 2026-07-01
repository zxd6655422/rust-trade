# 性能优化指南

## 当前优化 (2026-07-02)

### 1. 并发数据采集 ✅

**问题**: 原来串行执行，12 个交易对需要 12 倍时间

**优化**: 使用 `tokio::spawn` 并发执行
```rust
// 每个 symbol 独立任务
for symbol in &symbols {
    let handle = tokio::spawn(async move {
        backfill.run().await;
    });
}
```

**效果**: 12 个交易对同时采集，总时间 = 最慢单个的时间

### 2. 增量更新 ✅

**问题**: 每次启动都全量拉取历史数据

**优化**: 只拉取新数据 + 最近 7 天间隙检查
```rust
// 如果数据是最近的，跳过全量 backfill
if time_since_latest.num_hours() > 1 {
    self.fetch_range(symbol, latest_ts, now).await?;
}

// 只检查最近 7 天的间隙
let gap_check_start = now - chrono::Duration::days(7);
```

**效果**: 后续启动只需几秒完成增量更新

### 3. 请求限速优化 ✅

**问题**: 原来 100ms 限速

**优化**: 减少到 50ms + 指数退避
```rust
const RATE_LIMIT_MS: u64 = 50;  // 原 100ms
// 错误时指数退避: 1s, 2s, 4s, 8s, 16s
let backoff = 2u64.pow(consecutive_errors - 1);
```

**效果**: 吞吐量提升 2x

### 4. 并发轮询 ✅

**问题**: 轮询时串行获取每个 symbol

**优化**: 使用 `join_all` 并发获取
```rust
let fetch_futures: Vec<_> = symbols.iter().map(|symbol| {
    async move { ex.fetch_klines(&sym, "1m", 100).await }
}).collect();
let results = futures::future::join_all(fetch_futures).await;
```

**效果**: 12 个交易对轮询时间 = 最慢单个的时间

---

## 数据库优化建议

### 1. 连接池配置

```toml
[database]
max_connections = 20      # 根据 CPU 核心数调整
min_connections = 5
max_lifetime = 1800
idle_timeout = 300
```

### 2. 索引优化

```sql
-- kline_1m 表索引
CREATE INDEX IF NOT EXISTS idx_kline_1m_symbol_time ON kline_1m(symbol, timestamp DESC);
CREATE INDEX IF NOT EXISTS idx_kline_1m_timestamp ON kline_1m(timestamp);

-- tick_data 表索引
CREATE INDEX IF NOT EXISTS idx_tick_data_symbol_time ON tick_data(symbol, timestamp DESC);

-- 用于间隙检测的索引
CREATE INDEX IF NOT EXISTS idx_kline_1m_symbol_timestamp ON kline_1m(symbol, timestamp);
```

### 3. 批量插入优化

当前使用 `ON CONFLICT DO UPDATE` (UPSERT)，已经是最佳实践。

**进一步优化**: 使用 PostgreSQL COPY 命令（需要 sqlx 支持）
```rust
// 未来可以考虑使用 COPY 协议批量导入
// 比 INSERT 快 5-10x
```

---

## 策略执行优化

### 1. 并发策略执行

```rust
// 并发执行多个 symbol 的策略
let strategy_futures: Vec<_> = symbols.iter().map(|symbol| {
    let strategy = strategy.clone();
    let klines = klines_cache.get(symbol);
    async move {
        strategy.analyze(klines).await
    }
}).collect();

let signals = futures::future::join_all(strategy_futures).await;
```

### 2. 数据缓存

```rust
// 使用 Redis 缓存最近的 kline 数据
// 避免每次都查询数据库
pub struct KlineCache {
    redis: RedisPool,
    ttl: Duration,
}

impl KlineCache {
    pub async fn get_recent(&self, symbol: &str, timeframe: &str) -> Vec<OHLCData> {
        // 先查 Redis，miss 再查 DB
    }
}
```

### 3. 增量策略计算

```rust
// 不要每次都重新计算全部指标
// 只计算新增 kline 的影响
pub struct IncrementalStrategy {
    last_state: StrategyState,
}

impl IncrementalStrategy {
    pub fn update(&mut self, new_kline: &OHLCData) -> Signal {
        // 增量更新 RMA/SMA/EMA
        // 增量更新 RSI
        // 只需要 O(1) 时间
    }
}
```

---

## 扩展性设计

### 100+ 交易对支持

当交易对数量 > 50 时，需要考虑：

#### 1. 分片采集
```rust
// 将交易对分组，每组独立采集任务
let chunks = symbols.chunks(10);
for chunk in chunks {
    tokio::spawn(async move {
        for symbol in chunk {
            fetch_klines(symbol).await;
        }
    });
}
```

#### 2. 优先级队列
```rust
// 重要交易对优先采集
let high_priority = ["BTCUSDT", "ETHUSDT"];
let low_priority = other_symbols;

// 高优先级: 每 10 秒
// 低优先级: 每 60 秒
```

#### 3. 读写分离
```rust
// 写入: 主库
// 读取: 从库（回测、监控）
let write_pool = PgPool::connect(&primary_url);
let read_pool = PgPool::connect(&replica_url);
```

#### 4. 分区表
```sql
-- 按月分区 kline_1m 表
CREATE TABLE kline_1m (
    timestamp TIMESTAMPTZ NOT NULL,
    symbol VARCHAR(20) NOT NULL,
    ...
) PARTITION BY RANGE (timestamp);

-- 自动创建月分区
CREATE TABLE kline_1m_2026_07 PARTITION OF kline_1m
    FOR VALUES FROM ('2026-07-01') TO ('2026-08-01');
```

---

## 监控指标

### 关键指标
1. **采集延迟**: 每个 symbol 的最新 kline 时间与当前时间的差
2. **API 响应时间**: Binance API 调用耗时
3. **数据库写入速率**: 每秒插入的 kline 数量
4. **内存使用**: 缓存占用
5. **错误率**: API 失败 / 重试次数

### 监控命令
```bash
# 查看采集状态
curl http://localhost:8080/api/data/info

# 查看数据库大小
psql -c "SELECT pg_size_pretty(pg_total_relation_size('kline_1m'));"

# 查看最近写入
psql -c "SELECT symbol, MAX(timestamp) FROM kline_1m GROUP BY symbol;"
```

---

## 基准测试

### 单 symbol 性能
- Backfill 2020-2026: ~3 小时 (优化后)
- 增量更新: < 5 秒
- 轮询延迟: < 100ms

### 12 symbol 性能
- 并发 Backfill: ~3 小时 (最慢单个)
- 并发轮询: < 200ms
- 内存占用: ~200MB

### 50 symbol 预估
- 并发 Backfill: ~4-5 小时 (限速影响)
- 并发轮询: < 500ms
- 内存占用: ~500MB
- 需要: 4 核 CPU, 8GB RAM

---

## 待优化

1. [ ] 使用 PostgreSQL COPY 协议批量导入
2. [ ] 实现读写分离
3. [ ] 实现分区表
4. [ ] 增量策略计算
5. [ ] Redis kline 缓存
