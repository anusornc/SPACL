#!/bin/bash
# Generate benchmark summary for paper

echo "=== Tableauxx Benchmark Summary ==="
echo ""
echo "Generated: $(date)"
echo ""

# Extract data from criterion results
echo "## Sequential Performance (from quick_benchmark)"
echo ""
echo "| Classes | Time | Throughput |"
echo "|---------|------|------------|"

# Try to extract from criterion JSON files
for dir in target/criterion/quick_comparison/sequential/*/; do
    if [ -f "$dir/estimates.json" ]; then
        name=$(basename "$dir")
        time=$(cat "$dir/estimates.json" | grep -o '"mean":{[^}]*' | grep -o '"point_estimate":[0-9.]*' | cut -d: -f2)
        if [ -n "$time" ]; then
            # Convert nanoseconds to microseconds
            time_us=$(echo "$time / 1000" | bc -l 2>/dev/null || echo "N/A")
            echo "| $name | ${time_us} µs | - |"
        fi
    fi
done

echo ""
echo "## SPACL vs Sequential"
echo ""

# List available benchmarks
echo "Available benchmark data:"
ls -1 target/criterion/ | grep -E "(spacl|sequential|scalability)"

