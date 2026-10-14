#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
TMP_DIR="$(mktemp -d)"
trap 'rm -rf "$TMP_DIR"' EXIT

assert_file_contains() {
  local file="$1"
  local pattern="$2"
  if ! grep -Eq "$pattern" "$file"; then
    echo "[test][error] expected pattern not found in $file: $pattern" >&2
    exit 1
  fi
}

assert_file_not_contains() {
  local file="$1"
  local pattern="$2"
  if grep -Eq "$pattern" "$file"; then
    echo "[test][error] unexpected pattern found in $file: $pattern" >&2
    exit 1
  fi
}

assert_section_before() {
  local file="$1"
  local first="$2"
  local second="$3"
  local first_line second_line
  first_line="$(grep -n "$first" "$file" | head -n 1 | cut -d: -f1 || true)"
  second_line="$(grep -n "$second" "$file" | head -n 1 | cut -d: -f1 || true)"
  if [[ -z "$first_line" || -z "$second_line" || "$first_line" -ge "$second_line" ]]; then
    echo "[test][error] expected '$first' to appear before '$second' in $file" >&2
    exit 1
  fi
}

summary_field() {
  local group="$1"
  local reasoner="$2"
  local column="$3"
  awk -F '\t' -v g="$group" -v r="$reasoner" -v col="$column" '
    NR == 1 { for (i = 1; i <= NF; i++) header[$i] = i; next }
    $1 == g && $4 == r { print $header[col]; exit }
  ' "$ORE_SUMMARY_FILE"
}

assert_summary_field() {
  local group="$1"
  local reasoner="$2"
  local column="$3"
  local expected="$4"
  local actual
  actual="$(summary_field "$group" "$reasoner" "$column")"
  if [[ "$actual" != "$expected" ]]; then
    echo "[test][error] summary $group/$reasoner field $column: expected '$expected' got '${actual:-<missing>}'" >&2
    exit 1
  fi
}

mkdir -p "$TMP_DIR/source/pure_dl" "$TMP_DIR/source/el" "$TMP_DIR/staged"
printf '<Ontology></Ontology>\n' > "$TMP_DIR/source/pure_dl/a.owl"
printf '<Ontology></Ontology>\n' > "$TMP_DIR/source/el/b.owl"

mkdir -p "$TMP_DIR/metadata-source/pool_sample/pure_dl/consistency" \
  "$TMP_DIR/metadata-source/pool_sample/el/consistency" \
  "$TMP_DIR/metadata-source/pool_sample/files"
printf 'ore2015_filename\nore_ont_0001.owl\n' \
  > "$TMP_DIR/metadata-source/pool_sample/pure_dl/consistency/metadata.csv"
printf 'ore2015_filename\nore_ont_0002.owl\n' \
  > "$TMP_DIR/metadata-source/pool_sample/el/consistency/metadata.csv"
printf '<Ontology></Ontology>\n' > "$TMP_DIR/metadata-source/pool_sample/files/ore_ont_0001.owl"
printf '<Ontology></Ontology>\n' > "$TMP_DIR/metadata-source/pool_sample/files/ore_ont_0002.owl"

if ORE2015_SOURCE_DIR="$TMP_DIR/metadata-source" \
  ORE2015_STAGED_DIR="$TMP_DIR/metadata-staged" \
  ORE2015_LINK_MODE=copy \
  "$ROOT_DIR/benchmarks/external/ore/prepare.sh" >"$TMP_DIR/ore_metadata_prepare_test.log"; then
  metadata_manifest="$TMP_DIR/metadata-staged/manifest.tsv"
  assert_file_contains "$metadata_manifest" $'^pure_dl\tOWL DL\tOWL DL Consistency\t.*ore_ont_0001\.owl'
  assert_file_contains "$metadata_manifest" $'^el\tOWL 2 EL\tOWL EL Consistency\t.*ore_ont_0002\.owl'
else
  echo "[test][error] official ORE metadata layout should be supported" >&2
  exit 1
fi

ORE2015_SOURCE_DIR="$TMP_DIR/source" \
ORE2015_STAGED_DIR="$TMP_DIR/staged" \
ORE2015_LINK_MODE=copy \
"$ROOT_DIR/benchmarks/external/ore/prepare.sh" >"$TMP_DIR/ore_prepare_test.log"

