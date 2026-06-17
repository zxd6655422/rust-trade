# PostgreSQL 唯一约束缺失导致数据未写入

## 问题描述

- 日志显示：`Successfully flushed batch: X ticks inserted`
- 但数据库查询显示：20 分钟内无新数据插入

## 影响

- 数据看似插入成功，但实际未写入数据库
- 实时数据采集系统无法正常工作

## 原因分析

### 表结构问题

```sql
-- 存在唯一索引（INDEX）
CREATE UNIQUE INDEX idx_tick_unique ON public.tick_data 
USING btree (symbol, trade_id, "timestamp");

-- 但不存在唯一约束（CONSTRAINT）
-- pg_constraint 查询结果：tick_data_side_check (CHECK 约束)
```

### 代码使用了 ON CONFLICT 语法

```rust
query_builder.push(" ON CONFLICT (symbol, trade_id, timestamp) DO NOTHING");
```

### 关键区别

**PostgreSQL 的 `ON CONFLICT` 语法需要 UNIQUE CONSTRAINT，UNIQUE INDEX 不被识别**

| 类型 | 是否被 ON CONFLICT 识别 |
|------|------------------------|
| UNIQUE INDEX | ❌ 不识别 |
| UNIQUE CONSTRAINT | ✅ 识别 |

## 解决方案

执行以下 SQL：

```sql
-- 1. 删除现有唯一索引
DROP INDEX IF EXISTS idx_tick_unique;

-- 2. 添加唯一约束
ALTER TABLE tick_data 
ADD CONSTRAINT uq_tick_data_symbol_trade_id_timestamp 
UNIQUE (symbol, trade_id, "timestamp");
```

## 验证

```sql
-- 确认约束已添加
SELECT conname, contype 
FROM pg_constraint 
WHERE conrelid = 'tick_data'::regclass;

-- 应该看到：
-- uq_tick_data_symbol_trade_id_timestamp    u
```

## 修复日期

2026-06-17
