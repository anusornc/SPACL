#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/benchmark_result.sh"

AGGREGATION="$SCRIPT_DIR/run_comparative_aggregation.sh"

TMP_DIR="$(mktemp -d)"
trap 'rm -rf "$TMP_DIR"' EXIT

HISTORY="$TMP_DIR/results/history"
OUT_DIR="$HISTORY/aggregate_test"

assert_eq() {
    local expected="$1"
    local actual="$2"
    local message="$3"
    if [[ "$expected" != "$actual" ]]; then
        printf 'assertion failed: %s (expected=%s actual=%s)\n' "$message" "$expected" "$actual" >&2
        exit 1
    fi
}

assert_file_contains() {
    local file="$1"
    local pattern="$2"
    if ! grep -Eq "$pattern" "$file"; then
        printf 'assertion failed: expected pattern not found in %s: %s\n' "$file" "$pattern" >&2
        exit 1
    fi
}

assert_file_not_contains() {
    local file="$1"
    local pattern="$2"
    if grep -Eq "$pattern" "$file"; then
        printf 'assertion failed: unexpected pattern found in %s: %s\n' "$file" "$pattern" >&2
        exit 1
    fi
}

assert_command_fails() {
    local message="$1"
    shift
    if "$@" >/dev/null 2>&1; then
        printf 'assertion failed: %s\n' "$message" >&2
        exit 1
    fi
}

median_field() {
    local case_id="$1"
    local engine="$2"
    local column="$3"
    awk -F '\t' -v c="$case_id" -v e="$engine" -v col="$column" '
        NR == 1 { for (i = 1; i <= NF; i++) header[$i] = i; next }
        $1 == c && $3 == e { print $header[col]; exit }
    ' "$OUT_DIR/comparative_median_times.tsv"
}

assert_median_field() {
    local case_id="$1"
    local engine="$2"
    local column="$3"
    local expected="$4"
    local actual
    actual="$(median_field "$case_id" "$engine" "$column")"
    if [[ "$actual" != "$expected" ]]; then
        printf 'assertion failed: median %s/%s field %s: expected %s got %s\n' \
            "$case_id" "$engine" "$column" "$expected" "${actual:-<missing>}" >&2
        exit 1
    fi
}

summary_field() {
    local engine="$1"
    local column="$2"
    awk -F '\t' -v e="$engine" -v col="$column" '
        NR == 1 { for (i = 1; i <= NF; i++) header[$i] = i; next }
        $1 == e { print $header[col]; exit }
    ' "$OUT_DIR/comparative_engine_summary.tsv"
}

assert_summary_field() {
    local engine="$1"
    local column="$2"
    local expected="$3"
    local actual
    actual="$(summary_field "$engine" "$column")"
    if [[ "$actual" != "$expected" ]]; then
        printf 'assertion failed: summary %s field %s: expected %s got %s\n' \
            "$engine" "$column" "$expected" "${actual:-<missing>}" >&2
        exit 1
    fi
}

pass_metadata() {
    local pass_dir="$1"
    local run_id="$2"
    jq -n \
        --arg run_id "$run_id" \
        --arg ontology_suite "${3:-standard}" \
        --arg operation "${4:-consistency}" \
        '{
            run_id: $run_id,
            ontology_suite: $ontology_suite,
            operation: $operation,
            timeout_seconds: 300,
            reasoners: ["tableauxx", "konclude", "hermit"]
        }' > "$pass_dir/run_metadata.json"
}

write_case() {
    local pass_dir="$1"
    local reasoner="$2"
    local ontology="$3"
    local execution_detail="$4"
    local wall_ms="$5"
    write_benchmark_result \
        "$pass_dir/${reasoner}_${ontology%.owl}.json" \
        "$(basename "$pass_dir")" "standard" "standard:$ontology:consistency" \
        "$reasoner" "$ontology" "consistency" \
        "$execution_detail" "consistent" "not_evaluated" "not_applicable" "" "" "" \
        "$wall_ms" "$wall_ms" -1 -1 "$TMP_DIR/engine.output"
}

printf '%s\n' '{"reasoner": "fixture", "version": "1.0"}' > "$TMP_DIR/engine.output"

for i in 1 2 3; do
    mkdir -p "$HISTORY/pass_r$i"
    pass_metadata "$HISTORY/pass_r$i" "pass_r$i"
done

# alpha.owl: clean medians, konclude best.
write_case "$HISTORY/pass_r1" tableauxx alpha.owl success 100
write_case "$HISTORY/pass_r2" tableauxx alpha.owl success 300
write_case "$HISTORY/pass_r3" tableauxx alpha.owl success 200
write_case "$HISTORY/pass_r1" konclude alpha.owl success 150
write_case "$HISTORY/pass_r2" konclude alpha.owl success 150
write_case "$HISTORY/pass_r3" konclude alpha.owl success 150
write_case "$HISTORY/pass_r1" hermit alpha.owl success 400
write_case "$HISTORY/pass_r2" hermit alpha.owl success 500
write_case "$HISTORY/pass_r3" hermit alpha.owl success 600

