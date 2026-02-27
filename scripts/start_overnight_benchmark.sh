#!/bin/bash
# Start overnight benchmark in background with logging

REPO_DIR="/home/admindigit/tableauxx"
LOG_DIR="$REPO_DIR/results"
LOG_FILE="$LOG_DIR/overnight_benchmark_$(date +%Y%m%d_%H%M%S).log"
PID_FILE="$LOG_DIR/overnight_benchmark.pid"

# Create log directory
mkdir -p "$LOG_DIR"

echo "Starting overnight benchmark..."
echo "Log file: $LOG_FILE"
echo ""

# Change to repo directory
cd "$REPO_DIR"

# Build in release mode first (so we don't wait during benchmark)
echo "Building release binary..."
cargo build --release --example overnight_test 2>&1 | tee "$LOG_FILE"

echo ""
echo "Build complete. Starting benchmark..."
echo "$(date): Benchmark started" >> "$LOG_FILE"

# Run benchmark in background with nohup
nohup cargo run --release --example overnight_test >> "$LOG_FILE" 2>&1 &
BENCHMARK_PID=$!

# Save PID
echo $BENCHMARK_PID > "$PID_FILE"

echo "Benchmark started with PID: $BENCHMARK_PID"
echo "PID saved to: $PID_FILE"
echo ""
echo "To monitor progress:"
echo "  tail -f $LOG_FILE"
echo ""
echo "To check if running:"
echo "  ps aux | grep overnight_test"
echo ""
echo "To stop:"
echo "  kill $(cat $PID_FILE)"
echo ""

# Show initial log
echo "Showing first 50 lines of log..."
tail -n 50 "$LOG_FILE"
