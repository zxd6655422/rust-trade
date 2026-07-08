# Changelog

## [2026-07-08] 多时间框架 K线重构 + 数据完整性修复

### 统一 Timeframe 枚举

| 变更 | 说明 |
|------|------|
| 新增变体 | `ThreeMinutes`/`FortyFiveMinutes`/`SixHour`/`EightHour`/`TwelveHour` |
| 新增方法 | `level()` 排序级别、`is_on_demand()` 判断是否按需聚合 |
| 统一来源 | `trading-common::Timeframe` 为唯一权威定义，`redis_writer` 和 `repository` 全部基于此 |

### Redis 缓存分层架构

| 时间框架 | 缓存条数 | 覆盖时间 | TTL | 存储方式 |
|----------|----------|----------|-----|----------|
| 1m | 20160 | 14天 | 1小时 | DB + Redis |
| 3m | 2880 | 6天 | 1天 | 仅 Redis（按需聚合） |
| 5m | 8640 | 30天 | 1天 | DB + Redis |
| 15m | 2880 | 30天 | 1天 | DB + Redis |
| 30m | 1440 | 30天 | 1天 | DB + Redis |
| 45m | 1920 | 60天 | 1天 | 仅 Redis（按需聚合） |
| 1h | 4320 | 180天 | 7天 | DB + Redis |
| 2h | 2160 | 180天 | 7天 | DB + Redis |
| 4h | 1080 | 180天 | 7天 | DB + Redis |
| 6h | 720 | 180天 | 7天 | 仅 Redis（按需聚合） |
| 8h | 540 | 180天 | 7天 | 仅 Redis（按需聚合） |
| 12h | 365 | 180天 | 7天 | 仅 Redis（按需聚合） |
| 1d | 1825 | 5年 | 7天 | DB + Redis |
| 3d | 610 | 5年 | 7天 | DB + Redis |
| 1w | 500 | ~10年 | 7天 | DB + Redis |

### 多时间框架回填

| 模块 | 状态 | 说明 |
|------|------|------|
| BackfillConfig | ✅ | 多TF回填配置（symbols/start_date/timeframes/incremental） |
| run_multi_tf | ✅ | 多TF回填入口，顺序遍历 symbol × TF |
| backfill_high_tf | ✅ | 高TF增量回填，查询DB最新时间后拉取 |
| 定期增量更新 | ✅ | 后台每6小时自动执行一次多TF回填 |
| Redis 预热 | ✅ | 启动时从DB加载所有TF到Redis，含按需聚合框架 |

### 数据完整性修复

| # | 问题 | 修复 |
|---|------|------|
| 1 | Poll loop 只拉 100 根 1m，按需聚合数据不足 | 改为 1000 根 |
| 2 | warm-up 不加载按需聚合框架 | 追加从 1m 聚合 3m/45m/6h/8h/12h |
| 3 | 高TF Redis 缓存启动后不更新 | 每 30 分钟从 DB 刷新一次 |
| 4 | Redis/DB 写入竞态条件 | 先等 Redis 写完再写 DB |
| 5 | 1m 间隙检测只覆盖 7 天 | 扩展到 30 天 |
| 6 | Redis 缺少 trade_count | KlineZsetMember 加 `tc` 字段，`#[serde(default)]` 兼容旧数据 |

### 代码简化

| 变更 | 说明 |
|------|------|
| 删除 `write_on_demand_timeframe` | 统一用 `write_single_timeframe` |
| 删除 `get_on_demand_cache_size(&str)` | 合并到 `get_cache_size(&Timeframe)` |
| 删除 `get_on_demand_timeframes() -> Vec<&str>` | 改为返回 `Vec<Timeframe>` |
| 删除 `KlineData` (redis_writer) | 未使用的结构体 |
| `timeframe_key_suffix` | 直接委托给 `tf.as_str()` |
| `aggregate_klines/aggregate_window` | 参数从 `&str` 改为 `&Timeframe` |

### 文件改动

| 文件 | 改动 |
|------|------|
| `trading-common/src/data/types.rs` | Timeframe 枚举扩展 +5 变体 +3 方法 |
| `trading-common/src/data/repository.rs` | match 补全新 Timeframe 变体 |
| `trading-core/src/redis_writer.rs` | 缓存分层重构、删除字符串匹配、统一 Timeframe |
| `trading-core/src/main.rs` | 多TF回填集成、warm-up 增强、poll loop 修复 |
| `trading-core/src/service/backfill.rs` | 多TF回填支持、间隙检测扩展 |
| `trading-core/src/config.rs` | 新增 multi_tf_backfill 配置项 |
| `strategy-service/src/redis_reader.rs` | Timeframe 扩展、KlineData 加 trade_count |
| `strategy-service/src/indicators.rs` | KlineData 构造适配 |
| `src-tauri/src/commands.rs` | match 补全新 Timeframe 变体 |
| `src-tauri/src/types.rs` | SignalStats 字段重构 |
| `sql/kline_multi_timeframe.sql` | 新增 5m/15m/30m/1h/2h 表 |
| `sql/truncate_high_tf_klines.sql` | 新增高TF数据清理脚本 |

---

## [2026-07-07] 自动交易 + WebSocket 实时推送 + 告警系统

### 自动交易功能（严谨版）

| 模块 | 状态 | 说明 |
|------|------|------|
| TradeExecutor | ✅ | 交易执行器，将信号转化为订单 |
| TradeValidator | ✅ | 交易验证器，检查所有前置条件 |
| ExchangeClient (Binance) | ✅ | Binance API 客户端 |
| OkxClient | ✅ | OKX API 客户端 |
| OrderSync | ✅ | 订单状态同步模块 |
| MultiExchangeLoop | ✅ | 多交易所交易循环 |
| SymbolMapping | ✅ | 交易对映射表 |
| 多交易所支持 | ✅ | 支持 Binance/OKX 同时下单 |
| 多市场支持 | ✅ | 支持现货/合约交易 |
| 订单重复检查 | ✅ | 避免相同策略重复下单 |
| 仓位阈值检查 | ✅ | 检查持仓数量和金额限制 |
| 账户余额检查 | ✅ | 从交易所 API 获取实际余额 |
| 交易对精度 | ✅ | 从交易所 API 获取精度信息 |
| 最小下单金额 | ✅ | 检查最小下单金额限制 |
| 止损止盈单 | ✅ | 支持 STOP_MARKET、TAKE_PROFIT_MARKET |
| 订单状态同步 | ✅ | 轮询交易所订单状态，更新 trades 表 |
| 持仓自动更新 | ✅ | 订单成交后自动更新持仓 |
| 被拒订单记录 | ✅ | 记录被拒绝的订单和原因 |
| API 签名 | ✅ | Binance HMAC-SHA256 / OKX HMAC-SHA256 |

### WebSocket 实时推送

| 模块 | 状态 | 说明 |
|------|------|------|
| WebSocket 端点 | ✅ | ws://host:8082/ws/signals |
| 信号广播 | ✅ | 信号生成时实时推送到前端 |
| 客户端管理 | ✅ | 支持多客户端连接 |

### 告警系统

| 模块 | 状态 | 说明 |
|------|------|------|
| AlertManager | ✅ | 告警管理器 |
| 日志告警 | ✅ | 按级别输出到日志 |
| Webhook 告警 | ✅ | 支持发送到外部服务 |
| 冷却机制 | ✅ | 避免重复告警（默认5分钟） |
| 告警级别 | ✅ | Info/Warning/Critical |

### 文件改动

| 文件 | 改动 |
|------|------|
| `strategy-service/src/trade_executor.rs` | 新建交易执行器 |
| `strategy-service/src/websocket.rs` | 新建 WebSocket 模块 |
| `strategy-service/src/alert.rs` | 新建告警系统 |
| `strategy-service/src/engine.rs` | 集成自动交易/推送/告警 |
| `strategy-service/src/main.rs` | 初始化新模块 |
| `strategy-service/Cargo.toml` | 添加依赖 |