# beta.owl: tableauxx and hermit tie for the best median.
write_case "$HISTORY/pass_r1" tableauxx beta.owl success 120
write_case "$HISTORY/pass_r2" tableauxx beta.owl success 120
write_case "$HISTORY/pass_r3" tableauxx beta.owl success 120
write_case "$HISTORY/pass_r1" konclude beta.owl success 130
write_case "$HISTORY/pass_r2" konclude beta.owl success 150
write_case "$HISTORY/pass_r3" konclude beta.owl success 170
write_case "$HISTORY/pass_r1" hermit beta.owl success 110
write_case "$HISTORY/pass_r2" hermit beta.owl success 120
write_case "$HISTORY/pass_r3" hermit beta.owl success 130

# gamma.owl: konclude times out everywhere, hermit not available, TO/NA codes.
write_case "$HISTORY/pass_r1" tableauxx gamma.owl success 90
write_case "$HISTORY/pass_r2" tableauxx gamma.owl success 110
write_case "$HISTORY/pass_r3" tableauxx gamma.owl success 100
write_case "$HISTORY/pass_r1" konclude gamma.owl timeout 300000
write_case "$HISTORY/pass_r2" konclude gamma.owl timeout 300000
write_case "$HISTORY/pass_r3" konclude gamma.owl timeout 300000
write_case "$HISTORY/pass_r1" hermit gamma.owl not_available -1
write_case "$HISTORY/pass_r2" hermit gamma.owl not_available -1
write_case "$HISTORY/pass_r3" hermit gamma.owl not_available -1

# delta.owl: hermit fails everywhere; konclude completes 2 of 3 passes so its
# median is computed over recorded values only (even count -> fractional).
write_case "$HISTORY/pass_r1" tableauxx delta.owl success 210
write_case "$HISTORY/pass_r2" tableauxx delta.owl success 230
write_case "$HISTORY/pass_r3" tableauxx delta.owl success 220
write_case "$HISTORY/pass_r1" konclude delta.owl failed -1
write_case "$HISTORY/pass_r2" konclude delta.owl success 100
write_case "$HISTORY/pass_r3" konclude delta.owl success 201
write_case "$HISTORY/pass_r1" hermit delta.owl failed -1
write_case "$HISTORY/pass_r2" hermit delta.owl failed -1
write_case "$HISTORY/pass_r3" hermit delta.owl failed -1

# Two or more passes of the same track are required.
assert_command_fails "a single pass directory must be rejected" \
    env AGGREGATION_OUTPUT_DIR="$OUT_DIR" bash "$AGGREGATION" "$HISTORY/pass_r1"
assert_command_fails "no pass directories must be rejected" \
    env AGGREGATION_OUTPUT_DIR="$OUT_DIR" bash "$AGGREGATION"

# A pass from a different track must be rejected.
mkdir -p "$HISTORY/pass_other_track"
pass_metadata "$HISTORY/pass_other_track" "pass_other_track" "large" "consistency"
write_case "$HISTORY/pass_other_track" tableauxx alpha.owl success 100
assert_command_fails "a pass from a different ontology suite must be rejected" \
    env AGGREGATION_OUTPUT_DIR="$OUT_DIR" bash "$AGGREGATION" \
    "$HISTORY/pass_r1" "$HISTORY/pass_r2" "$HISTORY/pass_other_track"

AGGREGATION_OUTPUT_DIR="$OUT_DIR" bash "$AGGREGATION" \
    "$HISTORY/pass_r1" "$HISTORY/pass_r2" "$HISTORY/pass_r3" >"$TMP_DIR/aggregate.log"

MEDIAN_TSV="$OUT_DIR/comparative_median_times.tsv"
PAPER_MD="$OUT_DIR/comparative_paper_table.md"
SUMMARY_TSV="$OUT_DIR/comparative_engine_summary.tsv"
SUMMARY_MD="$OUT_DIR/comparative_engine_summary.md"
METADATA_JSON="$OUT_DIR/aggregation_metadata.json"

# Per-case repetition medians over recorded passes.
assert_file_contains "$MEDIAN_TSV" $'^case_id\tontology\tengine\taggregated_status\trecorded_passes\ttotal_passes\tmedian_wall_time_ms$'
assert_median_field standard:alpha.owl:consistency tableauxx aggregated_status success
assert_median_field standard:alpha.owl:consistency tableauxx recorded_passes 3
assert_median_field standard:alpha.owl:consistency tableauxx total_passes 3
assert_median_field standard:alpha.owl:consistency tableauxx median_wall_time_ms 200
assert_median_field standard:alpha.owl:consistency konclude median_wall_time_ms 150
assert_median_field standard:alpha.owl:consistency hermit median_wall_time_ms 500
assert_median_field standard:beta.owl:consistency tableauxx median_wall_time_ms 120
assert_median_field standard:beta.owl:consistency konclude median_wall_time_ms 150
assert_median_field standard:beta.owl:consistency hermit median_wall_time_ms 120
assert_median_field standard:gamma.owl:consistency tableauxx median_wall_time_ms 100
assert_median_field standard:gamma.owl:consistency konclude aggregated_status timeout
assert_median_field standard:gamma.owl:consistency konclude recorded_passes 0
assert_median_field standard:gamma.owl:consistency konclude median_wall_time_ms ''
assert_median_field standard:gamma.owl:consistency hermit aggregated_status not_available
assert_median_field standard:delta.owl:consistency tableauxx median_wall_time_ms 220
assert_median_field standard:delta.owl:consistency konclude aggregated_status success
assert_median_field standard:delta.owl:consistency konclude recorded_passes 2
assert_median_field standard:delta.owl:consistency konclude total_passes 3
assert_median_field standard:delta.owl:consistency konclude median_wall_time_ms 150.5
assert_median_field standard:delta.owl:consistency hermit aggregated_status failed

