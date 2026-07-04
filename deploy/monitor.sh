#!/bin/bash
# ============================================================
# 服务监控脚本
# 用法: ./monitor.sh [--watch]
# ============================================================

set -e

# 颜色定义
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

# 配置
APPS_DIR="$HOME/apps"
DATA_DIR="$APPS_DIR/trading-data"
WATCH_MODE=false

# 解析参数
for arg in "$@"; do
    case $arg in
        --watch) WATCH_MODE=true ;;
    esac
done

# 检查服务状态
check_services() {
    echo -e "${BLUE}================================================${NC}"
    echo -e "${BLUE}  服务状态检查 - $(date '+%Y-%m-%d %H:%M:%S')${NC}"
    echo -e "${BLUE}================================================${NC}"

    # 检查 trading-collector
    echo -e "\n${YELLOW}Trading Collector:${NC}"
    if systemctl is-active --quiet trading-collector; then
        echo -e "  状态: ${GREEN}● 运行中${NC}"
        echo -e "  PID: $(systemctl show trading-collector --property=MainPID --value)"
        echo -e "  内存: $(systemctl show trading-collector --property=MemoryCurrent --value | numfmt --to=iec 2>/dev/null || echo 'N/A')"
        echo -e "  运行时间: $(systemctl show trading-collector --property=ActiveEnterTimestamp --value)"
    else
        echo -e "  状态: ${RED}● 未运行${NC}"
    fi

    # 检查 trading-engine
    echo -e "\n${YELLOW}Trading Engine:${NC}"
    if systemctl is-active --quiet trading-engine; then
        echo -e "  状态: ${GREEN}● 运行中${NC}"
        echo -e "  PID: $(systemctl show trading-engine --property=MainPID --value)"
        echo -e "  内存: $(systemctl show trading-engine --property=MemoryCurrent --value | numfmt --to=iec 2>/dev/null || echo 'N/A')"
        echo -e "  运行时间: $(systemctl show trading-engine --property=ActiveEnterTimestamp --value)"
    else
        echo -e "  状态: ${RED}● 未运行${NC}"
    fi

    # 检查数据库连接
    echo -e "\n${YELLOW}数据库连接:${NC}"
    if PGPASSWORD=$DB_PASSWORD psql -h $DB_HOST -p $DB_PORT -U postgres -c '\q' 2>/dev/null; then
        echo -e "  PostgreSQL: ${GREEN}● 连接正常${NC}"
    else
        echo -e "  PostgreSQL: ${RED}● 连接失败${NC}"
    fi

    # 检查 Redis 连接
    if redis-cli ping 2>/dev/null | grep -q PONG; then
        echo -e "  Redis: ${GREEN}● 连接正常${NC}"
    else
        echo -e "  Redis: ${RED}● 连接失败${NC}"
    fi

    # 检查磁盘空间
    echo -e "\n${YELLOW}磁盘空间:${NC}"
    df -h $APPS_DIR | tail -1 | awk '{print "  使用率: "$5" (剩余: "$4")"}'

    # 检查日志文件
    echo -e "\n${YELLOW}日志文件:${NC}"
    for log in $APPS_DIR/trading-core/logs/*.log $APPS_DIR/trading-engine/logs/*.log; do
        if [ -f "$log" ]; then
            size=$(du -sh "$log" | cut -f1)
            echo -e "  $(basename $log): $size"
        fi
    done

    # 检查 Parquet 文件
    echo -e "\n${YELLOW}Parquet 数据:${NC}"
    if [ -d "$DATA_DIR/parquet" ]; then
        total_size=$(du -sh $DATA_DIR/parquet | cut -f1)
        total_files=$(find $DATA_DIR/parquet -name "*.parquet" | wc -l)
        echo -e "  总大小: $total_size"
        echo -e "  文件数: $total_files"
    else
        echo -e "  无 Parquet 数据"
    fi
}

# 检查系统资源
check_resources() {
    echo -e "\n${YELLOW}系统资源:${NC}"
    echo -e "  CPU 使用率: $(top -bn1 | grep "Cpu(s)" | awk '{print $2}')%"
    echo -e "  内存使用: $(free -h | awk '/Mem:/ {print $3"/"$2}')"
    echo -e "  负载: $(uptime | awk -F'load average:' '{print $2}')"
}

# 检查最近错误
check_errors() {
    echo -e "\n${YELLOW}最近错误 (最后 5 条):${NC}"
    for service in trading-collector trading-engine; do
        errors=$(journalctl -u $service --since "1 hour ago" -p err --no-pager 2>/dev/null | tail -5)
        if [ -n "$errors" ]; then
            echo -e "  ${RED}$service:${NC}"
            echo "$errors" | while read line; do
                echo "    $line"
            done
        fi
    done
}

# 主函数
main() {
    check_services
    check_resources
    check_errors
}

# 执行
if [ "$WATCH_MODE" = true ]; then
    while true; do
        clear
        main
        echo -e "\n${BLUE}按 Ctrl+C 退出监控${NC}"
        sleep 5
    done
else
    main
fi
