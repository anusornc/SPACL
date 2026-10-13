#!/bin/bash
# Monitor the overnight benchmark

REPO_DIR="/home/admindigit/tableauxx"
LOG_DIR="$REPO_DIR/results"
PID_FILE="$LOG_DIR/overnight_benchmark.pid"

echo "=== Overnight Benchmark Monitor ==="
echo ""

# Check if running
if [ -f "$PID_FILE" ]; then
    PID=$(cat "$PID_FILE")
    if ps -p "$PID" > /dev/null 2>&1; then
        echo "✓ Benchmark is RUNNING (PID: $PID)"
        echo ""
        echo "Recent progress:"
        tail -20 "$LOG_DIR"/overnight_benchmark_*.log 2>/dev/null | grep -E "PROGRESS|Testing:|✓|Speedup" | tail -10
    else
        echo "✗ Benchmark is NOT RUNNING"
        echo ""
        echo "Last 20 lines of latest log:"
        tail -20 "$LOG_DIR"/overnight_benchmark_*.log 2>/dev/null
    fi
else
    echo "? No PID file found. Checking for processes..."
    ps aux | grep overnight_test | grep -v grep
fi

echo ""
echo "=== Log Files ==="
ls -lh "$LOG_DIR"/overnight_benchmark_*.log 2>/dev/null || echo "No log files found"

echo ""
echo "=== Results Files ==="
ls -lh "$REPO_DIR"/results/overnight_results.json 2>/dev/null || echo "No results yet"

echo ""
echo "Commands:"
echo "  View full log:    tail -f $LOG_DIR/overnight_benchmark_*.log"
echo "  Check results:    cat $REPO_DIR/results/overnight_results.json"
echo "  Stop benchmark:   kill \$(cat $PID_FILE)"
