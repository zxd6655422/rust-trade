#!/bin/bash
# ============================================================
# logs.sh - 日志查看快捷脚本
# 用法: bash ~/apps/deploy/logs.sh [服务名] [选项]
# ============================================================

set -e

GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

# 默认服务
SERVICE="trading-collector"
LINES=100
FOLLOW=false
SINCE=""
LEVEL=""

# 解析参数
while [[ $# -gt 0 ]]; do
    case $1 in
        trading-collector|trading-engine|strategy-service|trading-archive)
            SERVICE="$1"; shift ;;
        -f|--follow)
            FOLLOW=true; shift ;;
        -n|--lines)
            LINES="$2"; shift 2 ;;
        -s|--since)
            SINCE="$2"; shift 2 ;;
        -p|--priority)
            LEVEL="$2"; shift 2 ;;
        -h|--help)
            echo "用法: logs.sh [服务名] [选项]"
            echo ""
            echo "服务名:"
            echo "  trading-collector   数据采集服务（默认）"
            echo "  trading-engine      交易引擎服务"
            echo "  strategy-service    策略分析服务"
            echo "  trading-archive     归档任务"
            echo ""
            echo "选项:"
            echo "  -f, --follow        实时跟踪日志"
            echo "  -n, --lines N       显示最近 N 行（默认 100）"
            echo "  -s, --since TIME    显示指定时间后的日志"
            echo "  -p, --priority LVL  只显示指定级别（emerg/alert/crit/err/warning/notice/info/debug）"
            echo ""
            echo "示例:"
            echo "  logs.sh                           # 查看采集服务最近 100 行"
            echo "  logs.sh trading-engine -f         # 实时跟踪引擎日志"
            echo "  logs.sh -n 50                     # 查看最近 50 行"
            echo "  logs.sh -s '1 hour ago'           # 最近 1 小时的日志"
            echo "  logs.sh -p err                    # 只看错误日志"
            echo "  logs.sh -s today                  # 今天的日志"
            exit 0
            ;;
        *)
            echo "未知选项: $1"
            exit 1
            ;;
    esac
done

# 构建命令
CMD="sudo journalctl -u $SERVICE"

if [ "$FOLLOW" = true ]; then
    CMD="$CMD -f"
fi

if [ -n "$SINCE" ]; then
    CMD="$CMD --since \"$SINCE\""
else
    CMD="$CMD -n $LINES"
fi

if [ -n "$LEVEL" ]; then
    CMD="$CMD -p $LEVEL"
fi

# 输出提示
echo -e "${GREEN}=== $SERVICE 日志 ===${NC}"
if [ "$FOLLOW" = true ]; then
    echo -e "${YELLOW}实时跟踪中... (按 Ctrl+C 退出)${NC}"
fi
echo ""

# 执行
eval $CMD
