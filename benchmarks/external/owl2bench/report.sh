#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
RUN_BENCH="$ROOT_DIR/benchmarks/competitors/scripts/run_benchmarks.sh"
RESULTS_HISTORY="$ROOT_DIR/benchmarks/competitors/results/history"

RUN_ID="${1:-${RUN_ID:-}}"

if [[ -z "$RUN_ID" ]]; then
  latest="$(ls -td "$RESULTS_HISTORY"/owl2bench_* 2>/dev/null | head -n 1 || true)"
  if [[ -z "$latest" ]]; then
    echo "[owl2bench-report][error] no owl2bench_* run directories found in $RESULTS_HISTORY" >&2
    exit 1
  fi
  RUN_ID="$(basename "$latest")"
fi

echo "[owl2bench-report] RUN_ID=$RUN_ID"
RUN_ID="$RUN_ID" "$RUN_BENCH" report

echo "[owl2bench-report] report: $RESULTS_HISTORY/$RUN_ID/benchmark_report.md"
echo "[owl2bench-report] csv   : $RESULTS_HISTORY/$RUN_ID/results.csv"