---

## [2026-07-07] 大周期分析支持 + Redis 缓存优化

### Redis 缓存优化

| 模块 | 状态 | 说明 |
|------|------|------|
| 缓存数量 | ✅ | 从 5000 增加到 20000 根 |
| 多时间框架 | ✅ | 新增 3d/1w 时间框架 |
| 内存占用 | ✅ | 约 280MB（10 symbol × 7 TF） |

### 高时间框架 K 线表

| 表 | 说明 |
|------|------|
| `kline_4h` | 4小时K线，支持大周期分析 |
| `kline_1d` | 日K线，支持中长期分析 |
| `kline_3d` | 3日K线，支持周期分析 |
| `kline_1w` | 周K线，支持长期趋势分析 |

### 大周期分析策略

| 模块 | 状态 | 说明 |
|------|------|------|
| macro_cycle 策略 | ✅ | 支持周K/3日K分析 |
| 历史高低点识别 | ✅ | 自动识别支撑/阻力位 |
| 趋势确认 | ✅ | ADX + 均线确认 |
| 关键位置信号 | ✅ | 接近历史高点/低点时生成信号 |

### 数据库脚本

| 文件 | 说明 |
|------|------|
| `sql/kline_high_timeframe.sql` | 高时间框架K线表 + 聚合函数 |
| `sql/strategy_performance.sql` | 策略性能统计表 |
| `sql/migrate_missing_tables.sql` | 增量迁移脚本 |

### 文件改动

| 文件 | 改动 |
|------|------|
| `trading-core/src/redis_writer.rs` | 增加缓存数量，支持 3d/1w |
| `trading-core/src/main.rs` | 添加高时间框架聚合任务 |
| `trading-common/src/data/types.rs` | 添加 TwoHour/ThreeDay 时间框架 |
| `trading-common/src/data/repository.rs` | 添加高时间框架K线写入 |
| `strategy-service/src/redis_reader.rs` | 支持多时间框架查询 |
| `strategy-service/src/indicators.rs` | 指标计算模块 |
| `strategy-service/src/strategies/macro_cycle.rs` | 大周期分析策略 |

---

## [2026-07-07] Strategy-Service 策略服务

### 新增 strategy-service 独立服务

| 模块 | 状态 | 说明 |
|------|------|------|
| strategy-service 框架 | ✅ | 新建独立 Rust 服务 |
| 6 个策略实现 | ✅ | RSI/MACD/布林带/成交量/趋势/多时间框架 |
| 数据库操作层 | ✅ | 策略CRUD、信号、交易、性能统计 |
| Redis 读取器 | ✅ | 从 Redis 读取 K线和指标数据 |
| 策略执行引擎 | ✅ | 定时轮询 PostgreSQL + Redis |
| HTTP API | ✅ | 策略管理/信号查询/交易记录/统计 |

### 新增数据库 Schema

| 文件 | 说明 |
|------|------|
| `config/schema_v5.sql` | 交易所/市场类型字段扩展 |
| `config/schema_v6.sql` | 策略服务相关表 |

### 前端优化

| 组件 | 说明 |
|------|------|
| AutoTradingStatus | 新建自动交易状态组件 |
| OrderPanel | 更新下单面板 |
| SymbolSelect | 更新交易对选择器 |
| i18n | 添加自动交易相关翻译 |

### Redis 数据写入

| 文件 | 说明 |
|------|------|
| `trading-core/src/redis_writer.rs` | 新建，从 trading-core 写入 K线和指标到 Redis |

### 文件改动

| 文件 | 改动 |
|------|------|
| `strategy-service/` | 新建完整服务目录 |
| `trading-core/src/redis_writer.rs` | 新建 Redis 写入器 |
| `trading-core/src/main.rs` | 集成 Redis 写入器 |
| `frontend/src/components/trading/AutoTradingStatus.tsx` | 新建自动交易状态组件 |
| `frontend/src/components/trading/OrderPanel.tsx` | 更新下单面板 |
| `frontend/src/lib/i18n/translations/en.ts` | 添加翻译 |
| `frontend/src/lib/i18n/translations/zh.ts` | 添加翻译 |

---

## [2026-07-06] 交易对管理增强（合并 SymbolManager）

### 合并后的 DataManager 功能

| Tab | 功能 |
|-----|------|
| **交易对配置** | 管理所有交易对，加入/移除监控 |
| **监控列表** | 查看当前监控中的交易对 |
| **新增交易对** | 从数据库选择 或 手动输入 |
| **数据归档** | 导出到 Parquet |

### 数据流

```
trading_pairs (所有交易对)
    ↓ 用户选择
symbol_config (监控列表)
    ↓
数据采集
```

### 文件改动

| 文件 | 改动 |
|------|------|
| `config/schema_v4.sql` | 新建交易对配置表 |
| `src-tauri/src/commands.rs` | 新增 7 个 API |
| `src-tauri/src/main.rs` | 注册新命令 |
| `trading-core/src/main.rs` | 新增配置同步逻辑 |
| `frontend/src/components/trading/DataManager.tsx` | 合并 SymbolManager 功能 |
| `frontend/src/app/trading/page.tsx` | 移除 SymbolManager 引用 |

---

## [2026-07-06] 交易对管理增强

### 新增 trading_pairs 表

| 模块 | 状态 | 说明 |
|------|------|------|
| 数据库 Schema V4 | ✅ | 新增 `trading_pairs` 表 |
| 交易对配置 API | ✅ | 增删改查 + 状态管理 |
| 前端 DataManager | ✅ | 三 Tab 设计（监控/新增/归档） |
| 从数据库选择 | ✅ | 从 kline_1m 获取已有交易对 |
| 新增交易对 | ✅ | 手动输入，支持现货/合约 |

### 交易对状态

| 状态 | 说明 | 采集 | 分析 |
|------|------|------|------|
| `active` | 正常采集 | ✅ | ✅ |
| `paused` | 暂停采集 | ❌ | ❌ |
| `archived` | 归档删除 | ❌ | ❌ |

### 文件改动

| 文件 | 改动 |
|------|------|
| `config/schema_v4.sql` | 新建交易对配置表 |
| `src-tauri/src/commands.rs` | 新增 7 个 API |
| `src-tauri/src/main.rs` | 注册新命令 |
| `frontend/src/components/trading/DataManager.tsx` | 重新设计 UI |

---

## [2026-07-06] 部署流程优化（代码目录保持干净）

### 部署目录分离

| 模块 | 状态 | 说明 |
|------|------|------|
| 脚本分离 | ✅ | 执行脚本在 `~/apps/deploy/`，源代码在 `~/rust-trade/deploy/` |
| 自动同步 | ✅ | `publish.sh` 会自动同步更新 `~/apps/deploy/` 中的脚本 |
| 定时归档 | ✅ | systemd timer 每天自动执行归档 |
| 文档更新 | ✅ | README.md、DEPLOY.md 已更新 |

### 新的部署流程

**首次部署：**
```bash
bash ~/rust-trade/deploy/first-time-setup.sh
```

**日常更新：**
```bash
bash ~/apps/deploy/publish.sh  # 从 apps 目录执行
```

### 文件改动

| 文件 | 改动 |
|------|------|
| `deploy/first-time-setup.sh` | 添加脚本复制到 ~/apps/deploy/ |
| `deploy/publish.sh` | 添加脚本同步到 ~/apps/deploy/ |
| `deploy/trading-archive.service` | 新建 systemd 服务配置 |
| `deploy/trading-archive.timer` | 新建定时任务配置（每天执行） |
| `deploy/README.md` | 更新部署说明 |
| `deploy/DEPLOY.md` | 更新部署指南 |

---

## [2026-07-06] 部署流程优化

### 部署脚本优化

| 模块 | 状态 | 说明 |
|------|------|------|
| publish.sh 优化 | ✅ | 支持 `--skip-build` / `--no-restart` 参数 |
| DEPLOY.md | ✅ | 新建部署指南文档 |
| README.md 更新 | ✅ | 优化部署说明 |

