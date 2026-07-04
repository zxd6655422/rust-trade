#!/bin/bash
cd "$(dirname "$0")"
export RUN_MODE=production
export RUST_LOG=info

mkdir -p logs
LOG_FILE="logs/trading-engine_$(date +%Y%m%d_%H%M%S).log"

echo "Starting Trading Engine... Log: $LOG_FILE"
nohup ./trading-engine > "$LOG_FILE" 2>&1 &
echo "PID: $!"