assert_file_contains "$TMP_DIR/staged/manifest.tsv" $'^pure_dl\tOWL DL\tOWL DL Consistency\t'
assert_file_contains "$TMP_DIR/staged/manifest.tsv" $'^el\tOWL 2 EL\tOWL EL Consistency\t'
jq -e '
  .corpus.doi == "10.5281/zenodo.18578"
  and .group_mapping.pure_dl.profile == "proper OWL DL"
  and .group_mapping.el.profile == "OWL 2 EL"
  and .oracle_policy.missing_or_ambiguous_oracle_result == "oracle_unavailable"
' "$TMP_DIR/staged/corpus_metadata.json" >/dev/null

RUN_ID=ore2015_test
RUN_DIR="$TMP_DIR/results/history/$RUN_ID"
mkdir -p "$RUN_DIR"

ORE2015_STAGED_DIR="$TMP_DIR/staged" \
ORE2015_RESULTS_HISTORY="$TMP_DIR/results/history" \
RUN_ID="$RUN_ID" \
ORE2015_DRY_RUN=1 \
"$ROOT_DIR/benchmarks/external/ore/run.sh" all run >"$TMP_DIR/ore_run_test.log"

jq -e '
  .group_mapping.pure_dl.official_group == "OWL DL"
  and .group_mapping.el.official_group == "OWL 2 EL"
  and .oracle_policy.missing_or_ambiguous_oracle_result == "oracle_unavailable"
' "$RUN_DIR/ore_metadata.json" >/dev/null

jq -n \
  '{ontology_suite:"standard", operation:"consistency", timeout_seconds:1, include_chebi:false, reasoners:["tableauxx","konclude"]}' \
  > "$RUN_DIR/run_metadata.json"

pure_dl_ontology="$(find "$TMP_DIR/staged" -maxdepth 1 -type f -name 'pure_dl_*.owl' -exec basename {} \; | sort | head -n 1)"
el_ontology="$(find "$TMP_DIR/staged" -maxdepth 1 -type f -name 'el_*.owl' -exec basename {} \; | sort | head -n 1)"
printf '%s\n%s\n' "$pure_dl_ontology" "$el_ontology" > "$RUN_DIR/ontology_manifest.txt"

jq -n \
  --arg run_id "$RUN_ID" \
  --arg ontology "$pure_dl_ontology" \
  '{
    schema_version: 1,
    run_id: $run_id,
    ontology_suite: "standard",
    case_id: ("standard:" + $ontology + ":consistency"),
    reasoner: "tableauxx",
    ontology: $ontology,
    operation: "consistency",
    status: "success",
    execution_status: "success",
    execution_detail: "success",
    result_status: "oracle_unavailable",
    semantic_result: "consistent",
    reasoning_result: "consistent",
    correctness_status: "not_evaluated",
    oracle: {status: "unavailable", source: "manifest.tsv", case_id: "", expected: null},
    wall_time_ms: 1,
    reported_time_ms: 1,
    parse_time_ms: -1,
    reason_time_ms: -1,
    timestamp: "2026-01-01T00:00:00Z",
    output: ""
  }' > "$RUN_DIR/tableauxx_${pure_dl_ontology%.owl}.json"

jq -n \
  --arg run_id "$RUN_ID" \
  --arg ontology "$el_ontology" \
  '{
    schema_version: 1,
    run_id: $run_id,
    ontology_suite: "standard",
    case_id: ("standard:" + $ontology + ":consistency"),
    reasoner: "tableauxx",
    ontology: $ontology,
    operation: "consistency",
    status: "success",
    execution_status: "success",
    execution_detail: "success",
    result_status: "oracle_unavailable",
    semantic_result: "consistent",
    reasoning_result: "consistent",
    correctness_status: "not_evaluated",
    oracle: {status: "unavailable", source: "manifest.tsv", case_id: "", expected: null},
    wall_time_ms: 1,
    reported_time_ms: 1,
    parse_time_ms: -1,
    reason_time_ms: -1,
    timestamp: "2026-01-01T00:00:00Z",
    output: ""
  }' > "$RUN_DIR/tableauxx_${el_ontology%.owl}.json"

