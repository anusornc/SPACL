#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
LANE_DIR="$ROOT_DIR/benchmarks/external/w3c-owl2"
RESULT_CONTRACT="$ROOT_DIR/benchmarks/competitors/scripts/benchmark_result.sh"
source "$RESULT_CONTRACT"

DATA_ROOT="${W3C_OWL2_DATA_ROOT:-$ROOT_DIR/benchmarks/ontologies/external/w3c-owl2}"
MANIFEST_FILE="${W3C_OWL2_MANIFEST:-$DATA_ROOT/staged/manifest.tsv}"
RESULTS_ROOT="${W3C_OWL2_RESULTS_ROOT:-$LANE_DIR/results/history}"
RUN_ID="${RUN_ID:-w3c_owl2_$(date +%Y%m%d_%H%M%S)}"
RUN_DIR="$RESULTS_ROOT/$RUN_ID"
RAW_DIR="$RUN_DIR/raw"
TIMEOUT_SECONDS="${TIMEOUT_SECONDS:-60}"
REASONER_BIN="${W3C_OWL2_REASONER_BIN:-$ROOT_DIR/target/release/owl2-reasoner}"
SOURCE_DIR="${W3C_OWL2_SOURCE_DIR:-$DATA_ROOT/source}"
SOURCE_URL="${W3C_OWL2_SOURCE_URL:-}"
SOURCE_REVISION="${W3C_OWL2_SOURCE_REVISION:-}"
SOURCE_SHA256="${W3C_OWL2_SOURCE_SHA256:-}"

now_iso() {
  python3 -c 'from datetime import datetime, timezone; print(datetime.now(timezone.utc).isoformat())'
}

if [[ ! -f "$MANIFEST_FILE" ]]; then
  echo "[w3c-owl2-run][error] missing manifest: $MANIFEST_FILE" >&2
  echo "[w3c-owl2-run][error] run benchmarks/external/w3c-owl2/prepare.sh first" >&2
  exit 1
fi

if [[ ! -x "$REASONER_BIN" ]]; then
  debug_bin="$ROOT_DIR/target/debug/owl2-reasoner"
  if [[ -x "$debug_bin" ]]; then
    REASONER_BIN="$debug_bin"
  elif [[ "${SKIP_BUILD:-0}" == "1" ]]; then
    echo "[w3c-owl2-run][error] reasoner binary not found: $REASONER_BIN" >&2
    exit 1
  else
    cargo build --bin owl2-reasoner
    REASONER_BIN="$debug_bin"
  fi
fi

mkdir -p "$RAW_DIR"

jq -n \
  --arg run_id "$RUN_ID" \
  --arg manifest "$MANIFEST_FILE" \
  --arg source_dir "$SOURCE_DIR" \
  --arg source_url "$SOURCE_URL" \
  --arg source_revision "$SOURCE_REVISION" \
  --arg source_sha256 "$SOURCE_SHA256" \
  --arg reasoner_bin "$REASONER_BIN" \
  --argjson timeout_seconds "$TIMEOUT_SECONDS" \
  --arg timestamp "$(now_iso)" \
  '{
    run_id: $run_id,
    ontology_suite: "w3c-owl2-approved",
    manifest: $manifest,
    source_dir: $source_dir,
    source_url: $source_url,
    source_revision: $source_revision,
    source_sha256: $source_sha256,
    reasoner: "tableauxx",
    reasoner_bin: $reasoner_bin,
    timeout_seconds: $timeout_seconds,
    timestamp: $timestamp
  }' > "$RUN_DIR/run_metadata.json"

slugify() {
  printf '%s' "$1" | tr -c 'A-Za-z0-9._-' '_'
}

now_ms() {
  python3 -c 'import time; print(int(time.time() * 1000))'
}

