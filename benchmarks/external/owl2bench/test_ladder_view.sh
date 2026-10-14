#!/usr/bin/env bash
set -euo pipefail

# Fixture tests for the OWL2Bench scalability ladder view (issue #197,
# ADR 0025/0026). The per-dataset engine summaries are produced by the real
# comparative aggregation over synthetic passes; no engine ever executes.

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
SCRIPT_DIR="$ROOT_DIR/benchmarks/external/owl2bench"
LADDER_VIEW="$SCRIPT_DIR/ladder_view.sh"
AGGREGATION="$ROOT_DIR/benchmarks/competitors/scripts/run_comparative_aggregation.sh"

source "$ROOT_DIR/benchmarks/competitors/scripts/benchmark_result.sh"

TMP_DIR="$(mktemp -d)"
trap 'rm -rf "$TMP_DIR"' EXIT

HISTORY="$TMP_DIR/results/history"
mkdir -p "$HISTORY"

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

assert_command_fails_with() {
    local message="$1"
    local pattern="$2"
    shift 2
    local log="$TMP_DIR/fail_$(date +%s%N).log"
    if "$@" >"$log" 2>&1; then
        printf 'assertion failed: %s (command unexpectedly succeeded)\n' "$message" >&2
        exit 1
    fi
    if ! grep -Eq "$pattern" "$log"; then
        printf 'assertion failed: %s (error output lacked pattern %s)\n' "$message" "$pattern" >&2
        cat "$log" >&2
        exit 1
    fi
}

line_number_of() {
    grep -nE "$1" "$2" | head -n 1 | cut -d: -f1 || true
}

assert_line_before() {
    local file="$1" first_pattern="$2" second_pattern="$3"
    local first_line second_line
    first_line="$(line_number_of "$first_pattern" "$file")"
    second_line="$(line_number_of "$second_pattern" "$file")"
    if [[ -z "$first_line" || -z "$second_line" || "$first_line" -ge "$second_line" ]]; then
        printf 'assertion failed: expected %s before %s in %s\n' "$first_pattern" "$second_pattern" "$file" >&2
        exit 1
    fi
}

pass_metadata() {
    local pass_dir="$1"
    local run_id="$2"
    local engines_json="$3"
    jq -n \
        --arg run_id "$run_id" \
        --argjson reasoners "$engines_json" \
        '{
            run_id: $run_id,
            ontology_suite: "standard",
            operation: "consistency",
            timeout_seconds: 300,
            reasoners: $reasoners
        }' > "$pass_dir/run_metadata.json"
}

write_case() {
    local pass_dir="$1"
    local run_id="$2"
    local reasoner="$3"
    local ontology="$4"
    local execution_detail="$5"
    local wall_ms="$6"
    local semantic="unknown"
    [[ "$execution_detail" == "success" ]] && semantic="consistent"
    write_benchmark_result \
        "$pass_dir/${reasoner}_${ontology%.owl}.json" \
        "$run_id" "standard" "standard:$ontology:consistency" \
        "$reasoner" "$ontology" "consistency" \
        "$execution_detail" "$semantic" "not_evaluated" "not_applicable" "" "" "" \
        "$wall_ms" "$wall_ms" -1 -1 "$ENGINE_OUTPUT"
}

