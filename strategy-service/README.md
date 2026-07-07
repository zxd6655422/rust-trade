# strategy-service

策略分析服务，负责加载策略配置、运行策略逻辑、生成交易信号、执行自动交易。

## 功能特性

- 📊 7种内置策略（RSI/MACD/布林带/趋势/多时间框架/大周期/成交量）
- 🔄 策略实例管理（动态创建、配置、启停）
- 📈 技术指标计算（动态参数）
- 🔔 信号生成与验证
- 💹 自动交易执行
- 📡 WebSocket 实时推送
- 🔔 告警系统
- 🔄 订单状态同步

## 模块结构

```
src/
├── main.rs                # 入口文件
├── config.rs              # 配置加载
├── db/                    # 数据库操作
│   ├── mod.rs
│   ├── strategies.rs      # 策略实例 CRUD
│   ├── signals.rs         # 信号写入/查询
│   ├── trades.rs          # 交易记录查询
│   └── performance.rs     # 策略性能统计
├── strategies/            # 策略实现
│   ├── mod.rs             # Strategy trait
│   ├── rsi.rs             # RSI 策略
│   ├── macd.rs            # MACD 策略
│   ├── bollinger.rs       # 布林带策略
│   ├── trend.rs           # 趋势策略
│   ├── volume.rs          # 成交量策略
│   ├── multi_tf.rs        # 多时间框架策略
│   └── macro_cycle.rs     # 大周期策略
├── indicators.rs          # 指标计算模块
├── redis_reader.rs        # Redis 数据读取
├── engine.rs              # 策略执行引擎
├── trade_executor.rs      # 交易执行器
├── exchange.rs            # Binance API 客户端
├── okx_client.rs          # OKX API 客户端
├── order_sync.rs          # 订单状态同步
├── websocket.rs           # WebSocket 推送
├── alert.rs               # 告警系统
└── api.rs                 # HTTP API
```

## 使用方法

### 启动服务

```bash
cargo run -p strategy-service
```

### 环境变量

```bash
# 数据库
DATABASE_URL=postgresql://localhost/trading_core
REDIS_URL=redis://localhost:6379

# Binance（用于自动交易）
BINANCE_API_KEY=your_api_key
BINANCE_API_SECRET=your_api_secret
BINANCE_TESTNET=false

# OKX（可选）
OKX_API_KEY=your_api_key
OKX_API_SECRET=your_api_secret
OKX_PASSPHRASE=your_passphrase
```

### API 端点

| 端点 | 方法 | 功能 |
|------|------|------|
| `/api/strategies` | GET | 列出所有策略实例 |
| `/api/strategies` | POST | 创建策略实例 |
| `/api/strategies/{id}` | GET | 获取策略详情 |
| `/api/strategies/{id}` | PUT | 更新策略参数 |
| `/api/strategies/{id}` | DELETE | 删除策略 |
| `/api/signals` | GET | 查询信号 |
| `/api/trades` | GET | 查询交易记录 |
| `/ws/signals` | WebSocket | 实时信号推送 |

## 策略配置示例

### RSI 策略

```json
{
  "strategy_type": "rsi",
  "display_name": "RSI-BTC-激进版",
  "params": {
    "period": 14,
    "overbought": 70,
    "oversold": 30,
    "confirm_candles": 2
  },
  "symbols": ["BTCUSDT"],
  "auto_trade": true,
  "position_size_pct": 10.0
}
```

### 大周期策略

```json
{
  "strategy_type": "macro_cycle",
  "display_name": "BTC周K分析",
  "params": {
    "primary_timeframe": "1w",
    "ma_periods": [20, 50, 100, 200],
    "proximity_threshold": 5.0,
    "adx_threshold": 25.0,
    "lookback_periods": 52
  },
  "symbols": ["BTCUSDT"],
  "auto_trade": true
}
```

## 指标计算

```rust
use strategy_service::indicators;

// 简单移动平均
let ma = indicators::calculate_ma(&klines, 20);

// RSI
let rsi = indicators::calculate_rsi(&klines, 14);

// MACD
let macd = indicators::calculate_macd(&klines, 12, 26, 9);

// 布林带
let bb = indicators::calculate_bollinger(&klines, 20, 2.0);

// ATR
let atr = indicators::calculate_atr(&klines, 14);

// ADX
let adx = indicators::calculate_adx(&klines, 14);
```

## 自动交易流程

```
1. 策略信号触发
   ↓
2. 创建订单组（主订单 + 止损单 + 止盈单）
   ↓
3. 验证订单
   - 交易类型检查
   - 订单重复检查
   - 仓位阈值检查
   - 账户余额检查
   - 交易对精度检查
   ↓
4. 提交订单到交易所
   ↓
5. 订单状态同步（每10秒）
   ↓
6. 订单成交后自动更新持仓
```

## WebSocket 消息格式

```json
{
  "msg_type": "signal",
  "data": {
    "id": "uuid",
    "symbol": "BTCUSDT",
    "strategy": "macro_cycle",
    "direction": "bullish",
    "entry_price": 50000.0,
    "signal_strength": 0.75,
    "confidence": 0.8,
    "reason": "接近历史支撑位",
    "stop_loss": 49000.0,
    "take_profit": 53000.0,
    "auto_trade": true,
    "created_at": "2026-07-07T12:00:00Z"
  }
}
```

## 依赖

```toml
[dependencies]
trading-common = { path = "../trading-common" }
tokio = { version = "1", features = ["full"] }
axum = { version = "0.7", features = ["ws"] }
sqlx = "0.7"
redis = "0.23"
reqwest = "0.11"
hmac = "0.12"
sha2 = "0.10"
serde = "1.0"
tracing = "0.1"
```
