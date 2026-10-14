#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
[[ -z "${OWL2BENCH_PROFILE_ADAPTER_NAME:-}" || "$OWL2BENCH_PROFILE_ADAPTER_NAME" == "ql-ruleset" ]] \
  || { echo "ql-ruleset adapter cannot be reprofiled" >&2; exit 1; }
[[ -z "${OWL2BENCH_PROFILE:-}" || "$OWL2BENCH_PROFILE" == "QL" ]] \
  || { echo "ql-ruleset adapter refuses a non-QL profile" >&2; exit 1; }
[[ -z "${OWL2BENCH_PROFILE_ENGINE_NAME:-}" || "$OWL2BENCH_PROFILE_ENGINE_NAME" == "QL-ruleset-engine" ]] \
  || { echo "ql-ruleset adapter refuses a non-ruleset engine" >&2; exit 1; }
export OWL2BENCH_PROFILE_ADAPTER_NAME="ql-ruleset"
export OWL2BENCH_PROFILE="QL"
export OWL2BENCH_PROFILE_ENGINE_NAME="QL-ruleset-engine"
export OWL2BENCH_PROFILE_BIN="${OWL2BENCH_PROFILE_BIN:-${QL_RULESET_BIN:-}}"
export OWL2BENCH_PROFILE_CONFIG="${OWL2BENCH_PROFILE_CONFIG:-${QL_RULESET_CONFIG:-}}"
export OWL2BENCH_PROFILE_QUERY_BIN="${OWL2BENCH_PROFILE_QUERY_BIN:-${QL_RULESET_QUERY_BIN:-}}"
export OWL2BENCH_PROFILE_STATE_DIR="${OWL2BENCH_PROFILE_STATE_DIR:-.ql-ruleset}"
export OWL2BENCH_PROFILE_CORE_MODE=1

exec "$SCRIPT_DIR/hermit_dl_engine.sh" "$@"
