#!/bin/bash
# Benchmark Runner with Timestamped Results
# Usage: ./scripts/run_benchmark.sh [benchmark_name]
# Example: ./scripts/run_benchmark.sh spacl_vs_sequential

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"
cd "$PROJECT_ROOT"

# Generate timestamp
TIMESTAMP=$(date +"%Y%m%d_%H%M%S")
DATE=$(date +"%Y-%m-%d %H:%M:%S")
HOSTNAME=$(hostname)

# Get benchmark name
BENCH_NAME="${1:-quick_benchmark}"
RESULTS_DIR="results/history/${TIMESTAMP}"
mkdir -p "$RESULTS_DIR"

echo "========================================"
echo "  Running Benchmark: $BENCH_NAME"
echo "========================================"
echo "Date: $DATE"
echo "Host: $HOSTNAME"
echo "Results: $RESULTS_DIR"
echo ""

# Record system info
cat > "$RESULTS_DIR/system_info.txt" << EOF
Benchmark Run: $BENCH_NAME
Date: $DATE
Hostname: $HOSTNAME

System Information:
$(uname -a)

CPU Info:
$(cat /proc/cpuinfo 2>/dev/null | grep "model name" | head -1 || echo "N/A")

Memory:
$(free -h 2>/dev/null || echo "N/A")

Rust Version:
$(rustc --version)

Cargo Version:
$(cargo --version)
EOF

echo "System info saved."
echo ""

# Run the benchmark
echo "Running cargo bench --bench $BENCH_NAME..."
cargo bench --bench "$BENCH_NAME" 2>&1 | tee "$RESULTS_DIR/benchmark_output.log"

# Copy criterion results if they exist
CRITERION_DIR="target/criterion/$BENCH_NAME"
if [ -d "$CRITERION_DIR" ]; then
    echo ""
    echo "Copying Criterion results..."
    cp -r "$CRITERION_DIR" "$RESULTS_DIR/criterion_report"
fi

# Extract key metrics
echo ""
echo "Extracting metrics..."
grep -E "(time:|Benchmarking|test.*ok|test.*FAILED)" "$RESULTS_DIR/benchmark_output.log" > "$RESULTS_DIR/summary.txt" 2>/dev/null || true

# Create result summary
cat > "$RESULTS_DIR/README.md" << EOF
# Benchmark Results: $BENCH_NAME

**Date:** $DATE  
**Host:** $HOSTNAME  
**Benchmark:** $BENCH_NAME

## Files

- \`benchmark_output.log\` - Full benchmark output
- \`system_info.txt\` - System configuration
- \`summary.txt\` - Key metrics summary
- \`criterion_report/\` - Criterion benchmark reports (if available)

## Quick Stats

\`\`\`
$(tail -20 "$RESULTS_DIR/summary.txt" 2>/dev/null || echo "See benchmark_output.log for details")
\`\`\`

## Comparison

To compare with previous runs:
\`\`\`bash
./scripts/compare_benchmarks.sh ${TIMESTAMP} <previous_timestamp>
\`\`\`
EOF

echo ""
echo "========================================"
echo "  Benchmark Complete!"
echo "========================================"
echo "Results saved to: $RESULTS_DIR"
echo ""
echo "To compare with previous runs:"
echo "  ./scripts/compare_benchmarks.sh ${TIMESTAMP} <previous_timestamp>"
echo ""
echo "To view all benchmark history:"
echo "  ls -la results/history/"
