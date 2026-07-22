# 双均线趋势回踩策略 - 实现计划

## 策略概述

基于MA288/MA488双均线的趋势回踩入场策略，通过多周期K线数据进行信号确认和止盈止损管理。

---

## 核心策略配置

### 策略1: 30m + MA288止损 (推荐首选)

```
时间周期: 30分钟K线
趋势判断: MA288 vs MA488
  - MA288 > MA488 → 多头趋势
  - MA288 < MA488 → 空头趋势

入场信号: 价格回踩MA288
  - 多头: open < MA288 且 close > MA288 (回踩后反弹)
  - 空头: open > MA288 且 close < MA288 (反弹后回落)

止损方式: MA288交叉止损
  - 多头止损: open > MA288 且 close < MA288 (价格跌破MA288)
  - 空头止损: open < MA288 且 close > MA288 (价格突破MA288)

止盈方式: 移动止盈
  - 激活阈值: 4% (盈利超过4%后开始追踪)
  - 回撤阈值: 2% (从最高点回撤2%止盈)

历史回测结果:
  - 交易数: 403笔/20个月
  - 胜率: 15.6%
  - 总收益: +45.05%
  - 平均收益: +0.112%/笔
  - 最大盈利: 16.89%
  - 盈亏比: 7.19
```

### 策略2: 30m + 固定止损 (高胜率版)

```
时间周期: 30分钟K线
趋势判断: MA288 vs MA488 (同上)

入场信号: 价格回踩MA288 (同上)

入场过滤:
  - MA288斜率 > 5 bps (趋势明确)
  - 布林带带宽 > 2.0% (波动充足)
  - 成交量 > 20日均量 × 0.6 (有量支撑)

止损方式: 固定百分比止损
  - 止损幅度: 2%

止盈方式: 移动止盈
  - 激活阈值: 3%
  - 回撤阈值: 3%

历史回测结果:
  - 交易数: ~33笔/20个月
  - 胜率: ~48%
  - 总收益: +37%
  - 盈亏比: ~3.5
```

### 策略3: 5m + MA288止损 (高频版)

```
时间周期: 5分钟K线
趋势判断: 5m MA288 vs MA488

入场信号: 价格回踩MA288 (同上)

入场过滤:
  - 30m趋势方向必须一致

止损方式: MA288交叉止损 (同上)

止盈方式: 移动止盈
  - 激活阈值: 1.5%
  - 回撤阈值: 1%

历史回测结果:
  - 交易数: 416笔/6个月
  - 胜率: 16.8%
  - 总收益: +8.86%
  - 盈亏比: 1.09
```

---

## 技术指标计算

### MA (移动平均线)

```rust
fn calc_ma(closes: &[f64], period: usize) -> Vec<Option<f64>> {
    let mut result = vec![None; closes.len()];
    for i in (period - 1)..closes.len() {
        let sum: f64 = closes[(i - period + 1)..=i].iter().sum();
        result[i] = Some(sum / period as f64);
    }
    result
}
```

### 布林带

```rust
fn calc_bollinger(closes: &[f64], period: usize, std_dev: f64) -> (Vec<Option<f64>>, Vec<Option<f64>>, Vec<Option<f64>>) {
    let ma = calc_ma(closes, period);
    let mut upper = vec![None; closes.len()];
    let mut lower = vec![None; closes.len()];
    let mut width = vec![None; closes.len()];

    for i in (period - 1)..closes.len() {
        if let Some(mid) = ma[i] {
            let sum_sq: f64 = closes[(i - period + 1)..=i]
                .iter()
                .map(|&x| (x - mid).powi(2))
                .sum();
            let std = (sum_sq / period as f64).sqrt();
            upper[i] = Some(mid + std_dev * std);
            lower[i] = Some(mid - std_dev * std);
            width[i] = Some((upper[i].unwrap() - lower[i].unwrap()) / mid * 100.0);
        }
    }

    (ma, upper, lower, width)
}
```

### MA288斜率

```rust
fn calc_slope(ma: &[Option<f64>], period: usize) -> Vec<Option<f64>> {
    let mut result = vec![None; ma.len()];
    for i in period..ma.len() {
        if let (Some(curr), Some(prev)) = (ma[i], ma[i - period]) {
            if prev != 0.0 {
                result[i] = Some((curr - prev) / prev * 10000.0); // bps
            }
        }
    }
    result
}
```

---

## 策略状态机