# build_point HISTORY_DIR PROFILE LOWER_PROFILE SIZE ENGINES_JSON SPEC_FILE
# Reads engine result lines from SPEC_FILE: "<engine> <detail> <w1> <w2> <w3>"
# and aggregates the three passes into an aggregation output directory.
build_point() {
    local history_dir="$1"
    local profile="$2"
    local lower_profile="$3"
    local universities="$4"
    local engines_json="$5"
    local spec_file="$6"
    local ontology="UNIV-BENCH-OWL2${profile}-seed1-universities${universities}.owl"

    local pass run_id pass_dir wall
    while IFS= read -r spec_line; do
        [[ -n "$spec_line" ]] || continue
        read -r engine detail w1 w2 w3 <<<"$spec_line"
        for pass in 1 2 3; do
            run_id="owl2bench_cmp_p${pass}_${lower_profile}_u${universities}"
            pass_dir="$history_dir/$run_id"
            if [[ ! -d "$pass_dir" ]]; then
                mkdir -p "$pass_dir"
                pass_metadata "$pass_dir" "$run_id" "$engines_json"
            fi
            case "$pass" in
                1) wall="$w1" ;;
                2) wall="$w2" ;;
                3) wall="$w3" ;;
            esac
            write_case "$pass_dir" "$run_id" "$engine" "$ontology" "$detail" "$wall"
        done
    done <"$spec_file"

    local out_dir="$history_dir/aggregate_${lower_profile}_u${universities}"
    AGGREGATION_OUTPUT_DIR="$out_dir" bash "$AGGREGATION" \
        "$history_dir/owl2bench_cmp_p1_${lower_profile}_u${universities}" \
        "$history_dir/owl2bench_cmp_p2_${lower_profile}_u${universities}" \
        "$history_dir/owl2bench_cmp_p3_${lower_profile}_u${universities}" \
        >"$TMP_DIR/aggregate_${lower_profile}_u${universities}.log"
    printf '%s\n' "$out_dir"
}

DL_ENGINES='["tableauxx","hermit","konclude"]'
EL_ENGINES='["tableauxx","hermit","konclude","elk"]'

ENGINE_OUTPUT="$TMP_DIR/engine.output"
printf '%s\n' '{"reasoner": "fixture", "version": "1.0"}' > "$ENGINE_OUTPUT"

# ---------------------------------------------------------------------------
# Synthetic ladder: DL and EL over universities 1 and 2. Covers bolded best
# medians, an all-timeout engine, NA, and FAIL cells.
# ---------------------------------------------------------------------------

DL1_SPEC="$TMP_DIR/dl_u1.spec"
cat > "$DL1_SPEC" <<'EOF'
tableauxx success 2000 2200 2100
hermit success 9000 8900 9100
konclude success 4000 4000 4000
EOF

DL2_SPEC="$TMP_DIR/dl_u2.spec"
cat > "$DL2_SPEC" <<'EOF'
tableauxx success 5000 5200 5400
hermit timeout 300000 300000 300000
konclude success 4800 4900 5000
EOF

EL1_SPEC="$TMP_DIR/el_u1.spec"
cat > "$EL1_SPEC" <<'EOF'
tableauxx success 800 800 800
hermit not_available -1 -1 -1
konclude timeout 300000 300000 300000
elk success 500 600 700
EOF

EL2_SPEC="$TMP_DIR/el_u2.spec"
cat > "$EL2_SPEC" <<'EOF'
tableauxx failed -1 -1 -1
hermit success 7000 7000 7000
konclude success 6500 6600 6700
elk success 12000 12100 12200
EOF

DL1_AGG="$(build_point "$HISTORY" DL dl 1 "$DL_ENGINES" "$DL1_SPEC")"
DL2_AGG="$(build_point "$HISTORY" DL dl 2 "$DL_ENGINES" "$DL2_SPEC")"
EL1_AGG="$(build_point "$HISTORY" EL el 1 "$EL_ENGINES" "$EL1_SPEC")"
EL2_AGG="$(build_point "$HISTORY" EL el 2 "$EL_ENGINES" "$EL2_SPEC")"

OUT_DIR="$TMP_DIR/ladder_out"
mkdir -p "$OUT_DIR"

run_view() {
    env OWL2BENCH_LADDER_OUTPUT_DIR="$OUT_DIR.new" \
        OWL2BENCH_LADDER_PROFILES="${OWL2BENCH_LADDER_PROFILES:-DL,EL}" \
        OWL2BENCH_LADDER_SIZES="${OWL2BENCH_LADDER_SIZES:-1,2}" \
        "$@"
}

# ---------------------------------------------------------------------------
# Fail-closed modes first (each must name what is missing or malformed).
# ---------------------------------------------------------------------------

assert_command_fails_with "missing size input must fail closed naming it" \
    "missing ladder point input.*EL:2" \
    run_view bash "$LADDER_VIEW" "$DL1_AGG" "$DL2_AGG" "$EL1_AGG"

assert_command_fails_with "missing profile input must fail closed naming it" \
    "missing ladder point input.*EL:1.*EL:2" \
    run_view bash "$LADDER_VIEW" "$DL1_AGG" "$DL2_AGG"

