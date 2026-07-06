# 部署指南

## 📁 目录结构

```
~/apps/                          # 服务运行目录
├── deploy/                    # 执行脚本（从源代码复制）
│   ├── publish.sh            # 日常更新
│   ├── archive.sh            # 数据归档
│   ├── monitor.sh            # 服务监控
│   └── backup.sh             # 数据备份
│
├── trading-core/                # 数据采集服务
│   ├── trading-core            # 二进制文件
│   ├── archive_klines          # 归档工具
│   ├── archive.sh              # 归档脚本
│   ├── config/
│   │   └── production.toml     # 配置文件
│   └── logs/
│
├── trading-engine/              # 交易引擎服务
│   ├── trading-engine          # 二进制文件
│   ├── config/
│   │   └── engine-production.toml
│   └── logs/
│
└── trading-data/                # 数据存储
    ├── parquet/                # Parquet 文件
    └── logs/

~/rust-trade/                   # 项目源代码（保持干净）
├── deploy/                     # 部署脚本（源代码）
│   ├── publish.sh
│   ├── first-time-setup.sh
│   └── ...
├── trading-core/
├── trading-engine/
└── ...
```

## 🚀 快速开始

### 首次部署

```bash
# 1. 克隆代码
cd ~
git clone <repo-url> rust-trade
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
```

## 📋 脚本说明

| 脚本 | 用途 | 执行位置 |
|------|------|----------|
| `first-time-setup.sh` | 首次部署 | `bash ~/apps/deploy/first-time-setup.sh` |
| `publish.sh` | 日常更新 | `bash ~/apps/deploy/publish.sh` |
| `archive.sh` | 数据归档 | `bash ~/apps/deploy/archive.sh` |
| `monitor.sh` | 服务监控 | `bash ~/apps/deploy/monitor.sh` |
| `backup.sh` | 数据备份 | `bash ~/apps/deploy/backup.sh` |

> 💡 首次部署从 `~/rust-trade/deploy/first-time-setup.sh` 执行，后续所有操作都在 `~/apps/deploy/` 中执行

## ⚠️ 重要说明

**代码目录保持干净**：
- 不要在 `~/rust-trade/deploy/` 中执行 `chmod` 或运行脚本
- 所有执行操作都在 `~/apps/deploy/` 中进行
- `publish.sh` 会自动同步更新 `~/apps/deploy/` 中的脚本

## 🔧 常用命令

### 服务管理

```bash
# 查看状态
sudo systemctl status trading-collector
sudo systemctl status trading-engine

# 重启服务
sudo systemctl restart trading-collector trading-engine

# 查看日志
sudo journalctl -u trading-collector -f
sudo journalctl -u trading-engine -f
```

### 数据归档

```bash
# 归档 7 天前的数据
bash ~/apps/deploy/archive.sh --days 7

# 归档指定交易对
bash ~/apps/deploy/archive.sh --symbol BTCUSDT --days 30
```

### 监控

```bash
# 查看服务状态
bash ~/apps/deploy/monitor.sh
```
