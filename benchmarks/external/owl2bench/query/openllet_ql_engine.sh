#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
[[ -z "${OWL2BENCH_PROFILE_ADAPTER_NAME:-}" \
    || "$OWL2BENCH_PROFILE_ADAPTER_NAME" == "openllet-ql" ]] \
  || { echo "openllet-ql adapter cannot be reprofiled" >&2; exit 1; }
[[ -z "${OWL2BENCH_PROFILE:-}" || "$OWL2BENCH_PROFILE" == "QL" ]] \
  || { echo "openllet-ql adapter refuses a non-QL profile" >&2; exit 1; }
[[ -z "${OWL2BENCH_PROFILE_ENGINE_NAME:-}" \
    || "$OWL2BENCH_PROFILE_ENGINE_NAME" == "Openllet" ]] \
  || { echo "openllet-ql adapter refuses a non-Openllet engine" >&2; exit 1; }

export OWL2BENCH_PROFILE_ADAPTER_NAME="openllet-ql"
export OWL2BENCH_PROFILE="QL"
export OWL2BENCH_PROFILE_ENGINE_NAME="Openllet"
export OWL2BENCH_PROFILE_BIN="${OWL2BENCH_PROFILE_BIN:-$SCRIPT_DIR/openllet_ql_materializer.sh}"
export OWL2BENCH_PROFILE_CONFIG="${OWL2BENCH_PROFILE_CONFIG:-$SCRIPT_DIR/openllet_ql_runtime_config.json}"
export OWL2BENCH_PROFILE_QUERY_BIN="${OWL2BENCH_PROFILE_QUERY_BIN:-${OPENLLET_QL_QUERY_BIN:-$SCRIPT_DIR/openllet_ql_query_engine.sh}}"
export OWL2BENCH_PROFILE_STATE_DIR="${OWL2BENCH_PROFILE_STATE_DIR:-${OPENLLET_QL_STATE_DIR:-.openllet-ql}}"
export OWL2BENCH_PROFILE_CORE_MODE=1

exec "$SCRIPT_DIR/hermit_dl_engine.sh" "$@"
