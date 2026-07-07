#!/bin/bash
cd "$(dirname "$0")"
export RUN_MODE=production
export RUST_LOG=info

mkdir -p logs
LOG_FILE="logs/strategy-service_$(date +%Y%m%d_%H%M%S).log"

echo "Starting Strategy Service... Log: $LOG_FILE"
nohup ./strategy-service > "$LOG_FILE" 2>&1 &
echo "PID: $!"
