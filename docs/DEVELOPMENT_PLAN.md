# 量化交易系统开发计划

## 架构概述

```
┌─────────────────────────────────────────────────────────────────┐
│  Layer 1: 数据采集层 (trading-core)                              │
│  ├── WebSocket 连接交易所                                        │
│  ├── 接收实时价格/成交                                          │
│  ├── 生成 K线数据                                               │
│  └── 发布到 Redis Pub/Sub                                       │
└─────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│  Layer 2: 指标计算层 (trading-indicator)                         │
│  ├── 订阅 Redis K线频道                                         │
│  ├── 预计算技术指标 (MA/RSI/MACD/Bollinger...)                   │
│  └── 缓存到 Redis                                               │
└─────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│  Layer 3: 策略分析层 (strategy-service)                          │
│  ├── 从 Redis 读取指标值（毫秒级）                               │
│  ├── 运行策略逻辑（Lua 脚本）                                    │
│  ├── 生成信号                                                   │
│  └── 写入 strategy_signals 表                                   │
└─────────────────────────────────────────────────────────────────┘
                              │
                              ▼
┌─────────────────────────────────────────────────────────────────┐
│  Layer 4: 交易执行层 (trading-engine)                            │
│  ├── 轮询 strategy_signals 表                                   │
│  ├── 风控检查                                                   │
│  ├── 调用交易所 API 下单                                        │
│  └── 更新信号状态                                               │
└─────────────────────────────────────────────────────────────────┘
```

---

## 开发阶段

### 阶段1: 指标预计算 + Redis 缓存

**目标**：将指标计算从策略分析中分离，预计算后缓存到 Redis

**任务清单**：
- [ ] 创建 trading-indicator 服务结构
- [ ] 实现技术指标计算库
  - [ ] MA (简单移动平均)
  - [ ] EMA (指数移动平均)
  - [ ] RSI (相对强弱指数)
  - [ ] MACD (指数平滑异同移动平均线)
  - [ ] Bollinger Bands (布林带)
  - [ ] ATR (平均真实波幅)
- [ ] 实现 Redis 缓存层
  - [ ] 指标缓存结构设计
  - [ ] 增量计算优化
  - [ ] 多时间框架支持
- [ ] 提供指标查询 API

**Redis 缓存结构**：
```
indicator:{symbol}:{timeframe}:ma:{period}    → Decimal
indicator:{symbol}:{timeframe}:rsi:{period}   → Decimal
indicator:{symbol}:{timeframe}:macd           → {macd, signal, histogram}
indicator:{symbol}:{timeframe}:bb:{period}    → {upper, middle, lower}
indicator:{symbol}:{timeframe}:atr:{period}   → Decimal
```

---

### 阶段2: 事件驱动架构

**目标**：解耦数据采集和策略分析，使用 Redis Pub/Sub 传递事件

**任务清单**：
- [ ] 修改 trading-core
  - [ ] K线闭合时发布事件到 Redis
  - [ ] 事件格式标准化
  - [ ] 支持多频道（按交易对/时间框架）
- [ ] 修改 trading-indicator
  - [ ] 订阅 Redis K线频道
  - [ ] 收到新K线后更新指标
  - [ ] 发布指标更新事件
- [ ] 策略服务订阅
  - [ ] 订阅指标更新事件
  - [ ] 触发策略分析
  - [ ] 写入信号表

**Redis Pub/Sub 频道**：
```
kline:{symbol}:{timeframe}     → K线数据
indicator:{symbol}:{timeframe} → 指标更新
signal:{strategy_id}           → 信号产生
```

---

### 阶段3: 热加载策略 (Lua 脚本)

**目标**：支持用户编写策略脚本，动态加载执行

**任务清单**：
- [ ] 集成 Lua 脚本引擎 (mlua)
- [ ] 设计策略 API
  - [ ] on_bar() 回调
  - [ ] indicator() 函数
  - [ ] signal() 函数
  - [ ] order() 函数
- [ ] 策略管理
  - [ ] 策略存储（数据库/文件）
  - [ ] 策略加载/卸载
  - [ ] 策略启停控制
- [ ] 前端策略编辑器
  - [ ] Monaco Editor 集成
  - [ ] Lua 语法高亮
  - [ ] 策略验证
  - [ ] 策略测试（回测）

**Lua 策略模板**：
```lua
Strategy {
    name = "My Strategy",
    symbols = {"BTCUSDT"},
    timeframe = "1h"
}

function on_bar(bar, indicators)
    local ma7 = indicators.MA(7)
    local ma25 = indicators.MA(25)
    
    if ma7 > ma25 then
        return Signal.LONG, {
            stop_loss = bar.close * 0.98,
            take_profit = bar.close * 1.05
        }
    end
    
    return Signal.NONE
end
```

---

## 服务部署

### 服务列表

| 服务 | 端口 | 说明 |
|------|------|------|
| trading-core | 8080 | 数据采集 + API |
| trading-indicator | 8081 | 指标计算 |
| trading-engine | - | 交易执行 |
| PostgreSQL | 5432 | 数据存储 |
| Redis | 6379 | 缓存 + 消息 |

### 部署脚本

```bash
# 启动所有服务
./scripts/start-all.sh

# 停止所有服务
./scripts/stop-all.sh

# 查看日志
./scripts/logs.sh
```

---

## 数据库表结构

### 现有表
- `tick_data` - 原始成交数据
- `kline_1m` - 1分钟K线
- `trading_pairs` - 交易对配置
- `strategy_signals` - 策略信号（引擎用）
- `strategy_analysis_log` - 分析日志（前端用）
- `system_config` - 系统配置

### 新增表
- `strategies` - 策略配置
```sql
CREATE TABLE strategies (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    name VARCHAR(100) NOT NULL,
    script TEXT NOT NULL,
    symbols TEXT[] NOT NULL,
    timeframe VARCHAR(10) NOT NULL,
    enabled BOOLEAN DEFAULT true,
    created_at TIMESTAMPTZ DEFAULT NOW(),
    updated_at TIMESTAMPTZ DEFAULT NOW()
);
```

---

## 技术栈

| 组件 | 技术 | 用途 |
|------|------|------|
| 后端 | Rust + Tokio | 高性能异步 |
| 数据库 | PostgreSQL | 持久化存储 |
| 缓存 | Redis | 指标缓存 + 消息 |
| 脚本 | Lua (mlua) | 策略热加载 |
| 前端 | React + Tauri | 桌面应用 |

---

## 进度跟踪

| 阶段 | 状态 | 开始时间 | 完成时间 |
|------|------|----------|----------|
| 阶段1: 指标预计算 | 🔄 进行中 | 2026-07-07 | - |
| 阶段2: 事件驱动 | ⏳ 待开始 | - | - |
| 阶段3: 热加载策略 | ⏳ 待开始 | - | - |
