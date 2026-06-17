# 环境配置说明

## 数据库配置

| 环境 | 数据库名 | 文件 |
|------|----------|------|
| 生产 (Production) | trading_core | `.env.production` |
| 开发 (Development) | mydb | `.env.development` |
| 测试 (Test) | mydb | `.env.test` |

## 使用方式（自动切换）

程序会根据编译模式**自动选择**环境：

```bash
# 开发环境（自动使用 mydb 数据库）
cargo run

# 生产环境（自动使用 trading_core 数据库）
cargo run --release
```

## 手动覆盖环境

如果需要特殊指定，可以通过环境变量覆盖：

```powershell
# PowerShell
$env:RUN_MODE="test"; cargo run
```

```bash
# Linux/Mac
RUN_MODE=test cargo run
```

## 环境变量说明

| 变量名 | 说明 | 示例 |
|--------|------|------|
| DATABASE_URL | PostgreSQL 连接字符串 | postgresql://user:pass@host:5432/db |
| REDIS_URL | Redis 连接字符串 | redis://:password@host:port |
| RUN_MODE | 运行模式（可选，默认自动） | development / test / production |

## Redis 密码

所有环境使用相同密码：`zxd6655422`

## 配置文件加载优先级

1. `RUN_MODE` 环境变量指定的文件（如有）
2. 根据编译模式自动选择（debug=development, release=production）
3. 回退到 `.env` 文件
