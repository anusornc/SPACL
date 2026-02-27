#!/bin/bash
# Compare two benchmark runs
# Usage: ./scripts/compare_benchmarks.sh <timestamp1> <timestamp2>
# Example: ./scripts/compare_benchmarks.sh 20260208_120000 20260208_130000

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"
cd "$PROJECT_ROOT"

if [ $# -lt 2 ]; then
    echo "Usage: $0 <timestamp1> <timestamp2>"
    echo "Example: $0 20260208_120000 20260208_130000"
    echo ""
    echo "Available benchmark runs:"
    ls -1 results/history/ 2>/dev/null || echo "  No benchmark history found"
    exit 1
fi

RUN1="$1"
RUN2="$2"
DIR1="results/history/$RUN1"
DIR2="results/history/$RUN2"

if [ ! -d "$DIR1" ]; then
    echo "Error: Benchmark run not found: $RUN1"
    exit 1
fi

if [ ! -d "$DIR2" ]; then
    echo "Error: Benchmark run not found: $RUN2"
    exit 1
fi

COMPARISON_FILE="results/comparisons/comparison_${RUN1}_vs_${RUN2}.md"
mkdir -p "results/comparisons"

echo "========================================"
echo "  Benchmark Comparison"
echo "========================================"
echo "Run 1: $RUN1"
echo "Run 2: $RUN2"
echo ""

# Extract dates
DATE1=$(head -5 "$DIR1/system_info.txt" | grep "Date:" | cut -d: -f2- | xargs || echo "Unknown")
DATE2=$(head -5 "$DIR2/system_info.txt" | grep "Date:" | cut -d: -f2- | xargs || echo "Unknown")

# Create comparison report
cat > "$COMPARISON_FILE" << EOF
# Benchmark Comparison Report

Generated: $(date +"%Y-%m-%d %H:%M:%S")

## Run 1: $RUN1
**Date:** $DATE1  
**Directory:** $DIR1

## Run 2: $RUN2
**Date:** $DATE2  
**Directory:** $DIR2

## System Information Comparison

### Run 1
\`\`\`
$(cat "$DIR1/system_info.txt" 2>/dev/null || echo "N/A")
\`\`\`

### Run 2
\`\`\`
$(cat "$DIR2/system_info.txt" 2>/dev/null || echo "N/A")
\`\`\`

## Benchmark Output Comparison

### Run 1 Summary
\`\`\`
$(cat "$DIR1/summary.txt" 2>/dev/null || echo "N/A")
\`\`\`

### Run 2 Summary
\`\`\`
$(cat "$DIR2/summary.txt" 2>/dev/null || echo "N/A")
\`\`\`

## Detailed Differences

\`\`\`diff
$(diff -u "$DIR1/summary.txt" "$DIR2/summary.txt" 2>/dev/null || echo "Files are identical or cannot be compared")
\`\`\`

## Notes

- Compare the criterion reports in each directory for detailed analysis
- Check system_info.txt for hardware/environment differences
- Significant performance differences may indicate:
  - System load variations
  - Code changes between runs
  - Hardware/environment differences

EOF

echo "Comparison report saved to:"
echo "  $COMPARISON_FILE"
echo ""
echo "View with:"
echo "  cat $COMPARISON_FILE"
