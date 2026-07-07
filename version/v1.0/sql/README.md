# SQL 脚本说明

## 目录结构

```
version/v1.0/sql/
├── README.md                              # 本文件
├── 2026-07-07_strategy-service/           # 策略服务开发任务
│   ├── 01_create_missing_tables.sql       # 创建缺失的表
│   ├── 02_create_high_tf_tables.sql       # 创建高时间框架K线表
│   └── 03_strategy_performance.sql        # 策略性能统计表
└── ...                                    # 其他开发任务
```

## 执行顺序

### 2026-07-07 策略服务开发

```bash
# 1. 创建缺失的表（strategy_performance, system_config）
psql -U postgres -d trading_core -f version/v1.0/sql/2026-07-07_strategy-service/01_create_missing_tables.sql

# 2. 创建高时间框架K线表（kline_4h, kline_1d, kline_3d, kline_1w）
psql -U postgres -d trading_core -f version/v1.0/sql/2026-07-07_strategy-service/02_create_high_tf_tables.sql

# 3. 首次聚合历史数据（可选）
psql -U postgres -d trading_core -c "SELECT * FROM aggregate_all_symbols_high_tf();"
```

## 表结构参考

完整的表结构定义在 `config/schema_latest.sql`，包含：
- kline_1m (1分钟K线，4G+数据)
- kline_4h (4小时K线，用于大周期分析)
- kline_1d (日K线)
- kline_3d (3日K线)
- kline_1w (周K线)
- strategy_instances (策略实例)
- strategy_signals (策略信号)
- strategy_analysis_log (分析日志)
- strategy_performance (策略性能统计)
- trades (交易记录)
- positions (持仓)
- trading_pairs (交易对配置)
- symbol_config (监控列表)
- system_config (系统配置)
