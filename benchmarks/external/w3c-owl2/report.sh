#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
LANE_DIR="$ROOT_DIR/benchmarks/external/w3c-owl2"
RESULTS_ROOT="${W3C_OWL2_RESULTS_ROOT:-$LANE_DIR/results/history}"
RUN_ID="${1:-${RUN_ID:-}}"

if [[ -z "$RUN_ID" ]]; then
  latest="$(find "$RESULTS_ROOT" -mindepth 1 -maxdepth 1 -type d -name 'w3c_owl2_*' 2>/dev/null | sort | tail -n 1 || true)"
  if [[ -z "$latest" ]]; then
    echo "[w3c-owl2-report][error] no w3c_owl2_* run directories found in $RESULTS_ROOT" >&2
    exit 1
  fi
  RUN_ID="$(basename "$latest")"
fi

python3 "$LANE_DIR/w3c_owl2_harness.py" report --run-dir "$RESULTS_ROOT/$RUN_ID"