```
┌─────────────────────────────────────────────────────────────┐
│                         状态机                              │
├─────────────────────────────────────────────────────────────┤
│                                                             │
│   [空仓] ──────────────────────────────────────────────►   │
│     │                                                       │
│     │ 入场信号:                                             │
│     │   多头: open < MA288 && close > MA288                │
│     │   空头: open > MA288 && close < MA288                │
│     ▼                                                       │
│   [持仓] ──────────────────────────────────────────────►   │
│     │                                                       │
│     │ 止损: MA288交叉 (价格穿越MA288)                      │
│     │ 止盈: 移动止盈 (盈利>4%, 回撤>2%)                    │
│     │ 反向: 趋势反转时平仓                                  │
│     ▼                                                       │
│   [平仓] ──────────────────────────────────────────────►   │
│     │                                                       │
│     │ 记录交易                                              │
│     │ 更新统计                                              │
│     ▼                                                       │
│   [空仓] ◄──────────────────────────────────────────────   │
│                                                             │
└─────────────────────────────────────────────────────────────┘
```

---

## 实现模块

### 1. 指标计算模块

```rust
// src/indicators.rs

pub struct Indicators {
    pub ma48: Vec<Option<f64>>,
    pub ma288: Vec<Option<f64>>,
    pub ma488: Vec<Option<f64>>,
    pub bb_mid: Vec<Option<f64>>,
    pub bb_upper: Vec<Option<f64>>,
    pub bb_lower: Vec<Option<f64>>,
    pub bb_width: Vec<Option<f64>>,
    pub ma288_slope: Vec<Option<f64>>,
    pub vol_ma: Vec<Option<f64>>,
    pub vol_ratio: Vec<Option<f64>>,
}

impl Indicators {
    pub fn new(klines: &[Kline]) -> Self { ... }
    pub fn update(&mut self, kline: &Kline) { ... }
}
```

### 2. 策略引擎模块

```rust
// src/strategy.rs

pub enum Trend {
    Bullish,
    Bearish,
    Neutral,
}

pub enum Position {
    Long,
    Short,
    None,
}

pub struct Strategy {
    pub config: StrategyConfig,
    pub state: Position,
    pub entry_price: f64,
    pub max_profit: f64,
}

impl Strategy {
    pub fn new(config: StrategyConfig) -> Self { ... }
    pub fn on_kline(&mut self, kline: &Kline, indicators: &Indicators) -> Option<Signal> { ... }
    pub fn check_stop_loss(&self, kline: &Kline, indicators: &Indicators) -> Option<Signal> { ... }
    pub fn check_take_profit(&self, kline: &Kline, indicators: &Indicators) -> Option<Signal> { ... }
}
```

### 3. 信号模块

```rust
// src/signal.rs

pub enum SignalType {
    EntryLong,
    EntryShort,
    ExitLong,
    ExitShort,
    StopLoss,
    TakeProfit,
}

pub struct Signal {
    pub time: DateTime<Utc>,
    pub signal_type: SignalType,
    pub price: f64,
    pub reason: String,
}
```

### 4. 回测模块

```rust
// src/backtest.rs

pub struct Backtest {
    pub strategy: Strategy,
    pub trades: Vec<Trade>,
    pub equity_curve: Vec<f64>,
}

impl Backtest {
    pub fn run(&mut self, klines: &[Kline]) -> BacktestResult { ... }
    pub fn calculate_stats(&self) -> TradeStats { ... }
}
```

---

## 实现步骤

### Phase 1: 基础框架 (1-2天)

- [ ] 创建K线数据结构
- [ ] 实现指标计算模块 (MA, BB, Slope)
- [ ] 实现基础策略状态机

### Phase 2: 策略实现 (2-3天)

- [ ] 实现入场信号检测
- [ ] 实现MA288交叉止损
- [ ] 实现移动止盈逻辑
- [ ] 实现趋势反转平仓

### Phase 3: 回测验证 (1-2天)

- [ ] 实现回测引擎
- [ ] 验证回测结果与分析一致
- [ ] 添加统计指标计算

### Phase 4: 实盘接口 (2-3天)

- [ ] 对接交易所API (Binance/OKX)
- [ ] 实现订单管理
- [ ] 添加风控模块
- [ ] 实现日志和监控

### Phase 5: 优化迭代 (持续)

- [ ] 添加多策略支持
- [ ] 实现参数优化
- [ ] 添加机器学习信号过滤

---

## 风险提示

1. **过拟合风险**: 策略在历史数据上表现良好，但未来市场可能不同
2. **滑点风险**: 实际交易会有滑点，影响收益
3. **资金管理**: 单笔交易风险建议控制在总资金的1-2%
4. **市场环境**: 策略在趋势行情中表现好，震荡行情可能亏损

---

## 参考资料

- 分析脚本: `F:\rust_projects\trade\src\`
- K线数据: `F:\rust_projects\trade\src\kline_*.csv`
- 外部API: `F:\rust_projects\rust-trade\api-doc\`
