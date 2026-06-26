# 快速开始 - v1.0 真实交易系统

## 前置条件

### 开发环境
- Rust 1.75+
- PostgreSQL 14+
- Redis 7+
- Binance Testnet 账号

### 获取 API Key

#### Binance Testnet
1. 访问 https://testnet.binance.vision/
2. 登录 GitHub 账号
3. 生成 API Key 和 Secret
4. **保存好，只显示一次**

#### OKX 模拟盘
1. 访问 https://www.okx.com/account/my-api
2. 创建 API Key
3. 勾选 "模拟交易"
4. 保存 API Key、Secret、Passphrase

---

## 项目结构

```
rust-trade/
├── trading-common/          # 共享库
├── trading-core/            # 数据采集服务 (现有)
├── trading-engine/          # 交易引擎服务 (新增)
├── src-tauri/               # 桌面监控应用
├── config/
│   ├── collector-development.toml
│   ├── collector-production.toml
│   ├── engine-development.toml
│   └── engine-production.toml
├── version/
│   └── v1.0/
│       ├── README.md        # 版本计划
│       ├── ARCHITECTURE.md  # 架构设计
│       └── QUICKSTART.md    # 快速开始
└── .env                     # 环境变量 (API Keys)
```

---

## 开发步骤

### Step 1: 创建 trading-engine crate

```bash
# 在 workspace 根目录
cargo init trading-engine
```

更新 `Cargo.toml`:
```toml
[workspace]
members = [
    "trading-common",
    "trading-core",
    "trading-engine",  # 新增
    "src-tauri"
]
resolver = "2"
```

### Step 2: 配置环境变量

创建 `.env`:
```bash
# 数据库
DATABASE_URL=postgresql://mydb:your_password@localhost:5432/trading_core
REDIS_URL=redis://:your_password@localhost:6379

# Binance Testnet
BINANCE_API_KEY=your_testnet_api_key
BINANCE_API_SECRET=your_testnet_api_secret
BINANCE_TESTNET=true
```

### Step 3: 实现 Exchange Adapter

参考 `version/v1.0/ARCHITECTURE.md` 中的接口设计，实现 Binance REST API 集成。

---

## 测试流程

### 1. 单元测试
```bash
cargo test -p trading-engine
```

### 2. Testnet 集成测试

```bash
# 确保 .env 中 BINANCE_TESTNET=true

# 运行交易引擎
cargo run -p trading-engine -- --testnet

# 观察输出
# - 连接 Testnet 成功
# - 查询账户余额
# - 尝试小额下单
```

### 3. 验证清单

- [ ] 能连接 Binance Testnet
- [ ] 能查询账户余额
- [ ] 能下单 (0.0001 BTC)
- [ ] 能撤单
- [ ] 订单状态更新正常
- [ ] 风控规则正确执行

---

## 部署步骤

### 1. 编译 Release 版本

```bash
# 数据采集服务
cargo build --release -p trading-core

# 交易引擎服务
cargo build --release -p trading-engine
```

### 2. 上传到服务器

```bash
# 创建目录
ssh user@server "mkdir -p /opt/trading/{bin,config,logs}"

# 上传二进制
scp target/release/trading-core user@server:/opt/trading/bin/
scp target/release/trading-engine user@server:/opt/trading/bin/

# 上传配置
scp config/collector-production.toml user@server:/opt/trading/config/
scp config/engine-production.toml user@server:/opt/trading/config/

# 上传环境变量
scp .env user@server:/opt/trading/.env
ssh user@server "chmod 600 /opt/trading/.env"
```

### 3. 配置 systemd

```bash
# 复制服务文件
sudo cp /opt/trading/*.service /etc/systemd/system/

# 启动服务
sudo systemctl daemon-reload
sudo systemctl enable trading-collector trading-engine
sudo systemctl start trading-collector trading-engine

# 查看状态
sudo systemctl status trading-collector trading-engine
```

### 4. 验证部署

```bash
# 查看日志
sudo journalctl -u trading-collector -f
sudo journalctl -u trading-engine -f

# 检查数据库
psql -U mydb -d trading_core -c "SELECT COUNT(*) FROM tick_data;"

# 检查 Redis
redis-cli KEYS "*"
```

---

## 常见问题

### Q: API Key 连接失败？
A: 检查:
1. API Key 是否正确
2. 是否选择了 Testnet
3. IP 是否在白名单
4. 网络是否能访问 Binance

### Q: 下单被拒绝？
A: 检查:
1. 账户余额是否充足
2. 订单参数是否正确
3. 风控规则是否触发
4. 是否超过 API 限制

### Q: 服务启动失败？
A: 检查:
1. 数据库是否运行
2. Redis 是否运行
3. 配置文件是否正确
4. 环境变量是否加载

---

## 下一步

1. 阅读 `README.md` 了解完整开发计划
2. 阅读 `ARCHITECTURE.md` 了解详细架构设计
3. 开始 Phase 1: 基础框架搭建

---

## 技术支持

如有问题，请查看：
- 项目 README.md
- Binance API 文档: https://binance-docs.github.io/apidocs/
- OKX API 文档: https://www.okx.com/docs-v5/
