#!/bin/bash
# ============================================================
# 数据备份脚本
# 用法: ./backup.sh [--full] [--parquet] [--config]
# ============================================================

set -e

# 颜色定义
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

# 配置
APPS_DIR="$HOME/apps"
DATA_DIR="$APPS_DIR/trading-data"
BACKUP_DIR="$DATA_DIR/backups"
RETENTION_DAYS=30

# 解析参数
BACKUP_FULL=false
BACKUP_PARQUET=false
BACKUP_CONFIG=false

for arg in "$@"; do
    case $arg in
        --full) BACKUP_FULL=true ;;
        --parquet) BACKUP_PARQUET=true ;;
        --config) BACKUP_CONFIG=true ;;
    esac
done

# 如果没有指定参数，默认备份配置
if [ "$BACKUP_FULL" = false ] && [ "$BACKUP_PARQUET" = false ] && [ "$BACKUP_CONFIG" = false ]; then
    BACKUP_CONFIG=true
fi

echo -e "${GREEN}================================================${NC}"
echo -e "${GREEN}  数据备份${NC}"
echo -e "${GREEN}================================================${NC}"

# 创建备份目录
TIMESTAMP=$(date +%Y%m%d_%H%M%S)
BACKUP_PATH="$BACKUP_DIR/$TIMESTAMP"
mkdir -p $BACKUP_PATH

# 加载环境变量
if [ -f "$APPS_DIR/trading-engine/.env" ]; then
    source $APPS_DIR/trading-engine/.env
fi

# 备份配置文件
if [ "$BACKUP_CONFIG" = true ]; then
    echo -e "\n${YELLOW}备份配置文件...${NC}"
    mkdir -p $BACKUP_PATH/config
    cp -r $APPS_DIR/trading-core/config/* $BACKUP_PATH/config/trading-core/ 2>/dev/null || true
    cp -r $APPS_DIR/trading-engine/config/* $BACKUP_PATH/config/trading-engine/ 2>/dev/null || true
    cp $APPS_DIR/strategy-service/config/.env.production $BACKUP_PATH/config/strategy-service.env.production 2>/dev/null || true
    echo -e "${GREEN}✓ 配置文件已备份${NC}"
fi

# 备份 Parquet 数据
if [ "$BACKUP_PARQUET" = true ] || [ "$BACKUP_FULL" = true ]; then
    echo -e "\n${YELLOW}备份 Parquet 数据...${NC}"
    if [ -d "$DATA_DIR/parquet" ]; then
        mkdir -p $BACKUP_PATH/parquet
        cp -r $DATA_DIR/parquet/* $BACKUP_PATH/parquet/
        echo -e "${GREEN}✓ Parquet 数据已备份${NC}"
    else
        echo -e "${YELLOW}⚠ 无 Parquet 数据${NC}"
    fi
fi

# 备份数据库
if [ "$BACKUP_FULL" = true ]; then
    echo -e "\n${YELLOW}备份数据库...${NC}"
    if PGPASSWORD=$DB_PASSWORD pg_isready -h $DB_HOST -p $DB_PORT -U postgres >/dev/null 2>&1; then
        PGPASSWORD=$DB_PASSWORD pg_dump -h $DB_HOST -p $DB_PORT -U postgres -Fc $DB_NAME > $BACKUP_PATH/database.dump
        echo -e "${GREEN}✓ 数据库已备份${NC}"
    else
        echo -e "${RED}✗ 无法连接数据库${NC}"
    fi
fi

# 压缩备份
echo -e "\n${YELLOW}压缩备份...${NC}"
BACKUP_ARCHIVE="$BACKUP_DIR/backup_$TIMESTAMP.tar.gz"
tar -czf $BACKUP_ARCHIVE -C $BACKUP_DIR $TIMESTAMP
rm -rf $BACKUP_PATH

echo -e "${GREEN}✓ 备份已压缩: $BACKUP_ARCHIVE${NC}"

# 清理旧备份
echo -e "\n${YELLOW}清理 $RETENTION_DAYS 天前的备份...${NC}"
find $BACKUP_DIR -name "backup_*.tar.gz" -mtime +$RETENTION_DAYS -delete
echo -e "${GREEN}✓ 旧备份已清理${NC}"

# 显示备份统计
echo -e "\n${GREEN}================================================${NC}"
echo -e "${GREEN}  备份完成!${NC}"
echo -e "${GREEN}================================================${NC}"
echo ""
echo -e "备份文件: ${YELLOW}$BACKUP_ARCHIVE${NC}"
echo -e "文件大小: ${YELLOW}$(du -sh $BACKUP_ARCHIVE | cut -f1)${NC}"
echo -e "保留天数: ${YELLOW}$RETENTION_DAYS 天${NC}"
echo ""