### 部署流程

**首次部署：**
```bash
bash ~/rust-trade/deploy/first-time-setup.sh
```

**日常更新：**
```bash
bash ~/rust-trade/deploy/publish.sh
```

### 文件改动

| 文件 | 改动 |
|------|------|
| `deploy/publish.sh` | 优化脚本，支持参数选项 |
| `deploy/DEPLOY.md` | 新建部署指南 |
| `deploy/README.md` | 更新部署说明 |

---

## [2026-07-06] 数据管理功能优化

### 新增数据管理功能

| 模块 | 状态 | 说明 |
|------|------|------|
| DataManager 组件 | ✅ | 前端数据管理面板，支持添加/归档/删除 |
| 数据采集状态 API | ✅ | `get_collection_status` / `get_all_collection_status` |
| 一键添加采集 | ✅ | `add_symbol_with_collection`，添加并开始采集 |
| 数据归档 API | ✅ | `archive_symbol_data` / `archive_all_symbols` |
| Parquet 导出 | ✅ | 集成 PolarsRepository 导出到 Parquet |

### 简化后的流程

**之前（繁琐）：**
1. 手动修改 `development.toml` 配置文件
2. 重启 trading-core 服务
3. 执行 `archive_klines` 命令行工具

**现在（简单）：**
1. 前端输入交易对 → 点击"添加"
2. 自动开始数据采集
3. 点击"归档"按钮 → 一键导出到 Parquet

### 文件改动

| 文件 | 改动 |
|------|------|
| `src-tauri/src/commands.rs` | 新增数据管理命令 |
| `src-tauri/src/main.rs` | 注册新命令 |
| `frontend/src/components/trading/DataManager.tsx` | 新建数据管理组件 |
| `frontend/src/app/trading/page.tsx` | 集成 DataManager |

---

## [2026-07-06] 前端交易对动态加载优化

### 交易对选择优化

| 模块 | 状态 | 说明 |
|------|------|------|
| SymbolSelect 组件 | ✅ | 新建通用交易对下拉选择组件 |
| PriceTicker 优化 | ✅ | 从数据库动态加载交易对，移除硬编码 |
| SymbolManager 增强 | ✅ | 新增 loadAllSymbols 方法 |
| PaperTrading 修复 | ✅ | 从数据库加载交易对列表 |

### 设计原则

- 交易对列表统一从数据库 `symbol_config` 表读取
- 下拉选择器自动过滤已启用的交易对
- 保持降级机制：数据库不可用时使用默认列表

### 文件改动

| 文件 | 改动 |
|------|------|
| `frontend/src/components/trading/SymbolSelect.tsx` | 新建通用交易对下拉选择组件 |
| `frontend/src/components/trading/PriceTicker.tsx` | 集成 SymbolSelect，动态加载交易对 |
| `frontend/src/components/trading/SymbolManager.tsx` | 新增 loadAllSymbols 方法 |
| `frontend/src/app/trading/page.tsx` | 移除硬编码默认值 |
| `frontend/src/app/trading/PaperTradingContent.tsx` | 从数据库加载交易对 |

---

## [2026-07-05] 策略信号闭环优化

### 策略信号生命周期管理

| 模块 | 状态 | 说明 |
|------|------|------|
| 信号表分离 | ✅ | `strategy_signals`（引擎专属）+ `strategy_analysis_log`（前端专属） |
| 生命周期闭环 | ✅ | pending → confirmed/invalidated/expired/superseded |
| 定时任务调度 | ✅ | `StrategyAnalysisScheduler`，每 5 分钟自动分析 |
| 信号验证追踪 | ✅ | best_price/worst_price/eval_count 追踪价格变化 |
| 过期清理机制 | ✅ | 24 小时自动过期，可配置 |

### 信号闭环流程

```
策略分析 → 生成信号 → 保存 pending
      ↓
定时任务验证:
  - 同方向+未过期 → 更新验证
  - 达到确认阈值 → confirmed
  - 方向反转 → superseded
  - 超时 → expired
```

### 文件改动

| 文件 | 改动 |
|------|------|
| `trading-core/src/service/strategy_scheduler.rs` | 新建策略分析调度器 |
| `trading-common/src/data/repository.rs` | 新增信号生命周期管理方法 |
| `config/schema_v3.sql` | 新建两张信号表（完全分离） |

---

## [2026-07-04] Polars 集成

### Polars 集成 (性能提升 10-50 倍)

| 模块 | 状态 | 说明 |
|------|------|------|
| Parquet 存储层 | ✅ | 按月分区存储，支持追加写入 |
| Polars 查询层 | ✅ | 延迟加载，向量化计算 |
| 技术指标计算 | ✅ | SMA/EMA/RSI/MACD/布林带 |
| 归档脚本 | ✅ | PostgreSQL → Parquet 导出 |

### 文件改动

| 文件 | 改动 |
|------|------|
| `trading-common/Cargo.toml` | 新增 polars 依赖 (parquet, lazy, rolling_window) |
| `trading-common/src/data/parquet_store.rs` | 新建 Parquet 存储管理 |
| `trading-common/src/data/polars_repository.rs` | 新建 Polars 查询层 + 技术指标 |
| `trading-common/src/data/mod.rs` | 导出新模块 |

---

## [2026-07-04] Polars 集成 + 本地开发优化 + 前端全面修复

### Polars 集成 (性能提升 10-50 倍)

| 模块 | 状态 | 说明 |
|------|------|------|
| Parquet 存储层 | ✅ | 按月分区存储，支持追加写入 |
| Polars 查询层 | ✅ | 延迟加载，向量化计算 |
| 技术指标计算 | ✅ | SMA/EMA/RSI/MACD/布林带 |
| 归档脚本 | ✅ | PostgreSQL → Parquet 导出 |

### 本地开发优化

| 模块 | 状态 | 说明 |
|------|------|------|
| MockExchange 适配器 | ✅ | 本地开发测试用，不依赖网络 |
| 历史数据回放 | ✅ | 从 PostgreSQL 加载 K线数据回放 |
| 模拟订单撮合 | ✅ | 本地模拟下单、撤单、持仓 |
| 数据库索引优化 | ✅ | 已执行 optimize_indexes.sql |
| Portfolio 单元测试 | ✅ | 25 个测试用例，覆盖做多/做空/盈亏 |
| K线聚合器单元测试 | ✅ | 16 个测试用例，覆盖多时间框架聚合 |
| Polars 计算测试 | ✅ | 3 个测试用例，覆盖 SMA/RSI/MACD |
| 重试机制工具 | ✅ | 指数退避重试、超时控制、错误上下文 |
| 重试工具单元测试 | ✅ | 6 个测试用例，覆盖成功/失败/超时 |

### 前端修复

| 模块 | 状态 | 说明 |
|------|------|------|
| Header 连接状态修复 | ✅ | 使用真实连接状态，支持手动刷新 |
| Settings 页面增强 | ✅ | 添加服务器配置、交易所 API 配置 |
| 连接状态 Context | ✅ | 全局连接状态管理 |
| K线图自动刷新 | ✅ | 支持自动刷新开关，可配置间隔 |
| Toast 通知组件 | ✅ | 全局错误/成功/警告提示 |
| i18n 翻译完善 | ✅ | 修复英文硬编码，添加缺失翻译 |
| 下单面板组件 | ✅ | 支持市价/限价/止损/止盈单 |
| 数据导出功能 | ✅ | 交易历史导出为 CSV/JSON |
| 价格告警组件 | ✅ | 支持突破/跌破告警，本地存储 |
| 全局错误边界 | ✅ | 捕获组件错误，友好错误页面 |
| Spot/Futures 切换修复 | ✅ | 传递 marketType 给子组件 |

### MockExchange 功能

- 实现 `MarketDataProvider` + `TradingOperations` 完整接口
- 支持从 PostgreSQL 加载历史 K线数据
- 模拟账户余额、持仓、订单管理
- 本地撮合市价单，实时计算盈亏
- 配置: `exchange.id = "mock"` 即可使用

