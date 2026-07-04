#!/bin/bash
# install-systemd.sh - 安装 systemd 服务
# 在服务器上执行一次: sudo bash deploy/install-systemd.sh
#
# 安装后可以使用:
#   sudo systemctl start/stop/restart trading-collector
#   sudo systemctl start/stop/restart trading-engine
#   sudo systemctl enable trading-collector trading-engine  # 开机自启

set -e

GREEN='\033[0;32m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
NC='\033[0m'

# 检查 root
if [ "$EUID" -ne 0 ]; then
    echo -e "${RED}请使用 sudo 运行此脚本${NC}"
    exit 1
fi

CURRENT_USER=$(logname 2>/dev/null || echo $SUDO_USER)
HOME_DIR="/home/$CURRENT_USER"

echo -e "${GREEN}========================================${NC}"
echo -e "${GREEN}  安装 systemd 服务${NC}"
echo -e "${GREEN}========================================${NC}"
echo "用户: $CURRENT_USER"
echo "目录: $HOME_DIR/apps"

# 创建 trading-collector.service
echo -e "\n${YELLOW}[1/3] 创建 trading-collector.service...${NC}"
cat > /etc/systemd/system/trading-collector.service << EOF
[Unit]
Description=Trading Data Collector Service
After=network.target postgresql.service redis.service
Wants=postgresql.service redis.service

[Service]
Type=simple
User=$CURRENT_USER
WorkingDirectory=$HOME_DIR/apps/trading-core
ExecStart=$HOME_DIR/apps/trading-core/trading-core service
Restart=always
RestartSec=10

# 环境变量
Environment=RUN_MODE=production
Environment=RUST_LOG=info

# 日志
StandardOutput=journal
StandardError=journal
SyslogIdentifier=trading-collector

[Install]
WantedBy=multi-user.target
EOF
echo -e "${GREEN}  trading-collector.service 创建完成${NC}"

# 创建 trading-engine.service
echo -e "\n${YELLOW}[2/3] 创建 trading-engine.service...${NC}"
cat > /etc/systemd/system/trading-engine.service << EOF
[Unit]
Description=Trading Engine Service
After=network.target trading-collector.service postgresql.service redis.service
Wants=postgresql.service redis.service

[Service]
Type=simple
User=$CURRENT_USER
WorkingDirectory=$HOME_DIR/apps/trading-engine
ExecStart=$HOME_DIR/apps/trading-engine/trading-engine
Restart=always
RestartSec=10

# 环境变量
EnvironmentFile=$HOME_DIR/apps/trading-engine/.env
Environment=RUST_LOG=info
Environment=RUN_MODE=production

# 日志
StandardOutput=journal
StandardError=journal
SyslogIdentifier=trading-engine

[Install]
WantedBy=multi-user.target
EOF
echo -e "${GREEN}  trading-engine.service 创建完成${NC}"

# 重载 systemd
echo -e "\n${YELLOW}[3/3] 重载 systemd...${NC}"
systemctl daemon-reload
echo -e "${GREEN}  systemd 重载完成${NC}"

# 完成
echo -e "\n${GREEN}========================================${NC}"
echo -e "${GREEN}  安装完成！${NC}"
echo -e "${GREEN}========================================${NC}"
echo ""
echo "常用命令:"
echo "  # 启动服务"
echo "  sudo systemctl start trading-collector"
echo "  sudo systemctl start trading-engine"
echo ""
echo "  # 设置开机自启"
echo "  sudo systemctl enable trading-collector"
echo "  sudo systemctl enable trading-engine"
echo ""
echo "  # 查看状态"
echo "  sudo systemctl status trading-collector"
echo "  sudo systemctl status trading-engine"
echo ""
echo "  # 查看日志"
echo "  sudo journalctl -u trading-collector -f"
echo "  sudo journalctl -u trading-engine -f"
echo ""
echo "  # 发布新版本"
echo "  bash ~/rust-trade/deploy/publish.sh"
echo ""
