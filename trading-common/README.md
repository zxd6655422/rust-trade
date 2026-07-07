# trading-common

共享库，包含数据类型定义、指标计算、回测引擎等核心功能。

## 模块结构

```
src/
├── data/
│   ├── types.rs           # 核心数据类型（TickData, OHLCData, Timeframe）
│   ├── repository.rs      # 数据库操作
│   ├── cache.rs           # 缓存层
│   ├── aggregator.rs      # K线聚合器
│   ├── parquet_store.rs   # Parquet 存储
│   └── polars_repository.rs # Polars 查询层
├── backtest/
│   ├── engine.rs          # 回测引擎
│   ├── portfolio.rs       # 投资组合管理
│   ├── metrics.rs         # 绩效指标计算
│   ├── market_state.rs    # 市场状态分析
│   ├── multi_symbol.rs    # 多交易对回测
│   ├── walk_forward.rs    # 滚动前进测试
│   └── strategy/          # 策略框架
│       ├── base.rs        # 策略 trait
│       ├── trend.rs       # 趋势策略
│       └── multi_timeframe.rs # 多时间框架策略
├── pricing/
│   ├── options.rs         # 期权定价（Black-Scholes）
│   └── greeks.rs          # Greeks 计算
├── simulation/
│   ├── brownian.rs        # 布朗运动
│   └── monte_carlo.rs     # 蒙特卡洛模拟
└── lib.rs
```

## 核心功能

### 数据类型

```rust
// 时间框架
pub enum Timeframe {
    OneMinute, FiveMinutes, FifteenMinutes, ThirtyMinutes,
    OneHour, TwoHour, FourHour, OneDay, ThreeDay, OneWeek,
}

// OHLC 数据
pub struct OHLCData {
    pub timestamp: DateTime<Utc>,
    pub symbol: String,
    pub timeframe: Timeframe,
    pub open: Decimal,
    pub high: Decimal,
    pub low: Decimal,
    pub close: Decimal,
    pub volume: Decimal,
}
```

### 指标计算

```rust
// Polars 查询层
let polars = PolarsRepository::new(config);
let sma = polars.calculate_sma(&df, 20)?;
let ema = polars.calculate_ema(&df, 20)?;
let rsi = polars.calculate_rsi(&df, 14)?;
let (macd, signal, hist) = polars.calculate_macd(&df, 12, 26, 9)?;
let (upper, middle, lower) = polars.calculate_bollinger_bands(&df, 20, 2.0)?;
```

### 回测引擎

```rust
let config = BacktestConfig::new(Decimal::from(10000));
let strategy = create_strategy("trend")?;
let mut engine = BacktestEngine::new(strategy, config)?;
let result = engine.run(data);

println!("收益率: {}%", result.return_pct);
println!("胜率: {}%", result.win_rate);
println!("夏普比率: {}", result.sharpe_ratio);
```

### 风险指标

```rust
let portfolio = Portfolio::new(capital);
let sharpe = portfolio.sharpe_ratio(risk_free_rate);
let max_dd = portfolio.max_drawdown();
let sortino = portfolio.sortino_ratio(risk_free_rate);
let var = portfolio.value_at_risk(confidence);
```

## 依赖

```toml
[dependencies]
chrono = "0.4"
rust_decimal = "1.32"
serde = "1.0"
sqlx = "0.7"
redis = "0.23"
polars = "0.35"
```
