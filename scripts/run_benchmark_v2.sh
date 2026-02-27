#!/bin/bash
# Benchmark Runner V2 with Epochs and Hardware Validation
# Usage: ./scripts/run_benchmark_v2.sh <benchmark_name> [num_epochs]

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"
cd "$PROJECT_ROOT"

# Configuration
BENCH_NAME="${1:-quick_benchmark}"
EPOCHS="${2:-3}"  # Default 3 epochs
TIMESTAMP=$(date +"%Y%m%d_%H%M%S")
DATE=$(date +"%Y-%m-%d %H:%M:%S")
HOSTNAME=$(hostname)

# Expected hardware (from paper claims - CORRECTED)
EXPECTED_CPU="Intel Xeon Silver 4214"
EXPECTED_ARCH="Intel"

echo "========================================"
echo "  Benchmark Runner V2"
echo "========================================"
echo ""
echo "Benchmark: $BENCH_NAME"
echo "Epochs: $EPOCHS"
echo "Date: $DATE"
echo ""

# ============================================
# HARDWARE VALIDATION
# ============================================
echo "=== Hardware Validation ==="

# Detect actual hardware
ACTUAL_CPU=$(cat /proc/cpuinfo 2>/dev/null | grep "model name" | head -1 | cut -d: -f2 | xargs || echo "Unknown")
ACTUAL_ARCH=$(uname -m)
CPU_CORES=$(nproc)
MEMORY_GB=$(free -g 2>/dev/null | grep Mem | awk '{print $2}' || echo "Unknown")

echo "Detected CPU: $ACTUAL_CPU"
echo "Architecture: $ACTUAL_ARCH"
echo "CPU Cores: $CPU_CORES"
echo "Memory: ${MEMORY_GB}GB"
echo ""

# Check for hardware inconsistency
if [[ "$ACTUAL_CPU" != *"$EXPECTED_ARCH"* ]] && [[ "$EXPECTED_CPU" == *"AMD"* ]]; then
    echo "⚠️  WARNING: HARDWARE INCONSISTENCY DETECTED!"
    echo ""
    echo "Expected (Paper Claims): $EXPECTED_CPU"
    echo "Actual (Current System): $ACTUAL_CPU"
    echo ""
    echo "This is a CRITICAL issue for paper reproducibility!"
    echo ""
    echo "Options:"
    echo "  1. Run on correct hardware ($EXPECTED_CPU)"
    echo "  2. Update paper hardware specifications"
    echo "  3. Continue with warning (document this difference)"
    echo ""
    read -p "Continue anyway? (y/N): " -n 1 -r
    echo
    if [[ ! $REPLY =~ ^[Yy]$ ]]; then
        echo "Benchmark cancelled. Please fix hardware inconsistency."
        exit 1
    fi
    echo "Continuing with WARNING..."
    echo ""
fi

# ============================================
# CREATE RESULTS DIRECTORY
# ============================================
RESULTS_DIR="results/history/${TIMESTAMP}"
mkdir -p "$RESULTS_DIR"

echo "Results directory: $RESULTS_DIR"
echo ""

# ============================================
# SAVE SYSTEM INFO
# ============================================
cat > "$RESULTS_DIR/system_info.txt" << EOF
Benchmark Run: $BENCH_NAME
Date: $DATE
Hostname: $HOSTNAME
Epochs: $EPOCHS

DETECTED HARDWARE:
CPU: $ACTUAL_CPU
Architecture: $ACTUAL_ARCH
Cores: $CPU_CORES
Memory: ${MEMORY_GB}GB

CLAIMED HARDWARE (from paper):
CPU: $EXPECTED_CPU

HARDWARE STATUS: $(if [[ "$ACTUAL_CPU" == *"$EXPECTED_ARCH"* ]]; then echo "MATCH"; else echo "MISMATCH - See WARNING"; fi)

System Information:
$(uname -a)

