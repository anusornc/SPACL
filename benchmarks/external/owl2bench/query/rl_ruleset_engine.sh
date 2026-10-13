#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
[[ -z "${OWL2BENCH_PROFILE_ADAPTER_NAME:-}" || "$OWL2BENCH_PROFILE_ADAPTER_NAME" == "rl-ruleset" ]] \
  || { echo "rl-ruleset adapter cannot be reprofiled" >&2; exit 1; }
[[ -z "${OWL2BENCH_PROFILE:-}" || "$OWL2BENCH_PROFILE" == "RL" ]] \
  || { echo "rl-ruleset adapter refuses a non-RL profile" >&2; exit 1; }
[[ -z "${OWL2BENCH_PROFILE_ENGINE_NAME:-}" || "$OWL2BENCH_PROFILE_ENGINE_NAME" == "RL-ruleset-engine" ]] \
  || { echo "rl-ruleset adapter refuses a non-ruleset engine" >&2; exit 1; }
export OWL2BENCH_PROFILE_ADAPTER_NAME="rl-ruleset"
export OWL2BENCH_PROFILE="RL"
export OWL2BENCH_PROFILE_ENGINE_NAME="RL-ruleset-engine"
export OWL2BENCH_PROFILE_BIN="${OWL2BENCH_PROFILE_BIN:-${RL_RULESET_BIN:-}}"
export OWL2BENCH_PROFILE_CONFIG="${OWL2BENCH_PROFILE_CONFIG:-${RL_RULESET_CONFIG:-}}"
export OWL2BENCH_PROFILE_QUERY_BIN="${OWL2BENCH_PROFILE_QUERY_BIN:-${RL_RULESET_QUERY_BIN:-}}"
export OWL2BENCH_PROFILE_STATE_DIR="${OWL2BENCH_PROFILE_STATE_DIR:-.rl-ruleset}"
export OWL2BENCH_PROFILE_CORE_MODE=1

exec "$SCRIPT_DIR/hermit_dl_engine.sh" "$@"