jq -n \
  --arg run_id "$RUN_ID" \
  --arg ontology "$pure_dl_ontology" \
  '{
    schema_version: 1,
    run_id: $run_id,
    ontology_suite: "standard",
    case_id: ("standard:" + $ontology + ":consistency"),
    reasoner: "konclude",
    ontology: $ontology,
    operation: "consistency",
    status: "timeout",
    execution_status: "timeout",
    execution_detail: "timeout",
    result_status: "timeout",
    semantic_result: "unknown",
    reasoning_result: "unknown",
    correctness_status: "not_evaluated",
    oracle: {status: "unavailable", source: "manifest.tsv", case_id: "", expected: null},
    wall_time_ms: 3000,
    reported_time_ms: -1,
    parse_time_ms: 1500,
    reason_time_ms: -1,
    timestamp: "2026-01-01T00:00:00Z",
    output: ""
  }' > "$RUN_DIR/konclude_${pure_dl_ontology%.owl}.json"

jq -n \
  --arg run_id "$RUN_ID" \
  --arg ontology "$el_ontology" \
  '{
    schema_version: 1,
    run_id: $run_id,
    ontology_suite: "standard",
    case_id: ("standard:" + $ontology + ":consistency"),
    reasoner: "konclude",
    ontology: $ontology,
    operation: "consistency",
    status: "success",
    execution_status: "success",
    execution_detail: "success",
    result_status: "oracle_unavailable",
    semantic_result: "consistent",
    reasoning_result: "consistent",
    correctness_status: "not_evaluated",
    oracle: {status: "unavailable", source: "manifest.tsv", case_id: "", expected: null},
    wall_time_ms: 2,
    reported_time_ms: 2,
    parse_time_ms: 500,
    reason_time_ms: 700,
    timestamp: "2026-01-01T00:00:00Z",
    output: ""
  }' > "$RUN_DIR/konclude_${el_ontology%.owl}.json"

printf '{"result_status_counts":{}}\n' > "$RUN_DIR/summary.json"

source "$ROOT_DIR/benchmarks/competitors/scripts/benchmark_result.sh"
mapfile -t result_files < <(
  find "$RUN_DIR" -maxdepth 1 -type f -name '*.json' \
    ! -name 'run_metadata.json' ! -name 'ore_metadata.json' ! -name 'summary.json' | sort
)
for result_file in "${result_files[@]}"; do
  if ! validate_benchmark_result "$result_file"; then
    echo "[test][error] invalid result before shared report: $result_file" >&2
    jq '{run_id,ontology,execution_status,result_status,correctness_status,oracle}' "$result_file" >&2
    exit 1
  fi
done

ORE2015_RESULTS_HISTORY="$TMP_DIR/results/history" \
ORE2015_MANIFEST_FILE="$TMP_DIR/staged/manifest.tsv" \
RESULTS_DIR_OVERRIDE="$TMP_DIR/results" \
"$ROOT_DIR/benchmarks/external/ore/report.sh" "$RUN_ID" >"$TMP_DIR/ore_report_test.log" || {
  cat "$TMP_DIR/ore_report_test.log" >&2
  exit 1
}

if [[ "$(wc -l < "$RUN_DIR/results.csv" | tr -d '[:space:]')" != "5" ]]; then
  echo "[test][error] shared report must exclude ore_metadata.json from result CSV" >&2
  exit 1
fi

ORE_SUMMARY_FILE="$RUN_DIR/ore_consistency_summary.tsv"
ORE_REPORT_FILE="$RUN_DIR/ore_consistency_report.md"
ORE_DETAILS_FILE="$RUN_DIR/ore_consistency_details.tsv"

# Details TSV keeps its columns; every adjudicated row keeps oracle_unavailable.
assert_file_contains "$ORE_DETAILS_FILE" $'^local_group\tofficial_group\tore_track\tontology\tsource_ontology\treasoner\texecution_status\tresult_status\tcorrectness_status\toracle_status\tsemantic_result\toracle_case_id\tconsensus_expected\toracle_source$'
assert_file_contains "$ORE_DETAILS_FILE" $'^pure_dl\tOWL DL\tOWL DL Consistency\t[^\t]*\t[^\t]*\ttableauxx\tsuccess\toracle_unavailable\t'
assert_file_contains "$ORE_DETAILS_FILE" $'^el\tOWL 2 EL\tOWL EL Consistency\t[^\t]*\t[^\t]*\ttableauxx\tsuccess\toracle_unavailable\t'
assert_file_contains "$ORE_DETAILS_FILE" $'^pure_dl\tOWL DL\tOWL DL Consistency\t[^\t]*\t[^\t]*\tkonclude\ttimeout\ttimeout\t'
assert_file_contains "$ORE_DETAILS_FILE" $'^el\tOWL 2 EL\tOWL EL Consistency\t[^\t]*\t[^\t]*\tkonclude\tsuccess\toracle_unavailable\t'

