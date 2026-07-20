# 概览页面实施计划

## Context

当前 Dashboard (`/`) 是一个数据概览/欢迎页，包含 OHLC 预览、策略能力矩阵、快速回测等功能。这些更适合放在回测或策略页面。需要创建一个真正的运营仪表盘作为新的首页，让交易者 5 秒内了解账户状态。

## 实施步骤

### 1. 添加 i18n 翻译键

**文件**: `src/lib/i18n/translations/en.ts` + `zh.ts`

在 `en.ts` 的 `export const en = {...}` 对象中添加 `overview` section，在 `zh.ts` 中添加对应中文翻译。

### 2. 更新 Sidebar 导航

**文件**: `src/components/layout/Sidebar.tsx`

- 将 `menuItems` 改为 6 项：概览、行情、交易、策略、回测、设置
- 使用新图标：`LayoutDashboard`(概览)、`LineChart`(行情)、`Activity`(交易)、`Brain`(策略)、`Flask`(回测)、`Settings`(设置)
- 更新 i18n key 引用

### 3. 创建概览页面主文件

**文件**: `src/app/overview/page.tsx`

结构：
```tsx
'use client'
// 并行获取 pnl_summary + positions + signal_history + scheduler_status
// 使用 Promise.all 并行请求
// 30s 自动刷新
// 布局: space-y-6, grid 统计卡片 + 2x2 grid(曲线+持仓, 信号+状态)
```

### 4. 创建 StatCards 组件

**文件**: `src/components/overview/StatCards.tsx`

5 个卡片：总资产、今日PnL、持仓数、胜率、信号数
- 使用 `grid grid-cols-5 gap-4`
- 每个卡片带图标、数值、副标题（涨跌幅/笔数等）
- 正数绿色、负数红色

### 5. 创建 ActivePositions 组件

**文件**: `src/components/overview/ActivePositions.tsx`

精简版持仓表：
- 只显示：交易对、方向、未实现盈亏
- 最多显示 5 条，超出显示"查看全部"
- 复用 `get_positions` 命令

### 6. 创建 RecentSignals 组件

**文件**: `src/components/overview/RecentSignals.tsx`

精简版信号列表：
- 只显示：交易对、方向、状态、盈亏、时间
- 最多显示 5 条
- 复用 `get_signal_history` 命令

### 7. 创建 SystemStatus 组件

**文件**: `src/components/overview/SystemStatus.tsx`

系统状态卡片：
- Trading Core 连接状态
- Database 连接状态
- 自动交易状态（运行中/暂停/停止）
- 数据覆盖天数
- 当前交易所
- 复用 `useConnection()` context

### 8. 更新路由

- 将 `/` (原 Dashboard) 重定向到 `/overview`
- 原 Dashboard 保留为 `/dashboard`（可在 Sidebar 中隐藏或保留为"数据"入口）

## 文件清单

| 操作 | 文件 |
|------|------|
| 修改 | `src/lib/i18n/translations/en.ts` |
| 修改 | `src/lib/i18n/translations/zh.ts` |
| 修改 | `src/components/layout/Sidebar.tsx` |
| 新建 | `src/app/overview/page.tsx` |
| 新建 | `src/components/overview/StatCards.tsx` |
| 新建 | `src/components/overview/ActivePositions.tsx` |
| 新建 | `src/components/overview/RecentSignals.tsx` |
| 新建 | `src/components/overview/SystemStatus.tsx` |

## 验证

1. `npm run dev` 启动开发服务器
2. 访问 `/overview` 确认页面正常加载
3. 确认 5 个统计卡片正确显示数据
4. 确认收益曲线正确渲染
5. 确认持仓表和信号列表正确显示
6. 确认系统状态正确反映连接状态
7. 确认 30s 自动刷新正常工作
8. 确认 Sidebar 导航正确高亮和跳转
9. 确认中英文切换正常
