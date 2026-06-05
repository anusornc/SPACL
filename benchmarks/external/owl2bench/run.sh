#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
RUN_BENCH="$ROOT_DIR/benchmarks/competitors/scripts/run_benchmarks.sh"

STAGED_DIR="${OWL2BENCH_STAGED_DIR:-$ROOT_DIR/benchmarks/ontologies/external/owl2bench/staged}"
ACTION="${1:-all}" # prepare|build|run|report|all (forwarded)

if [[ ! -x "$RUN_BENCH" ]]; then
  echo "[owl2bench-run][error] missing runner: $RUN_BENCH" >&2
  exit 1
fi

if [[ "$ACTION" != "report" ]]; then
  count="$(find "$STAGED_DIR" -maxdepth 1 -type f -name '*.owl' | wc -l | tr -d '[:space:]')"
  if [[ "$count" == "0" ]]; then
    echo "[owl2bench-run][error] no staged ontologies in $STAGED_DIR" >&2
    echo "[owl2bench-run][error] run benchmarks/external/owl2bench/prepare.sh first" >&2
    exit 1
  fi
fi

export ONTOLOGIES_DIR_OVERRIDE="${ONTOLOGIES_DIR_OVERRIDE:-$STAGED_DIR}"
export ONTOLOGY_SUITE="${ONTOLOGY_SUITE:-standard}"
export INCLUDE_CHEBI="${INCLUDE_CHEBI:-0}"
# Paper-facing OWL2Bench core runs use a 300s timeout in the benchmark runbook.
export TIMEOUT_SECONDS="${TIMEOUT_SECONDS:-300}"
export SKIP_BUILD="${SKIP_BUILD:-1}"
export RUN_ID="${RUN_ID:-owl2bench_$(date +%Y%m%d_%H%M%S)}"
export ONTOLOGY_REGEX="${ONTOLOGY_REGEX:-^...._.*\\.owl$}"

if [[ -z "${REASONERS_OVERRIDE:-}" ]]; then
  export REASONERS_OVERRIDE="tableauxx,hermit,konclude,openllet,elk,jfact,pellet"
fi

echo "[owl2bench-run] action=$ACTION run_id=$RUN_ID"
echo "[owl2bench-run] ONTOLOGIES_DIR_OVERRIDE=$ONTOLOGIES_DIR_OVERRIDE"
echo "[owl2bench-run] reasoners=$REASONERS_OVERRIDE timeout=${TIMEOUT_SECONDS}s regex=$ONTOLOGY_REGEX"

"$RUN_BENCH" "$ACTION"