Memory Details:
$(free -h 2>/dev/null || echo "N/A")

Rust Version:
$(rustc --version)

Cargo Version:
$(cargo --version)
EOF

echo "System info saved."
echo ""

# ============================================
# RUN MULTIPLE EPOCHS
# ============================================
echo "=== Running $EPOCHS Epochs ==="
echo ""

for epoch in $(seq 1 $EPOCHS); do
    EPOCH_DIR="$RESULTS_DIR/epoch_$epoch"
    mkdir -p "$EPOCH_DIR"
    
    echo "--- Epoch $epoch of $EPOCHS ---"
    echo "Started: $(date +"%H:%M:%S")"
    
    # Run the benchmark
    if cargo bench --bench "$BENCH_NAME" 2>&1 | tee "$EPOCH_DIR/benchmark_output.log"; then
        echo "Epoch $epoch: COMPLETED" >> "$RESULTS_DIR/epochs_summary.txt"
    else
        echo "Epoch $epoch: FAILED" >> "$RESULTS_DIR/epochs_summary.txt"
        echo "Warning: Epoch $epoch failed, continuing..."
    fi
    
    echo "Finished: $(date +"%H:%M:%S")"
    echo ""
done

# ============================================
# EXTRACT METRICS FROM ALL EPOCHS
# ============================================
echo "=== Extracting Metrics ==="

for epoch in $(seq 1 $EPOCHS); do
    EPOCH_DIR="$RESULTS_DIR/epoch_$epoch"
    if [ -f "$EPOCH_DIR/benchmark_output.log" ]; then
        # Extract timing information
        grep -E "(time:|Benchmarking|test result)" "$EPOCH_DIR/benchmark_output.log" > "$EPOCH_DIR/metrics.txt" 2>/dev/null || true
    fi
done

# ============================================
# CREATE COMPARISON ACROSS EPOCHS
# ============================================
echo "Creating epoch comparison..."

cat > "$RESULTS_DIR/epoch_comparison.md" << EOF
# Benchmark Epoch Comparison

**Benchmark:** $BENCH_NAME  
**Date:** $DATE  
**Total Epochs:** $EPOCHS  
**Hardware:** $ACTUAL_CPU

## Hardware Validation

| Property | Expected (Paper) | Actual (This Run) | Status |
|----------|------------------|-------------------|--------|
| CPU | $EXPECTED_CPU | $ACTUAL_CPU | $(if [[ "$ACTUAL_CPU" == *"$EXPECTED_ARCH"* ]]; then echo "✅ Match"; else echo "⚠️ Mismatch"; fi) |
| Cores | 12/24 | $CPU_CORES | - |
| Memory | 64GB | ${MEMORY_GB}GB | - |

$(if [[ "$ACTUAL_CPU" != *"$EXPECTED_ARCH"* ]]; then echo "**⚠️ WARNING:** Hardware mismatch detected! Results may not match paper claims."; fi)

## Epoch Results

| Epoch | Status | Timestamp |
|-------|--------|-----------|
$(for e in $(seq 1 $EPOCHS); do 
    status=$(grep "Epoch $e:" "$RESULTS_DIR/epochs_summary.txt" 2>/dev/null | cut -d: -f2 | xargs || echo "Unknown")
    echo "| $e | $status | $(stat -c %y "$RESULTS_DIR/epoch_$e/benchmark_output.log" 2>/dev/null | cut -d. -f1 || echo "N/A") |"
done)

## Performance Consistency

$(if [ $EPOCHS -gt 1 ]; then
    echo "### Variance Analysis"
    echo ""
    echo "To analyze variance across epochs, check individual epoch metrics:"
    for e in $(seq 1 $EPOCHS); do
        echo "- Epoch $e: \`epoch_$e/metrics.txt\`"
    done
    echo ""
    echo "### Coefficient of Variation (CV)"
    echo "Calculate CV to check consistency:"
    echo '\`\`\`bash'
    echo "# Extract times and calculate statistics"
    echo "for f in epoch_*/metrics.txt; do echo \"\$f:\"; grep 'time:' \$f | head -5; done"
    echo '\`\`\`'
else
    echo "Only 1 epoch run. For variance analysis, run with multiple epochs:"
    echo "\`./scripts/run_benchmark_v2.sh $BENCH_NAME 5\`"
fi)