assert_command_fails_with "default size ladder must demand every official size" \
    "missing ladder point input.*DL:5" \
    env OWL2BENCH_LADDER_OUTPUT_DIR="$OUT_DIR.new" \
        OWL2BENCH_LADDER_PROFILES="DL" \
        bash "$LADDER_VIEW" "$DL1_AGG" "$DL2_AGG"

EMPTY_AGG="$TMP_DIR/aggregate_empty"
mkdir -p "$EMPTY_AGG"
assert_command_fails_with "missing engine summary must fail closed naming it" \
    "missing comparative_engine_summary.tsv: $EMPTY_AGG" \
    run_view bash "$LADDER_VIEW" "$EMPTY_AGG"

NO_METADATA_AGG="$TMP_DIR/aggregate_no_metadata"
mkdir -p "$NO_METADATA_AGG"
cp "$DL1_AGG/comparative_engine_summary.tsv" "$NO_METADATA_AGG/"
assert_command_fails_with "missing aggregation metadata must fail closed naming it" \
    "missing aggregation_metadata.json: $NO_METADATA_AGG" \
    run_view bash "$LADDER_VIEW" "$NO_METADATA_AGG"

BAD_JSON_AGG="$TMP_DIR/aggregate_bad_json"
mkdir -p "$BAD_JSON_AGG"
cp "$DL1_AGG/comparative_engine_summary.tsv" "$BAD_JSON_AGG/"
printf 'not json\n' > "$BAD_JSON_AGG/aggregation_metadata.json"
assert_command_fails_with "malformed aggregation metadata must fail closed" \
    "malformed aggregation metadata" \
    run_view bash "$LADDER_VIEW" "$BAD_JSON_AGG"

BAD_HEADER_AGG="$TMP_DIR/aggregate_bad_header"
mkdir -p "$BAD_HEADER_AGG"
cp "$DL1_AGG/comparative_engine_summary.tsv" "$BAD_HEADER_AGG/"
cp "$DL1_AGG/comparative_median_times.tsv" "$BAD_HEADER_AGG/"
cp "$DL1_AGG/aggregation_metadata.json" "$BAD_HEADER_AGG/"
sed -i '1s/.*/engine\tbroken\theader/' "$BAD_HEADER_AGG/comparative_engine_summary.tsv"
assert_command_fails_with "summary header drift must fail closed" \
    "malformed engine summary header" \
    run_view bash "$LADDER_VIEW" "$BAD_HEADER_AGG"

EMPTY_MEDIAN_AGG="$TMP_DIR/aggregate_empty_median"
mkdir -p "$EMPTY_MEDIAN_AGG"
cp "$DL1_AGG/comparative_engine_summary.tsv" "$EMPTY_MEDIAN_AGG/"
cp "$DL1_AGG/aggregation_metadata.json" "$EMPTY_MEDIAN_AGG/"
cp "$DL1_AGG/comparative_median_times.tsv" "$EMPTY_MEDIAN_AGG/"
awk -F '\t' 'BEGIN { OFS="\t" } NR>1 && $1=="tableauxx" { $8="" } { print }' \
    "$EMPTY_MEDIAN_AGG/comparative_engine_summary.tsv" \
    > "$EMPTY_MEDIAN_AGG/comparative_engine_summary.tsv.tmp"
mv "$EMPTY_MEDIAN_AGG/comparative_engine_summary.tsv.tmp" "$EMPTY_MEDIAN_AGG/comparative_engine_summary.tsv"
assert_command_fails_with "completed engine without median must fail closed" \
    "malformed summary row.*tableauxx" \
    run_view bash "$LADDER_VIEW" "$EMPTY_MEDIAN_AGG"

assert_command_fails_with "duplicate ladder point must fail closed" \
    "duplicate ladder point DL:1" \
    run_view bash "$LADDER_VIEW" "$DL1_AGG" "$DL1_AGG"

