#!/bin/bash
# ============================================================
# publish.sh - 一键发布更新脚本
#
# 用法:
#   bash ~/rust-trade/deploy/publish.sh          # 完整更新（拉取+编译+部署）
#   bash ~/rust-trade/deploy/publish.sh --skip-build  # 跳过编译（只部署）
#   bash ~/rust-trade/deploy/publish.sh --no-restart   # 不重启服务
#
# 前置条件: 已执行过 first-time-setup.sh 完成首次部署
# ============================================================

set -e

# 颜色定义
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
CYAN='\033[0;36m'
NC='\033[0m'

# 目录配置
REPO_DIR="$HOME/rust-trade"
APPS_DIR="$HOME/apps"
DEPLOY_DIR="$APPS_DIR/deploy"

# 解析参数
SKIP_BUILD=false
NO_RESTART=false

while [[ $# -gt 0 ]]; do
    case $1 in
        --skip-build) SKIP_BUILD=true; shift ;;
        --no-restart) NO_RESTART=true; shift ;;
        -h|--help)
            echo "用法: bash publish.sh [选项]"
            echo ""
            echo "选项:"
            echo "  --skip-build   跳过编译步骤"
            echo "  --no-restart   不重启服务"
            echo "  -h, --help     显示帮助"
            exit 0
            ;;
        *) echo -e "${RED}未知选项: $1${NC}"; exit 1 ;;
    esac
done

# 打印Banner
echo ""
echo -e "${CYAN}╔══════════════════════════════════════════╗${NC}"
echo -e "${CYAN}║     Trading System - 一键发布更新        ║${NC}"
echo -e "${CYAN}╚══════════════════════════════════════════╝${NC}"
echo ""

# ============================================================
# 1. 拉取最新代码
# ============================================================
echo -e "${YELLOW}[1/4] 拉取最新代码...${NC}"
cd "$REPO_DIR"
git pull origin main 2>/dev/null || git pull
echo -e "${GREEN}  ✓ 代码更新完成${NC}"

# ============================================================
# 2. 编译 release（可跳过）
# ============================================================
if [ "$SKIP_BUILD" = false ]; then
    echo -e "\n${YELLOW}[2/4] 编译 release 版本...${NC}"
    echo -e "  ${CYAN}这可能需要几分钟，请耐心等待...${NC}"
    cargo build --release -p trading-core -p trading-engine -p strategy-service -p archive-klines
    echo -e "${GREEN}  ✓ 编译完成${NC}"
else
    echo -e "\n${YELLOW}[2/4] 跳过编译 (--skip-build)${NC}"
fi

# ============================================================
# 3. 停止服务
# ============================================================
if [ "$NO_RESTART" = false ]; then
    echo -e "\n${YELLOW}[3/4] 停止服务...${NC}"
    sudo systemctl stop trading-collector 2>/dev/null || true
    sudo systemctl stop trading-engine 2>/dev/null || true
    sudo systemctl stop strategy-service 2>/dev/null || true
    sleep 2
    echo -e "${GREEN}  ✓ 服务已停止${NC}"
else
    echo -e "\n${YELLOW}[3/4] 跳过停止服务 (--no-restart)${NC}"
fi

# ============================================================
# 4. 部署新版本
# ============================================================
echo -e "\n${YELLOW}[4/4] 部署新版本...${NC}"

# 部署 trading-core
if [ -f "$REPO_DIR/target/release/trading-core" ]; then
    cp "$REPO_DIR/target/release/trading-core" "$APPS_DIR/trading-core/trading-core"
    chmod +x "$APPS_DIR/trading-core/trading-core"
    echo -e "  ${GREEN}✓ trading-core${NC}"
fi

# 部署 trading-engine
if [ -f "$REPO_DIR/target/release/trading-engine" ]; then
    cp "$REPO_DIR/target/release/trading-engine" "$APPS_DIR/trading-engine/trading-engine"
    chmod +x "$APPS_DIR/trading-engine/trading-engine"
    echo -e "  ${GREEN}✓ trading-engine${NC}"
fi

# 部署 strategy-service
if [ -f "$REPO_DIR/target/release/strategy-service" ]; then
    cp "$REPO_DIR/target/release/strategy-service" "$APPS_DIR/strategy-service/strategy-service"
    chmod +x "$APPS_DIR/strategy-service/strategy-service"
    echo -e "  ${GREEN}✓ strategy-service${NC}"
fi

# 部署 archive_klines
if [ -f "$REPO_DIR/target/release/archive_klines" ]; then
    cp "$REPO_DIR/target/release/archive_klines" "$APPS_DIR/trading-core/archive_klines"
    chmod +x "$APPS_DIR/trading-core/archive_klines"
    echo -e "  ${GREEN}✓ archive_klines${NC}"
fi

# 同步归档脚本到 trading-core
cp "$REPO_DIR/deploy/archive.sh" "$APPS_DIR/trading-core/archive.sh" 2>/dev/null && \
    chmod +x "$APPS_DIR/trading-core/archive.sh" && \
    echo -e "  ${GREEN}✓ archive.sh${NC}"

# 同步部署脚本到 ~/apps/deploy/
mkdir -p "$DEPLOY_DIR"
cp "$REPO_DIR/deploy/publish.sh" "$DEPLOY_DIR/publish.sh"
cp "$REPO_DIR/deploy/monitor.sh" "$DEPLOY_DIR/monitor.sh"
cp "$REPO_DIR/deploy/backup.sh" "$DEPLOY_DIR/backup.sh"
cp "$REPO_DIR/deploy/archive.sh" "$DEPLOY_DIR/archive.sh"
cp "$REPO_DIR/deploy/logs.sh" "$DEPLOY_DIR/logs.sh"
cp "$REPO_DIR/deploy/first-time-setup.sh" "$DEPLOY_DIR/first-time-setup.sh"
chmod +x "$DEPLOY_DIR"/*.sh
echo -e "  ${GREEN}✓ 部署脚本 → $DEPLOY_DIR/${NC}"

# ============================================================
# 5. 启动服务
# ============================================================
if [ "$NO_RESTART" = false ]; then
    echo -e "\n${YELLOW}启动服务...${NC}"
    sudo systemctl start trading-collector
    sudo systemctl start trading-engine
    sudo systemctl start strategy-service
    echo -e "${GREEN}  ✓ 服务已启动${NC}"
fi

# ============================================================
# 完成
# ============================================================
echo ""
echo -e "${GREEN}╔══════════════════════════════════════════╗${NC}"
echo -e "${GREEN}║           发布完成！ 🎉                  ║${NC}"
echo -e "${GREEN}╚══════════════════════════════════════════╝${NC}"
echo ""
echo -e "${CYAN}常用命令:${NC}"
echo "  查看状态:  sudo systemctl status trading-collector"
echo "  查看状态:  sudo systemctl status trading-engine"
echo "  查看状态:  sudo systemctl status strategy-service"
echo "  查看日志:  sudo journalctl -u trading-collector -f"
echo "  查看日志:  sudo journalctl -u strategy-service -f"
echo "  查看日志:  sudo journalctl -u trading-engine -f"
echo "  数据归档:  bash ~/apps/trading-core/archive.sh --days 7"
echo ""
