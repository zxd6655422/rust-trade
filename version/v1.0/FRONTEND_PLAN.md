# 监控前端开发计划

**日期**: 2026-07-04
**状态**: ✅ 已完成

---

## Context

rust-trade 后端 API (P8-P10) 已全部就绪，前端 Trading 页面为空壳。需要：
1. 三个主 TAB 切换：实盘交易、回测、模拟测试
2. 实盘交易内现货/合约子 TAB
3. 完整交易记录 + 策略持仓结算 + 胜率展示
4. 优雅的 UI 风格

## 现有资产

- shadcn/ui: Card, Button, Badge, Input (无 Tabs 组件)
- recharts: 图表库
- lucide-react: 图标
- Tailwind CSS + tailwindcss-animate
- Tauri commands 已全部实现

---

## 实施步骤

### Step 1: 创建 Tabs UI 组件 ✅

**文件**: `frontend/src/components/ui/tabs.tsx`

基于 Tailwind 创建轻量 Tabs 组件 (不依赖 radix，纯 Tailwind 实现)：
- `Tabs`, `TabsList`, `TabsTrigger`, `TabsContent`

### Step 2: 创建 Trading 页类型定义 ✅

**文件**: `frontend/src/types/trading.ts`

从 `src-tauri/src/types.rs` 映射 TypeScript 接口：
- `RealtimePrice`, `KlineData`, `PositionInfo`, `TradeRecord`
- `PnlSummary`, `EquityCurvePoint`, `PerformanceMetrics`, `CommissionStats`

### Step 3: 创建子组件 ✅

**目录**: `frontend/src/components/trading/`

| 组件 | 功能 |
|------|------|
| `PriceTicker.tsx` | 实时价格卡片，显示 BTC/ETH/SOL 价格 + 涨跌 |
| `KlineChart.tsx` | K 线图表，支持时间框架选择 (recharts) |
| `PositionTable.tsx` | 持仓列表表格，显示 symbol/side/qty/pnl |
| `TradeHistory.tsx` | 交易历史表格，分页，完整记录 |
| `PnlSummaryCards.tsx` | 盈亏汇总卡片：总盈亏/胜率/最佳最差 |
| `EquityCurve.tsx` | 资金曲线图 (recharts AreaChart) |
| `PerformancePanel.tsx` | 性能指标面板：夏普/Sortino/回撤/Calmar |
| `CommissionStats.tsx` | 手续费统计，按交易对/按月 |
| `StrategyWinRate.tsx` | 策略持仓结算 + 胜率展示 |

### Step 4: 重写 Trading 页面 ✅

**文件**: `frontend/src/app/trading/page.tsx`

页面结构：
```
┌─────────────────────────────────────────────────┐
│  [实盘交易]  [回测]  [模拟测试]     ← 主 TAB    │
├─────────────────────────────────────────────────┤
│                                                 │
│  实盘交易 TAB 内容:                              │
│  ┌─────────────────────────────────────────┐    │
│  │  [现货]  [合约]           ← 子 TAB      │    │
│  ├─────────────────────────────────────────┤    │
│  │  价格卡片 → K线图表 → 持仓 + 盈亏汇总   │    │
│  │  → 策略胜率 → 资金曲线 + 性能指标       │    │
│  │  → 手续费统计 → 交易历史 (分页)         │    │
│  └─────────────────────────────────────────┘    │
│                                                 │
│  回测 TAB: 复用 BacktestContent 组件            │
│  模拟测试 TAB: 占位 (Coming Soon)               │
│                                                 │
└─────────────────────────────────────────────────┘
```

### Step 5: 更新 Sidebar 导航 ✅

**文件**: `frontend/src/components/layout/Sidebar.tsx`

- 移除旧的 Trading 和 Backtest 独立菜单项
- 改为 Dashboard / Trading / Settings 三项
- 添加图标和活跃状态高亮

### Step 6: 更新 Header ✅

**文件**: `frontend/src/components/layout/Header.tsx`

- 添加 dark mode 支持 (class 策略已在 tailwind 配置中)
- Header 添加深色主题切换按钮
- 显示连接状态

---

## 文件变更清单

| 操作 | 文件 | 说明 |
|------|------|------|
| 新增 | `frontend/src/components/ui/tabs.tsx` | Tabs 组件 |
| 新增 | `frontend/src/types/trading.ts` | 类型定义 |
| 新增 | `frontend/src/components/trading/PriceTicker.tsx` | 实时价格 |
| 新增 | `frontend/src/components/trading/KlineChart.tsx` | K线图表 |
| 新增 | `frontend/src/components/trading/PositionTable.tsx` | 持仓列表 |
| 新增 | `frontend/src/components/trading/TradeHistory.tsx` | 交易历史 |
| 新增 | `frontend/src/components/trading/PnlSummaryCards.tsx` | 盈亏汇总 |
| 新增 | `frontend/src/components/trading/EquityCurve.tsx` | 资金曲线 |
| 新增 | `frontend/src/components/trading/PerformancePanel.tsx` | 性能指标 |
| 新增 | `frontend/src/components/trading/CommissionStats.tsx` | 手续费统计 |
| 新增 | `frontend/src/components/trading/StrategyWinRate.tsx` | 策略胜率 |
| 新增 | `frontend/src/app/trading/BacktestContent.tsx` | 回测内容 |
| 修改 | `frontend/src/app/trading/page.tsx` | 重写为三 TAB |
| 修改 | `frontend/src/components/layout/Sidebar.tsx` | 更新导航 |
| 修改 | `frontend/src/components/layout/Header.tsx` | Dark Mode |

---

## 验证

```bash
cd frontend && npm run dev
```

1. ✅ 三个主 TAB 能正常切换
2. ✅ 现货/合约子 TAB 切换
3. ✅ 价格、持仓、交易历史数据展示
4. ✅ 胜率、资金曲线、性能指标展示
5. ✅ 构建通过 (`npm run build`)