write_augmented_result() {
  local result_file="$1"
  local test_type="$2"
  local lane_status="$3"
  local unsupported_reason="$4"
  local w3c_expected="$5"
  local source_manifest="$6"

  tmp="$(mktemp)"
  jq \
    --arg test_type "$test_type" \
    --arg lane_status "$lane_status" \
    --arg unsupported_reason "$unsupported_reason" \
    --arg w3c_expected "$w3c_expected" \
    --arg source_manifest "$source_manifest" \
    '. + {
      w3c_test_type: $test_type,
      w3c_lane_status: $lane_status,
      w3c_unsupported_reason: $unsupported_reason,
      w3c_expected_result: $w3c_expected,
      w3c_source_manifest: $source_manifest
    }' "$result_file" > "$tmp"
  mv "$tmp" "$result_file"
}

extract_evaluator_json() {
  local input_file="$1"
  local output_file="$2"
  python3 - "$input_file" "$output_file" <<'PY'
import json
import pathlib
import sys

input_path = pathlib.Path(sys.argv[1])
output_path = pathlib.Path(sys.argv[2])
text = input_path.read_text(encoding="utf-8", errors="replace")
decoder = json.JSONDecoder()

for index, character in enumerate(text):
    if character != "{":
        continue
    try:
        value, _ = decoder.raw_decode(text[index:])
    except json.JSONDecodeError:
        continue
    output_path.write_text(json.dumps(value) + "\n", encoding="utf-8")
    break
else:
    sys.exit(1)
PY
}

tail -n +2 "$MANIFEST_FILE" | while IFS=$'\t' read -r \
  case_id source_manifest operation expected_semantic_result w3c_expected_result \
  lane_status unsupported_reason oracle_status oracle_source oracle_case_id \
  premise_path conclusion_path test_type approval_status semantics normative_syntax species profiles
