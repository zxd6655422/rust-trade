#!/bin/bash
# ============================================================
# 数据归档脚本
# 将 PostgreSQL 中的历史 K线数据导出到 Parquet 文件
# 用法: ./archive.sh [--days 7] [--symbol BTCUSDT]
# ============================================================

set -e

# 颜色定义
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

# 配置
APP_DIR="${APP_DIR:-/opt/trading}"
PARQUET_DIR="${APP_DIR}/data/parquet"
LOG_FILE="${APP_DIR}/logs/archive_$(date +%Y%m%d_%H%M%S).log"

# 默认参数
DAYS=7
SYMBOL=""

# 解析参数
while [[ $# -gt 0 ]]; do
    case $1 in
        --days)
            DAYS="$2"
            shift 2
            ;;
        --symbol)
            SYMBOL="$2"
            shift 2
            ;;
        *)
            echo -e "${RED}Unknown option: $1${NC}"
            exit 1
            ;;
    esac
done

echo -e "${GREEN}================================================${NC}"
echo -e "${GREEN}  K线数据归档${NC}"
echo -e "${GREEN}================================================${NC}"
echo ""
echo -e "归档天数: ${YELLOW}${DAYS} 天前${NC}"
if [ -n "$SYMBOL" ]; then
    echo -e "交易对: ${YELLOW}${SYMBOL}${NC}"
else
    echo -e "交易对: ${YELLOW}全部${NC}"
fi
echo ""

# 加载环境变量
if [ -f "$APP_DIR/.env" ]; then
    source $APP_DIR/.env
fi

# 检查数据库连接
echo -e "${YELLOW}检查数据库连接...${NC}"
if ! PGPASSWORD=$DB_PASSWORD psql -h $DB_HOST -p $DB_PORT -U postgres -c '\q' 2>/dev/null; then
    echo -e "${RED}✗ 无法连接数据库${NC}"
    exit 1
fi
echo -e "${GREEN}✓ 数据库连接正常${NC}"

# 执行归档
echo -e "\n${YELLOW}开始归档...${NC}"

ARCHIVE_CMD="cargo run --release --bin archive_klines -- --days $DAYS --output $PARQUET_DIR"

if [ -n "$SYMBOL" ]; then
    ARCHIVE_CMD="$ARCHIVE_CMD --symbol $SYMBOL"
fi

# 执行归档命令
cd $APP_DIR
eval $ARCHIVE_CMD 2>&1 | tee $LOG_FILE

# 检查结果
if [ ${PIPESTATUS[0]} -eq 0 ]; then
    echo -e "\n${GREEN}================================================${NC}"
    echo -e "${GREEN}  归档完成!${NC}"
    echo -e "${GREEN}================================================${NC}"
    echo ""
    echo -e "Parquet 文件位置: ${YELLOW}${PARQUET_DIR}${NC}"
    echo -e "日志文件: ${YELLOW}${LOG_FILE}${NC}"
    echo ""

    # 显示统计
    echo -e "${YELLOW}Parquet 文件统计:${NC}"
    for symbol_dir in $PARQUET_DIR/*/; do
        if [ -d "$symbol_dir" ]; then
            symbol=$(basename "$symbol_dir")
            count=$(ls -1 "$symbol_dir"/*.parquet 2>/dev/null | wc -l)
            size=$(du -sh "$symbol_dir" 2>/dev/null | cut -f1)
            echo -e "  $symbol: $count 文件, $size"
        fi
    done
else
    echo -e "\n${RED}✗ 归档失败，请查看日志: ${LOG_FILE}${NC}"
    exit 1
fi
