#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
RUN_BENCH="$ROOT_DIR/benchmarks/competitors/scripts/run_benchmarks.sh"

if [[ $# -lt 1 ]]; then
  cat <<'EOF'
Usage: run_stage_benchmark.sh <ontology-file-name>

Examples:
  run_stage_benchmark.sh go-basic.owl
  TIMEOUT_SECONDS=1800 run_stage_benchmark.sh chebi.owl

Environment:
  ONTOLOGY_SUITE       default: large
  INCLUDE_CHEBI        default: 1 if ontology is chebi.owl, otherwise 0
  TIMEOUT_SECONDS      default: 900
  SKIP_BUILD           default: 1
EOF
  exit 2
fi

if [[ ! -x "$RUN_BENCH" ]]; then
  echo "[error] missing benchmark runner: $RUN_BENCH" >&2
  exit 2
fi

ONTOLOGY_FILE="$1"
ONTOLOGY_BASE="${ONTOLOGY_FILE%.owl}"
if [[ "$ONTOLOGY_BASE" == "$ONTOLOGY_FILE" ]]; then
  echo "[error] ontology file must end with .owl (got: $ONTOLOGY_FILE)" >&2
  exit 2
fi

ONTOLOGY_SUITE="${ONTOLOGY_SUITE:-large}"
TIMEOUT_SECONDS="${TIMEOUT_SECONDS:-900}"
SKIP_BUILD="${SKIP_BUILD:-1}"
if [[ "$ONTOLOGY_FILE" == "chebi.owl" ]]; then
  INCLUDE_CHEBI="${INCLUDE_CHEBI:-1}"
else
  INCLUDE_CHEBI="${INCLUDE_CHEBI:-0}"
fi

escaped="$(printf '%s' "$ONTOLOGY_FILE" | sed -E 's/[][(){}.^$*+?|\\-]/\\&/g')"
ONTOLOGY_REGEX="^${escaped}$"
DATE_TAG="$(date +%Y%m%d_%H%M%S)"
RUNSET_ID="stages_${ONTOLOGY_BASE//[^a-zA-Z0-9_]/_}_${DATE_TAG}"

RESULT_ROOT="$ROOT_DIR/benchmarks/competitors/results/history/$RUNSET_ID"
mkdir -p "$RESULT_ROOT"

run_once() {
  local run_id="$1"
  shift
  echo "[info] run_id=$run_id"
  (
    cd "$ROOT_DIR"
    env \
      RUN_ID="$run_id" \
      ONTOLOGY_SUITE="$ONTOLOGY_SUITE" \
      INCLUDE_CHEBI="$INCLUDE_CHEBI" \
      ONTOLOGY_REGEX="$ONTOLOGY_REGEX" \
      REASONERS_OVERRIDE=tableauxx \
      TIMEOUT_SECONDS="$TIMEOUT_SECONDS" \
      SKIP_BUILD="$SKIP_BUILD" \
      "$@" \
      "$RUN_BENCH" run
  )
}

extract_field() {
  local run_id="$1"
  local field="$2"
  local json_file="$ROOT_DIR/benchmarks/competitors/results/history/$run_id/tableauxx_${ONTOLOGY_BASE}.json"
  if [[ ! -f "$json_file" ]]; then
    echo "[error] missing result file: $json_file" >&2
    exit 2
  fi
  jq -r ".$field" "$json_file"
}

echo "[info] Stage benchmark set: $RUNSET_ID"
echo "[info] Ontology: $ONTOLOGY_FILE"
echo "[info] Suite: $ONTOLOGY_SUITE"
echo "[info] Contract: cold text path only"

# Step 1: E2E cold (text parse, no auto cache)
E2E_RUN="${RUNSET_ID}_e2e_cold"
run_once "$E2E_RUN" \
  OWL2_REASONER_FORCE_TEXT=1 \
  OWL2_REASONER_BIN_ONLY=0 \
  OWL2_REASONER_AUTO_CACHE=0 \
  OWL2_REASONER_DISABLE_PARSE_FALLBACK=1 \
  OWL2_REASONER_STAGE_TIMING=1

e2e_wall="$(extract_field "$E2E_RUN" wall_time_ms)"
e2e_parse="$(extract_field "$E2E_RUN" parse_time_ms)"
e2e_reason="$(extract_field "$E2E_RUN" reason_time_ms)"
e2e_status="$(extract_field "$E2E_RUN" status)"

if [[ "$e2e_status" != "success" ]]; then
  echo "[error] E2E run did not succeed (status=$e2e_status)" >&2
  exit 1
fi

reason_only_stage="$e2e_reason"

summary_csv="$RESULT_ROOT/stage_summary.csv"
summary_md="$RESULT_ROOT/stage_summary.md"

cat > "$summary_csv" <<EOF
run_set_id,ontology,mode,metric_ms,notes
$RUNSET_ID,$ONTOLOGY_FILE,e2e_cold_wall,$e2e_wall,full end-to-end wall clock
$RUNSET_ID,$ONTOLOGY_FILE,parse_only,$e2e_parse,extracted parse stage from e2e cold run
$RUNSET_ID,$ONTOLOGY_FILE,reason_only_stage,$reason_only_stage,reason stage extracted from the same e2e cold run
EOF

cat > "$summary_md" <<EOF
# Stage Benchmark Summary

- Run set: \`$RUNSET_ID\`
- Ontology: \`$ONTOLOGY_FILE\`
- Suite: \`$ONTOLOGY_SUITE\`
- Contract: \`cold_text_only\`

## Results (ms)

| Mode | Value (ms) | Notes |
|---|---:|---|
| E2E cold wall | $e2e_wall | Full wall time (cold text path) |
| Parse-only (stage) | $e2e_parse | Parse stage extracted from E2E cold |
| Reason-only stage | $reason_only_stage | Reason stage extracted from the same E2E cold run |

## Raw Run IDs

- E2E cold: \`$E2E_RUN\`
EOF

echo "[ok] Stage summary: $summary_csv"
echo "[ok] Stage summary: $summary_md"
