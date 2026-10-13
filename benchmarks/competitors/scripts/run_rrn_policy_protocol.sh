#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BENCH_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
RESULTS_HISTORY="$BENCH_ROOT/results/history"

RUN_ID="${RUN_ID:-rrn_policy_locked_$(date +%Y%m%d_%H%M%S)}"
REPEATS="${REPEATS:-3}"
TIMEOUT_SECONDS="${TIMEOUT_SECONDS:-900}"
SKIP_BUILD="${SKIP_BUILD:-0}"
TASKSET_CPUS="${TASKSET_CPUS:-}"
RRN_MODEL_PATH="${RRN_MODEL_PATH:-}"

# Locked comparison matrix for policy-level ablation.
MODE_MATRIX="${MODE_MATRIX:-adaptive|1|baseline,adaptive|1|heuristic,adaptive|1|hybrid_rrn}"

# Focus on branch-heavy synthetic + one standard control by default.
WORKLOADS="${WORKLOADS:-benchmarks/ontologies/disjunctive/disjunctive_10k.owl,benchmarks/ontologies/disjunctive/disjunctive_30k.owl,tests/data/univ-bench.owl}"

mkdir -p "$RESULTS_HISTORY/$RUN_ID"
if [[ -z "${SPACL_BRANCH_SNAPSHOT_FILE:-}" ]]; then
  export SPACL_BRANCH_SNAPSHOT_FILE="$RESULTS_HISTORY/$RUN_ID/branch_snapshots.jsonl"
fi

if [[ -n "$RRN_MODEL_PATH" ]]; then
  export SPACL_RRN_MODEL_PATH="$RRN_MODEL_PATH"
else
  # hybrid_rrn remains valid; it will fallback to deterministic heuristic ranking.
  export SPACL_RRN_MODEL_PATH=""
fi

export OWL2_REASONER_LARGE_PARSE="${OWL2_REASONER_LARGE_PARSE:-1}"

cat <<INFO
[rrn-policy-protocol] RUN_ID=$RUN_ID
[rrn-policy-protocol] REPEATS=$REPEATS TIMEOUT_SECONDS=$TIMEOUT_SECONDS SKIP_BUILD=$SKIP_BUILD
[rrn-policy-protocol] MODE_MATRIX=$MODE_MATRIX
[rrn-policy-protocol] WORKLOADS=$WORKLOADS
[rrn-policy-protocol] SPACL_BRANCH_SNAPSHOT_FILE=$SPACL_BRANCH_SNAPSHOT_FILE
[rrn-policy-protocol] SPACL_RRN_MODEL_PATH=${SPACL_RRN_MODEL_PATH:-<none>}
INFO

RUN_ID="$RUN_ID" \
REPEATS="$REPEATS" \
TIMEOUT_SECONDS="$TIMEOUT_SECONDS" \
SKIP_BUILD="$SKIP_BUILD" \
TASKSET_CPUS="$TASKSET_CPUS" \
MODE_MATRIX="$MODE_MATRIX" \
WORKLOADS="$WORKLOADS" \
"$SCRIPT_DIR/run_spacl_ablation.sh"

echo "[rrn-policy-protocol] done: $RESULTS_HISTORY/$RUN_ID/results.csv"