# Summary TSV is performance-first and keeps the group breakdown distinguishable.
assert_file_contains "$ORE_SUMMARY_FILE" $'^local_group\tofficial_group\tore_track\treasoner\ttotal\tcompleted\ttimeout\tfailed\tnot_available\t'
assert_file_contains "$ORE_SUMMARY_FILE" $'wall_time_ms_recorded\twall_time_ms_min\twall_time_ms_mean\twall_time_ms_max'
assert_file_contains "$ORE_SUMMARY_FILE" $'reason_time_ms_recorded\treason_time_ms_min\treason_time_ms_mean\treason_time_ms_max\tpassed\twrong\toracle_unavailable\tnot_evaluated\tunsupported$'
assert_file_contains "$ORE_SUMMARY_FILE" $'^pure_dl\tOWL DL\tOWL DL Consistency\t'
assert_file_contains "$ORE_SUMMARY_FILE" $'^el\tOWL 2 EL\tOWL EL Consistency\t'
if grep -Fq $'unknown\t' "$ORE_SUMMARY_FILE"; then
  echo "[test][error] generated summary.json must not be treated as an ORE case" >&2
  exit 1
fi

# Completability counts lead the summary per group/track/reasoner.
assert_summary_field pure_dl tableauxx total 1
assert_summary_field pure_dl tableauxx completed 1
assert_summary_field pure_dl tableauxx timeout 0
assert_summary_field pure_dl tableauxx failed 0
assert_summary_field pure_dl tableauxx not_available 0
assert_summary_field pure_dl konclude total 1
assert_summary_field pure_dl konclude completed 0
assert_summary_field pure_dl konclude timeout 1
assert_summary_field el tableauxx total 1
assert_summary_field el tableauxx completed 1

# Timing appears where the run records it; -1 means not recorded and is never invented.
assert_summary_field pure_dl tableauxx wall_time_ms_recorded 1
assert_summary_field pure_dl tableauxx wall_time_ms_min 1
assert_summary_field pure_dl tableauxx wall_time_ms_mean 1.0
assert_summary_field pure_dl tableauxx wall_time_ms_max 1
assert_summary_field pure_dl tableauxx reported_time_ms_recorded 1
assert_summary_field pure_dl tableauxx reported_time_ms_max 1
assert_summary_field pure_dl tableauxx parse_time_ms_recorded 0
assert_summary_field pure_dl tableauxx parse_time_ms_min ''
assert_summary_field pure_dl tableauxx parse_time_ms_mean ''
assert_summary_field pure_dl tableauxx parse_time_ms_max ''
assert_summary_field pure_dl tableauxx reason_time_ms_recorded 0
assert_summary_field pure_dl tableauxx reason_time_ms_max ''
assert_summary_field pure_dl konclude wall_time_ms_recorded 1
assert_summary_field pure_dl konclude wall_time_ms_min 3000
assert_summary_field pure_dl konclude wall_time_ms_max 3000
assert_summary_field pure_dl konclude reported_time_ms_recorded 0
assert_summary_field pure_dl konclude reported_time_ms_min ''
assert_summary_field pure_dl konclude reported_time_ms_max ''
assert_summary_field pure_dl konclude parse_time_ms_recorded 1
assert_summary_field pure_dl konclude parse_time_ms_max 1500
assert_summary_field el konclude completed 1
assert_summary_field el konclude wall_time_ms_recorded 1
assert_summary_field el konclude wall_time_ms_max 2
assert_summary_field el konclude parse_time_ms_max 500
assert_summary_field el konclude reason_time_ms_recorded 1
assert_summary_field el konclude reason_time_ms_max 700

# Correctness record columns remain (no per-row status changes).
assert_summary_field pure_dl tableauxx oracle_unavailable 1
assert_summary_field pure_dl tableauxx passed 0
assert_summary_field pure_dl tableauxx wrong 0
assert_summary_field el tableauxx oracle_unavailable 1
assert_summary_field el konclude oracle_unavailable 1
assert_summary_field pure_dl konclude oracle_unavailable 0

# Report header declares the performance-lane contract (ADR 0024).
assert_file_contains "$ORE_REPORT_FILE" 'Performance-lane contract'
assert_file_contains "$ORE_REPORT_FILE" 'adjudicates no correctness'
assert_file_contains "$ORE_REPORT_FILE" 'no public oracle'
assert_file_not_contains "$ORE_REPORT_FILE" 'Oracle policy:'

