# Rust Trade

一个全面的加密货币交易系统，支持实时数据采集、高级回测功能和专业桌面界面。

[![Rust](https://img.shields.io/badge/rust-1.70+-orange.svg)](https://www.rust-lang.org/)
[![License](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![Platform](https://img.shields.io/badge/platform-Windows%20%7C%20macOS%20%7C%20Linux-lightgrey.svg)](https://tauri.app/)

## 🎯 概述

Rust Trade 将高性能市场数据处理与复杂的回测工具相结合，为加密货币量化交易提供了完整的解决方案。该系统具有从交易所实时采集数据、支持多种策略的强大回测引擎，以及直观的桌面界面。

## 🏗️ 架构

### **实时数据采集模式**
```
┌─────────────────┐    ┌─────────────────┐    ┌─────────────────┐
│   交易所        │───▶│    服务层       │───▶│   数据仓库      │
│   (WebSocket)   │    │  (数据处理)     │    │   (持久化存储)   │
└─────────────────┘    └─────────────────┘    └─────────────────┘
         │                       │                       │
         │                       ▼                       ▼
    币安 API             ┌─────────────┐         ┌─────────────┐
    - 实时数据            │ 多级缓存    │         │ PostgreSQL  │
    - 模拟交易            │ (L1 + L2)   │         │  数据库     │
                         └─────────────┘         └─────────────┘
                                   │
                                   ▼
                         ┌─────────────────┐
                         │   模拟交易      │
                         │     引擎        │
                         └─────────────────┘
```

### **桌面应用模式**
```
┌─────────────────┐    ┌─────────────────┐    ┌─────────────────┐
│   Next.js       │───▶│  Tauri 命令层   │───▶│  交易公共库     │
│   前端          │    │  (src-tauri)    │    │   (Library)     │
└─────────────────┘    └─────────────────┘    └─────────────────┘
                                                       │
                               ┌───────────────────────┴───────────────────────┐
                               ▼                                               ▼
                       ┌─────────────────┐                             ┌─────────────────┐
                       │  回测引擎       │                             │   数据仓库      │
                       │  + 策略模块     │                             │   + 数据库      │
                       └─────────────────┘                             └─────────────────┘
```

## 📁 项目结构
```
rust-trade/
├── assets/                # 项目资源和截图
├── config/                # 全局配置文件
│   ├── development.toml   # 开发环境配置
│   ├── production.toml    # 生产环境配置
│   ├── schema.sql         # PostgreSQL 表定义
│   └── test.toml          # 测试环境配置
├── frontend/              # Next.js 前端应用
│   ├── src/               # 前端源代码
│   │   ├── app/           # App 路由页面
│   │   │   ├── page.tsx   # 仪表盘首页
│   │   │   └── backtest/  # 回测界面
│   │   ├── components/    # 可复用 UI 组件
│   │   │   ├── layout/    # 布局组件
│   │   │   └── ui/        # shadcn/ui 组件
│   │   └── types/         # TypeScript 类型定义
│   ├── tailwind.config.js # Tailwind CSS 配置
│   └── package.json       # 前端依赖
├── src-tauri/             # 桌面应用后端
│   ├── src/               # Tauri 命令处理和状态管理
│   │   ├── commands.rs    # Tauri 命令实现
│   │   ├── main.rs        # 应用入口
│   │   ├── state.rs       # 应用状态管理
│   │   └── types.rs       # 前端接口类型
│   ├── Cargo.toml         # Tauri 依赖（使用 trading-common）
│   └── tauri.conf.json    # Tauri 配置
├── trading-common/        # 所有 crate 的共享库
│   ├── src/
│   │   ├── backtest/      # 回测引擎和策略
│   │   │   ├── engine.rs  # 核心回测逻辑
│   │   │   ├── metrics.rs # 性能指标计算
│   │   │   ├── portfolio.rs # 投资组合管理
│   │   │   └── strategy/  # 交易策略（RSI、SMA）
│   │   ├── data/          # 数据层
│   │   │   ├── cache.rs   # 多级缓存系统
│   │   │   ├── repository.rs # 数据库操作
│   │   │   └── types.rs   # 核心数据结构
│   │   └── lib.rs         # 库入口
│   └── Cargo.toml         # 公共依赖
├── trading-core/          # CLI 交易系统
│   ├── src/
│   │   ├── exchange/      # 交易所集成
│   │   │   └── binance.rs # 币安 WebSocket 客户端
│   │   ├── live_trading/  # 模拟交易系统
│   │   │   └── paper_trading.rs # 实时策略执行
│   │   ├── service/       # 业务逻辑层
│   │   │   └── market_data.rs # 数据处理服务
│   │   ├── config.rs      # 配置管理
│   │   ├── lib.rs         # 库入口（重新导出 trading-common）
│   │   └── main.rs        # CLI 应用入口
│   ├── benches/           # 性能基准测试
│   ├── Cargo.toml         # 核心依赖
│   └── README.md          # 核心系统文档
└── README.md              # 本文件
```

## 🚀 快速开始

### 前置条件

- **Rust 1.70+** - [安装 Rust](https://rustup.rs/)
- **Node.js 18+** - [安装 Node.js](https://nodejs.org/)
- **PostgreSQL 12+** - [安装 PostgreSQL](https://www.postgresql.org/download/)
- **Redis 6+** - [安装 Redis](https://redis.io/download/)（可选但推荐）

### 1. 克隆仓库

```bash
git clone https://github.com/Erio-Harrison/rust-trade.git
cd rust-trade
```

### 2. 数据库设置

```bash
# 创建数据库
createdb trading_core

# 初始化表结构
运行 config 目录下的 SQL 命令来创建数据库表。
```

### 3. 环境配置

在根目录和 `trading-core/` 目录下分别创建 `.env` 文件：

```bash
# .env
DATABASE_URL=postgresql://username:password@localhost/trading_core
REDIS_URL=redis://127.0.0.1:6379
RUN_MODE=development
```

### 4. 安装依赖

```bash
# 安装 Rust 依赖
cd trading-core
cargo build
cd ..

# 安装前端依赖
cd frontend
npm install
cd ..

# 安装 Tauri 依赖
cd src-tauri
cargo build
cd ..
```

## 🎮 运行应用

### 方式一：桌面应用（推荐）

```bash
# 开发模式（支持热重载）
cd frontend && npm run tauri dev
# 或者
cd frontend && cargo tauri dev

# 生产构建
cd frontend && npm run tauri build
# 或者
cd frontend && cargo tauri build
```

### 方式二：核心交易系统（CLI）

```bash
cd trading-core

# 启动实时数据采集
cargo run live

# 启动实时数据采集并开启模拟交易
cargo run live --paper-trading

# 运行回测界面
cargo run backtest

# 查看帮助
cargo run -- --help
```

### 方式三：仅 Web 界面

```bash
cd frontend

# 开发服务器
npm run dev

# 生产构建
npm run build
npm start
```

## 📊 功能特性

### **实时数据采集**
- 通过 WebSocket 实时连接加密货币交易所
- 高性能数据处理（单条插入约 390µs，批量插入约 13ms）
- 基于 Redis 和内存的多级缓存系统
- 自动重试机制和错误处理

### **高级回测**
- 多种交易策略（SMA、RSI）
- 专业性能指标（夏普比率、最大回撤、胜率）
- 投资组合管理和盈亏追踪
- 交互式参数配置

### **桌面界面**
- 实时数据可视化
- 直观的策略配置
- 综合的结果分析
- 跨平台支持（Windows、macOS、Linux）

## 🖼️ 截图

### 回测配置
![回测配置](assets/backtestPage1.png)

### 结果仪表盘
![结果仪表盘](assets/backtestPage2.png)

### 交易分析
![交易分析](assets/backtestPage3.png)

## ⚙️ 配置

### 交易品种

编辑 `config/development.toml`：

```toml
# 要监控的交易对
symbols = ["BTCUSDT", "ETHUSDT", "ADAUSDT"]

[server]
host = "0.0.0.0"
port = 8080

[database]
max_connections = 5
min_connections = 1
max_lifetime = 1800

[cache]
[cache.memory]
max_ticks_per_symbol = 1000
ttl_seconds = 300

[cache.redis]
pool_size = 10
ttl_seconds = 3600
max_ticks_per_symbol = 10000
```

### 日志配置

通过环境变量设置日志级别：

```bash
# 应用日志
RUST_LOG=trading_core=info

# 调试模式
RUST_LOG=trading_core=debug,sqlx=info
```

## 📈 性能

基于全面的基准测试结果：

| 操作 | 性能 | 应用场景 |
|------|------|----------|
| 单条数据插入 | ~390µs | 实时数据 |
| 批量插入（100条） | ~13ms | 批量处理 |
| 缓存命中 | ~10µs | 数据查询 |
| 历史数据查询 | ~450µs | 回测 |

## 🔧 开发

### 运行测试

```bash
# 核心系统测试
cd trading-core
cargo test

# 基准测试
cargo bench

# 前端测试
cd frontend
npm test
```

### 生产构建

```bash
# 构建交易核心
cd trading-core
cargo build --release

# 构建桌面应用
cd ../frontend
npm run tauri build

# 构建 Web 界面
npm run build
```

## 📚 文档

- **交易核心**：详见 `trading-core/README.md` 获取后端文档
- **桌面应用**：详见 `src-tauri/README.md` 获取 Tauri 应用详情

## 🤝 贡献指南

1. Fork 本仓库
2. 创建你的功能分支（`git checkout -b feature/amazing-feature`）
3. 提交你的更改（`git commit -m 'Add amazing feature'`）
4. 推送到分支（`git push origin feature/amazing-feature`）
5. 创建一个 Pull Request

## 📄 许可证

本项目基于 MIT 许可证 - 详情参见 [LICENSE](LICENSE) 文件。

## 👨‍💻 作者

**Erio Harrison** - [GitHub](https://github.com/Erio-Harrison)


---

使用 Rust、Tauri 和 Next.js ❤️ 构建
