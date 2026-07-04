# 生产服务器部署指南

## 服务器架构

```
~/rust-trade/                  # 代码仓库
~/apps/
├── trading-core/              # 数据采集服务
│   ├── trading-core           # 二进制
│   ├── archive_klines         # 归档工具
│   ├── archive.sh             # 归档脚本
│   ├── config/
│   │   └── production.toml    # 配置
│   └── logs/
│
├── trading-engine/            # 交易引擎
│   ├── trading-engine         # 二进制
│   ├── config/
│   │   └── engine-production.toml
│   └── logs/
│
└── trading-data/              # 归档数据
    ├── parquet/               # Parquet 历史数据
    └── logs/                  # 归档日志
```

## 首次部署

```bash
# 1. 克隆代码
git clone <repo_url> ~/rust-trade
cd ~/rust-trade/deploy
chmod +x *.sh

# 2. 执行首次部署脚本
bash first-time-setup.sh

# 3. 编辑配置（脚本会提示具体路径）
vim ~/apps/trading-core/config/production.toml
vim ~/apps/trading-engine/config/engine-production.toml

# 4. 启动服务
sudo systemctl start trading-collector
sudo systemctl start trading-engine
```

## 日常更新

```bash
# 一条命令完成：拉取 → 编译 → 停服 → 部署 → 启动
bash ~/rust-trade/deploy/publish.sh
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

| 脚本 | 用途 | 执行频率 |
|------|------|----------|
| `first-time-setup.sh` | 首次部署：编译、创建目录、安装 systemd | 只需一次 |
| `publish.sh` | 日常更新：拉取、编译、替换二进制、重启 | 每次更新 |
| `monitor.sh` | 服务监控，检查状态和资源 | 按需 |
| `archive.sh` | 数据归档，PostgreSQL → Parquet | 定期 |
| `backup.sh` | 数据备份，支持增量备份 | 定期 |
