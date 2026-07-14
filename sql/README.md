# SQL 脚本目录

## 目录结构

```
sql/
├── README.md                    # 本文件
├── core/                        # 核心表结构
│   ├── kline_1m.sql             # 1分钟K线表
│   ├── kline_high_timeframe.sql # 高时间框架K线表 (4h/1d/3d/1w)
│   ├── kline_multi_timeframe.sql# 多时间框架K线表 (5m/15m/30m/1h/2h)
│   ├── trading_pairs.sql        # 交易对配置表
│   ├── symbol_config.sql        # 监控列表表
│   ├── symbol_mapping.sql       # 交易对映射表
│   ├── strategy_instances.sql   # 策略实例表
│   ├── strategy_signals.sql     # 策略信号表
│   ├── strategy_analysis_log.sql# 策略分析日志表
│   ├── strategy_performance.sql # 策略性能统计表
│   ├── trades.sql               # 交易记录表
│   ├── positions.sql            # 持仓表
│   ├── backtest_results.sql     # 回测结果表
│   ├── price_cache.sql          # 价格缓存表
│   ├── system_config.sql        # 系统配置表
│   ├── tick_data.sql            # Tick数据表 (历史)
│   ├── live_strategy_log.sql    # 实时策略日志表
│   ├── exchange_config.sql      # 交易所配置表
│   ├── trading_orders.sql       # 交易订单表
│   ├── trading_positions.sql    # 交易持仓表
│   ├── stop_orders.sql          # 止损止盈订单表
│   ├── account_snapshot.sql     # 账户快照表
│   ├── risk_logs.sql            # 风控日志表
│   ├── trade_logs.sql           # 交易日志表
│   └── market_sentiment.sql     # 市场情绪数据表
│
├── extensions/                  # 表扩展脚本
│   ├── strategy_signals_extend.sql    # 策略信号表扩展
│   ├── trading_positions_extend.sql   # 持仓表扩展
│   └── add_column_comments.sql        # 添加字段注释
│
├── indexes/                     # 索引优化脚本
│   ├── optimize_indexes.sql     # 通用索引优化
│   ├── kline_covering_indexes.sql # K线表覆盖索引
│   └── 20260714_optimize_kline_indexes.sql # K线索引去重优化
│
└── migrations/                  # 迁移脚本
    ├── migrate_missing_tables.sql     # 缺失表迁移
    ├── truncate_high_tf_klines.sql    # 高TF数据清理
    └── 20260714_remove_foreign_keys.sql # 移除外键约束
```

---

## 使用说明

### 完整初始化（新数据库）

使用 `config/schema_latest.sql` 进行完整初始化：

```bash
psql -U postgres -d trading_core -f config/schema_latest.sql
```

### 增量迁移（已有数据库）

使用迁移脚本添加缺失的表：

```bash
psql -U postgres -d trading_core -f sql/migrations/migrate_missing_tables.sql
```

### 单独执行某个表

```bash
# 创建策略实例表
psql -U postgres -d trading_core -f sql/core/strategy_instances.sql

# 创建交易所配置表
psql -U postgres -d trading_core -f sql/core/exchange_config.sql
```

### 索引优化

```bash
# 执行覆盖索引优化（推荐，可提高查询性能）
psql -U postgres -d trading_core -f sql/indexes/kline_covering_indexes.sql
```

---

## 表分类说明

### Layer 1: 数据采集层 (trading-core)

| 表名 | 说明 |
|------|------|
| `kline_1m` | 1分钟K线数据 |
| `kline_*_timeframe` | 多时间框架K线数据 |
| `trading_pairs` | 交易对配置 |
| `symbol_config` | 监控列表 |
| `tick_data` | Tick数据（历史） |

### Layer 2: 策略分析层 (strategy-service)

| 表名 | 说明 |
|------|------|
| `strategy_instances` | 策略实例配置 |
| `strategy_signals` | 策略信号 |
| `strategy_analysis_log` | 分析日志 |
| `strategy_performance` | 策略性能统计 |

### Layer 3: 交易执行层 (trading-engine)

| 表名 | 说明 |
|------|------|
| `exchange_config` | 交易所实例配置 |
| `trading_orders` | 交易订单 |
| `trading_positions` | 交易持仓 |
| `stop_orders` | 止损止盈订单 |
| `account_snapshot` | 账户快照 |
| `risk_logs` | 风控日志 |
| `trade_logs` | 交易日志 |

### 共享表

| 表名 | 说明 |
|------|------|
| `trades` | 交易记录 |
| `positions` | 持仓记录 |
| `system_config` | 系统配置 |
| `market_sentiment` | 市场情绪数据 |

---

## 注意事项

1. **完整 Schema**：`config/schema_latest.sql` 包含所有表的最新结构，推荐用于新数据库初始化
2. **增量迁移**：使用 `sql/migrations/migrate_missing_tables.sql` 安全地添加缺失表
3. **索引优化**：`sql/indexes/kline_covering_indexes.sql` 使用 `CREATE INDEX CONCURRENTLY`，可在不停机的情况下创建索引
4. **扩展脚本**：`sql/extensions/` 下的脚本用于扩展现有表，使用 `ADD COLUMN IF NOT EXISTS` 确保幂等性
5. **外键策略**：本项目不使用数据库外键，数据完整性由应用层保证

---

## 外键策略说明

本项目采用**无外键**设计，原因：

| 维度 | 外键约束 | 应用层控制（本项目） |
|------|----------|----------------------|
| **性能** | ❌ 每次写入都要验证 | ✅ 无额外开销 |
| **锁竞争** | ❌ 级联操作可能锁表 | ✅ 无锁问题 |
| **扩展性** | ❌ 分库分表困难 | ✅ 天然支持 |
| **灵活性** | ❌ 迁移/重构困难 | ✅ 灵活调整 |
| **数据恢复** | ❌ 级联删除无法恢复 | ✅ 可使用软删除 |

### 线上迁移

如需对已有数据库移除外键约束，执行：

```bash
psql -U postgres -d trading_core -f sql/migrations/20260714_remove_foreign_keys.sql
```

---

## 索引优化策略

### K线索引设计原则

| 查询模式 | 索引类型 | 说明 |
|----------|----------|------|
| `WHERE symbol = $1 ORDER BY timestamp DESC LIMIT $2` | 覆盖索引 | 最常用查询，避免回表 |
| `SELECT MIN/MAX(timestamp) WHERE symbol = $1` | 单列索引 | 快速定位时间范围 |
| `SELECT DISTINCT symbol` | 主键索引 | 利用主键的 symbol 前缀 |

### kline_1m 表索引（优化后）

```sql
-- 主键（自动创建索引）
PRIMARY KEY (symbol, timestamp)

-- 覆盖索引：避免回表，支持 Index Only Scan
idx_kline_1m_cover (symbol, timestamp DESC) INCLUDE (open, high, low, close, volume, trade_count)

-- 单列时间索引：用于 MIN/MAX 查询
idx_kline_1m_timestamp (timestamp)
```

### 线上索引优化

如需清理重复索引，执行：

```bash
psql -U postgres -d trading_core -f sql/indexes/20260714_optimize_kline_indexes.sql
```

**预期效果**：
- 删除 2 个重复索引（kline_1m）
- 减少存储空间
- 提升写入性能
