#!/bin/bash
# publish.sh - 服务器发布脚本
# 在服务器上执行: bash deploy/publish.sh
#
# 前置条件:
#   - 代码在 ~/rust-trade/ 目录
#   - 应用部署在 ~/apps/trading-core/ 和 ~/apps/trading-engine/
#   - 已安装 Rust 编译环境

set -e

# 颜色
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
NC='\033[0m'

PROJECT_DIR="$HOME/rust-trade"
APPS_DIR="$HOME/apps"

echo -e "${GREEN}========================================${NC}"
echo -e "${GREEN}  Trading System - 发布脚本${NC}"
echo -e "${GREEN}========================================${NC}"

# 1. 拉取最新代码
echo -e "\n${YELLOW}[1/5] 拉取最新代码...${NC}"
cd "$PROJECT_DIR"
git pull
echo -e "${GREEN}  代码更新完成${NC}"

# 2. 编译 release 版本
echo -e "\n${YELLOW}[2/5] 编译 release 版本 (可能需要几分钟)...${NC}"
cargo build --release
echo -e "${GREEN}  编译完成${NC}"

# 3. 停止服务
echo -e "\n${YELLOW}[3/5] 停止当前服务...${NC}"

# 检查 systemd 服务是否存在
if systemctl list-units --full -all | grep -q "trading-collector.service"; then
    sudo systemctl stop trading-collector 2>/dev/null || true
    echo -e "  ${GREEN}trading-collector 已停止${NC}"
else
    # 尝试用 pkill 停止
    pkill -f "trading-core" 2>/dev/null || true
    echo -e "  ${YELLOW}trading-collector (进程模式) 已停止${NC}"
fi

if systemctl list-units --full -all | grep -q "trading-engine.service"; then
    sudo systemctl stop trading-engine 2>/dev/null || true
    echo -e "  ${GREEN}trading-engine 已停止${NC}"
else
    pkill -f "trading-engine" 2>/dev/null || true
    echo -e "  ${YELLOW}trading-engine (进程模式) 已停止${NC}"
fi

sleep 2

# 4. 复制新版本
echo -e "\n${YELLOW}[4/5] 部署新版本...${NC}"

# 复制 trading-core
cp "$PROJECT_DIR/target/release/trading-core" "$APPS_DIR/trading-core/trading-core"
chmod +x "$APPS_DIR/trading-core/trading-core"
echo -e "  ${GREEN}trading-core 已更新${NC}"

# 复制 trading-engine
cp "$PROJECT_DIR/target/release/trading-engine" "$APPS_DIR/trading-engine/trading-engine"
chmod +x "$APPS_DIR/trading-engine/trading-engine"
echo -e "  ${GREEN}trading-engine 已更新${NC}"

# 复制配置文件 (仅同步新文件，不覆盖已有配置)
if [ -d "$PROJECT_DIR/config" ]; then
    # 只复制新文件，不覆盖已有的 production.toml
    for f in "$PROJECT_DIR/config/"*.toml; do
        filename=$(basename "$f")
        if [ ! -f "$APPS_DIR/trading-core/config/$filename" ]; then
            cp "$f" "$APPS_DIR/trading-core/config/"
            echo -e "  ${GREEN}新增配置: $filename${NC}"
        fi
    done
    echo -e "  ${GREEN}配置文件检查完成 (已有配置未覆盖)${NC}"
fi

# 5. 启动服务
echo -e "\n${YELLOW}[5/5] 启动服务...${NC}"

if systemctl list-units --full -all | grep -q "trading-collector.service"; then
    sudo systemctl start trading-collector
    echo -e "  ${GREEN}trading-collector 已启动 (systemd)${NC}"
else
    # 使用 start.sh 启动
    cd "$APPS_DIR/trading-core"
    bash start.sh
    echo -e "  ${GREEN}trading-collector 已启动 (start.sh)${NC}"
fi

if systemctl list-units --full -all | grep -q "trading-engine.service"; then
    sudo systemctl start trading-engine
    echo -e "  ${GREEN}trading-engine 已启动 (systemd)${NC}"
else
    cd "$APPS_DIR/trading-engine"
    bash start.sh
    echo -e "  ${GREEN}trading-engine 已启动 (start.sh)${NC}"
fi

# 完成
echo -e "\n${GREEN}========================================${NC}"
echo -e "${GREEN}  发布完成！${NC}"
echo -e "${GREEN}========================================${NC}"
echo ""
echo "查看日志:"
echo "  tail -f $APPS_DIR/trading-core/logs/*.log"
echo "  tail -f $APPS_DIR/trading-engine/logs/*.log"
echo ""
echo "查看进程:"
echo "  ps aux | grep trading"
echo ""