# Paper table: best median bolded, ties all bolded, TO/NA/FAIL codes.
assert_file_contains "$PAPER_MD" $'^\| Ontology \| hermit \| konclude \| tableauxx \|$'
assert_file_contains "$PAPER_MD" $'^\| alpha\.owl \| 500 \| \*\*150\*\* \| 200 \|$'
assert_file_contains "$PAPER_MD" $'^\| beta\.owl \| \*\*120\*\* \| 150 \| \*\*120\*\* \|$'
assert_file_contains "$PAPER_MD" $'^\| gamma\.owl \| NA \| TO \| \*\*100\*\* \|$'
assert_file_contains "$PAPER_MD" $'^\| delta\.owl \| FAIL \| \*\*150\.5\*\* \| 220 \|$'
assert_file_contains "$PAPER_MD" 'TO=timeout'
assert_file_contains "$PAPER_MD" 'NA=not available'
assert_file_contains "$PAPER_MD" 'FAIL='
assert_file_contains "$PAPER_MD" 'best median wall time among engines that completed'
alpha_row="$(grep -n '^| alpha' "$PAPER_MD" | cut -d: -f1)"
beta_row="$(grep -n '^| beta' "$PAPER_MD" | cut -d: -f1)"
gamma_row="$(grep -n '^| gamma' "$PAPER_MD" | cut -d: -f1)"
delta_row="$(grep -n '^| delta' "$PAPER_MD" | cut -d: -f1)"
if [[ "$alpha_row" -ge "$beta_row" || "$beta_row" -ge "$delta_row" || "$delta_row" -ge "$gamma_row" ]]; then
    echo "assertion failed: paper table rows must be sorted by case" >&2
    exit 1
fi

# Per-engine completability and timing summary.
assert_file_contains "$SUMMARY_TSV" $'^engine\tcases\tcompleted\ttimeout\tfailed\tnot_available\tother_execution\tmedian_wall_time_ms\ttotal_wall_time_ms$'
assert_summary_field tableauxx cases 4
assert_summary_field tableauxx completed 4
assert_summary_field tableauxx timeout 0
assert_summary_field tableauxx failed 0
assert_summary_field tableauxx not_available 0
assert_summary_field tableauxx other_execution 0
assert_summary_field tableauxx median_wall_time_ms 160
assert_summary_field tableauxx total_wall_time_ms 640
assert_summary_field konclude cases 4
assert_summary_field konclude completed 3
assert_summary_field konclude timeout 1
assert_summary_field konclude failed 0
assert_summary_field konclude not_available 0
assert_summary_field konclude other_execution 0
assert_summary_field konclude median_wall_time_ms 150
assert_summary_field konclude total_wall_time_ms 450.5
assert_summary_field hermit cases 4
assert_summary_field hermit completed 2
assert_summary_field hermit timeout 0
assert_summary_field hermit failed 1
assert_summary_field hermit not_available 1
assert_summary_field hermit other_execution 0
assert_summary_field hermit median_wall_time_ms 310
assert_summary_field hermit total_wall_time_ms 620
assert_file_contains "$SUMMARY_MD" $'^\| tableauxx \| 4 \| 4 \| 0 \| 0 \| 0 \| 0 \| 160 \| 640 \|$'
assert_file_contains "$SUMMARY_MD" $'^\| konclude \| 4 \| 3 \| 1 \| 0 \| 0 \| 0 \| 150 \| 450\.5 \|$'
assert_file_contains "$SUMMARY_MD" $'^\| hermit \| 4 \| 2 \| 0 \| 1 \| 1 \| 0 \| 310 \| 620 \|$'

# Aggregation metadata pins the pass identities and the track.
jq -e '
    .aggregation_type == "comparative_repetition_median"
    and .ontology_suite == "standard"
    and .operation == "consistency"
    and .pass_count == 3
    and (.pass_run_ids == ["pass_r1", "pass_r2", "pass_r3"])
    and (.engines == ["hermit", "konclude", "tableauxx"])
    and (.artifacts.paper_table == "comparative_paper_table.md")
' "$METADATA_JSON" >/dev/null

# Execution-performance evidence only: no correctness vocabulary appears in
# the new artifacts.
for artifact in "$MEDIAN_TSV" "$PAPER_MD" "$SUMMARY_TSV" "$SUMMARY_MD"; do
    assert_file_not_contains "$artifact" 'oracle|passed|wrong|correctness'
done

echo "comparative aggregation tests passed"
