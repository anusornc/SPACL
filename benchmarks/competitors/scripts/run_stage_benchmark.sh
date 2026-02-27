#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
RUN_BENCH="$ROOT_DIR/benchmarks/competitors/scripts/run_benchmarks.sh"

if [[ $# -lt 1 ]]; then
  cat <<'EOF'
Usage: run_stage_benchmark.sh <ontology-file-name>

Examples:
  run_stage_benchmark.sh go-basic.owl
  REPEAT_WARM=5 TIMEOUT_SECONDS=1800 run_stage_benchmark.sh chebi.owl

Environment:
  ONTOLOGY_SUITE       default: large
  INCLUDE_CHEBI        default: 1 if ontology is chebi.owl, otherwise 0
  REPEAT_WARM          default: 3
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
REPEAT_WARM="${REPEAT_WARM:-3}"
TIMEOUT_SECONDS="${TIMEOUT_SECONDS:-900}"
SKIP_BUILD="${SKIP_BUILD:-1}"
GENERATE_OWLBIN="${GENERATE_OWLBIN:-1}"
if [[ "$ONTOLOGY_FILE" == "chebi.owl" ]]; then
  INCLUDE_CHEBI="${INCLUDE_CHEBI:-1}"
else
  INCLUDE_CHEBI="${INCLUDE_CHEBI:-0}"
fi

if ! [[ "$REPEAT_WARM" =~ ^[0-9]+$ ]] || [[ "$REPEAT_WARM" -lt 1 ]]; then
  echo "[error] REPEAT_WARM must be a positive integer" >&2
  exit 2
fi

escaped="$(printf '%s' "$ONTOLOGY_FILE" | sed -E 's/[][(){}.^$*+?|\\-]/\\&/g')"
ONTOLOGY_REGEX="^${escaped}$"
DATE_TAG="$(date +%Y%m%d_%H%M%S)"
RUNSET_ID="stages_${ONTOLOGY_BASE//[^a-zA-Z0-9_]/_}_${DATE_TAG}"

RESULT_ROOT="$ROOT_DIR/benchmarks/competitors/results/history/$RUNSET_ID"
mkdir -p "$RESULT_ROOT"

find_source_ontology() {
  local candidates=(
    "$ROOT_DIR/benchmarks/competitors/ontologies/$ONTOLOGY_FILE"
    "$ROOT_DIR/benchmarks/ontologies/other/$ONTOLOGY_FILE"
    "$ROOT_DIR/tests/data/$ONTOLOGY_FILE"
  )
  local c
  for c in "${candidates[@]}"; do
    if [[ -f "$c" ]]; then
      echo "$c"
      return 0
    fi
  done
  return 1
}

ensure_owlbin() {
  local source_file="$1"
  local bin_file="${source_file%.owl}.owlbin"
  if [[ -f "$bin_file" ]]; then
    echo "[info] Using existing owlbin: $bin_file"
    return 0
  fi

  if [[ "$GENERATE_OWLBIN" != "1" ]]; then
    echo "[warn] GENERATE_OWLBIN=0 and missing owlbin: $bin_file"
    return 1
  fi

  echo "[info] Generating owlbin on host: $bin_file"
  (
    cd "$ROOT_DIR"
    cargo run --release --bin owl2-reasoner -- convert "$source_file" "$bin_file"
  )
}

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

median_of_list() {
  local values=("$@")
  printf '%s\n' "${values[@]}" | sort -n | awk '{
    arr[NR]=$1
  } END {
    if (NR == 0) { exit 1 }
    if (NR % 2 == 1) {
      print arr[(NR+1)/2]
    } else {
      print int((arr[NR/2] + arr[NR/2+1]) / 2)
    }
  }'
}

echo "[info] Stage benchmark set: $RUNSET_ID"
echo "[info] Ontology: $ONTOLOGY_FILE"
echo "[info] Suite: $ONTOLOGY_SUITE"
echo "[info] Warm repeats: $REPEAT_WARM"

warm_bin_only=0
if source_path="$(find_source_ontology)"; then
  if ensure_owlbin "$source_path"; then
    warm_bin_only=1
  fi
else
  echo "[warn] Source ontology file not found on host; warm mode will not enforce BIN_ONLY"
fi

# Step 1: Warm-up parse with auto cache to generate/update .owlbin.
WARMUP_RUN="${RUNSET_ID}_warmup"
run_once "$WARMUP_RUN" \
  OWL2_REASONER_FORCE_TEXT=1 \
  OWL2_REASONER_BIN_ONLY=0 \
  OWL2_REASONER_AUTO_CACHE=1 \
  OWL2_REASONER_DISABLE_PARSE_FALLBACK=1 \
  OWL2_REASONER_STAGE_TIMING=1

# Step 2: E2E cold (text parse, no auto cache)
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

# Step 3: Reason-only warm approximation (bin-only repeated runs)
warm_reason_values=()
warm_wall_values=()
warm_parse_values=()
for i in $(seq 1 "$REPEAT_WARM"); do
  run_id="${RUNSET_ID}_warm_${i}"
  if [[ "$warm_bin_only" -eq 1 ]]; then
    run_once "$run_id" \
      OWL2_REASONER_FORCE_TEXT=0 \
      OWL2_REASONER_BIN_ONLY=1 \
      OWL2_REASONER_AUTO_CACHE=0 \
      OWL2_REASONER_DISABLE_PARSE_FALLBACK=1 \
      OWL2_REASONER_STAGE_TIMING=1
  else
    run_once "$run_id" \
      OWL2_REASONER_FORCE_TEXT=0 \
      OWL2_REASONER_BIN_ONLY=0 \
      OWL2_REASONER_AUTO_CACHE=0 \
      OWL2_REASONER_DISABLE_PARSE_FALLBACK=1 \
      OWL2_REASONER_STAGE_TIMING=1
  fi

  status="$(extract_field "$run_id" status)"
  if [[ "$status" != "success" ]]; then
    echo "[error] warm run $i failed (status=$status)" >&2
    exit 1
  fi

  warm_reason_values+=("$(extract_field "$run_id" reason_time_ms)")
  warm_wall_values+=("$(extract_field "$run_id" wall_time_ms)")
  warm_parse_values+=("$(extract_field "$run_id" parse_time_ms)")
done

warm_reason_median="$(median_of_list "${warm_reason_values[@]}")"
warm_wall_median="$(median_of_list "${warm_wall_values[@]}")"
warm_parse_median="$(median_of_list "${warm_parse_values[@]}")"

summary_csv="$RESULT_ROOT/stage_summary.csv"
summary_md="$RESULT_ROOT/stage_summary.md"

cat > "$summary_csv" <<EOF
run_set_id,ontology,mode,metric_ms,notes
$RUNSET_ID,$ONTOLOGY_FILE,e2e_cold_wall,$e2e_wall,full end-to-end wall clock
$RUNSET_ID,$ONTOLOGY_FILE,parse_only,$e2e_parse,extracted parse stage from e2e cold run
$RUNSET_ID,$ONTOLOGY_FILE,reason_only_warm_median,$warm_reason_median,median reason stage with bin-only warm path
$RUNSET_ID,$ONTOLOGY_FILE,warm_wall_median,$warm_wall_median,median wall in warm bin-only runs
$RUNSET_ID,$ONTOLOGY_FILE,warm_parse_median,$warm_parse_median,median parse stage in warm bin-only runs
EOF

cat > "$summary_md" <<EOF
# Stage Benchmark Summary

- Run set: \`$RUNSET_ID\`
- Ontology: \`$ONTOLOGY_FILE\`
- Suite: \`$ONTOLOGY_SUITE\`
- Warm repeats: \`$REPEAT_WARM\`

## Results (ms)

| Mode | Value (ms) | Notes |
|---|---:|---|
| E2E cold wall | $e2e_wall | Full wall time (cold text path) |
| Parse-only (stage) | $e2e_parse | Parse stage extracted from E2E cold |
| Reason-only warm (median stage) | $warm_reason_median | Reason stage median from bin-only warm runs |
| Warm wall median | $warm_wall_median | Wall median in warm bin-only runs |
| Warm parse median | $warm_parse_median | Parse stage median in warm bin-only runs |

## Raw Run IDs

- Warm-up: \`$WARMUP_RUN\`
- E2E cold: \`$E2E_RUN\`
- Warm mode bin-only: \`$warm_bin_only\`
$(for i in $(seq 1 "$REPEAT_WARM"); do echo "- Warm run $i: \`${RUNSET_ID}_warm_${i}\`"; done)
EOF

echo "[ok] Stage summary: $summary_csv"
echo "[ok] Stage summary: $summary_md"
