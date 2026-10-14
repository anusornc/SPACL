#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
[[ -z "${OWL2BENCH_PROFILE_ADAPTER_NAME:-}" || "$OWL2BENCH_PROFILE_ADAPTER_NAME" == "elk-el" ]] \
  || { echo "elk-el adapter cannot be reprofiled" >&2; exit 1; }
[[ -z "${OWL2BENCH_PROFILE:-}" || "$OWL2BENCH_PROFILE" == "EL" ]] \
  || { echo "elk-el adapter refuses a non-EL profile" >&2; exit 1; }
[[ -z "${OWL2BENCH_PROFILE_ENGINE_NAME:-}" || "$OWL2BENCH_PROFILE_ENGINE_NAME" == "ELK" ]] \
  || { echo "elk-el adapter refuses a non-ELK engine" >&2; exit 1; }
export OWL2BENCH_PROFILE_ADAPTER_NAME="elk-el"
export OWL2BENCH_PROFILE="EL"
export OWL2BENCH_PROFILE_ENGINE_NAME="ELK"
export OWL2BENCH_PROFILE_BIN="${OWL2BENCH_PROFILE_BIN:-${ELK_EL_BIN:-}}"
export OWL2BENCH_PROFILE_CONFIG="${OWL2BENCH_PROFILE_CONFIG:-${ELK_EL_CONFIG:-}}"
export OWL2BENCH_PROFILE_QUERY_BIN="${OWL2BENCH_PROFILE_QUERY_BIN:-${ELK_EL_QUERY_BIN:-}}"
export OWL2BENCH_PROFILE_STATE_DIR="${OWL2BENCH_PROFILE_STATE_DIR:-.elk-el}"
export OWL2BENCH_PROFILE_CORE_MODE=1

exec "$SCRIPT_DIR/hermit_dl_engine.sh" "$@"
