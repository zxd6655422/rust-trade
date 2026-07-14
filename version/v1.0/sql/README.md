# v1.0 SQL 脚本（历史记录）

> **注意**：此目录下的 SQL 脚本是 v1.0 版本开发过程中的历史记录。
> 
> **最新的表结构请参考**：`config/schema_latest.sql` 或 `sql/core/` 目录。

## 目录结构

```
version/v1.0/sql/
├── README.md                              # 本文件
└── 2026-07-07_strategy-service/           # 策略服务开发任务（历史）
    ├── 01_create_missing_tables.sql       # 创建缺失的表
    ├── 02_create_high_tf_tables.sql       # 创建高时间框架K线表
    ├── 03_strategy_performance.sql        # 策略性能统计表
    ├── 04_symbol_mapping.sql              # 交易对映射表
    └── 05_account_snapshot.sql            # 账户快照表
```

## 说明

这些 SQL 脚本是在 2026-07-07 开发 strategy-service 时创建的，用于：

1. 创建策略服务所需的表
2. 创建高时间框架 K 线表
3. 添加策略性能统计功能
4. 支持多交易所交易对映射
5. 支持账户余额快照同步

**当前状态**：所有表结构已整合到 `config/schema_latest.sql`，此目录仅作为历史参考保留。
