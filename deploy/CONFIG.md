# 服务配置说明

## 目录结构

```
deploy/
├── trading-core/           # 数据采集服务
│   ├── production.toml     # 主配置文件
│   └── start.sh            # 启动脚本
│
├── strategy-service/       # 策略分析服务
│   ├── .env.production     # 环境变量配置
│   └── start.sh            # 启动脚本
│
└── trading-engine/         # 交易执行引擎
    ├── engine-production.toml  # 主配置文件
    ├── .env.production         # 环境变量配置（API Key等敏感信息）
    └── start.sh                # 启动脚本
```

---

## 部署步骤

### 1. 准备工作

```bash
# 1. 创建部署目录
sudo mkdir -p /opt/trading/{trading-core,strategy-service,trading-engine}
sudo mkdir -p /opt/trading/config

# 2. 复制配置文件
sudo cp deploy/trading-core/production.toml /opt/trading/trading-core/config/
sudo cp deploy/strategy-service/.env.production /opt/trading/strategy-service/
sudo cp deploy/trading-engine/engine-production.toml /opt/trading/trading-engine/config/
sudo cp deploy/trading-engine/.env.production /opt/trading/trading-engine/
```

### 2. 修改配置

```bash
# 编辑配置文件，修改以下内容：
# - 数据库连接字符串
# - Redis 连接字符串
# - API Key 和 Secret
# - 交易对列表
# - 风控参数

sudo nano /opt/trading/trading-core/config/production.toml
sudo nano /opt/trading/strategy-service/.env.production
sudo nano /opt/trading/trading-engine/.env.production
```

### 3. 数据库迁移

```bash
# 执行SQL创建多时间框架表
psql -U trading -d trading_db -f sql/kline_multi_timeframe.sql
```

### 4. 启动服务

```bash
# 启动 trading-core
cd /opt/trading/trading-core
./trading-core service

# 启动 strategy-service
cd /opt/trading/strategy-service
source .env.production
./strategy-service

# 启动 trading-engine
cd /opt/trading/trading-engine
./trading-engine
```

---

## 配置说明

### Trading Core（数据采集服务）

| 配置项 | 说明 | 默认值 |
|-------|------|--------|
| `collector.mode` | 采集模式 | `candle1m` |
| `collector.stored_timeframes` | 存储的时间框架 | 见配置文件 |
| `collector.multi_tf_backfill_enabled` | 启用多时间框架回填 | `true` |
| `collector.multi_tf_backfill_interval_hours` | 增量回填间隔 | `6` |
| `strategy.interval_secs` | 策略分析间隔 | `300` |

### Strategy Service（策略分析服务）

| 环境变量 | 说明 | 默认值 |
|---------|------|--------|
| `DATABASE_URL` | 数据库连接字符串 | 必填 |
| `REDIS_URL` | Redis连接字符串 | 必填 |
| `ENGINE_POLL_INTERVAL_SECS` | 策略轮询间隔 | `5` |

### Trading Engine（交易执行引擎）

| 配置项 | 说明 | 默认值 |
|-------|------|--------|
| `trading.mode` | 交易模式 | `live` |
| `trading.data_source` | 数据源类型 | `candle1m` |
| `risk_control.max_position_size` | 单笔最大仓位 | `2000.0` |
| `risk_control.stop_loss_pct` | 止损百分比 | `0.02` |
| `risk_control.max_daily_loss` | 日最大亏损 | `500.0` |

---

## 多时间框架配置

### 存储的时间框架

| 时间框架 | 缓存大小 | 覆盖时间 | 用途 |
|---------|---------|---------|------|
| 1m | 20,160 | 2周 | 实时聚合、短线指标 |
| 5m | 8,640 | 1个月 | 短线策略 |
| 15m | 2,880 | 1个月 | 日内策略 |
| 30m | 1,440 | 1个月 | 日内波段 |
| 1h | 4,320 | 6个月 | 波段策略 |
| 2h | 2,160 | 6个月 | 中线策略 |
| 4h | 1,080 | 6个月 | 中线策略 |
| 1d | 1,825 | 5年 | 长线策略 |
| 3d | 610 | 5年 | 大周期分析 |
| 1w | 500 | ~10年 | 宏观分析 |

### 按需聚合（不存数据库）

| 时间框架 | 缓存大小 | 说明 |
|---------|---------|------|
| 3m | 2,880 | 从1m实时聚合 |
| 45m | 1,920 | 从1m实时聚合 |

---

## 注意事项

1. **首次启动**：会执行历史数据回填，耗时较长
2. **API限制**：Binance 20 req/s，已配置限速
3. **磁盘空间**：多时间框架数据增加约30-50%存储
4. **Redis内存**：确保有足够内存（建议2GB+）
5. **API Key**：务必通过环境变量传入，不要写在配置文件中
