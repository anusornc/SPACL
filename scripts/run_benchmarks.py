#!/usr/bin/env python3
"""
Script to run benchmarks and generate reports
"""
import subprocess
import json
import sys
from pathlib import Path
from datetime import datetime

def run_cargo_bench(bench_name, timeout=300):
    """Run a cargo benchmark with timeout"""
    print(f"\n{'='*60}")
    print(f"Running benchmark: {bench_name}")
    print(f"{'='*60}")
    
    try:
        result = subprocess.run(
            ["cargo", "bench", "--bench", bench_name],
            capture_output=True,
            text=True,
            timeout=timeout
        )
        print(result.stdout)
        if result.stderr:
            print("STDERR:", result.stderr)
        return result.returncode == 0
    except subprocess.TimeoutExpired:
        print(f"⚠️  Benchmark timed out after {timeout}s")
        return False
    except Exception as e:
        print(f"❌ Error running benchmark: {e}")
        return False

def parse_criterion_results():
    """Parse criterion benchmark results from target/criterion"""
    criterion_dir = Path("target/criterion")
    results = {}
    
    if not criterion_dir.exists():
        return results
    
    for bench_dir in criterion_dir.iterdir():
        if bench_dir.is_dir():
            bench_name = bench_dir.name
            results[bench_name] = {}
            
            # Look for estimates.json in new/ directories
            for estimates_file in bench_dir.rglob("new/estimates.json"):
                try:
                    with open(estimates_file) as f:
                        data = json.load(f)
                    
                    # Get median time in nanoseconds
                    median_ns = data.get("median", {}).get("point_estimate", 0)
                    median_ms = median_ns / 1_000_000.0  # Convert to ms
                    
                    # Extract test name from path
                    parts = estimates_file.parts
                    if "new" in parts:
                        new_idx = parts.index("new")
                        test_name = "/".join(parts[len(criterion_dir.parts):new_idx])
                        results[bench_name][test_name] = {
                            "median_ms": median_ms,
                            "median_ns": median_ns
                        }
                except Exception as e:
                    print(f"Warning: Could not parse {estimates_file}: {e}")
    
    return results

def generate_report(results):
    """Generate a markdown report"""
    report = []
    report.append("# Benchmark Report")
    report.append(f"\n**Generated**: {datetime.now().isoformat()}\n")
    
    for bench_name, tests in results.items():
        report.append(f"\n## {bench_name}\n")
        report.append("| Test | Median Time |")
        report.append("|------|-------------|")
        
        for test_name, data in sorted(tests.items()):
            median_ms = data["median_ms"]
            if median_ms < 1:
                time_str = f"{data['median_ns']/1000:.2f} µs"
            else:
                time_str = f"{median_ms:.2f} ms"
            report.append(f"| {test_name} | {time_str} |")
    
    return "\n".join(report)

def main():
    print("🚀 Tableauxx Benchmark Runner")
    print("=" * 60)
    
    # Check if cargo is available
    try:
        subprocess.run(["cargo", "--version"], check=True, capture_output=True)
    except Exception:
        print("❌ Cargo not found. Please install Rust.")
        sys.exit(1)
    
    # Create results directory
    Path("results").mkdir(exist_ok=True)
    
    # Run quick benchmark first
    print("\n📊 Running quick benchmark (fast)...")
    success = run_cargo_bench("quick_benchmark", timeout=120)
    
    if not success:
        print("⚠️  Quick benchmark had issues, continuing...")
    
    # Try full benchmark with longer timeout
    print("\n📊 Running full SPACL benchmark (slow)...")
    print("Note: This may take several minutes. Press Ctrl+C to skip.")
    try:
        run_cargo_bench("spacl_vs_sequential", timeout=600)
    except KeyboardInterrupt:
        print("\n⚠️  Skipped full benchmark")
    
    # Parse and report results
    print("\n📈 Parsing results...")
    results = parse_criterion_results()
    
    if results:
        report = generate_report(results)
        print("\n" + report)
        
        # Save report
        report_path = Path("results/benchmark_report.md")
        with open(report_path, "w") as f:
            f.write(report)
        print(f"\n✅ Report saved to {report_path}")
        
        # Also save JSON
        json_path = Path("results/benchmark_data.json")
        with open(json_path, "w") as f:
            json.dump(results, f, indent=2)
        print(f"✅ Data saved to {json_path}")
    else:
        print("⚠️  No benchmark results found")
    
    print("\n✨ Done!")

if __name__ == "__main__":
    main()