# The report leads with execution performance and completability.
assert_section_before "$ORE_REPORT_FILE" '## Execution Performance and Completability' '## Correctness Record'
assert_file_contains "$ORE_REPORT_FILE" '\| pure_dl \| OWL DL \| OWL DL Consistency \| tableauxx \| 1 \| 1 \| 0 \| 0 \| 0 \|'
assert_file_contains "$ORE_REPORT_FILE" '\| pure_dl \| OWL DL \| OWL DL Consistency \| konclude \| 1 \| 0 \| 1 \| 0 \| 0 \|'
assert_file_contains "$ORE_REPORT_FILE" '\| el \| OWL 2 EL \| OWL EL Consistency \| tableauxx \| 1 \| 1 \| 0 \| 0 \| 0 \|'
assert_file_contains "$ORE_REPORT_FILE" '\| el \| OWL 2 EL \| OWL EL Consistency \| konclude \| 1 \| 1 \| 0 \| 0 \| 0 \|'
assert_file_contains "$ORE_REPORT_FILE" 'Timing policy'
assert_file_contains "$ORE_REPORT_FILE" 'never coerced or invented'

# Differential appendix section contract: labelled, observational, no gate authority.
assert_section_before "$ORE_REPORT_FILE" '## Correctness Record' '## Differential Appendix'
assert_file_contains "$ORE_REPORT_FILE" '## Differential Appendix'
assert_file_contains "$ORE_REPORT_FILE" 'observational'
assert_file_contains "$ORE_REPORT_FILE" 'no gate authority'

# Multi-reasoner fixture: the appendix fills with the observational
# which-engine-returned-what table and never claims the empty contract.
# Engines appear in deterministic sorted order, matching the report's other
# per-reasoner tables.
assert_file_contains "$ORE_REPORT_FILE" $'^\| Ontology \| konclude \| tableauxx \|$'
assert_file_contains "$ORE_REPORT_FILE" "^\\| $pure_dl_ontology \\| TO \\| consistent \\|$"
assert_file_contains "$ORE_REPORT_FILE" "^\\| $el_ontology \\| consistent \\| consistent \\|$"
assert_file_contains "$ORE_REPORT_FILE" 'adjudicates no correctness'
assert_file_not_contains "$ORE_REPORT_FILE" 'intentionally empty'

# Single-reasoner fixture: no comparative data, so the appendix renders the
# explicitly empty contract section and no engine-output table.
SINGLE_RUN_ID=ore2015_test_single
SINGLE_RUN_DIR="$TMP_DIR/results/history/$SINGLE_RUN_ID"
mkdir -p "$SINGLE_RUN_DIR"
cp "$RUN_DIR/tableauxx_${pure_dl_ontology%.owl}.json" "$SINGLE_RUN_DIR/"
cp "$RUN_DIR/tableauxx_${el_ontology%.owl}.json" "$SINGLE_RUN_DIR/"
cp "$RUN_DIR/ontology_manifest.txt" "$SINGLE_RUN_DIR/"
printf '{"result_status_counts":{}}\n' > "$SINGLE_RUN_DIR/summary.json"
jq -n \
  '{ontology_suite:"standard", operation:"consistency", timeout_seconds:1, include_chebi:false, reasoners:["tableauxx"]}' \
  > "$SINGLE_RUN_DIR/run_metadata.json"

ORE2015_RESULTS_HISTORY="$TMP_DIR/results/history" \
ORE2015_MANIFEST_FILE="$TMP_DIR/staged/manifest.tsv" \
ORE2015_SKIP_SHARED_REPORT=1 \
"$ROOT_DIR/benchmarks/external/ore/report.sh" "$SINGLE_RUN_ID" >"$TMP_DIR/ore_report_single_test.log" || {
  cat "$TMP_DIR/ore_report_single_test.log" >&2
  exit 1
}

SINGLE_REPORT_FILE="$SINGLE_RUN_DIR/ore_consistency_report.md"
assert_file_contains "$SINGLE_REPORT_FILE" '## Differential Appendix'
assert_file_contains "$SINGLE_REPORT_FILE" 'observational'
assert_file_contains "$SINGLE_REPORT_FILE" 'no gate authority'
assert_file_contains "$SINGLE_REPORT_FILE" 'intentionally empty'
assert_file_not_contains "$SINGLE_REPORT_FILE" $'^\| Ontology \| tableauxx \|$'

echo "[test] ORE lane shell tests passed"
