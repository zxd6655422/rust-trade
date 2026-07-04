# 部署指南

## 目录结构

服务器上的目录结构：

```
~/
├── rust-trade/                    # 源代码目录
│   ├── deploy/
│   │   ├── publish.sh             # 发布脚本
│   │   ├── install-systemd.sh     # systemd 安装脚本
│   │   └── README.md              # 本文档
│   └── ...
│
└── apps/                          # 应用部署目录
    ├── trading-core/
    │   ├── trading-core           # 二进制文件
    │   ├── config/
    │   │   └── production.toml
    │   ├── logs/
    │   └── start.sh
    │
    └── trading-engine/
        ├── trading-engine         # 二进制文件
        ├── config/
        │   └── engine-production.toml
        ├── logs/
        ├── .env                   # 环境变量
        └── start.sh
```

## 首次部署

### 1. 安装 systemd 服务 (只需执行一次)

```bash
cd ~/rust-trade
sudo bash deploy/install-systemd.sh
```

### 2. 配置环境变量

```bash
# 编辑 trading-engine 的环境变量
nano ~/apps/trading-engine/.env
```

内容：
```bash
DATABASE_URL=postgresql://user:password@localhost:5432/trading_core
REDIS_URL=redis://:password@localhost:6379
BINANCE_API_KEY=your_api_key
BINANCE_API_SECRET=your_api_secret
```

### 3. 启动服务

```bash
sudo systemctl start trading-collector
sudo systemctl start trading-engine

# 设置开机自启
sudo systemctl enable trading-collector
sudo systemctl enable trading-engine
```

## 日常发布

每次代码更新后，执行：

```bash
cd ~/rust-trade
bash deploy/publish.sh
```

脚本会自动：
1. 拉取最新代码
2. 编译 release 版本
3. 停止当前服务
4. 复制新版本到 apps/
5. 启动服务

## 常用命令

### 服务管理

```bash
# 启动
sudo systemctl start trading-collector
sudo systemctl start trading-engine

# 停止
sudo systemctl stop trading-collector
sudo systemctl stop trading-engine

# 重启
sudo systemctl restart trading-collector
sudo systemctl restart trading-engine

# 查看状态
sudo systemctl status trading-collector
sudo systemctl status trading-engine
```

### 查看日志

```bash
# 实时日志
sudo journalctl -u trading-collector -f
sudo journalctl -u trading-engine -f

# 最近 100 行
sudo journalctl -u trading-collector -n 100
sudo journalctl -u trading-engine -n 100

# 今天的日志
sudo journalctl -u trading-collector --since today
sudo journalctl -u trading-engine --since today

# 日志文件
tail -f ~/apps/trading-core/logs/*.log
tail -f ~/apps/trading-engine/logs/*.log
```

### 进程管理

```bash
# 查看进程
ps aux | grep trading

# 查看端口
netstat -tlnp | grep 8080
```

## 故障排查

### 服务启动失败

```bash
# 查看详细错误
sudo journalctl -u trading-collector -n 50 --no-pager
sudo journalctl -u trading-engine -n 50 --no-pager

# 检查二进制文件
ls -la ~/apps/trading-core/trading-core
ls -la ~/apps/trading-engine/trading-engine

# 手动运行测试
cd ~/apps/trading-core
./trading-core service

cd ~/apps/trading-engine
./trading-engine
```

### 数据库连接失败

```bash
# 测试 PostgreSQL 连接
psql -h localhost -U your_user -d trading_core

# 测试 Redis 连接
redis-cli -h localhost -a your_password ping
```

### 日志轮转

```bash
# 安装 logrotate 配置
sudo cp ~/rust-trade/deploy/logrotate/trading /etc/logrotate.d/trading

# 手动测试
sudo logrotate -d /etc/logrotate.d/trading
```
