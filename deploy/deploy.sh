#!/bin/bash
# ============================================================
# 生产服务器部署脚本
# 用法: ./deploy.sh [--skip-build] [--skip-db]
# ============================================================

set -e

# 颜色定义
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

# 配置
APP_DIR="${APP_DIR:-/opt/trading}"
DB_HOST="${DB_HOST:-localhost}"
DB_PORT="${DB_PORT:-5432}"
DB_NAME="${DB_NAME:-trading_core}"
REDIS_URL="${REDIS_URL:-redis://localhost:6379}"

# 解析参数
SKIP_BUILD=false
SKIP_DB=false
for arg in "$@"; do
    case $arg in
        --skip-build) SKIP_BUILD=true ;;
        --skip-db) SKIP_DB=true ;;
    esac
done

echo -e "${GREEN}================================================${NC}"
echo -e "${GREEN}  Trading System Deployment${NC}"
echo -e "${GREEN}================================================${NC}"

# ============================================================
# 1. 系统依赖
# ============================================================
echo -e "\n${YELLOW}[1/7] Installing system dependencies...${NC}"

if command -v apt-get &> /dev/null; then
    sudo apt-get update
    sudo apt-get install -y \
        postgresql-client \
        redis-tools \
        curl \
        wget \
        unzip \
        logrotate
elif command -v yum &> /dev/null; then
    sudo yum install -y \
        postgresql \
        redis \
        curl \
        wget \
        unzip \
        logrotate
fi

echo -e "${GREEN}✓ System dependencies installed${NC}"

# ============================================================
# 2. 创建目录结构
# ============================================================
echo -e "\n${YELLOW}[2/7] Creating directory structure...${NC}"

sudo mkdir -p $APP_DIR/{bin,config,logs,data/parquet}
sudo mkdir -p $APP_DIR/data/parquet/{BTCUSDT,ETHUSDT,SOLUSDT,BNBUSDT,SUIUSDT}

# 设置权限
sudo chown -R $USER:$USER $APP_DIR

echo -e "${GREEN}✓ Directory structure created${NC}"

# ============================================================
# 3. 编译项目
# ============================================================
if [ "$SKIP_BUILD" = false ]; then
    echo -e "\n${YELLOW}[3/7] Building project...${NC}"

    cd "$(dirname "$0")/.."

    # 编译 release 版本
    cargo build --release

    # 复制二进制文件
    cp target/release/trading-core $APP_DIR/bin/
    cp target/release/trading-engine $APP_DIR/bin/

    # 复制归档脚本
    cp scripts/archive_klines.rs $APP_DIR/bin/archive_klines.rs

    echo -e "${GREEN}✓ Project built successfully${NC}"
else
    echo -e "\n${YELLOW}[3/7] Skipping build...${NC}"
fi

# ============================================================
# 4. 配置文件
# ============================================================
echo -e "\n${YELLOW}[4/7] Setting up configuration...${NC}"

# 复制配置文件
cp deploy/config/production.toml $APP_DIR/config/
cp deploy/config/engine-production.toml $APP_DIR/config/

# 创建环境变量文件
if [ ! -f "$APP_DIR/.env" ]; then
    cat > $APP_DIR/.env << EOF
# 数据库配置
DATABASE_URL=postgresql://trading:password@${DB_HOST}:${DB_PORT}/${DB_NAME}
REDIS_URL=${REDIS_URL}

# Binance API (从环境变量加载，不写入文件)
# BINANCE_API_KEY=
# BINANCE_API_SECRET=
# BINANCE_TESTNET=false

# OKX API (可选)
# OKX_API_KEY=
# OKX_API_SECRET=
# OKX_PASSPHRASE=

# 日志级别
RUST_LOG=info
EOF

    echo -e "${YELLOW}⚠ Created .env template. Please edit with your API keys.${NC}"
fi

echo -e "${GREEN}✓ Configuration files set up${NC}"

# ============================================================
# 5. 数据库初始化
# ============================================================
if [ "$SKIP_DB" = false ]; then
    echo -e "\n${YELLOW}[5/7] Initializing database...${NC}"

    # 检查数据库连接
    if PGPASSWORD=$DB_PASSWORD psql -h $DB_HOST -p $DB_PORT -U postgres -c '\q' 2>/dev/null; then
        # 创建数据库（如果不存在）
        PGPASSWORD=$DB_PASSWORD psql -h $DB_HOST -p $DB_PORT -U postgres -tc \
            "SELECT 1 FROM pg_database WHERE datname = '$DB_NAME'" | grep -q 1 || \
            PGPASSWORD=$DB_PASSWORD psql -h $DB_HOST -p $DB_PORT -U postgres -c \
            "CREATE DATABASE $DB_NAME"

        # 执行 schema
        PGPASSWORD=$DB_PASSWORD psql -h $DB_HOST -p $DB_PORT -U postgres -d $DB_NAME \
            -f config/schema_v2.sql

        # 执行索引优化
        PGPASSWORD=$DB_PASSWORD psql -h $DB_HOST -p $DB_PORT -U postgres -d $DB_NAME \
            -f version/v1.0/optimize_indexes.sql

        echo -e "${GREEN}✓ Database initialized${NC}"
    else
        echo -e "${RED}✗ Cannot connect to database. Please configure DATABASE_URL in .env${NC}"
    fi
else
    echo -e "\n${YELLOW}[5/7] Skipping database initialization...${NC}"
fi

# ============================================================
# 6. 日志轮转配置
# ============================================================
echo -e "\n${YELLOW}[6/7] Setting up log rotation...${NC}"

# 复制日志轮转配置
sudo cp deploy/logrotate.conf /etc/logrotate.d/trading

echo -e "${GREEN}✓ Log rotation configured${NC}"

# ============================================================
# 7. Systemd 服务
# ============================================================
echo -e "\n${YELLOW}[7/7] Installing systemd services...${NC}"

# 复制服务文件
sudo cp deploy/systemd/trading-collector.service /etc/systemd/system/
sudo cp deploy/systemd/trading-engine.service /etc/systemd/system/

# 重新加载 systemd
sudo systemctl daemon-reload

# 启用服务
sudo systemctl enable trading-collector
sudo systemctl enable trading-engine

echo -e "${GREEN}✓ Systemd services installed${NC}"

# ============================================================
# 完成
# ============================================================
echo -e "\n${GREEN}================================================${NC}"
echo -e "${GREEN}  Deployment Complete!${NC}"
echo -e "${GREEN}================================================${NC}"
echo ""
echo -e "Next steps:"
echo -e "  1. Edit ${YELLOW}$APP_DIR/.env${NC} with your API keys"
echo -e "  2. Start services:"
echo -e "     ${YELLOW}sudo systemctl start trading-collector${NC}"
echo -e "     ${YELLOW}sudo systemctl start trading-engine${NC}"
echo -e "  3. Check status:"
echo -e "     ${YELLOW}sudo systemctl status trading-collector${NC}"
echo -e "     ${YELLOW}sudo systemctl status trading-engine${NC}"
echo -e "  4. View logs:"
echo -e "     ${YELLOW}sudo journalctl -u trading-collector -f${NC}"
echo -e "     ${YELLOW}sudo journalctl -u trading-engine -f${NC}"
echo ""