## Recommendations

$(if [ $EPOCHS -lt 3 ]; then
    echo "⚠️ **Low epoch count** ($EPOCHS). For statistical significance, recommend at least 3-5 epochs."
    echo ""
fi)

$(if [[ "$ACTUAL_CPU" != *"$EXPECTED_ARCH"* ]]; then
    echo "⚠️ **Hardware mismatch** detected. To ensure reproducibility:"
    echo "1. Run benchmarks on claimed hardware ($EXPECTED_CPU)"
    echo "2. OR update paper hardware specifications"
    echo "3. OR add erratum noting hardware change"
    echo ""
fi)

## Files

- \`system_info.txt\` - Complete system configuration
- \`epochs_summary.txt\` - Summary of all epochs
- \`epoch_N/benchmark_output.log\` - Full output from epoch N
- \`epoch_N/metrics.txt\` - Extracted metrics from epoch N

## Next Steps

1. Check consistency across epochs
2. Calculate average and standard deviation
3. Compare with paper claims
4. Document any discrepancies
EOF

echo "Epoch comparison created."

# ============================================
# CREATE SUMMARY README
# ============================================
cat > "$RESULTS_DIR/README.md" << EOF
# Benchmark Results: $BENCH_NAME

**Date:** $DATE  
**Host:** $HOSTNAME  
**Epochs:** $EPOCHS  
**Hardware:** $ACTUAL_CPU

## Quick Stats

$(cat "$RESULTS_DIR/epochs_summary.txt" 2>/dev/null || echo "See epoch_comparison.md for details")

## Files

- \`epoch_comparison.md\` - Detailed comparison across epochs
- \`system_info.txt\` - System configuration
- \`epoch_N/\` - Individual epoch results

## Hardware Validation

$(if [[ "$ACTUAL_CPU" != *"$EXPECTED_ARCH"* ]]; then
    echo "⚠️ **WARNING:** Hardware does not match paper claims!"
    echo "- Paper claims: $EXPECTED_CPU"
    echo "- Actual: $ACTUAL_CPU"
    echo ""
    echo "**See BENCHMARK_INCONSISTENCY_WARNING.md for details.**"
else
    echo "✅ Hardware matches paper claims ($EXPECTED_CPU)"
fi)

## Comparison

Compare with other runs:
\`\`\`bash
./scripts/compare_benchmarks.sh ${TIMESTAMP} <other_timestamp>
\`\`\`
EOF

# ============================================
# FINAL SUMMARY
# ============================================
echo ""
echo "========================================"
echo "  Benchmark Complete!"
echo "========================================"
echo ""
echo "Results saved to: $RESULTS_DIR"
echo ""
echo "Epochs completed: $EPOCHS"
cat "$RESULTS_DIR/epochs_summary.txt" 2>/dev/null || echo "See epoch_comparison.md"
echo ""

# Hardware warning
if [[ "$ACTUAL_CPU" != *"$EXPECTED_ARCH"* ]]; then
    echo "⚠️  WARNING: Hardware mismatch detected!"
    echo "    See: $RESULTS_DIR/README.md"
    echo "    And: paper/guides/BENCHMARK_INCONSISTENCY_WARNING.md"
    echo ""
fi

echo "View detailed comparison:"
echo "  cat $RESULTS_DIR/epoch_comparison.md"
echo ""
echo "Compare with other runs:"
echo "  ./scripts/compare_benchmarks.sh ${TIMESTAMP} <other_timestamp>"