### 文件改动

| 文件 | 改动 |
|------|------|
| `trading-engine/src/exchange/adapters/mock_exchange.rs` | 新建 MockExchange 适配器 |
| `trading-engine/src/exchange/adapters/mod.rs` | 导出 MockExchange |
| `trading-engine/src/exchange/mod.rs` | 注册到 ExchangeFactory |
| `version/v1.0/optimize_indexes.sql` | 数据库索引优化脚本 |

---

## [2026-07-04] P11: Exchange Trait 分层重构

### 已完成

| 模块 | 状态 | 说明 |
|------|------|------|
| MarketDataProvider trait | ✅ | 只读市场数据接口（12 个方法） |
| TradingOperations trait | ✅ | 认证交易操作接口（16 个方法） |
| Exchange 组合 trait | ✅ | 自动为同时实现两个 trait 的类型实现 |
| BinanceAdapter 拆分 | ✅ | impl MarketDataProvider + impl TradingOperations |
| BinanceSpotAdapter 拆分 | ✅ | impl MarketDataProvider + impl TradingOperations |
| OkxAdapter 拆分 | ✅ | impl MarketDataProvider + impl TradingOperations |

### 设计说明

- `MarketDataProvider` — 公开 API，无需认证（行情、K线、订单簿等）
- `TradingOperations` — 私有 API，需要 API Key（下单、撤单、持仓等）
- `Exchange = MarketDataProvider + TradingOperations` — 组合 trait，向后兼容
- 消费端可按需使用更精确的类型约束

### 文件改动

| 文件 | 改动 |
|------|------|
| `trading-engine/src/exchange/traits.rs` | 拆分为 MarketDataProvider + TradingOperations + Exchange |
| `trading-engine/src/exchange/mod.rs` | 更新 exports |
| `trading-engine/src/exchange/adapters/binance_adapter.rs` | impl 拆分 |
| `trading-engine/src/exchange/adapters/binance_spot_adapter.rs` | impl 拆分 |
| `trading-engine/src/exchange/adapters/okx_adapter.rs` | impl 拆分 |

---

## [2026-07-04] WebSocket 实时推送 + 双机制数据源

### 架构设计

```
trading-core 服务运行中:
  前端 → WebSocket (ws://server:8080/ws) → 实时价格推送 (Live)
  前端 → 轮询关闭 (fallback_only=true)

trading-core 服务未运行:
  前端 → WebSocket 连接失败 → 自动降级
  前端 → Tauri 命令轮询数据库 (Polling)
  前端 → 定期重连 WebSocket → 连上后自动切换回推送
```

### 实现

#### 1. 配置文件 (frontend/public/config/trading-core.json)

```json
{
  "server": { "host": "localhost", "port": 8080, "protocol": "http" },
  "websocket": { "enabled": true, "reconnect_interval_ms": 5000, "max_reconnect_attempts": 10 },
  "polling": { "enabled": true, "interval_ms": 10000, "fallback_only": true }
}
```

- `server.host/port` — 支持远程服务器地址
- `polling.fallback_only` — true=仅在 WebSocket 不可用时轮询

#### 2. 配置加载器 (frontend/src/lib/config.ts)

- `loadTradingCoreConfig()` — 加载配置文件，失败使用默认值
- `getWebSocketUrl()` / `getApiBaseUrl()` — 生成连接 URL

#### 3. 实时数据 Hook (frontend/src/lib/useRealtimeData.ts)

- `useRealtimeData()` — 双机制实时数据 hook
- 优先 WebSocket → 自动降级轮询 → 定期重连
- 返回: prices, dataSource ('websocket'|'polling'|'disconnected'), isConnected, reconnect()

#### 4. PriceTicker 更新 (frontend/src/components/trading/PriceTicker.tsx)

- 使用 `useRealtimeData` hook 获取实时价格
- 显示数据源状态指示器: 🟢 Live (WebSocket) / 🔵 Polling / 🔴 Offline
- 初始加载使用 Tauri 命令，后续由 WebSocket 更新

#### 5. 后端价格广播 (trading-core/src/main.rs)

- candle1m 轮询获取 kline 后，通过 `tick_tx` 广播最新价格
- WebSocket 客户端收到实时价格更新

### 文件改动

| 文件 | 改动 |
|------|------|
| `frontend/public/config/trading-core.json` | **新建** 服务器配置 |
| `frontend/src/lib/config.ts` | **新建** 配置加载器 |
| `frontend/src/lib/useRealtimeData.ts` | **新建** 双机制实时数据 hook |
| `frontend/src/components/trading/PriceTicker.tsx` | 使用 useRealtimeData + 数据源指示器 |
| `trading-core/src/main.rs` | candle1m 轮询广播最新价格 |

---

## [2026-07-04] 后端错误处理加固

### 问题

后端存在多处 `unwrap()` 调用，运行时可能因数据库连接失败、Redis 不可用、策略不存在等原因导致 panic 崩溃。

### 修复

#### trading-core/src/main.rs

| 修复项 | 原代码 | 修复后 |
|--------|--------|--------|
| Tick 数据库连接 | `create_database_pool_for_service().await.unwrap()` | `match` + `error!()` + `return` |
| Tick 缓存连接 | `create_cache_for_service().await.unwrap()` | `match` + `error!()` + `return` |
| Candle1m 数据库连接 | 同上 | 同上 |
| Candle1m 缓存连接 | 同上 | 同上 |
| 日期解析 | `date.and_hms_opt(0,0,0).unwrap()` | `match` + `error!()` + `return` |
| 信号量获取 | `sem.acquire().await.unwrap()` | `match` + `error!()` + `return` |

#### trading-core/src/api/handlers.rs

| 修复项 | 原代码 | 修复后 |
|--------|--------|--------|
| 策略创建 (3处) | `create_multi_timeframe_strategy(&id).unwrap()` | `unwrap_or_else` + fallback to "trend" |
| 空数据访问 (3处) | `data.first().unwrap().timestamp` | `.map_or("N/A", ...)` |
| Decimal 常量 | `Decimal::from_str("0.001").unwrap()` | 提取为 `default_commission_decimal()` 函数 |

#### trading-core/src/service/backfill.rs

| 修复项 | 原代码 | 修复后 |
|--------|--------|--------|
| K线访问 | `klines.last().unwrap().timestamp` | `match klines.last()` + `continue` |

#### src-tauri/src/commands.rs

| 修复项 | 原代码 | 修复后 |
|--------|--------|--------|
| 策略创建 (3处) | `create_multi_timeframe_strategy(&id).unwrap()` | `unwrap_or_else` + fallback |
| Decimal 解析 | `Decimal::from_str("0.7").unwrap()` | `Decimal::new(7, 1)` |

### 测试结果

- ✅ `cargo check -p trading-core` 编译通过
- ✅ `cargo check -p trading-desktop` 编译通过

---

## [2026-07-04] 全面中英文国际化 (i18n)

### 问题

Dashboard 页面、回测页面、设置页面、K线图组件等大量使用硬编码英文文本，无法切换中文。

### 实现

#### 1. 新增翻译 key (en.ts / zh.ts)

- `dashboard` — 仪表盘页面 50+ 个翻译 key
- `advancedBacktest` — 高级回测 60+ 个翻译 key
- `backtestContent` — 回测内容 20+ 个翻译 key
- `settingsPage` — 设置页面 6 个翻译 key

#### 2. 页面更新

| 页面/组件 | 改动 |
|-----------|------|
| `app/page.tsx` (Dashboard) | 全部硬编码文本替换为 t.dashboard.* |
| `app/backtest/page.tsx` | 全部替换为 t.backtestContent.* |
| `app/trading/BacktestContent.tsx` | 全部替换为 t.backtestContent.* |
| `app/trading/AdvancedBacktestContent.tsx` | 全部替换为 t.advancedBacktest.* |
| `app/settings/page.tsx` | 重写，完整语言切换 UI |
| `components/trading/KlineChart.tsx` | Tooltip/加载文本替换为 t.klineChart.* / t.common.* |

