#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
OUTPUT_DIR="${CAMPAIGN_OUTPUT_DIR:-$ROOT_DIR/benchmarks/campaign/results/latest}"

args=(
  "$ROOT_DIR/benchmarks/campaign/campaign_report.py"
  --output-dir "$OUTPUT_DIR"
  --w3c-dir "${W3C_RUN_DIR:-}"
  --ore-dir "${ORE_RUN_DIR:-}"
  --owl2bench-dir "${OWL2BENCH_RUN_DIR:-}"
  --query-dir "${OWL2BENCH_QUERY_RUN_DIR:-}"
)

python3 "${args[@]}"