do
  [[ -n "${case_id:-}" ]] || continue
  [[ "$premise_path" == "-" ]] && premise_path=""
  [[ "$conclusion_path" == "-" ]] && conclusion_path=""
  [[ "$unsupported_reason" == "-" ]] && unsupported_reason=""
  result_file="$RUN_DIR/$(slugify "$case_id").json"
  output_file="$RAW_DIR/$(slugify "$case_id").output"
  eval_json_file="$RAW_DIR/$(slugify "$case_id").evaluate.json"

  full_lane=0
  if [[ "$lane_status" != "baseline" ]]; then
    if [[ "$species" == "FULL" && -n "$premise_path" ]]; then
      # Track B (ADR 0006): the FULL lane's formal debut — rows the
      # pre-lane taxonomy parked as unsupported execute through the
      # RDF-based flag; the evaluator's fallback protects shapes its
      # landed families do not cover.
      full_lane=1
    else
    jq -n \
      --arg lane_status "$lane_status" \
      --arg unsupported_reason "$unsupported_reason" \
      --arg approval_status "$approval_status" \
      --arg semantics "$semantics" \
      --arg normative_syntax "$normative_syntax" \
      --arg species "$species" \
      --arg profiles "$profiles" \
      '{
        lane_status: $lane_status,
        unsupported_reason: $unsupported_reason,
        approval_status: $approval_status,
        semantics: $semantics,
        normative_syntax: $normative_syntax,
        species: $species,
        profiles: $profiles
      }' > "$output_file"
    write_benchmark_result \
      "$result_file" "$RUN_ID" "w3c-owl2-approved" "$case_id" "tableauxx" "$premise_path" "$operation" \
      "unsupported" "unknown" "unsupported" "not_applicable" "$oracle_source" "$oracle_case_id" "" \
      -1 -1 -1 -1 "$output_file"
    write_augmented_result "$result_file" "$test_type" "$lane_status" "$unsupported_reason" "$w3c_expected_result" "$source_manifest"
    continue
    fi
  fi

  start_ms="$(now_ms)"
  # Track B (ADR 0006): FULL-species rows route through the RDF-based
  # lane's opt-in flag; the evaluator falls back to the Direct
  # evaluation for shapes its landed families do not cover.
  lane_args=()
  if [[ "$full_lane" -eq 1 ]]; then
    lane_args+=(--rdf-based)
  fi
  set +e
  if [[ "$operation" == "entailment" ]]; then
    timeout "$TIMEOUT_SECONDS" "$REASONER_BIN" evaluate "$operation" "${lane_args[@]}" "$premise_path" "$conclusion_path" > "$output_file" 2>&1
    cmd_status="$?"
  else
    timeout "$TIMEOUT_SECONDS" "$REASONER_BIN" evaluate "$operation" "${lane_args[@]}" "$premise_path" > "$output_file" 2>&1
    cmd_status="$?"
  fi
  set -e
  end_ms="$(now_ms)"
  wall_time_ms="$((end_ms - start_ms))"

  execution_detail="success"
  semantic_result="unknown"
  correctness_status="not_evaluated"
  reported_time_ms="-1"
  parse_time_ms="-1"
  reason_time_ms="-1"

  if [[ "$cmd_status" -eq 124 ]]; then
    execution_detail="timeout"
  elif [[ "$cmd_status" -ne 0 ]]; then
    execution_detail="failed"
  elif jq -e . "$output_file" >/dev/null 2>&1; then
    cp "$output_file" "$eval_json_file"
    execution_detail="$(jq -r '.execution_status // "success"' "$eval_json_file")"
    semantic_result="$(jq -r '.semantic_result // "unknown"' "$eval_json_file")"
    correctness_status="$(jq -r '.status // "not_evaluated"' "$eval_json_file")"
    reported_time_ms="$(jq -r '.duration_ms // .wall_time_ms // -1' "$eval_json_file")"
    parse_time_ms="$(jq -r '.parse_time_ms // -1' "$eval_json_file")"
    reason_time_ms="$(jq -r '.reason_time_ms // -1' "$eval_json_file")"
  elif extract_evaluator_json "$output_file" "$eval_json_file" \
    && jq -e . "$eval_json_file" >/dev/null 2>&1; then
    execution_detail="$(jq -r '.execution_status // "success"' "$eval_json_file")"
    semantic_result="$(jq -r '.semantic_result // "unknown"' "$eval_json_file")"
    correctness_status="$(jq -r '.status // "not_evaluated"' "$eval_json_file")"
    reported_time_ms="$(jq -r '.duration_ms // .wall_time_ms // -1' "$eval_json_file")"
    parse_time_ms="$(jq -r '.parse_time_ms // -1' "$eval_json_file")"
    reason_time_ms="$(jq -r '.reason_time_ms // -1' "$eval_json_file")"
  else
    execution_detail="failed"
  fi

  # Executing the FULL lane makes the official expected value the
  # working oracle (the pre-lane staging parked these rows as
  # not_applicable because no lane could consume them).
  lane_oracle_status="$oracle_status"
  if [[ "$full_lane" -eq 1 ]]; then
    lane_oracle_status="available"
  fi
  write_benchmark_result \
    "$result_file" "$RUN_ID" "w3c-owl2-approved" "$case_id" "tableauxx" "$premise_path" "$operation" \
    "$execution_detail" "$semantic_result" "$correctness_status" "$lane_oracle_status" "$oracle_source" "$oracle_case_id" "$expected_semantic_result" \
    "$wall_time_ms" "$reported_time_ms" "$parse_time_ms" "$reason_time_ms" "$output_file"
  write_augmented_result "$result_file" "$test_type" "$(if [[ "$full_lane" -eq 1 ]]; then echo full; else echo "$lane_status"; fi)" "$unsupported_reason" "$w3c_expected_result" "$source_manifest"
done

validate_benchmark_result_count "$RUN_DIR" "$(tail -n +2 "$MANIFEST_FILE" | wc -l | tr -d '[:space:]')"
python3 "$LANE_DIR/w3c_owl2_harness.py" report --run-dir "$RUN_DIR"

echo "[w3c-owl2-run] run_id=$RUN_ID"
echo "[w3c-owl2-run] run_dir=$RUN_DIR"