#### 3. 设置页面

全新实现设置页面，包含：
- 语言切换（中文/English）带国旗图标
- 主题切换（深色/浅色）占位

### 测试结果

- ✅ 所有页面支持中英文切换
- ✅ 翻译文件 TypeScript 类型完整

---

## [2026-07-04] P11: 高级回测功能 Tauri 集成

### 问题

后端已实现多时间框架回测、滚动前进测试、样本外测试、多交易对回测、市场状态分析等高级功能，但 Tauri 桌面端只有基础回测。

### 实现

#### 1. Tauri Commands (src-tauri/src/commands.rs)

新增 5 个命令，直接调用 trading-common 回测引擎：

- `run_multi_timeframe_backtest` — 多时间框架回测（逐 1m bar 模拟交易）
- `run_walk_forward_test` — 滚动前进测试（训练/测试窗口滚动，过拟合检测）
- `run_out_of_sample_test` — 样本外测试（70/30 划分，过拟合比率）
- `run_multi_symbol_backtest` — 多交易对回测（跨标的鲁棒性验证）
- `analyze_market_state` — 市场状态分析（ATR/ADX 趋势/震荡分布）

#### 2. 类型定义 (src-tauri/src/types.rs)

新增 10 个请求/响应类型：MultiTimeframeBacktestRequest, WalkForwardRequest, WalkForwardResult, OutOfSampleRequest, OutOfSampleResult, MultiSymbolBacktestRequest, MultiSymbolBacktestResult, MarketStateAnalysisRequest, MarketStateResult 等。

#### 3. 前端组件 (frontend/src/app/trading/AdvancedBacktestContent.tsx)

全新高级回测 UI，包含 5 个 tab：
- Multi-Timeframe: 策略配置 + 完整回测结果（收益曲线、交易明细）
- Walk-Forward: 训练/测试窗口配置 + 轮次明细表格 + 过拟合状态
- Out-of-Sample: 训练/测试集对比 + 过拟合比率
- Multi-Symbol: 多标的选择 + 汇总统计 + 逐标的明细
- Market State: 市场状态分布 + 趋势/震荡比例 + 数据质量评分

#### 4. 前端类型 (frontend/src/types/backtest.ts)

新增 10 个 TypeScript 接口，与 Tauri 命令一一对应。

### 文件改动

| 文件 | 改动 |
|------|------|
| `src-tauri/src/types.rs` | 新增 10 个高级回测类型 |
| `src-tauri/src/commands.rs` | 新增 5 个 Tauri 命令 |
| `src-tauri/src/main.rs` | 注册新命令到 invoke_handler |
| `frontend/src/types/backtest.ts` | 新增 10 个 TypeScript 接口 |
| `frontend/src/app/trading/AdvancedBacktestContent.tsx` | **新建** 高级回测 UI 组件 |
| `frontend/src/app/trading/page.tsx` | 集成 Advanced Backtest tab |

### 测试结果

- ✅ Rust 编译通过 (cargo check -p trading-desktop)
- ✅ 所有 5 个 Tauri 命令实现完成
- ✅ 前端 UI 组件完整

---

## [2026-07-04] 基础设施完善 + Bybit 适配器

### 已完成

| 模块 | 状态 | 说明 |
|------|------|------|
| systemd 服务 | ✅ | trading-collector.service + trading-engine.service |
| 日志轮转 | ✅ | logrotate 配置，30天轮转+压缩 |
| 告警机制 | ✅ | AlertNotifier 支持日志+Webhook，含冷却机制 |
| WebSocket 用户数据流 | ✅ | 订单状态实时推送，替代轮询 |
| Bybit 适配器 | ✅ | 骨架实现，支持 V5 API 签名 |
| 部署脚本 | ✅ | setup.sh 自动化部署 |

### 文件改动

| 文件 | 改动 |
|------|------|
| `deploy/systemd/trading-collector.service` | 新建 数据采集服务配置 |
| `deploy/systemd/trading-engine.service` | 新建 交易引擎服务配置 |
| `deploy/logrotate/trading` | 新建 日志轮转配置 |
| `deploy/setup.sh` | 新建 自动化部署脚本 |
| `trading-common/src/alert/mod.rs` | 新建 告警模块 |
| `trading-common/src/alert/notifier.rs` | 新建 告警通知器 |
| `trading-common/src/lib.rs` | 添加 alert 模块导出 |
| `trading-common/Cargo.toml` | 添加 reqwest 依赖 |
| `trading-engine/src/engine/trading_loop.rs` | 集成用户数据流 |
| `trading-engine/src/exchange/adapters/bybit_adapter.rs` | 新建 Bybit 适配器骨架 |
| `trading-engine/src/exchange/adapters/mod.rs` | 导出 BybitAdapter |

### 告警类型

| 类型 | 级别 | 触发场景 |
|------|------|----------|
| TradeFailure | Warning | 交易执行失败 |
| RiskControl | Warning | 风控规则触发 |
| ServiceError | Critical | 服务异常 |
| ConnectionLost | Warning | 连接断开 |
| BlackSwan | Critical | 黑天鹅检测 |
| CircuitBreaker | Critical | 熔断触发 |

### 部署命令

```bash
# 在服务器上执行
sudo bash deploy/setup.sh

# 启动服务
sudo systemctl start trading-collector
sudo systemctl start trading-engine

# 设置开机自启
sudo systemctl enable trading-collector
sudo systemctl enable trading-engine

# 查看日志
sudo journalctl -u trading-collector -f
sudo journalctl -u trading-engine -f
```

---

## [2026-07-04] Paper Trading 模拟交易

### 已完成

| 模块 | 状态 | 说明 |
|------|------|------|
| PaperTrader (Rust) | ✅ | 虚拟交易引擎，复用 Portfolio，支持市价/限价/止损/止盈单 |
| Tauri Commands | ✅ | 8个命令: start/stop/status/order/trades/pending/cancel/reset |
| Frontend UI | ✅ | 配置面板、状态概览、手动下单、持仓列表、交易记录 |
| i18n | ✅ | 中英文翻译 (paperTrading 模块) |

### 技术实现

