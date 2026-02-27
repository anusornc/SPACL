#!/bin/bash
# Extract benchmark results from completed runs and format for paper
# Usage: ./scripts/extract_benchmark_results.sh <timestamp>

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"
cd "$PROJECT_ROOT"

if [ -z "$1" ]; then
    echo "Usage: $0 <timestamp>"
    echo ""
    echo "Available benchmark runs:"
    ./scripts/list_benchmarks.sh
    exit 1
fi

TIMESTAMP="$1"
RESULTS_DIR="results/history/$TIMESTAMP"

if [ ! -d "$RESULTS_DIR" ]; then
    echo "Error: Benchmark run not found: $TIMESTAMP"
    exit 1
fi

echo "========================================"
echo "  Extracting Benchmark Results"
echo "========================================"
echo ""
echo "Timestamp: $TIMESTAMP"
echo "Directory: $RESULTS_DIR"
echo ""

# Read system info
BENCH_NAME=$(head -1 "$RESULTS_DIR/system_info.txt" | cut -d: -f2 | xargs || echo "Unknown")
CPU=$(grep "CPU:" "$RESULTS_DIR/system_info.txt" | cut -d: -f2 | xargs || echo "Unknown")
DATE=$(grep "Date:" "$RESULTS_DIR/system_info.txt" | head -1 | cut -d: -f2- | xargs || echo "Unknown")

echo "Benchmark: $BENCH_NAME"
echo "CPU: $CPU"
echo "Date: $DATE"
echo ""

# Extract epoch results
echo "=== Epoch Results ==="
for epoch_dir in "$RESULTS_DIR"/epoch_*/; do
    if [ -d "$epoch_dir" ]; then
        EPOCH=$(basename "$epoch_dir")
        echo ""
        echo "--- $EPOCH ---"
        
        # Try to extract timing information
        if [ -f "$epoch_dir/benchmark_output.log" ]; then
            # Look for Criterion timing output
            grep -E "time:\s+\[" "$epoch_dir/benchmark_output.log" | head -5 || echo "No timing data found"
            
            # Count test results
            grep "test result:" "$epoch_dir/benchmark_output.log" | tail -1 || echo "No test results"
        fi
    fi
done

echo ""
echo "=== Summary for Paper ==="
echo ""
echo "To update paper tables, manually extract these values:"
echo "  - Mean time from all epochs"
echo "  - Standard deviation"
echo "  - Speedup ratio"
echo ""
echo "Check detailed comparison:"
echo "  cat $RESULTS_DIR/epoch_comparison.md"
echo ""

# Create summary for paper
OUTPUT_FILE="results/extracted_${TIMESTAMP}.md"

cat > "$OUTPUT_FILE" << EOF
# Extracted Benchmark Results

**Source:** $RESULTS_DIR  
**Benchmark:** $BENCH_NAME  
**Date:** $DATE  
**CPU:** $CPU

## Raw Results by Epoch

| Epoch | Status | Key Metrics |
|-------|--------|-------------|
$(for epoch_dir in "$RESULTS_DIR"/epoch_*/; do
    if [ -d "$epoch_dir" ]; then
        EPOCH=$(basename "$epoch_dir")
        STATUS=$(grep "test result:" "$epoch_dir/benchmark_output.log" 2>/dev/null | tail -1 | cut -d: -f2 | xargs || echo "Unknown")
        echo "| $EPOCH | $STATUS | See logs |"
    fi
done)

## For Paper Tables

### Template (fill in after analysis):

| Metric | Value | Unit |
|--------|-------|------|
| Mean Time | _ | ms |
| Std Dev | _ | ms |
| Min | _ | ms |
| Max | _ | ms |
| Speedup | _ | × |

### LaTeX Table Entry:

\`\`\`latex
% Add to paper/submission/manuscript.tex
\textbf{Result} & \$MEAN \pm \$STD & \$SPEEDUP\\times \\
\`\`\`

## Files to Review

- Full output: \`$RESULTS_DIR/epoch_*/benchmark_output.log\`
- Comparison: \`$RESULTS_DIR/epoch_comparison.md\`
- System info: \`$RESULTS_DIR/system_info.txt\`

## Next Steps

1. Calculate statistics from epoch results
2. Update paper table with new numbers
3. Add statistical notation (mean ± std)
4. Verify speedup calculations
EOF

echo "Extraction complete!"
echo ""
echo "Summary saved to: $OUTPUT_FILE"
echo ""
echo "View with: cat $OUTPUT_FILE"
