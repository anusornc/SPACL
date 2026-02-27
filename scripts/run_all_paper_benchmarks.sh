#!/bin/bash
# Run ALL paper benchmarks with proper epochs
# This script runs benchmarks sequentially and saves all results
# Usage: ./scripts/run_all_paper_benchmarks.sh
# Estimated time: 2-4 hours

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"
cd "$PROJECT_ROOT"

# Configuration
EPOCHS=5
LOG_FILE="results/benchmark_run_$(date +%Y%m%d_%H%M%S).log"

# Benchmarks from the paper
BENCHMARKS=(
    "spacl_vs_sequential"
    "disjunctive_ontologies" 
    "scalability"
    "real_world_benchmark"
)

echo "========================================" | tee -a "$LOG_FILE"
echo "  FULL PAPER BENCHMARK RUN" | tee -a "$LOG_FILE"
echo "========================================" | tee -a "$LOG_FILE"
echo "" | tee -a "$LOG_FILE"
echo "Started: $(date)" | tee -a "$LOG_FILE"
echo "Epochs per benchmark: $EPOCHS" | tee -a "$LOG_FILE"
echo "Total benchmarks: ${#BENCHMARKS[@]}" | tee -a "$LOG_FILE"
echo "Estimated time: 2-4 hours" | tee -a "$LOG_FILE"
echo "" | tee -a "$LOG_FILE"
echo "Log file: $LOG_FILE" | tee -a "$LOG_FILE"
echo "" | tee -a "$LOG_FILE"

# Confirm hardware
echo "=== Hardware Check ===" | tee -a "$LOG_FILE"
ACTUAL_CPU=$(cat /proc/cpuinfo 2>/dev/null | grep "model name" | head -1 | cut -d: -f2 | xargs || echo "Unknown")
echo "CPU: $ACTUAL_CPU" | tee -a "$LOG_FILE"
echo "" | tee -a "$LOG_FILE"

if [[ "$ACTUAL_CPU" != *"Intel"* ]] && [[ "$ACTUAL_CPU" != *"Xeon"* ]]; then
    echo "WARNING: This doesn't match paper hardware (Intel Xeon)" | tee -a "$LOG_FILE"
    echo "Expected: Intel Xeon Silver 4214" | tee -a "$LOG_FILE"
    echo "Actual: $ACTUAL_CPU" | tee -a "$LOG_FILE"
    echo "" | tee -a "$LOG_FILE"
fi

# Run each benchmark
TOTAL=${#BENCHMARKS[@]}
CURRENT=0

for BENCH in "${BENCHMARKS[@]}"; do
    CURRENT=$((CURRENT + 1))
    echo "========================================" | tee -a "$LOG_FILE"
    echo "  Benchmark $CURRENT/$TOTAL: $BENCH" | tee -a "$LOG_FILE"
    echo "  Started: $(date)" | tee -a "$LOG_FILE"
    echo "========================================" | tee -a "$LOG_FILE"
    echo "" | tee -a "$LOG_FILE"
    
    # Run benchmark with epochs
    if ./scripts/run_benchmark_v2.sh "$BENCH" $EPOCHS <<< "y" 2>&1 | tee -a "$LOG_FILE"; then
        echo "" | tee -a "$LOG_FILE"
        echo "✓ Benchmark $BENCH completed successfully" | tee -a "$LOG_FILE"
    else
        echo "" | tee -a "$LOG_FILE"
        echo "✗ Benchmark $BENCH failed or timed out" | tee -a "$LOG_FILE"
    fi
    
    echo "" | tee -a "$LOG_FILE"
    echo "Completed: $(date)" | tee -a "$LOG_FILE"
    echo "" | tee -a "$LOG_FILE"
    
    # Small delay between benchmarks
    sleep 10
done

echo "========================================" | tee -a "$LOG_FILE"
echo "  ALL BENCHMARKS COMPLETE" | tee -a "$LOG_FILE"
echo "========================================" | tee -a "$LOG_FILE"
echo "" | tee -a "$LOG_FILE"
echo "Finished: $(date)" | tee -a "$LOG_FILE"
echo "" | tee -a "$LOG_FILE"
echo "Results saved to: results/history/" | tee -a "$LOG_FILE"
echo "" | tee -a "$LOG_FILE"
echo "Next steps:" | tee -a "$LOG_FILE"
echo "  1. Review benchmark results in results/history/" | tee -a "$LOG_FILE"
echo "  2. Extract performance numbers from each run" | tee -a "$LOG_FILE"
echo "  3. Update paper/tables with new numbers" | tee -a "$LOG_FILE"
echo "  4. Recompile manuscript.pdf" | tee -a "$LOG_FILE"
echo "" | tee -a "$LOG_FILE"
echo "To compare results:" | tee -a "$LOG_FILE"
echo "  ./scripts/list_benchmarks.sh" | tee -a "$LOG_FILE"
echo "" | tee -a "$LOG_FILE"

echo ""
echo "========================================"
echo "  FULL BENCHMARK RUN COMPLETE"
echo "========================================"
echo ""
echo "Log saved to: $LOG_FILE"
echo "View with: cat $LOG_FILE"
echo ""
