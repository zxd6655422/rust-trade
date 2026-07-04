#!/bin/bash
# ============================================================
# 数据归档脚本
# 将 PostgreSQL 中的历史 K线数据导出到 Parquet 文件
# 用法: bash ~/apps/trading-core/archive.sh [--days 7] [--symbol BTCUSDT]
# ============================================================

set -e

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

# 目录配置
APPS_DIR="$HOME/apps"
ARCHIVE_BIN="$APPS_DIR/trading-core/archive_klines"
PARQUET_DIR="$APPS_DIR/trading-data/parquet"
LOG_DIR="$APPS_DIR/trading-data/logs"

# 默认参数
DAYS=7
SYMBOL=""

while [[ $# -gt 0 ]]; do
    case $1 in
        --days) DAYS="$2"; shift 2 ;;
        --symbol) SYMBOL="$2"; shift 2 ;;
        *) echo -e "${RED}Unknown option: $1${NC}"; exit 1 ;;
    esac
done

# 检查二进制
if [ ! -f "$ARCHIVE_BIN" ]; then
    echo -e "${RED}✗ 未找到 archive_klines，请先执行 publish.sh 编译${NC}"
    exit 1
fi

echo -e "${GREEN}================================================${NC}"
echo -e "${GREEN}  K线数据归档${NC}"
echo -e "${GREEN}================================================${NC}"
echo ""
echo -e "归档天数: ${YELLOW}${DAYS} 天前${NC}"
echo -e "交易对: ${YELLOW}${SYMBOL:-全部}${NC}"
echo -e "输出路径: ${YELLOW}${PARQUET_DIR}${NC}"
echo ""

# 构建命令
mkdir -p "$PARQUET_DIR" "$LOG_DIR"
LOG_FILE="$LOG_DIR/archive_$(date +%Y%m%d_%H%M%S).log"

CMD="$ARCHIVE_BIN --days $DAYS --output $PARQUET_DIR"
if [ -n "$SYMBOL" ]; then
    CMD="$CMD --symbol $SYMBOL"
fi

# 执行归档
echo -e "${YELLOW}开始归档...${NC}"
$CMD 2>&1 | tee "$LOG_FILE"

if [ ${PIPESTATUS[0]} -eq 0 ]; then
    echo -e "\n${GREEN}================================================${NC}"
    echo -e "${GREEN}  归档完成!${NC}"
    echo -e "${GREEN}================================================${NC}"
    echo ""
    echo -e "日志: ${YELLOW}${LOG_FILE}${NC}"

    # 显示统计
    echo -e "\n${YELLOW}Parquet 文件统计:${NC}"
    for symbol_dir in "$PARQUET_DIR"/*/; do
        if [ -d "$symbol_dir" ]; then
            sym=$(basename "$symbol_dir")
            count=$(find "$symbol_dir" -name "*.parquet" | wc -l)
            size=$(du -sh "$symbol_dir" 2>/dev/null | cut -f1)
            echo -e "  $sym: $count 文件, $size"
        fi
    done
else
    echo -e "\n${RED}✗ 归档失败，请查看日志: ${LOG_FILE}${NC}"
    exit 1
fi
