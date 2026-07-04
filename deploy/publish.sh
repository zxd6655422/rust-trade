#!/bin/bash
# publish.sh - 日常更新发布脚本
# 在服务器上执行: bash ~/rust-trade/deploy/publish.sh
#
# 前置条件: 已执行过 first-time-setup.sh 完成首次部署

set -e

GREEN='\033[0;32m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
NC='\033[0m'

REPO_DIR="$HOME/rust-trade"
APPS_DIR="$HOME/apps"

echo -e "${GREEN}========================================${NC}"
echo -e "${GREEN}  Trading System - 发布更新${NC}"
echo -e "${GREEN}========================================${NC}"

# 1. 拉取最新代码
echo -e "\n${YELLOW}[1/4] 拉取最新代码...${NC}"
cd "$REPO_DIR"
git pull
echo -e "${GREEN}  代码更新完成${NC}"

# 2. 编译 release
echo -e "\n${YELLOW}[2/4] 编译 release (可能需要几分钟)...${NC}"
cargo build --release
echo -e "${GREEN}  编译完成${NC}"

# 3. 停止服务 → 部署 → 启动
echo -e "\n${YELLOW}[3/4] 停止服务...${NC}"
sudo systemctl stop trading-collector 2>/dev/null || true
sudo systemctl stop trading-engine 2>/dev/null || true
sleep 2
echo -e "${GREEN}  服务已停止${NC}"

echo -e "\n${YELLOW}[4/4] 部署新版本并重启...${NC}"

# 更新二进制
cp "$REPO_DIR/target/release/trading-core" "$APPS_DIR/trading-core/trading-core"
chmod +x "$APPS_DIR/trading-core/trading-core"
echo -e "  ${GREEN}trading-core ✓${NC}"

cp "$REPO_DIR/target/release/trading-engine" "$APPS_DIR/trading-engine/trading-engine"
chmod +x "$APPS_DIR/trading-engine/trading-engine"
echo -e "  ${GREEN}trading-engine ✓${NC}"

# 更新 archive_klines 工具
cp "$REPO_DIR/target/release/archive_klines" "$APPS_DIR/trading-core/archive_klines"
chmod +x "$APPS_DIR/trading-core/archive_klines"
echo -e "  ${GREEN}archive_klines ✓${NC}"

# 同步归档脚本
cp "$REPO_DIR/deploy/archive.sh" "$APPS_DIR/trading-core/archive.sh"
chmod +x "$APPS_DIR/trading-core/archive.sh"

# 启动服务
sudo systemctl start trading-collector
sudo systemctl start trading-engine
echo -e "  ${GREEN}服务已启动${NC}"

# 完成
echo -e "\n${GREEN}========================================${NC}"
echo -e "${GREEN}  发布完成！${NC}"
echo -e "${GREEN}========================================${NC}"
echo ""
echo "查看日志:"
echo "  sudo journalctl -u trading-collector -f"
echo "  sudo journalctl -u trading-engine -f"
echo ""
echo "查看状态:"
echo "  sudo systemctl status trading-collector"
echo "  sudo systemctl status trading-engine"
echo ""
