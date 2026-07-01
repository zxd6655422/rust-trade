#!/bin/bash
# 统一打包部署脚本 (Linux/macOS)

set -e

echo "🚀 开始打包..."

# 清理
rm -rf dist
mkdir -p dist

# ==================== trading-core ====================
echo "📦 编译 trading-core..."
cargo build --release -p trading-core

CORE_DIR="dist/trading-core"
mkdir -p $CORE_DIR/config

cp target/release/trading-core $CORE_DIR/
cp config/development.toml $CORE_DIR/config/
cp config/production.toml $CORE_DIR/config/

cat > $CORE_DIR/start.sh << 'EOF'
#!/bin/bash
cd "$(dirname "$0")"
export RUN_MODE=${RUN_MODE:-production}
echo "🚀 Starting Trading Core (mode: $RUN_MODE)..."
./trading-core service
EOF
chmod +x $CORE_DIR/start.sh

# ==================== trading-engine ====================
echo "📦 编译 trading-engine..."
cargo build --release -p trading-engine

ENGINE_DIR="dist/trading-engine"
mkdir -p $ENGINE_DIR/config

cp target/release/trading-engine $ENGINE_DIR/
cp config/engine-development.toml $ENGINE_DIR/config/
cp config/engine-production.toml $ENGINE_DIR/config/

cat > $ENGINE_DIR/start.sh << 'EOF'
#!/bin/bash
cd "$(dirname "$0")"
export RUN_MODE=${RUN_MODE:-production}
echo "🚀 Starting Trading Engine (mode: $RUN_MODE)..."
./trading-engine
EOF
chmod +x $ENGINE_DIR/start.sh

echo ""
echo "✅ 打包完成!"
echo ""
echo "📦 生成目录:"
echo "  - dist/trading-core/"
echo "  - dist/trading-engine/"
echo ""
echo "📋 部署步骤:"
echo "  # 复制到服务器"
echo "  scp -r dist/ user@server:/opt/"
echo ""
echo "  # 服务器上启动"
echo "  cd /opt/dist/trading-core && ./start.sh"
echo "  cd /opt/dist/trading-engine && ./start.sh"
