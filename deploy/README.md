# 生产服务器部署指南

## 服务器架构

```
┌─────────────────────────────────────────────────────────────────┐
│                      应用服务器 (40GB)                           │
├─────────────────────────────────────────────────────────────────┤
│                                                                  │
│  /opt/trading/                                                   │
│  ├── bin/                    # 二进制文件                        │
│  │   ├── trading-core                                            │
│  │   ├── trading-engine                                          │
│  │   └── archive_klines.rs                                      │
│  │                                                               │
│  ├── config/                 # 配置文件                          │
│  │   ├── production.toml                                        │
│  │   └── engine-production.toml                                 │
│  │                                                               │
│  ├── data/                   # 数据文件                          │
│  │   └── parquet/            # Parquet 历史数据                  │
│  │       ├── BTCUSDT/                                           │
│  │       ├── ETHUSDT/                                           │
│  │       └── ...                                                │
│  │                                                               │
│  ├── logs/                   # 日志文件                          │
│  │   ├── trading-collector.log                                  │
│  │   └── trading-engine.log                                     │
│  │                                                               │
│  └── backups/                # 备份文件                          │
│      └── backup_*.tar.gz                                        │
│                                                                  │
└─────────────────────────────────────────────────────────────────┘

┌─────────────────────────────────────────────────────────────────┐
│                      数据库服务器                                │
├─────────────────────────────────────────────────────────────────┤
│                                                                  │
│  PostgreSQL: 存储实时数据、订单、持仓                            │
│  Redis: 缓存行情数据                                             │
│                                                                  │
└─────────────────────────────────────────────────────────────────┘
```

## 快速部署

### 1. 准备工作

```bash
# 安装 Rust
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# 安装系统依赖
sudo apt-get update
sudo apt-get install -y postgresql-client redis-tools curl wget unzip logrotate
```

### 2. 部署

```bash
# 克隆代码
git clone <repo_url> ~/rust-trade
cd ~/rust-trade/deploy

# 添加执行权限
chmod +x *.sh

# 运行部署脚本
./deploy.sh
```

### 3. 配置

```bash
# 编辑环境变量
vim /opt/trading/.env

# 配置内容:
DATABASE_URL=postgresql://user:password@db-host:5432/trading_core
REDIS_URL=redis://redis-host:6379
BINANCE_API_KEY=your_api_key
BINANCE_API_SECRET=your_api_secret
```

### 4. 启动服务

```bash
# 启动数据采集服务
sudo systemctl start trading-collector

# 启动交易引擎
sudo systemctl start trading-engine

# 查看状态
sudo systemctl status trading-collector
sudo systemctl status trading-engine
```

## 日常运维

### 监控服务

```bash
# 实时监控
./monitor.sh --watch

# 单次检查
./monitor.sh
```

### 数据归档

```bash
# 归档 7 天前的数据
./archive.sh --days 7

# 归档指定交易对
./archive.sh --days 30 --symbol BTCUSDT
```

### 数据备份

```bash
# 备份配置文件
./backup.sh --config

# 备份 Parquet 数据
./backup.sh --parquet

# 完整备份 (配置 + Parquet + 数据库)
./backup.sh --full
```

### 查看日志

```bash
# 实时查看日志
sudo journalctl -u trading-collector -f
sudo journalctl -u trading-engine -f

# 查看最近错误
sudo journalctl -u trading-collector -p err --since "1 hour ago"
```

### 重启服务

```bash
# 重启单个服务
sudo systemctl restart trading-collector
sudo systemctl restart trading-engine

# 重启所有服务
sudo systemctl restart trading-collector trading-engine
```

## 常见问题

### 1. 服务无法启动

```bash
# 检查日志
sudo journalctl -u trading-collector -n 100

# 检查配置
cat /opt/trading/.env

# 检查数据库连接
psql -h db-host -U postgres -d trading_core
```

### 2. 内存不足

```bash
# 检查内存使用
free -h

# 检查进程内存
ps aux | grep trading

# 调整 PostgreSQL 缓存
vim /etc/postgresql/*/main/postgresql.conf
# shared_buffers = 256MB
```

### 3. 磁盘空间不足

```bash
# 检查磁盘使用
df -h

# 清理旧日志
find /opt/trading/logs -name "*.log" -mtime +30 -delete

# 清理旧备份
find /opt/trading/backups -name "*.tar.gz" -mtime +30 -delete
```

## 配置说明

### 环境变量

| 变量 | 说明 | 示例 |
|------|------|------|
| DATABASE_URL | PostgreSQL 连接字符串 | postgresql://user:pass@host:5432/db |
| REDIS_URL | Redis 连接字符串 | redis://host:6379 |
| BINANCE_API_KEY | Binance API Key | your_api_key |
| BINANCE_API_SECRET | Binance API Secret | your_api_secret |
| RUST_LOG | 日志级别 | info / debug / warn |

### 风控参数

| 参数 | 说明 | 默认值 |
|------|------|--------|
| max_position_size | 单笔最大仓位 (USDT) | 1000 |
| max_order_size | 单笔最大下单量 (BTC) | 0.01 |
| stop_loss_pct | 止损百分比 | 2% |
| take_profit_pct | 止盈百分比 | 4% |
| max_daily_loss | 日最大亏损 (USDT) | 200 |
| max_drawdown_pct | 最大回撤百分比 | 15% |

## 脚本说明

| 脚本 | 用途 |
|------|------|
| `deploy.sh` | 首次部署，编译、配置、安装服务 |
| `archive.sh` | 数据归档，PostgreSQL → Parquet |
| `monitor.sh` | 服务监控，检查状态和资源 |
| `backup.sh` | 数据备份，支持增量备份 |