**trading-common/src/paper/**
- `PaperTrader` - 模拟交易器核心
- 复用 `backtest::Portfolio` 进行持仓和PnL跟踪
- 支持市价单（即时成交+滑点）、限价单、止损单、止盈单
- 挂单在价格更新时自动检查触发
- `SharedPaperTrader = Arc<RwLock<PaperTrader>>` 支持 Tauri 多线程访问

**src-tauri/src/commands.rs**
- `start_paper_trading` - 启动（可配置初始资金、手续费率、滑点、交易对）
- `stop_paper_trading` / `reset_paper_trading` - 停止/重置
- `get_paper_status` - 获取状态快照（余额、持仓、PnL、胜率）
- `place_paper_order` - 手动下单（市价/限价/止损/止盈）
- `get_paper_trades` / `get_paper_pending_orders` - 查询记录
- `cancel_paper_order` - 取消挂单

**frontend/src/app/trading/PaperTradingContent.tsx**
- 配置面板（初始资金、交易对选择）
- 状态概览（总资产、PnL、胜率、手续费）
- 手动下单面板（买卖方向、订单类型、数量、价格）
- 持仓列表（实时更新）
- 交易记录表格（分页显示）
- 5秒自动刷新

### 文件改动

| 文件 | 改动 |
|------|------|
| `trading-common/src/paper/mod.rs` | 新建 模块定义 |
| `trading-common/src/paper/trader.rs` | 新建 PaperTrader 核心 |
| `trading-common/src/lib.rs` | 添加 paper 模块导出 |
| `src-tauri/src/state.rs` | 添加 SharedPaperTrader 状态 |
| `src-tauri/src/commands.rs` | 新增 8 个 Tauri 命令 |
| `src-tauri/src/types.rs` | 新增 Paper Trading 类型 |
| `src-tauri/src/main.rs` | 注册新命令 |
| `frontend/src/app/trading/page.tsx` | 替换占位符为 PaperTradingContent |
| `frontend/src/app/trading/PaperTradingContent.tsx` | 新建 Paper Trading UI |
| `frontend/src/lib/i18n/translations/en.ts` | 添加 paperTrading 翻译 |
| `frontend/src/lib/i18n/translations/zh.ts` | 添加 paperTrading 翻译 |

---

## [2026-07-04] Phase 6: 监控桌面应用前端

### 已完成

| 模块 | 状态 | 说明 |
|------|------|------|
| Dashboard | ✅ | 系统概览、OHLC预览、策略矩阵、快速回测 (827行) |
| Trading Center | ✅ | 4标签: 实时交易、回测、模拟(占位)、高级回测 |
| 实时行情组件 | ✅ | PriceTicker (WebSocket+轮询回退) + KlineChart |
| 持仓/交易组件 | ✅ | PositionTable + TradeHistory + PnlSummaryCards |
| 统计分析组件 | ✅ | PerformancePanel + CommissionStats + StrategyWinRate |
| 资金曲线 | ✅ | EquityCurve (recharts AreaChart) |
| 高级回测 | ✅ | 5子标签: 多TF/滚动前进/样本外/多交易对/市场状态 |
| i18n | ✅ | 中英文切换，19个翻译模块，localStorage持久化 |
| 主题切换 | ✅ | Dark/Light模式，Settings页面 + Header双入口 |
| 代码清理 | ✅ | 移除重复backtest页面、移除未使用lightweight-charts依赖 |

### 文件改动

| 文件 | 改动 |
|------|------|
| `frontend/src/app/settings/page.tsx` | 修复主题切换按钮功能 |
| `frontend/src/app/backtest/page.tsx` | 删除（合并到Trading Center） |
| `frontend/src/app/page.tsx` | 更新链接指向 /trading |
| `frontend/package.json` | 移除未使用的 lightweight-charts |
| `frontend/src/app/page.tsx` | 修复未闭合的 label 标签 |
| `frontend/src/lib/i18n/translations/en.ts` | 补充 advancedBacktest.totalTrades 翻译 |
| `frontend/src/lib/i18n/translations/zh.ts` | 补充 advancedBacktest.totalTrades 翻译 |
| `frontend/src/lib/config.ts` | 修复 ESLint any 类型警告 |
| `frontend/src/app/trading/AdvancedBacktestContent.tsx` | 移除未使用的 CheckCircle 导入 |
| `frontend/src/components/trading/AccountProfitDashboard.tsx` | 移除未使用的 TrendingDown 导入 |
| `version/v1.0/DEVELOPMENT_SUMMARY.md` | 更新Phase 6状态 |
| `version/v1.0/PHASE6_COMPLETE.md` | 新建完成报告 |

### 技术架构

- Next.js 15 App Router + React 18 + TypeScript
- Tauri 2 IPC (21个commands)
- Tailwind CSS + shadcn/ui组件
- recharts 图表库
- WebSocket实时数据 + Tauri轮询回退

---

## 开发进度总结 (2026-07-01)

### ✅ 已完成

| 模块 | 状态 | 说明 |
|------|------|------|
| trading-engine | ✅ | 交易引擎核心功能 |
| trading-core service | ✅ | 数据采集 + HTTP API + WebSocket |
| 多时间框架策略框架 | ✅ | K线聚合器 + MultiTimeframeStrategy trait + TrendStrategy |
| 数据库 Schema V2 | ✅ | kline_1m, backtest_results, strategy_signals 等表 |
| candle1m REST 轮询采集 | ✅ | 每 10 秒拉取 Binance K线，写入 kline_1m 表 |
| 历史数据回填 (Backfill) | ✅ | 服务启动自动拉取历史数据 + 缺失 gap 检测补齐 |
| 多时间框架回测引擎 | ✅ | 逐 bar 模拟交易 + 做多做空 + 完整 BacktestResult |
| 样本外测试 + 滚动前进测试 | ✅ | WalkForwardEngine + 过拟合检测 |
| 多交易对回测 + 市场状态分析 | ✅ | MultiSymbolBacktestEngine + MarketStateAnalyzer |

### 🔄 进行中

无

### ⏳ 待完成

| 任务 | 优先级 | 说明 |
|------|--------|------|
| 监控桌面应用 | 低 | Tauri 桌面端 (P8-P10) |

### 关键文件

```
trading-core/
├── src/main.rs                    # service 命令入口
├── src/api/handlers.rs            # HTTP API 处理器
├── src/api/websocket.rs           # WebSocket 处理器
└── src/api/server.rs              # Web 服务器

trading-common/
├── src/data/aggregator.rs         # K线聚合器
├── src/data/repository.rs         # 数据库操作
└── src/backtest/strategy/
    ├── multi_timeframe.rs         # 多时间框架策略 trait
    └── trend_strategy.rs          # 趋势策略实现

config/
├── schema.sql                     # 原始表 (tick_data)
└── schema_v2.sql                  # 新增表 (kline_1m 等)
```

---

## [2026-07-01] 多交易对回测 + 市场状态分析 (P7)

### 问题

- 回测只能在单个交易对上运行，无法验证策略在不同标的上的鲁棒性
- 缺乏对回测数据质量的评估（数据是否覆盖了多种市场状态）

### 实现

#### 1. MarketStateAnalyzer (`market_state.rs`)

分析 K 线数据的市场状态分布：

- **ATR (Average True Range)** — 衡量波动率
- **ADX (Average Directional Index)** — 衡量趋势强度
- **+DI / -DI** — 判断趋势方向
- 滑动窗口分析，输出各状态占比

市场状态分类：
- `StrongUptrend` — ADX > 25, +DI > -DI
- `Uptrend` — 趋势强度 > 0.2
- `Ranging` — 震荡/横盘
- `Downtrend` — 趋势强度 < -0.2
- `StrongDowntrend` — ADX > 25, -DI > +DI
- `HighVolatility` — ATR 百分位 > 3%

数据质量评分：趋势和震荡都有覆盖 = 好数据

#### 2. MultiSymbolBacktestEngine (`multi_symbol.rs`)

多交易对回测编排器：

```
for each symbol in symbols:
  1. 加载 1m K 线数据
  2. 运行 MultiTimeframeBacktestEngine
  3. 运行 MarketStateAnalyzer
  4. 收集结果

汇总：
  - 盈利/亏损 symbol 比例
  - 平均收益率/Sharpe/胜率
  - 最佳/最差 symbol
  - 跨 symbol 相关性
```

#### 3. API 端点

- `POST /api/backtest/multi-symbol` — 多交易对回测
- `POST /api/analysis/market-state` — 市场状态分析

### 文件改动

| 文件 | 改动 |
|------|------|
| `trading-common/src/backtest/market_state.rs` | **新建** 市场状态分析器 |
| `trading-common/src/backtest/multi_symbol.rs` | **新建** 多交易对回测编排器 |
| `trading-common/src/backtest/mod.rs` | 导出新模块 |
| `trading-core/src/api/handlers.rs` | 新增 2 个 API handler |
| `trading-core/src/api/server.rs` | 注册新路由 |

### API 使用示例

```bash
# 多交易对回测（自动获取所有可用 symbol）
curl -X POST http://localhost:8080/api/backtest/multi-symbol \
  -H "Content-Type: application/json" \
  -d '{
    "strategy": "trend",
    "symbols": ["BTCUSDT", "ETHUSDT", "SOLUSDT"],
    "capital": 10000,
    "data_count": 50000
  }'

# 市场状态分析
curl -X POST http://localhost:8080/api/analysis/market-state \
  -H "Content-Type: application/json" \
  -d '{
    "symbol": "BTCUSDT",
    "data_count": 50000,
    "window": 50
  }'
```

---

## [2026-07-01] 样本外测试 + 滚动前进测试 (P6)

### 问题

- 回测只能在完整数据集上运行一次，无法检测过拟合
- 训练集表现好不代表实盘表现好，需要样本外验证
- 缺乏系统化的过拟合检测机制

### 实现

#### 1. WalkForwardEngine (`walk_forward.rs`)

滚动前进回测引擎，核心流程：

```
数据: [===========================================]
       |  train  | test |
       |    |  train  | test |
       |       |  train  | test |
              滚动窗口 →
每轮:
  1. 在 train 数据上运行 MultiTimeframeBacktestEngine
  2. 在 test 数据上运行 MultiTimeframeBacktestEngine
  3. 计算过拟合比率 = (train_sharpe - test_sharpe) / train_sharpe
```

#### 2. 样本外测试 (Out-of-Sample)

简化版：单次 70/30 划分，分别回测比较。

#### 3. 过拟合检测

- 过拟合比率 = (train_sharpe - test_sharpe) / max(train_sharpe, 0.01)
- 阈值默认 0.5，超过判定为过拟合
- 汇总指标：测试集累计收益率、平均 Sharpe、平均回撤、盈利轮次比例

#### 4. API 端点

- `POST /api/backtest/walk-forward` — 滚动前进测试
- `POST /api/backtest/out-of-sample` — 样本外测试

### 文件改动

| 文件 | 改动 |
|------|------|
| `trading-common/src/backtest/walk_forward.rs` | **新建** 滚动前进引擎 |
| `trading-common/src/backtest/mod.rs` | 导出新模块 |
| `trading-core/src/api/handlers.rs` | 新增 2 个 API handler |
| `trading-core/src/api/server.rs` | 注册新路由 |

### API 使用示例

```bash
# 滚动前进测试
curl -X POST http://localhost:8080/api/backtest/walk-forward \
  -H "Content-Type: application/json" \
  -d '{
    "strategy": "trend",
    "symbol": "BTCUSDT",
    "capital": 10000,
    "train_candles": 43200,
    "test_candles": 10080,
    "step_candles": 10080,
    "data_count": 100000
  }'

# 样本外测试
curl -X POST http://localhost:8080/api/backtest/out-of-sample \
  -H "Content-Type: application/json" \
  -d '{
    "strategy": "trend",
    "symbol": "BTCUSDT",
    "capital": 10000,
    "train_ratio": 0.7,
    "data_count": 50000
  }'
```

---

## [2026-07-01] 多时间框架回测引擎 (P5)

### 问题

- `BacktestEngine` 只支持单时间框架的 `Strategy` trait
- `MultiTimeframeStrategy` 是独立 trait，不被回测引擎消费
- `/api/backtest/multi-timeframe` 只做一次性分析，不执行模拟交易
- Portfolio 不支持做空（`EntryDirection::Short` 无法执行）

### 实现

#### 1. Portfolio 做空支持

- 新增 `PositionSide` 枚举：`Long` / `Short`
- `Position` 增加 `side` 字段
- 新增 `execute_short_open()` — 开空仓（借入卖出，获得 proceeds）
- 新增 `execute_short_close()` — 平空仓（买入归还，计算盈亏）
- `update_price()` 正确计算空头 `unrealized_pnl`
- `total_value()` 正确处理空头持仓
- 新增 `has_long_position()` / `has_short_position()` / `get_position_side()` 辅助方法

#### 2. MultiTimeframeBacktestEngine

逐 1m bar 模拟交易的核心引擎：

```
for each 1m kline:
  1. aggregator.update(kline)          // 更新聚合器
  2. portfolio.update_price(close)      // 更新价格
  3. check has_sufficient_data()        // 检查数据充足性
  4. get all_timeframes                 // 获取多时间框架快照
  5. strategy.analyze(&all_klines)      // 策略分析
  6. should_enter / should_exit         // 信号判断
  7. execute buy/sell/short             // 执行交易
```

#### 3. API 更新

- `POST /api/backtest/multi-timeframe` 现在返回完整回测结果
- 新增 `strategy_params` 请求字段
- 优先从 `kline_1m` 表读取数据，回退到 tick 数据生成

### 文件改动

| 文件 | 改动 |
|------|------|
| `trading-common/src/backtest/portfolio.rs` | 做空支持 |
| `trading-common/src/backtest/multi_timeframe_engine.rs` | **新建** 回测引擎 |
| `trading-common/src/backtest/mod.rs` | 导出新模块 |
| `trading-core/src/api/handlers.rs` | 更新 API handler |
| `trading-common/src/backtest/strategy/multi_timeframe.rs` | 修复测试浮点数问题 |

### API 使用示例

```bash
curl -X POST http://localhost:8080/api/backtest/multi-timeframe \
  -H "Content-Type: application/json" \
  -d '{
    "strategy": "trend",
    "symbol": "BTCUSDT",
    "capital": 10000,
    "data_count": 50000,
    "commission_rate": 0.1
  }'
```

### 响应示例（完整回测结果）

```json
{
  "success": true,
  "message": "Multi-timeframe backtest completed successfully",
  "data": {
    "strategy": "trend",
    "symbol": "BTCUSDT",
    "initial_capital": "$10000",
    "final_capital": "$10234.56",
    "total_return_pct": "2.35%",
    "total_trades": 12,
    "winning_trades": 7,
    "losing_trades": 5,
    "win_rate": "58.33%",
    "max_drawdown": "3.21%",
    "sharpe_ratio": "1.45",
    "profit_factor": "1.67"
  }
}
```

---

## [2026-06-30] candle1m 数据采集 + 历史回填

### 问题

candle1m 模式和 tick 模式走的是完全相同的 WebSocket 代码路径，`kline_1m` 表从未被写入。

### 修复

#### 1. candle1m REST 轮询

- `Exchange` trait 新增 `fetch_klines()` 和 `fetch_klines_with_time()` 方法
- `BinanceExchange` 实现调用 `GET /api/v3/klines` REST API
- `main.rs` candle1m 分支改为定时轮询（默认 10 秒），每次拉取最新 100 条 K 线
- `TickDataRepository` 新增 `insert_kline()` / `batch_insert_klines()` / `get_klines()` 写入 `kline_1m` 表
- 使用 `ON CONFLICT DO UPDATE`（upsert），同一根 K 线在未完成前会随时间更新

#### 2. 历史数据回填 (Backfill)

- 新增 `service/backfill.rs` — `BackfillService`
- 服务启动时自动执行：
  - 查询数据库已有数据的最早/最新时间
  - 从配置起始日期（如 2024-01-01）拉取到已有数据开始时间
  - 分页拉取（每次 1000 条），限速 100ms/请求
  - 检测已有数据中的缺失时间段（gap > 2 分钟），逐段补齐

#### 3. 新增 Repository 方法

- `get_kline_earliest(symbol)` — 获取某 symbol 最早的 kline 时间戳
- `get_kline_latest(symbol)` — 获取某 symbol 最新的 kline 时间戳
- `find_kline_gaps(symbol, start, end)` — 查找指定时间范围内的缺失时间段

#### 4. 配置变更

```toml
[collector]
mode = "candle1m"
poll_interval_secs = 10          # 轮询间隔（秒）
backfill_enabled = true          # 是否启用历史回填
backfill_start_date = "2024-01-01"  # 回填起始日期
```

### 文件变更

| 文件 | 改动 |
|------|------|
| `trading-core/src/exchange/traits.rs` | 新增 `fetch_klines` / `fetch_klines_with_time` |
| `trading-core/src/exchange/binance.rs` | 实现 REST K 线拉取，重构为 `do_fetch_klines` |
| `trading-core/src/exchange/types.rs` | 新增 `KlineData` 结构体 |
| `trading-common/src/data/repository.rs` | 新增 kline 写入 + 时间查询 + gap 检测方法 |
| `trading-core/src/service/backfill.rs` | 新建历史数据回填服务 |
| `trading-core/src/service/mod.rs` | 导出 `BackfillService` |
| `trading-core/src/config.rs` | 新增 `backfill_enabled` / `backfill_start_date` |
| `trading-core/src/main.rs` | candle1m 先 backfill 再轮询 |
| `config/development.toml` | 新增 backfill 配置 |
| `config/production.toml` | 新增 backfill 配置 |

---

## [2026-06-30] 数据库 Schema V2

### 新增表结构

| 表名 | 用途 | 说明 |
|------|------|------|
| `kline_1m` | K线数据 | 存储 1m K线，用于多时间框架聚合 |
| `backtest_results` | 回测结果 | 存储历史回测结果，便于比较分析 |
| `strategy_signals` | 策略信号 | 记录策略生成的交易信号 |
| `positions` | 持仓状态 | 记录当前持仓 |
| `trades` | 交易记录 | 记录所有已执行的交易 |
| `price_cache` | 价格缓存 | 缓存最新价格 |

### 文件位置

- `config/schema_v2.sql` - 完整的数据库 Schema V2

### 初始化命令

```bash
# 连接到 PostgreSQL
psql -U postgres -d trading_core

# 执行 Schema V2
\i config/schema_v2.sql
```

### 存储估算

| 表 | 数据量 | 存储空间 |
|------|------|------|
| `kline_1m` | ~525,600 条/年/交易对 | ~100MB/年/交易对 |
| `backtest_results` | 每次回测 1 条 | ~1KB/次 |
| `strategy_signals` | ~1,440 条/天 | ~1MB/天 |

### 设计说明

- **不使用存储过程**：所有聚合查询在 Rust 代码中实现（KlineAggregator），便于数据库迁移
- **不使用触发器**：应用层处理数据一致性，避免数据库层复杂性

---

## [2026-06-30] 多时间框架策略

### 新增功能

#### 1. K线聚合器 (`trading-common/src/data/aggregator.rs`)
- 将 1m K线聚合为其他时间框架（5m, 15m, 30m, 1h, 4h, 1d）
- 支持实时更新和批量聚合
- 自动处理时间窗口对齐

#### 2. 多时间框架策略框架
- `MultiTimeframeStrategy` trait - 多时间框架策略接口
- `TrendDirection` - 趋势方向枚举（Bullish/Bearish/Neutral）
- `MultiTimeframeAnalysis` - 多时间框架分析结果

#### 3. 趋势策略实现 (`trend_strategy.rs`)
- 4h 时间框架：使用 EMA20/EMA50 判断大趋势
- 1h 时间框架：使用 MACD 确认趋势
- 15m 时间框架：使用 RSI 寻找入场点
- 综合评分：加权计算整体置信度

#### 4. 新增 API 端点
- `POST /api/backtest/multi-timeframe` - 多时间框架策略分析

### 文件改动

#### 新增文件
- `trading-common/src/data/aggregator.rs` - K线聚合器
- `trading-common/src/backtest/strategy/multi_timeframe.rs` - 多时间框架策略 trait
- `trading-common/src/backtest/strategy/trend_strategy.rs` - 趋势策略实现

#### 修改文件
- `trading-common/src/data/types.rs` - Timeframe 添加 Hash trait
- `trading-common/src/data/mod.rs` - 导出 aggregator
- `trading-common/src/backtest/strategy/mod.rs` - 导出新策略
- `trading-core/src/api/handlers.rs` - 添加多时间框架回测 API
- `trading-core/src/api/server.rs` - 添加新路由

### API 使用示例

```bash
# 获取策略列表（包含多时间框架策略）
curl http://localhost:8080/api/strategies

# 执行多时间框架分析
curl -X POST http://localhost:8080/api/backtest/multi-timeframe \
  -H "Content-Type: application/json" \
  -d '{"strategy": "trend", "symbol": "BTCUSDT", "capital": 10000, "data_count": 10000}'
```

### 响应示例

```json
{
  "success": true,
  "message": "Multi-timeframe analysis completed",
  "data": {
    "strategy": "Multi-Timeframe Trend",
    "symbol": "BTCUSDT",
    "overall_direction": "Bullish",
    "overall_confidence": "0.75",
    "entry_allowed": true,
    "entry_direction": "Long",
    "timeframe_analyses": [
      {
        "timeframe": "4h",
        "direction": "Bullish",
        "confidence": "0.8",
        "description": "4h EMA20 > EMA50 by 2.50%"
      },
      {
        "timeframe": "1h",
        "direction": "Bullish",
        "confidence": "0.7",
        "description": "1h MACD histogram positive"
      },
      {
        "timeframe": "15m",
        "direction": "Bullish",
        "confidence": "0.6",
        "description": "15m RSI oversold at 28.50"
      }
    ],
    "data_points": 1000
  }
}
```

---

## [2026-06-30] trading-core 服务化改造

### 背景
- trading-engine 停机维护时数据采集不应中断
- 回测功能应随时可用，不需要重启服务
- 支持同时启用多种数据采集模式

### 新增功能

#### 1. `service` 命令
```bash
cargo run service        # 完整服务（数据采集 + API + 回测）
cargo run collector      # 仅数据采集
cargo run backtest       # CLI 回测（保留）
cargo run live           # 旧模式（保留）
```

#### 2. HTTP REST API (端口 8080)
- `GET /health` - 健康检查
- `GET /api/data/info` - 数据信息
- `GET /api/strategies` - 策略列表
- `POST /api/backtest` - 执行回测

#### 3. WebSocket 实时数据
- `ws://0.0.0.0:8080/ws` - 实时数据推送
- 支持订阅/取消订阅交易对
- 心跳检测

#### 4. 数据采集配置
```toml
[collector]
mode = "candle1m"           # disabled / tick / candle1m
enable_tick = false         # 是否同时启用 tick 采集
poll_interval_secs = 60     # 采集间隔
```

### 文件改动

#### 新增文件
- `trading-core/src/api/mod.rs` - API 模块入口
- `trading-core/src/api/handlers.rs` - HTTP 处理器
- `trading-core/src/api/websocket.rs` - WebSocket 处理器
- `trading-core/src/api/server.rs` - Web 服务器

#### 修改文件
- `trading-core/Cargo.toml` - 添加 actix-web, actix-cors, actix-ws 依赖
- `trading-core/src/config.rs` - 添加 CollectorConfig, CollectorMode
- `trading-core/src/main.rs` - 添加 service 命令和 run_service_mode
- `config/development.toml` - 添加 collector 配置
- `config/production.toml` - 添加 collector 配置

### 依赖新增
```toml
actix-web = "4"
actix-cors = "0.7"
actix-ws = "0.2"
actix-web-actors = "4"
actix = "0.13"
```

### 测试结果
- ✅ 服务启动成功
- ✅ 数据库/Redis 连接正常
- ✅ 交易所 WebSocket 连接正常
- ✅ API 端点全部响应正常
- ✅ 回测 API 可用

### 架构优势
| 场景 | 旧方案 | 新方案 |
|------|--------|--------|
| 停机维护 | 数据断档 | 数据采集继续 |
| 执行回测 | 需要重启 | HTTP API 随时调用 |
| 实时监控 | 无 | WebSocket 推送 |
| 多模式采集 | 不支持 | 配置开关控制 |

### 使用示例
```bash
# 启动完整服务
cd trading-core
cargo run --release -- service

# 测试 API
curl http://localhost:8080/health
curl http://localhost:8080/api/strategies

# 执行回测
curl -X POST http://localhost:8080/api/backtest \
  -H "Content-Type: application/json" \
  -d '{"strategy": "rsi", "symbol": "BTCUSDT", "capital": 10000, "data_count": 10000}'
```

---

## [2026-06-29] OkxAdapter 修复与数据源可配置

### 已完成
- OkxAdapter 6项修复
- 数据源可配置 (trades/tickers/candle1m)
- 数据积累方案确定

---

## [2026-06-28] 交易引擎核心功能

### 已完成
- Phase 1-4: 交易引擎核心功能
- BinanceAdapter: USDⓈ-M 合约
- BinanceSpotAdapter: 现货
- OkxAdapter: 基础实现
- Exchange trait: 统一接口
