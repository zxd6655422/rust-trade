#!/bin/bash
# ============================================================
# 首次部署脚本（只需执行一次）
# 用法: bash ~/rust-trade/deploy/first-time-setup.sh
#
# 完成后日常更新用: bash ~/rust-trade/deploy/publish.sh
# ============================================================

set -e

GREEN='\033[0;32m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
NC='\033[0m'

# 目录配置
REPO_DIR="$HOME/rust-trade"
APPS_DIR="$HOME/apps"
DEPLOY_DIR="$APPS_DIR/deploy"
COLLECTOR_DIR="$APPS_DIR/trading-core"
ENGINE_DIR="$APPS_DIR/trading-engine"
CURRENT_USER=$(whoami)

echo -e "${GREEN}========================================${NC}"
echo -e "${GREEN}  Trading System - 首次部署${NC}"
echo -e "${GREEN}========================================${NC}"
echo ""

# ============================================================
# 1. 检查 Rust 环境
# ============================================================
echo -e "${YELLOW}[1/7] 检查 Rust 环境...${NC}"
if ! command -v cargo &> /dev/null; then
    echo -e "${RED}  未找到 Rust，请先安装:${NC}"
    echo "  curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh"
    exit 1
fi
echo -e "${GREEN}  Rust $(rustc --version) ✓${NC}"

# ============================================================
# 2. 编译 release
# ============================================================
echo -e "\n${YELLOW}[2/7] 编译 release 版本 (可能需要几分钟)...${NC}"
cd "$REPO_DIR"
cargo build --release -p trading-core -p trading-engine -p archive-klines
echo -e "${GREEN}  编译完成 ✓${NC}"

# ============================================================
# 3. 创建目录结构
# ============================================================
echo -e "\n${YELLOW}[3/7] 创建目录结构...${NC}"
DATA_DIR="$APPS_DIR/trading-data"
for dir in "$DEPLOY_DIR" "$COLLECTOR_DIR" "$COLLECTOR_DIR/config" "$COLLECTOR_DIR/logs" \
           "$ENGINE_DIR" "$ENGINE_DIR/config" "$ENGINE_DIR/logs" \
           "$DATA_DIR/parquet" "$DATA_DIR/logs"; do
    mkdir -p "$dir"
done
echo -e "${GREEN}  $DEPLOY_DIR/      ✓${NC}"
echo -e "${GREEN}  $COLLECTOR_DIR/  ✓${NC}"
echo -e "${GREEN}  $ENGINE_DIR/     ✓${NC}"
echo -e "${GREEN}  $DATA_DIR/       ✓${NC}"

# ============================================================
# 4. 复制二进制文件
# ============================================================
echo -e "\n${YELLOW}[4/7] 部署二进制文件...${NC}"

# 先停止服务（如果正在运行）
echo -e "  ${CYAN}停止服务...${NC}"
sudo systemctl stop trading-collector 2>/dev/null || true
sudo systemctl stop trading-engine 2>/dev/null || true
sleep 1

cp "$REPO_DIR/target/release/trading-core" "$COLLECTOR_DIR/trading-core"
chmod +x "$COLLECTOR_DIR/trading-core"
echo -e "  ${GREEN}trading-core ✓${NC}"

cp "$REPO_DIR/target/release/trading-engine" "$ENGINE_DIR/trading-engine"
chmod +x "$ENGINE_DIR/trading-engine"
echo -e "  ${GREEN}trading-engine ✓${NC}"

cp "$REPO_DIR/target/release/archive_klines" "$COLLECTOR_DIR/archive_klines"
chmod +x "$COLLECTOR_DIR/archive_klines"
echo -e "  ${GREEN}archive_klines ✓${NC}"

# 复制归档脚本
cp "$REPO_DIR/deploy/archive.sh" "$COLLECTOR_DIR/archive.sh"
chmod +x "$COLLECTOR_DIR/archive.sh"
echo -e "  ${GREEN}archive.sh ✓${NC}"

