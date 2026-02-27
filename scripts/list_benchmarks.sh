#!/bin/bash
# List all benchmark runs and show statistics

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"
cd "$PROJECT_ROOT"

HISTORY_DIR="results/history"

echo "========================================"
echo "  Benchmark History"
echo "========================================"
echo ""

if [ ! -d "$HISTORY_DIR" ] || [ -z "$(ls -A "$HISTORY_DIR" 2>/dev/null)" ]; then
    echo "No benchmark history found."
    echo ""
    echo "Run a benchmark with:"
    echo "  ./scripts/run_benchmark.sh <benchmark_name>"
    exit 0
fi

# List all benchmark runs
echo "Available benchmark runs:"
echo ""
printf "%-20s %-20s %-30s\n" "Timestamp" "Benchmark" "Date"
printf "%-20s %-20s %-30s\n" "--------" "---------" "----"

for dir in "$HISTORY_DIR"/*; do
    if [ -d "$dir" ]; then
        TIMESTAMP=$(basename "$dir")
        BENCH_NAME=$(head -1 "$dir/system_info.txt" 2>/dev/null | grep "Benchmark Run:" | cut -d: -f2 | xargs || echo "Unknown")
        DATE=$(head -5 "$dir/system_info.txt" 2>/dev/null | grep "Date:" | cut -d: -f2- | xargs || echo "Unknown")
        printf "%-20s %-20s %-30s\n" "$TIMESTAMP" "$BENCH_NAME" "$DATE"
    fi
done

echo ""
echo "Total benchmark runs: $(ls -1 "$HISTORY_DIR" 2>/dev/null | wc -l)"
echo ""
echo "To compare two runs:"
echo "  ./scripts/compare_benchmarks.sh <timestamp1> <timestamp2>"
echo ""
echo "To view a specific run:"
echo "  cat results/history/<timestamp>/README.md"