# Non-ladder run ids in the aggregation metadata.
FOREIGN_SPEC="$TMP_DIR/foreign.spec"
cat > "$FOREIGN_SPEC" <<'EOF'
tableauxx success 100 100 100
hermit success 200 200 200
konclude success 300 300 300
EOF
mkdir -p "$HISTORY/foreign_p1" "$HISTORY/foreign_p2" "$HISTORY/foreign_p3"
for pass in 1 2 3; do
    pass_metadata "$HISTORY/foreign_p$pass" "foreign_p$pass" "$DL_ENGINES"
done
while IFS= read -r spec_line; do
    read -r engine detail w1 w2 w3 <<<"$spec_line"
    write_case "$HISTORY/foreign_p1" "foreign_p1" "$engine" "UNIV-BENCH-OWL2DL-seed1-universities1.owl" "$detail" "$w1"
    write_case "$HISTORY/foreign_p2" "foreign_p2" "$engine" "UNIV-BENCH-OWL2DL-seed1-universities1.owl" "$detail" "$w2"
    write_case "$HISTORY/foreign_p3" "foreign_p3" "$engine" "UNIV-BENCH-OWL2DL-seed1-universities1.owl" "$detail" "$w3"
done <"$FOREIGN_SPEC"
FOREIGN_AGG="$TMP_DIR/aggregate_foreign"
AGGREGATION_OUTPUT_DIR="$FOREIGN_AGG" bash "$AGGREGATION" \
    "$HISTORY/foreign_p1" "$HISTORY/foreign_p2" "$HISTORY/foreign_p3" >/dev/null
assert_command_fails_with "non-ladder run ids must fail closed naming them" \
    "run id shape.*foreign_p1" \
    run_view bash "$LADDER_VIEW" "$FOREIGN_AGG"

# Two passes only: the ladder point must aggregate exactly passes 1, 2, 3.
TWO_PASS_AGG="$TMP_DIR/aggregate_two_pass"
AGGREGATION_OUTPUT_DIR="$TWO_PASS_AGG" bash "$AGGREGATION" \
    "$HISTORY/owl2bench_cmp_p1_dl_u1" "$HISTORY/owl2bench_cmp_p2_dl_u1" >/dev/null
assert_command_fails_with "two-pass point must fail closed" \
    "must aggregate exactly passes 1,2,3" \
    run_view bash "$LADDER_VIEW" "$TWO_PASS_AGG"

# Wrong engine set for the profile (openllet instead of konclude on DL).
WRONG_ENGINES_AGG="$TMP_DIR/aggregate_wrong_engines"
mkdir -p "$WRONG_ENGINES_AGG"
sed 's/^konclude\t/openllet\t/' "$DL1_AGG/comparative_engine_summary.tsv" \
    > "$WRONG_ENGINES_AGG/comparative_engine_summary.tsv"
sed 's/\tkonclude\t/\topenllet\t/' "$DL1_AGG/comparative_median_times.tsv" \
    > "$WRONG_ENGINES_AGG/comparative_median_times.tsv"
cp "$DL1_AGG/aggregation_metadata.json" "$WRONG_ENGINES_AGG/"
assert_command_fails_with "engine set drift must fail closed naming the point" \
    "DL:1.*do not match.*tableauxx, hermit, konclude" \
    run_view bash "$LADDER_VIEW" "$WRONG_ENGINES_AGG"

# Lane metadata files dropped by ladder_run.sh into sweep pass directories
# must stay invisible to the comparative aggregation: summaries are
# byte-identical with and without them.
LANE_META_AGG="$TMP_DIR/aggregate_with_lane_metadata"
for pass in 1 2 3; do
    cp -r "$HISTORY/owl2bench_cmp_p${pass}_dl_u1" "$HISTORY/seam_p${pass}_dl_u1"
    jq -n --arg run_id "owl2bench_cmp_p${pass}_dl_u1" \
        '{lane: "owl2bench_consistency_scalability", run_id: $run_id}' \
        > "$HISTORY/seam_p${pass}_dl_u1/owl2bench_ladder_metadata.json"
done
AGGREGATION_OUTPUT_DIR="$LANE_META_AGG" bash "$AGGREGATION" \
    "$HISTORY/seam_p1_dl_u1" "$HISTORY/seam_p2_dl_u1" "$HISTORY/seam_p3_dl_u1" >/dev/null