# 复制部署脚本到 ~/apps/deploy/
cp "$REPO_DIR/deploy/publish.sh" "$DEPLOY_DIR/publish.sh"
cp "$REPO_DIR/deploy/monitor.sh" "$DEPLOY_DIR/monitor.sh"
cp "$REPO_DIR/deploy/backup.sh" "$DEPLOY_DIR/backup.sh"
cp "$REPO_DIR/deploy/archive.sh" "$DEPLOY_DIR/archive.sh"
cp "$REPO_DIR/deploy/logs.sh" "$DEPLOY_DIR/logs.sh"
cp "$REPO_DIR/deploy/first-time-setup.sh" "$DEPLOY_DIR/first-time-setup.sh"
chmod +x "$DEPLOY_DIR"/*.sh
echo -e "  ${GREEN}部署脚本 → $DEPLOY_DIR/ ✓${NC}"

# ============================================================
# 5. 复制配置和启动脚本（不覆盖已有配置）
# ============================================================
echo -e "\n${YELLOW}[5/7] 复制配置和启动脚本...${NC}"

# trading-core 配置（从 dist/ 复制，已有则跳过）
if [ ! -f "$COLLECTOR_DIR/config/production.toml" ]; then
    cp "$REPO_DIR/dist/trading-core/config/production.toml" "$COLLECTOR_DIR/config/production.toml"
    echo -e "  ${GREEN}production.toml (新建)${NC}"
else
    echo -e "  ${YELLOW}production.toml (已存在，跳过)${NC}"
fi

# trading-engine 配置
if [ ! -f "$ENGINE_DIR/config/engine-production.toml" ]; then
    cp "$REPO_DIR/dist/trading-engine/config/engine-production.toml" "$ENGINE_DIR/config/engine-production.toml"
    echo -e "  ${GREEN}engine-production.toml (新建)${NC}"
else
    echo -e "  ${YELLOW}engine-production.toml (已存在，跳过)${NC}"
fi

# 复制 start.sh
cp "$REPO_DIR/deploy/trading-core/start.sh" "$COLLECTOR_DIR/start.sh"
cp "$REPO_DIR/deploy/trading-engine/start.sh" "$ENGINE_DIR/start.sh"
chmod +x "$COLLECTOR_DIR/start.sh" "$ENGINE_DIR/start.sh"
echo -e "  ${GREEN}start.sh ✓${NC}"

# ============================================================
# 6. 安装 systemd 服务
# ============================================================
echo -e "\n${YELLOW}[6/7] 安装 systemd 服务...${NC}"

# trading-collector.service
sudo tee /etc/systemd/system/trading-collector.service > /dev/null << EOF
[Unit]
Description=Trading Data Collector Service
After=network.target postgresql.service redis.service
Wants=postgresql.service redis.service

[Service]
Type=simple
User=$CURRENT_USER
WorkingDirectory=$COLLECTOR_DIR
ExecStart=$COLLECTOR_DIR/trading-core service
Restart=always
RestartSec=10

Environment=RUN_MODE=production
Environment=RUST_LOG=info

StandardOutput=journal
StandardError=journal
SyslogIdentifier=trading-collector

[Install]
WantedBy=multi-user.target
EOF
echo -e "  ${GREEN}trading-collector.service ✓${NC}"

# trading-engine.service
sudo tee /etc/systemd/system/trading-engine.service > /dev/null << EOF
[Unit]
Description=Trading Engine Service
After=network.target trading-collector.service postgresql.service redis.service
Wants=postgresql.service redis.service

[Service]
Type=simple
User=$CURRENT_USER
WorkingDirectory=$ENGINE_DIR
ExecStart=$ENGINE_DIR/trading-engine
Restart=always
RestartSec=10

EnvironmentFile=$ENGINE_DIR/.env
Environment=RUST_LOG=info
Environment=RUN_MODE=production

StandardOutput=journal
StandardError=journal
SyslogIdentifier=trading-engine

[Install]
WantedBy=multi-user.target
EOF
echo -e "  ${GREEN}trading-engine.service ✓${NC}"

sudo systemctl daemon-reload
sudo systemctl enable trading-collector trading-engine 2>/dev/null || true
echo -e "  ${GREEN}systemd 已重载并设置开机自启 ✓${NC}"

# ============================================================
# 6.5 安装归档定时任务
# ============================================================
echo -e "\n${YELLOW}[6.5/7] 安装归档定时任务...${NC}"

# 复制 systemd 文件
sudo cp "$REPO_DIR/deploy/trading-archive.service" /etc/systemd/system/
sudo cp "$REPO_DIR/deploy/trading-archive.timer" /etc/systemd/system/
sudo chmod 644 /etc/systemd/system/trading-archive.service /etc/systemd/system/trading-archive.timer

# 启用定时任务
sudo systemctl daemon-reload
sudo systemctl enable trading-archive.timer 2>/dev/null || true
sudo systemctl start trading-archive.timer 2>/dev/null || true
echo -e "  ${GREEN}trading-archive.timer ✓${NC}"
echo -e "  ${GREEN}每天自动执行归档（保留 7 天数据）✓${NC}"

# ============================================================
# 7. 完成
# ============================================================
echo -e "\n${GREEN}========================================${NC}"
echo -e "${GREEN}  首次部署完成！${NC}"
echo -e "${GREEN}========================================${NC}"
echo ""
echo -e "${YELLOW}接下来需要手动完成:${NC}"
echo ""
echo "  1. 编辑配置文件:"
echo "     vim $COLLECTOR_DIR/config/production.toml"
echo "     vim $ENGINE_DIR/config/engine-production.toml"
echo ""
echo "  2. 创建环境变量文件 (trading-engine 需要):"
echo "     vim $ENGINE_DIR/.env"
echo "     # 内容: DATABASE_URL=postgresql://... REDIS_URL=redis://..."
echo ""
echo "  3. 启动服务:"
echo "     sudo systemctl start trading-collector"
echo "     sudo systemctl start trading-engine"
echo ""
echo "  4. 查看状态:"
echo "     sudo systemctl status trading-collector"
echo "     sudo systemctl status trading-engine"
echo ""
echo "  5. 查看日志:"
echo "     sudo journalctl -u trading-collector -f"
echo "     sudo journalctl -u trading-engine -f"
echo ""
echo "  后续更新代码用: bash ~/rust-trade/deploy/publish.sh"
echo ""
