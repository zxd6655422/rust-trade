# MA Trend Pullback Strategy - 数据库配置指南

## 策略概述

双均线趋势回踩策略已集成到 rust-trade 项目。

**回测验证结果 (30m + MA288止损, 无扩散过滤):**
- BTC: +42.79% (胜率15.2%, 盈亏比10.75)
- ETH: +39.47% (胜率21.4%, 盈亏比7.23)
- SOL: +41.47% (胜率15.3%, 盈亏比2.87)

**回测验证结果 (30m + MA288止损 + 5m扩散过滤):**
- BTC: +40.46% (5m入场+30m趋势+5m扩散, 胜率18.4%)
- ETH: +69.44% (5m入场+30m趋势+5m扩散+夹角>1°, 胜率23.3%)
- SOL: +84.45% (30m MA288+5m+30m双扩散, 胜率21.1%)

## 配置步骤

### 1. 确保数据库表已创建

```bash
# 运行初始化脚本
psql -h 117.72.220.253 -U mydb -d trading_core -f sql/init_database.sql
```

### 2. 插入策略实例配置

```bash
# 运行策略配置脚本
psql -h 117.72.220.253 -U mydb -d trading_core -f sql/strategies/ma_trend_pullback_setup.sql
```

或者直接连接数据库执行:

```bash
# 使用项目环境变量
source .env.development
psql $DATABASE_URL -f sql/strategies/ma_trend_pullback_setup.sql
```

### 3. 验证配置

```sql
-- 查看已配置的策略
SELECT
    id,
    display_name,
    symbols,
    auto_trade,
    status,
    params->>'fast_ma_period' as fast_ma,
    params->>'slow_ma_period' as slow_ma,
    params->>'use_5m_expanding' as use_5m_expanding,
    params->>'min_angle_5m' as min_angle_5m
FROM strategy_instances
WHERE strategy_type = 'ma_trend_pullback'
ORDER BY created_at DESC;
```

### 4. 启动策略服务

```bash
# 启动 strategy-service（模拟模式）
cargo run --package strategy-service
```

## 策略参数说明

| 参数 | 默认值 | 说明 |
|------|--------|------|
| `fast_ma_period` | 288 | 快速均线周期 |
| `slow_ma_period` | 488 | 慢速均线周期 |
| `trailing_activate_pct` | 5.0 | 移动止盈激活阈值(%) |
| `trailing_callback_pct` | 5.0 | 移动止盈回撤阈值(%) |
| `primary_timeframe` | 30m | 主要分析周期 |
| `slope_threshold` | 0 | 均线斜率过滤(0=禁用) |
| `bbw_threshold` | 0 | 布林带宽度过滤(0=禁用) |
| `vol_threshold` | 0 | 成交量比率过滤(0=禁用) |
| `use_5m_expanding` | false | 5m双均线扩散过滤(推荐ETH/SOL启用) |
| `min_angle_5m` | 0 | 最小夹角阈值(0=禁用, 推荐ETH:1.0) |

## 5m扩散过滤配置 (第十三次分析优化)

### 何时启用

- **ETH**: 强烈推荐启用，收益从+29.95%提升到+69.44% (+132%)
- **SOL**: 强烈推荐启用，收益从+41.47%提升到+84.45% (+104%)
- **BTC**: 不建议启用，收益从+42.79%下降到+40.46% (-5%)

### 配置示例

```json
{
    "fast_ma_period": 288,
    "slow_ma_period": 488,
    "stop_mode": "ma288",
    "take_profit_mode": "trailing",
    "trailing_activate_pct": 5.0,
    "trailing_callback_pct": 5.0,
    "use_5m_expanding": true,
    "min_angle_5m": 1.0
}
```

### 参数说明

- `use_5m_expanding`: 启用5m双均线扩散过滤
  - 只在5m MA288/MA488价差扩大时入场
  - 过滤掉收敛阶段的假信号

- `min_angle_5m`: 最小夹角阈值
  - 0: 不限制夹角（仅检查扩散方向）
  - 0.3: 过滤小角度扩散
  - 1.0: 只保留强趋势（推荐ETH使用）

## 模式切换

### 模拟模式 (当前配置)
- `auto_trade = false`
- 策略只生成信号，不自动下单
- 用于观察策略表现

### 自动交易模式
```sql
-- 验证通过后启用
UPDATE strategy_instances
SET auto_trade = true, updated_at = now()
WHERE strategy_type = 'ma_trend_pullback'
  AND display_name = 'MA趋势回踩-BTC';
```

## 监控信号

```sql
-- 查看最近生成的信号
SELECT
    s.id,
    s.symbol,
    s.direction,
    s.entry_price,
    s.confidence,
    s.status,
    s.created_at,
    si.display_name as strategy_name
FROM strategy_signals s
JOIN strategy_instances si ON s.strategy_id = si.id
WHERE si.strategy_type = 'ma_trend_pullback'
ORDER BY s.created_at DESC
LIMIT 20;
```

## 回测命令

```bash
# 运行回测（如果支持CLI）
cargo run --package trading-core -- backtest \
  --strategy ma_trend_pullback \
  --symbol BTCUSDT \
  --timeframe 30m \
  --start-date 2024-01-01 \
  --end-date 2026-07-23
```

## 文件位置

- 策略实现: `trading-common/src/strategy/ma_trend_pullback.rs`
- 回测策略: `trading-common/src/backtest/strategy/ma_trend_pullback.rs`
- 策略适配器: `strategy-service/src/strategies/mod.rs`
- SQL配置: `sql/strategies/ma_trend_pullback_setup.sql`
- JS分析脚本: `trade/src/第十三次分析_双均线扩散.js`