cmp "$DL1_AGG/comparative_engine_summary.tsv" "$LANE_META_AGG/comparative_engine_summary.tsv"
cmp "$DL1_AGG/comparative_median_times.tsv" "$LANE_META_AGG/comparative_median_times.tsv"

# ---------------------------------------------------------------------------
# Rendering: per-profile ladder tables from the four fixture points.
# ---------------------------------------------------------------------------

LADDER_OUT="$OUT_DIR.full"
OWL2BENCH_LADDER_OUTPUT_DIR="$LADDER_OUT" \
OWL2BENCH_LADDER_PROFILES="DL,EL" \
OWL2BENCH_LADDER_SIZES="1,2" \
    bash "$LADDER_VIEW" "$DL1_AGG" "$DL2_AGG" "$EL1_AGG" "$EL2_AGG" \
    >"$TMP_DIR/ladder_view.log"

DL_MD="$LADDER_OUT/ladder_dl.md"
EL_MD="$LADDER_OUT/ladder_el.md"
LADDER_META="$LADDER_OUT/ladder_metadata.json"

assert_file_contains "$DL_MD" $'^\| Universities \| tableauxx \| hermit \| konclude \|$'
assert_file_contains "$EL_MD" $'^\| Universities \| tableauxx \| hermit \| konclude \| elk \|$'
assert_file_contains "$DL_MD" $'^\| 1 \| \*\*2100ms \(1/1\)\*\* \| 9000ms \(1/1\) \| 4000ms \(1/1\) \|$'
assert_file_contains "$DL_MD" $'^\| 2 \| 5200ms \(1/1\) \| TO \(0/1\) \| \*\*4900ms \(1/1\)\*\* \|$'
assert_file_contains "$EL_MD" $'^\| 1 \| 800ms \(1/1\) \| NA \(0/1\) \| TO \(0/1\) \| \*\*600ms \(1/1\)\*\* \|$'
assert_file_contains "$EL_MD" $'^\| 2 \| FAIL \(0/1\) \| 7000ms \(1/1\) \| \*\*6600ms \(1/1\)\*\* \| 12100ms \(1/1\) \|$'

# Sizes ascend within each table.
assert_line_before "$DL_MD" '^\| 1 \|' '^\| 2 \|'
assert_line_before "$EL_MD" '^\| 1 \|' '^\| 2 \|'

# Legend consistent with the paper table.
for md in "$DL_MD" "$EL_MD"; do
    assert_file_contains "$md" 'TO=timeout'
    assert_file_contains "$md" 'NA=not available'
    assert_file_contains "$md" 'FAIL=hard runtime failure'
    assert_file_contains "$md" 'NC=non-comparable'
    assert_file_contains "$md" 'completed/cases'
    assert_file_contains "$md" 'execution performance and completability evidence only'
done

# Metadata records the input run ids per ladder point.
jq -e \
    --arg dl1 "$DL1_AGG" \
    '
    .ladder_type == "owl2bench_consistency_scalability"
    and (.expected.profiles == ["DL", "EL"])
    and (.expected.sizes == [1, 2])
    and (.profiles.DL.engines == ["tableauxx", "hermit", "konclude"])
    and (.profiles.EL.engines == ["tableauxx", "hermit", "konclude", "elk"])
    and (.profiles.DL.table == "ladder_dl.md")
    and (.profiles.DL.points[0].universities == 1)
    and (.profiles.DL.points[0].aggregation_dir == $dl1)
    and (.profiles.DL.points[0].pass_run_ids
         == ["owl2bench_cmp_p1_dl_u1", "owl2bench_cmp_p2_dl_u1", "owl2bench_cmp_p3_dl_u1"])
    and (.profiles.EL.points[1].universities == 2)
    and (.profiles.EL.points[1].pass_run_ids
         == ["owl2bench_cmp_p1_el_u2", "owl2bench_cmp_p2_el_u2", "owl2bench_cmp_p3_el_u2"])
    ' "$LADDER_META" >/dev/null

# Performance-lane wording only: no correctness vocabulary, no gate.
for artifact in "$DL_MD" "$EL_MD" "$LADDER_META"; do
    assert_file_not_contains "$artifact" 'oracle|passed|wrong|correctness|\bgate\b'
done

echo "owl2bench ladder view tests passed"
