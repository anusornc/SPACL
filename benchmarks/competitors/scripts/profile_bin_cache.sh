#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
RUN_BENCH="$ROOT_DIR/benchmarks/competitors/scripts/run_benchmarks.sh"

if [[ $# -lt 1 ]]; then
  cat <<'EOF'
Usage: profile_bin_cache.sh <ontology-file-name>

Examples:
  profile_bin_cache.sh go-basic.owl
  REPEAT_WARM=5 TIMEOUT_SECONDS=1800 profile_bin_cache.sh chebi.owl

Environment:
  ONTOLOGY_SUITE       default: large
  INCLUDE_CHEBI        default: 1 if ontology is chebi.owl, otherwise 0
  REPEAT_WARM          default: 3
  TIMEOUT_SECONDS      default: 900
  SKIP_BUILD           default: 1
  GENERATE_OWLBIN      default: 1
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
RUNSET_ID="bin_cache_${ONTOLOGY_BASE//[^a-zA-Z0-9_]/_}_${DATE_TAG}"

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
    OWL2_REASONER_LARGE_PARSE=1 \
    OWL2_REASONER_FORCE_TEXT=1 \
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

echo "[info] Bin-cache probe set: $RUNSET_ID"
echo "[info] Ontology: $ONTOLOGY_FILE"
echo "[info] Suite: $ONTOLOGY_SUITE"
echo "[info] Repeats: $REPEAT_WARM"

if ! source_path="$(find_source_ontology)"; then
  echo "[error] could not find source ontology for $ONTOLOGY_FILE" >&2
  exit 2
fi

if ! ensure_owlbin "$source_path"; then
  echo "[error] missing owlbin for $ONTOLOGY_FILE" >&2
  exit 2
fi

bin_wall_values=()
bin_load_values=()
reason_values=()
for i in $(seq 1 "$REPEAT_WARM"); do
  run_id="${RUNSET_ID}_run_${i}"
  run_once "$run_id" \
    OWL2_REASONER_FORCE_TEXT=0 \
    OWL2_REASONER_BIN_ONLY=1 \
    OWL2_REASONER_AUTO_CACHE=0 \
    OWL2_REASONER_DISABLE_PARSE_FALLBACK=1 \
    OWL2_REASONER_STAGE_TIMING=1

  status="$(extract_field "$run_id" status)"
  if [[ "$status" != "success" ]]; then
    echo "[error] bin-cache run $i failed (status=$status)" >&2
    exit 1
  fi

  bin_wall_values+=("$(extract_field "$run_id" wall_time_ms)")
  bin_load_values+=("$(extract_field "$run_id" parse_time_ms)")
  reason_values+=("$(extract_field "$run_id" reason_time_ms)")
done

bin_wall_median="$(median_of_list "${bin_wall_values[@]}")"
bin_load_median="$(median_of_list "${bin_load_values[@]}")"
reason_median="$(median_of_list "${reason_values[@]}")"

summary_csv="$RESULT_ROOT/bin_cache_summary.csv"
summary_md="$RESULT_ROOT/bin_cache_summary.md"

cat > "$summary_csv" <<EOF
run_set_id,ontology,mode,metric_ms,notes
$RUNSET_ID,$ONTOLOGY_FILE,bin_cache_wall_median,$bin_wall_median,diagnostic wall median from bin-only runs
$RUNSET_ID,$ONTOLOGY_FILE,bin_cache_load_median,$bin_load_median,diagnostic load-stage median from bin-only runs
$RUNSET_ID,$ONTOLOGY_FILE,reason_stage_median,$reason_median,reason stage median reported by bin-only runs
EOF

cat > "$summary_md" <<EOF
# Bin Cache Diagnostic Summary

- Run set: \`$RUNSET_ID\`
- Ontology: \`$ONTOLOGY_FILE\`
- Suite: \`$ONTOLOGY_SUITE\`
- Repeats: \`$REPEAT_WARM\`

This diagnostic isolates the legacy \`.owlbin\` path. It is not part of the primary stage benchmark contract.

## Results (ms)

| Mode | Value (ms) | Notes |
|---|---:|---|
| Bin-cache wall median | $bin_wall_median | Wall median from bin-only runs |
| Bin-cache load median | $bin_load_median | Load-stage median from bin-only runs |
| Reason stage median | $reason_median | Reported reason stage median from bin-only runs |

## Raw Run IDs

$(for i in $(seq 1 "$REPEAT_WARM"); do echo "- Bin-cache run $i: \`${RUNSET_ID}_run_${i}\`"; done)
EOF

echo "[ok] Bin-cache summary: $summary_csv"
echo "[ok] Bin-cache summary: $summary_md"
