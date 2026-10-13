#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../../.." && pwd)"
RESULTS_ROOT="${OWL2BENCH_QUERY_RESULTS_ROOT:-$ROOT_DIR/benchmarks/external/owl2bench/query/results/history}"
RUN_ID="${1:-${RUN_ID:-}}"

if [[ -z "$RUN_ID" ]]; then
  latest_dir=""
  latest_metadata=""
  for candidate in "$RESULTS_ROOT"/*; do
    [[ -d "$candidate" ]] || continue
    metadata_file="$candidate/run_metadata.json"
    [[ -f "$metadata_file" ]] || continue
    if [[ -z "$latest_dir" || "$metadata_file" -nt "$latest_metadata" ]]; then
      latest_dir="$candidate"
      latest_metadata="$metadata_file"
    fi
  done
  if [[ -z "$latest_dir" ]]; then
    echo "[owl2bench-query-report][error] no query runs found in: $RESULTS_ROOT" >&2
    exit 1
  fi
  RUN_ID="$(basename "$latest_dir")"
fi
RUN_DIR="$RESULTS_ROOT/$RUN_ID"

if [[ ! -d "$RUN_DIR" ]]; then
  echo "[owl2bench-query-report][error] run directory not found: $RUN_DIR" >&2
  exit 1
fi

if [[ -n "${OWL2BENCH_QUERY_VARIANT_CONTROL_FILE:-}" ]]; then
  export OWL2BENCH_QUERY_VARIANT_CONTROL_FILE
  python3 - "$RUN_DIR/run_metadata.json" <<'PY'
import hashlib
import json
import os
import pathlib
import sys

metadata_path = pathlib.Path(sys.argv[1])
validation_path = pathlib.Path(os.environ["OWL2BENCH_QUERY_VARIANT_CONTROL_FILE"]).resolve()
if not validation_path.is_file():
    raise SystemExit(f"variant control validation file not found: {validation_path}")
metadata = json.loads(metadata_path.read_text(encoding="utf-8"))
metadata["variant_control"] = {
    "validation_file": str(validation_path),
    "sha256": "sha256:" + hashlib.sha256(validation_path.read_bytes()).hexdigest(),
}
tmp_path = metadata_path.with_name(".run_metadata.variant_control.tmp")
tmp_path.write_text(json.dumps(metadata, indent=2, sort_keys=True) + "\n", encoding="utf-8")
tmp_path.replace(metadata_path)
PY
fi

python3 "$ROOT_DIR/benchmarks/external/owl2bench/query/report_profiles.py" "$RUN_DIR"
