#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
SCRIPT_DIR="$ROOT_DIR/benchmarks/external/ore"
source "$SCRIPT_DIR/lib.sh"

RUN_BENCH="$ROOT_DIR/benchmarks/competitors/scripts/run_benchmarks.sh"
RESULTS_HISTORY="${ORE2015_RESULTS_HISTORY:-$ROOT_DIR/benchmarks/competitors/results/history}"
STAGED_DIR="${ORE2015_STAGED_DIR:-$ROOT_DIR/benchmarks/ontologies/external/ore/staged}"
MANIFEST_FILE="$STAGED_DIR/manifest.tsv"
CORPUS_METADATA_FILE="$STAGED_DIR/corpus_metadata.json"

GROUP="${1:-all}" # pure_dl|el|all
ACTION="${2:-all}" # prepare|build|run|report|all
ore_validate_group "$GROUP"

if [[ ! -x "$RUN_BENCH" ]]; then
  ore_err "Missing runner: $RUN_BENCH"
  exit 1
fi

if [[ "$ACTION" != "report" ]]; then
  if [[ ! -f "$MANIFEST_FILE" ]]; then
    ore_err "Missing staged manifest: $MANIFEST_FILE"
    ore_err "Run benchmarks/external/ore/prepare.sh first."
    exit 1
  fi
  count="$(find "$STAGED_DIR" -maxdepth 1 -type f -name '*.owl' | wc -l | tr -d '[:space:]')"
  if [[ "$count" == "0" ]]; then
    ore_err "No staged ORE ontologies in $STAGED_DIR"
    exit 1
  fi
fi

export ONTOLOGIES_DIR_OVERRIDE="${ONTOLOGIES_DIR_OVERRIDE:-$STAGED_DIR}"
export ONTOLOGY_SUITE="${ONTOLOGY_SUITE:-standard}"
export INCLUDE_CHEBI="${INCLUDE_CHEBI:-0}"
export OPERATION="${OPERATION:-consistency}"
export TIMEOUT_SECONDS="${TIMEOUT_SECONDS:-300}"
export SKIP_BUILD="${SKIP_BUILD:-1}"
export RUN_ID="${RUN_ID:-ore2015_${GROUP}_consistency_$(date +%Y%m%d_%H%M%S)}"
export ONTOLOGY_REGEX="${ONTOLOGY_REGEX:-$(ore_group_regex_for "$GROUP")}"
export ORACLE_STATUS="${ORACLE_STATUS:-unavailable}"
export ORACLE_SOURCE="${ORACLE_SOURCE:-$MANIFEST_FILE}"
export CORRECTNESS_STATUS="${CORRECTNESS_STATUS:-oracle_unavailable}"

if [[ -z "${REASONERS_OVERRIDE:-}" ]]; then
  export REASONERS_OVERRIDE="tableauxx,hermit,konclude,openllet,elk,jfact,pellet"
fi

RUN_DIR="$RESULTS_HISTORY/$RUN_ID"
write_ore_run_metadata() {
  mkdir -p "$RUN_DIR"
  local corpus_json='{}'
  if [[ -f "$CORPUS_METADATA_FILE" ]]; then
    corpus_json="$(jq -c . "$CORPUS_METADATA_FILE")"
  fi
  jq -n \
    --arg run_id "$RUN_ID" \
    --arg group "$GROUP" \
    --arg operation "$OPERATION" \
    --arg ontology_regex "$ONTOLOGY_REGEX" \
    --arg staged_dir "$STAGED_DIR" \
    --arg manifest_file "$MANIFEST_FILE" \
    --arg oracle_status "$ORACLE_STATUS" \
    --arg oracle_source "$ORACLE_SOURCE" \
    --arg correctness_status "$CORRECTNESS_STATUS" \
    --argjson corpus_metadata "$corpus_json" \
    '{
      run_id: $run_id,
      lane: "ore2015_consistency",
      requested_group: $group,
      operation: $operation,
      ontology_regex: $ontology_regex,
      staged_dir: $staged_dir,
      manifest_file: $manifest_file,
      corpus_metadata: $corpus_metadata,
      group_mapping: {
        pure_dl: {local_group: "pure_dl", official_group: "OWL DL", profile: "proper OWL DL", track: "OWL DL Consistency"},
        el: {local_group: "el", official_group: "OWL 2 EL", profile: "OWL 2 EL", track: "OWL EL Consistency"}
      },
      oracle_policy: {
        harness_oracle_status: $oracle_status,
        harness_oracle_source: $oracle_source,
        harness_correctness_status: $correctness_status,
        consensus_inputs_are_provenance: true,
        external_gold_labels_present: false,
        missing_or_ambiguous_oracle_result: "oracle_unavailable"
      }
    }' > "$RUN_DIR/ore_metadata.json"
}

write_ore_run_metadata

ore_log "action=$ACTION run_id=$RUN_ID group=$GROUP"
ore_log "ONTOLOGIES_DIR_OVERRIDE=$ONTOLOGIES_DIR_OVERRIDE"
ore_log "reasoners=$REASONERS_OVERRIDE timeout=${TIMEOUT_SECONDS}s regex=$ONTOLOGY_REGEX"
ore_log "oracle_status=$ORACLE_STATUS oracle_source=$ORACLE_SOURCE"

if ore_is_truthy "${ORE2015_DRY_RUN:-0}"; then
  ore_log "Dry run complete"
  exit 0
fi

"$RUN_BENCH" "$ACTION"
