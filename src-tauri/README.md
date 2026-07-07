# src-tauri

Tauri 桌面应用，提供实时行情监控、持仓管理、策略分析、回测功能。

## 功能特性

- 📊 实时行情展示（WebSocket + 轮询）
- 📈 K线图表（7种时间框架）
- 💼 持仓监控（实时盈亏）
- 📋 交易历史（分页查询）
- 🎯 策略管理（创建/编辑/启停）
- 🔬 回测分析（多时间框架/滚动前进/样本外）
- 📱 模拟交易（Paper Trading）
- 🌍 中英文切换
- 🌙 深色/浅色主题

## 模块结构

```
src/
├── main.rs                # 入口文件
├── commands.rs            # Tauri 命令
├── types.rs               # 类型定义
├── state.rs               # 状态管理
└── lib.rs
```

## Tauri 命令

### 数据查询

```rust
// 获取数据统计
get_data_info() -> DataInfo

// 获取可用策略
get_available_strategies() -> Vec<StrategyInfo>

// 获取 OHLC 数据
get_ohlc_preview(symbol, timeframe) -> Vec<OHLCData>
```

### 回测功能

```rust
// 运行回测
run_backtest(config) -> BacktestResult

// 多时间框架回测
run_multi_timeframe_backtest(config) -> BacktestResult

// 滚动前进测试
run_walk_forward_test(config) -> WalkForwardResult

// 样本外测试
run_out_of_sample_test(config) -> OutOfSampleResult

// 多交易对回测
run_multi_symbol_backtest(config) -> MultiSymbolResult

// 市场状态分析
analyze_market_state(config) -> MarketStateResult
```

### 模拟交易

```rust
// 启动模拟交易
start_paper_trading(config) -> bool

// 停止模拟交易
stop_paper_trading() -> bool

// 获取模拟交易状态
get_paper_status() -> PaperStatus

// 下单
place_paper_order(order) -> OrderResult

// 获取交易记录
get_paper_trades() -> Vec<Trade>
```

### 策略信号

```rust
// 获取信号历史
get_signal_history(request) -> Vec<Signal>

// 获取信号统计
get_signal_stats(request) -> SignalStats

// 获取调度器状态
get_scheduler_status() -> SchedulerStatus
```

## 前端组件

```
frontend/src/
├── app/
│   ├── page.tsx                   # Dashboard
│   ├── trading/
│   │   ├── page.tsx               # Trading Center
│   │   ├── BacktestContent.tsx    # 回测
│   │   ├── AdvancedBacktestContent.tsx # 高级回测
│   │   ├── PaperTradingContent.tsx # 模拟交易
│   │   └── DataManager.tsx        # 数据管理
│   └── settings/
│       └── page.tsx               # 设置
├── components/
│   └── trading/
│       ├── PriceTicker.tsx        # 价格行情
│       ├── KlineChart.tsx         # K线图
│       ├── PositionTable.tsx      # 持仓列表
│       ├── TradeHistory.tsx       # 交易历史
│       ├── OrderPanel.tsx         # 下单面板
│       ├── AutoTradingStatus.tsx  # 自动交易状态
│       └── SymbolSelect.tsx       # 交易对选择
└── lib/
    └── i18n/                      # 国际化
        ├── context.tsx
        └── translations/
            ├── en.ts
            └── zh.ts
```

## 开发

```bash
# 安装依赖
cd frontend && npm install

# 开发模式
npm run tauri dev

# 构建
npm run tauri build
```

## 依赖

```toml
[dependencies]
tauri = "2.0"
serde = "1.0"
serde_json = "1.0"
tokio = "1.0"
```

```json
// frontend/package.json
{
  "dependencies": {
    "next": "15.0",
    "react": "18.0",
    "recharts": "2.0",
    "tailwindcss": "3.0"
  }
}
```
