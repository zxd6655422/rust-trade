# 生产服务器部署指南

## 📁 服务器架构

```
~/rust-trade/                  # 项目源代码目录（保持干净）
├── deploy/                    # 部署脚本（源代码）
│   ├── publish.sh
│   ├── first-time-setup.sh
│   └── ...
├── trading-core/
├── trading-engine/
└── ...

~/apps/                         # 服务运行目录
├── deploy/                    # 执行脚本（从源代码复制）
│   ├── publish.sh            # 日常更新
│   ├── archive.sh            # 数据归档
│   ├── monitor.sh            # 服务监控
│   └── backup.sh             # 数据备份
│
├── trading-core/               # 数据采集服务
│   ├── trading-core           # 二进制文件
│   ├── archive_klines         # 归档工具
│   ├── archive.sh             # 归档脚本
│   ├── config/
│   │   └── production.toml    # 配置文件
│   └── logs/
│
├── trading-engine/             # 交易引擎服务
│   ├── trading-engine         # 二进制文件
│   ├── config/
│   │   └── engine-production.toml
│   └── logs/
│
└── trading-data/               # 数据存储
    ├── parquet/               # Parquet 历史数据
    └── logs/                  # 归档日志
```

## 🚀 快速开始

### 首次部署

```bash
# 1. 克隆代码
cd ~
git clone <repo_url> rust-trade
cd rust-trade

# 2. 执行首次部署（会自动复制脚本到 ~/apps/deploy/）
bash deploy/first-time-setup.sh

# 3. 后续操作都可以从 ~/apps/deploy/ 执行
bash ~/apps/deploy/first-time-setup.sh  # 也可以这样执行
```

### 日常更新（一键部署）

```bash
# ✅ 推荐：从 ~/apps/deploy/ 执行（代码目录保持干净）
bash ~/apps/deploy/publish.sh

# 完整更新（拉取 + 编译 + 部署 + 重启）
bash ~/apps/deploy/publish.sh

# 跳过编译（只部署）
bash ~/apps/deploy/publish.sh --skip-build

# 不重启服务
bash ~/apps/deploy/publish.sh --no-restart
```

### 数据归档

**自动归档（推荐）：**
- 首次部署时会自动安装 `trading-archive.timer`
- 每天自动执行归档，保留 7 天数据
- 查看定时任务状态：`systemctl status trading-archive.timer`

**手动归档：**
```bash
# 归档 7 天前的数据
bash ~/apps/deploy/archive.sh --days 7

# 归档指定交易对
bash ~/apps/deploy/archive.sh --symbol BTCUSDT --days 30
```

## 📋 脚本说明

| 脚本 | 用途 | 执行位置 |
|------|------|----------|
| `first-time-setup.sh` | 首次部署 | `bash ~/apps/deploy/first-time-setup.sh` |
| `publish.sh` | 日常更新 | `bash ~/apps/deploy/publish.sh` |
| `archive.sh` | 数据归档 | `bash ~/apps/deploy/archive.sh` |
| `monitor.sh` | 服务监控 | `bash ~/apps/deploy/monitor.sh` |
| `backup.sh` | 数据备份 | `bash ~/apps/deploy/backup.sh` |
| `logs.sh` | 查看日志 | `bash ~/apps/deploy/logs.sh` |

> 💡 首次部署从 `~/rust-trade/deploy/first-time-setup.sh` 执行，后续所有操作都在 `~/apps/deploy/` 中执行

## 🔧 常用命令

### 服务管理

```bash
# 查看状态
sudo systemctl status trading-collector
sudo systemctl status trading-engine

# 重启服务
sudo systemctl restart trading-collector trading-engine
```

### 查看日志

```bash
# 使用 logs.sh 脚本（推荐）
bash ~/apps/deploy/logs.sh                           # 查看采集服务最近 100 行
bash ~/apps/deploy/logs.sh trading-engine -f         # 实时跟踪引擎日志
bash ~/apps/deploy/logs.sh -n 50                     # 查看最近 50 行
bash ~/apps/deploy/logs.sh -s '1 hour ago'           # 最近 1 小时的日志
bash ~/apps/deploy/logs.sh -p err                    # 只看错误日志
bash ~/apps/deploy/logs.sh -s today                  # 今天的日志

# 直接使用 journalctl
sudo journalctl -u trading-collector -f
sudo journalctl -u trading-engine -f
```

### 数据备份

```bash
# 备份配置文件
bash ~/apps/deploy/backup.sh --config

# 备份 Parquet 数据
bash ~/apps/deploy/backup.sh --parquet

# 完整备份
bash ~/apps/deploy/backup.sh --full
```

## ⚠️ 注意事项

1. **代码目录保持干净**：不要在 `~/rust-trade/deploy/` 中执行 `chmod`，执行脚本在 `~/apps/deploy/`
2. **首次部署前**：确保已安装 Rust、PostgreSQL、Redis
3. **配置文件**：首次部署后需要手动编辑配置文件
4. **环境变量**：trading-engine 需要 `.env` 文件
5. **更新脚本**：执行 `publish.sh` 会自动同步更新 `~/apps/deploy/` 中的脚本
