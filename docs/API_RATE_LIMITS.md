# API 请求限制参考

## Binance

### REST API 限制
| 端点 | 权重 | 限制 |
|------|------|------|
| `/api/v3/klines` | 1 | 1200 weight/min |
| `/api/v3/trades` | 1 | 1200 weight/min |
| `/api/v3/aggTrades` | 1 | 1200 weight/min |
| `/fapi/v1/klines` | 1 | 2400 weight/min (合约) |

### 实际计算
- 1200 weight/min = 20 req/s
- 12 symbol 并发 backfill = 12 * (1000/50) = 240 req/s ❌ **超限！**

### 安全配置
- 单 symbol backfill: 100ms/req = 10 req/s
- 多 symbol 并发: 200ms/req = 5 req/s per symbol
- 12 symbol 总速率: 60 req/s (安全边际 3x)

---

## OKX

### REST API 限制
| 端点 | 限制 |
|------|------|
| `/api/v5/market/candles` | 60 req/5s = 12 req/s |
| `/api/v5/market/tickers` | 20 req/2s = 10 req/s |
| `/api/v5/trade/order` | 60 req/5s = 12 req/s |

### 安全配置
- 单 symbol: 200ms/req = 5 req/s
- 多 symbol 并发: 300ms/req = 3 req/s per symbol

---

## 通用建议

### Backfill（历史数据拉取）
```toml
[rate_limit]
backfill_interval_ms = 200      # 每个请求间隔 200ms
max_concurrent_symbols = 5      # 最大并发 symbol 数
burst_limit = 10                # 突发请求限制
```

### 轮询（实时数据更新）
```toml
[rate_limit]
poll_interval_secs = 30         # 轮询间隔 30 秒
batch_size = 5                  # 每批获取的 symbol 数
```

---

## 当前配置

| 场景 | 间隔 | 速率 | 并发 |
|------|------|------|------|
| Backfill (单 symbol) | 200ms | 5 req/s | 1 |
| Backfill (多 symbol) | 200ms | 5 req/s each | 5 max |
| 轮询 | 30s | - | all |

---

## 监控告警

当接近限制时应触发告警：
- Binance: > 15 req/s (75% of limit)
- OKX: > 8 req/s (80% of limit)

```bash
# 监控当前请求速率
tail -f logs/trading-core_*.log | grep "fetch_klines" | wc -l
```
