# 发布与部署指南

## 服务器目录结构

```
~/
├── rust-trade/                        # 源代码目录
│   ├── deploy/
│   │   ├── publish.sh                 # 发布脚本
│   │   ├── install-systemd.sh         # systemd 安装脚本
│   │   └── README.md
│   └── ...
│
└── apps/                              # 应用运行目录
    ├── trading-core/                  # 数据采集服务
    │   ├── trading-core               # 编译后的二进制文件
    │   ├── config/
    │   │   ├── production.toml        # 生产环境配置
    │   │   └── development.toml       # 开发环境配置
    │   ├── logs/                      # 日志目录
    │   └── start.sh                   # 启动脚本 (nohup 方式)
    │
    └── trading-engine/                # 交易引擎服务
        ├── trading-engine             # 编译后的二进制文件
        ├── config/
        ├── logs/
        ├── .env                       # 环境变量 (API Key 等)
        └── start.sh
```

---

## 首次部署

### 1. 安装 systemd 服务 (推荐，只需执行一次)

```bash
cd ~/rust-trade
sudo bash deploy/install-systemd.sh
```

安装后可以使用 systemctl 管理服务：

```bash
# 启动服务
sudo systemctl start trading-collector
sudo systemctl start trading-engine

# 设置开机自启
sudo systemctl enable trading-collector
sudo systemctl enable trading-engine
```

### 2. 配置环境变量

```bash
nano ~/apps/trading-engine/.env
```

填写内容：

```bash
# 数据库
DATABASE_URL=postgresql://user:password@localhost:5432/trading_core
REDIS_URL=redis://:password@localhost:6379

# Binance API
BINANCE_API_KEY=your_api_key
BINANCE_API_SECRET=your_api_secret

# OKX API (可选)
OKX_API_KEY=your_api_key
OKX_API_SECRET=your_api_secret
OKX_PASSPHRASE=your_passphrase
```

### 3. 验证服务运行

```bash
# 查看服务状态
sudo systemctl status trading-collector
sudo systemctl status trading-engine

# 查看日志
sudo journalctl -u trading-collector -f
sudo journalctl -u trading-engine -f
```

---

## 日常发布

代码更新后，在服务器上执行：

```bash
cd ~/rust-trade
bash deploy/publish.sh
```

脚本自动完成以下步骤：

1. `git pull` — 拉取最新代码
2. `cargo build --release` — 编译 release 版本
3. 停止当前服务
4. 复制新二进制文件到 `~/apps/`
5. 启动服务

> **注意**: 已有的 `config/production.toml` 不会被覆盖。

---

## 服务管理命令

### systemctl 方式 (推荐)

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

### start.sh 方式 (不用 systemd)

```bash
# 启动
cd ~/apps/trading-core && bash start.sh
cd ~/apps/trading-engine && bash start.sh

# 停止
pkill -f trading-core
pkill -f trading-engine

# 查看进程
ps aux | grep trading
```

---

## 日志查看

### systemd 日志

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

# 按时间范围
sudo journalctl -u trading-collector --since "2026-07-04" --until "2026-07-05"
```

### 日志文件

```bash
# 实时查看
tail -f ~/apps/trading-core/logs/*.log
tail -f ~/apps/trading-engine/logs/*.log

# 最近 100 行
tail -n 100 ~/apps/trading-core/logs/*.log
```

---

## systemd 与 start.sh 对比

| 特性 | start.sh (nohup) | systemd |
|------|------------------|---------|
| 进程崩溃 | 需手动重启 | 自动重启 (10秒后) |
| 服务器重启 | 需手动启动 | 自动启动 (开机自启) |
| 查看日志 | `tail -f logs/*.log` | `journalctl -u xxx -f` |
| 停止服务 | `pkill` | `systemctl stop` |
| 后台运行 | nohup + & | 由 systemd 管理 |

---

## 故障排查

### 服务启动失败

```bash
# 查看详细错误
sudo journalctl -u trading-collector -n 50 --no-pager
sudo journalctl -u trading-engine -n 50 --no-pager

# 手动运行测试
cd ~/apps/trading-core
./trading-core service

cd ~/apps/trading-engine
./trading-engine
```

### 数据库连接失败

```bash
# 测试 PostgreSQL
psql -h localhost -U your_user -d trading_core

# 测试 Redis
redis-cli -h localhost -a your_password ping
```

### 编译失败

```bash
cd ~/rust-trade

# 清理后重新编译
cargo clean
cargo build --release

# 查看详细错误
cargo build --release 2>&1 | tail -50
```

### 端口被占用

```bash
# 查看 8080 端口占用
netstat -tlnp | grep 8080
lsof -i :8080
```

---

## 日志轮转 (可选)

安装 logrotate 配置，自动清理旧日志：

```bash
sudo cp ~/rust-trade/deploy/logrotate/trading /etc/logrotate.d/trading
```

配置说明：
- 每日轮转
- 保留 30 天
- gzip 压缩

手动测试：
```bash
sudo logrotate -d /etc/logrotate.d/trading
```
