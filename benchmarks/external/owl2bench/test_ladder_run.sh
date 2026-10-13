#!/usr/bin/env bash
set -euo pipefail

# Fixture tests for the OWL2Bench consistency scalability sweep runner
# (issue #197, ADR 0025/0026). Everything here runs against synthetic
# manifests and a stub harness; the real ladder is never executed.

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
SCRIPT_DIR="$ROOT_DIR/benchmarks/external/owl2bench"
LADDER_RUN="$SCRIPT_DIR/ladder_run.sh"

source "$ROOT_DIR/benchmarks/competitors/scripts/benchmark_result.sh"

TMP_DIR="$(mktemp -d)"
trap 'rm -rf "$TMP_DIR"' EXIT

GENERATOR_REVISION="5a27536c8e7f7621b34c61c8a57ed9eb63e7829a"
JAR_SHA256="c2bacf1d80e91ec05625b4c458f899a626ef92b85c60c6bac1f0a519060839ff"

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

sha256_hex() {
    sha256sum "$1" | cut -d' ' -f1
}

# ---------------------------------------------------------------------------
# Fixture ladder manifest: two profiles are enough to pin the per-profile
# engine sets; u1 exercises the anchor-reuse point, u2 a generated point.
# ---------------------------------------------------------------------------

DATASET_DIR="$TMP_DIR/scalability"
mkdir -p "$DATASET_DIR"

make_dataset() {
    local profile="$1"
    local universities="$2"
    local name="UNIV-BENCH-OWL2${profile}-seed1-universities${universities}.owl"
    printf '<Ontology>fixture %s u%s</Ontology>\n' "$profile" "$universities" > "$DATASET_DIR/$name"
    printf '%s\n' "$DATASET_DIR/$name"
}

make_point() {
    local profile="$1"
    local universities="$2"
    local path="$3"
    local origin="generated"
    [[ "$universities" == "1" ]] && origin="anchor-reuse"
    jq -n \
        --arg profile "$profile" \
        --argjson universities "$universities" \
        --arg path "$path" \
        --arg sha256 "sha256:$(sha256_hex "$path")" \
        --argjson bytes "$(stat -c%s "$path")" \
        --arg invocation "kracr/owl2bench@${GENERATOR_REVISION}:OWL2Bench.jar ${universities} ${profile} 1" \
        --arg revision "$GENERATOR_REVISION" \
        --arg jar_sha256 "sha256:${JAR_SHA256}" \
        --arg origin "$origin" \
        '{
            profile: $profile,
            universities: $universities,
            seed: 1,
            origin: $origin,
            invocation: $invocation,
            generator_revision: $revision,
            generator_jar_sha256: $jar_sha256,
            output: {path: $path, repo_relative_path: null, sha256: $sha256, bytes: $bytes}
        }'
}

DL1_PATH="$(make_dataset DL 1)"
DL2_PATH="$(make_dataset DL 2)"
EL1_PATH="$(make_dataset EL 1)"
EL2_PATH="$(make_dataset EL 2)"

jq -n \
    --arg revision "$GENERATOR_REVISION" \
    --arg jar_sha256 "sha256:${JAR_SHA256}" \
    --argjson points "$(jq -s . < <(make_point DL 1 "$DL1_PATH"; make_point DL 2 "$DL2_PATH"; make_point EL 1 "$EL1_PATH"; make_point EL 2 "$EL2_PATH"))" \
    '{
        schema_version: 1,
        manifest_id: "owl2bench-scalability-ladder:seed=1",
        adr: "docs/adr/0026-owl2bench-multisize-consistency-performance-lane.md",
        status: "complete",
        source_corpus: "owl2bench",
        generator: {
            name: "OWL2Bench",
            revision: $revision,
            jar_sha256: $jar_sha256,
            command: "java -jar OWL2Bench.jar <universities> <PROFILE> <seed>",
            seed: 1
        },
        points: $points,
        no_answer_keys: true,
        identity_sha256: "sha256:fixture"
    }' > "$TMP_DIR/ladder-manifest.json"

MISSING_MANIFEST="$TMP_DIR/does-not-exist/ladder-manifest.json"
HISTORY="$TMP_DIR/results/history"
mkdir -p "$HISTORY"

# A stub harness records the invocation seam without executing engines.
STUB_LOG="$TMP_DIR/harness_invocation.log"
cat > "$TMP_DIR/stub_harness.sh" <<STUB
#!/usr/bin/env bash
set -euo pipefail
echo "action=\$1" >> "$STUB_LOG"
echo "run_id=\$RUN_ID" >> "$STUB_LOG"
echo "reasoners=\$REASONERS_OVERRIDE" >> "$STUB_LOG"
echo "timeout=\$TIMEOUT_SECONDS" >> "$STUB_LOG"
echo "ontology_dir=\$ONTOLOGIES_DIR_OVERRIDE" >> "$STUB_LOG"
echo "regex=\$ONTOLOGY_REGEX" >> "$STUB_LOG"
echo "suite=\$ONTOLOGY_SUITE" >> "$STUB_LOG"
echo "operation=\$OPERATION" >> "$STUB_LOG"
echo "skip_build=\$SKIP_BUILD" >> "$STUB_LOG"
echo "include_chebi=\$INCLUDE_CHEBI" >> "$STUB_LOG"
if [[ -f "\$OWL2BENCH_LADDER_RESULTS_HISTORY/\$RUN_ID/owl2bench_ladder_metadata.json" ]]; then
    echo "metadata_present_at_invocation=1" >> "$STUB_LOG"
fi
staged_dataset="\$(find "\$ONTOLOGIES_DIR_OVERRIDE" -maxdepth 1 -type f -name '*.owl' | sort | tr '\n' ' ')"
echo "staged=\$staged_dataset" >> "$STUB_LOG"
exit 0
STUB
chmod +x "$TMP_DIR/stub_harness.sh"

# ---------------------------------------------------------------------------
# Fail-closed shape validation.
# ---------------------------------------------------------------------------

assert_command_fails_with "missing manifest must fail closed" \
    "ladder manifest is missing or unreadable" \
    env OWL2BENCH_LADDER_MANIFEST="$MISSING_MANIFEST" \
        OWL2BENCH_LADDER_RESULTS_HISTORY="$HISTORY" \
        OWL2BENCH_LADDER_RUN_BENCH="$TMP_DIR/stub_harness.sh" \
        bash "$LADDER_RUN" DL 10 1 all

printf 'not json at all\n' > "$TMP_DIR/malformed-manifest.json"
assert_command_fails_with "malformed manifest must fail closed" \
    "ladder manifest is malformed" \
    env OWL2BENCH_LADDER_MANIFEST="$TMP_DIR/malformed-manifest.json" \
        OWL2BENCH_LADDER_RESULTS_HISTORY="$HISTORY" \
        OWL2BENCH_LADDER_RUN_BENCH="$TMP_DIR/stub_harness.sh" \
        bash "$LADDER_RUN" DL 1 1 all

assert_command_fails_with "missing ladder point must fail closed" \
    "ladder manifest has no point for profile DL universities=10" \
    env OWL2BENCH_LADDER_MANIFEST="$TMP_DIR/ladder-manifest.json" \
        OWL2BENCH_LADDER_RESULTS_HISTORY="$HISTORY" \
        OWL2BENCH_LADDER_RUN_BENCH="$TMP_DIR/stub_harness.sh" \
        bash "$LADDER_RUN" DL 10 1 all

mv "$DL2_PATH" "$DL2_PATH.away"
assert_command_fails_with "missing dataset file must fail closed" \
    "dataset file.*not found" \
    env OWL2BENCH_LADDER_MANIFEST="$TMP_DIR/ladder-manifest.json" \
        OWL2BENCH_LADDER_RESULTS_HISTORY="$HISTORY" \
        OWL2BENCH_LADDER_RUN_BENCH="$TMP_DIR/stub_harness.sh" \
        bash "$LADDER_RUN" DL 2 1 all
mv "$DL2_PATH.away" "$DL2_PATH"

printf '<Ontology>drifted</Ontology>\n' > "$TMP_DIR/drifted-dataset.owl"
jq --arg path "$TMP_DIR/drifted-dataset.owl" \
    '.points[0].output.path = $path' "$TMP_DIR/ladder-manifest.json" \
    > "$TMP_DIR/drifted-manifest.json"
assert_command_fails_with "dataset hash drift must fail closed" \
    "sha256 mismatch" \
    env OWL2BENCH_LADDER_MANIFEST="$TMP_DIR/drifted-manifest.json" \
        OWL2BENCH_LADDER_RESULTS_HISTORY="$HISTORY" \
        OWL2BENCH_LADDER_RUN_BENCH="$TMP_DIR/stub_harness.sh" \
        bash "$LADDER_RUN" DL 1 1 all

assert_command_fails "unsupported profile must fail" \
    env OWL2BENCH_LADDER_MANIFEST="$TMP_DIR/ladder-manifest.json" \
        OWL2BENCH_LADDER_RESULTS_HISTORY="$HISTORY" \
        bash "$LADDER_RUN" XX 1 1 all
assert_command_fails "pass out of range must fail" \
    env OWL2BENCH_LADDER_MANIFEST="$TMP_DIR/ladder-manifest.json" \
        OWL2BENCH_LADDER_RESULTS_HISTORY="$HISTORY" \
        bash "$LADDER_RUN" DL 1 4 all
assert_command_fails "missing arguments must fail" \
    env OWL2BENCH_LADDER_MANIFEST="$TMP_DIR/ladder-manifest.json" \
        OWL2BENCH_LADDER_RESULTS_HISTORY="$HISTORY" \
        bash "$LADDER_RUN" DL 1

# ---------------------------------------------------------------------------
# Invocation shape: one size x profile x pass through the harness seam.
# ---------------------------------------------------------------------------

run_wrapper() {
    env OWL2BENCH_LADDER_MANIFEST="$TMP_DIR/ladder-manifest.json" \
        OWL2BENCH_LADDER_RESULTS_HISTORY="$HISTORY" \
        OWL2BENCH_LADDER_RUN_BENCH="$TMP_DIR/stub_harness.sh" \
        "$@"
}

: > "$STUB_LOG"
run_wrapper bash "$LADDER_RUN" dl 2 1 all

assert_file_contains "$STUB_LOG" '^action=all$'
assert_file_contains "$STUB_LOG" '^run_id=owl2bench_cmp_p1_dl_u2$'
assert_file_contains "$STUB_LOG" '^reasoners=tableauxx,hermit,konclude$'
assert_file_contains "$STUB_LOG" '^timeout=300$'
assert_file_contains "$STUB_LOG" '^suite=standard$'
assert_file_contains "$STUB_LOG" '^operation=consistency$'
assert_file_contains "$STUB_LOG" '^skip_build=1$'
assert_file_contains "$STUB_LOG" '^include_chebi=0$'
assert_file_contains "$STUB_LOG" '^regex=\^UNIV-BENCH-OWL2DL-seed1-universities2\\.owl\$$'
assert_file_contains "$STUB_LOG" '^staged=.*UNIV-BENCH-OWL2DL-seed1-universities2\.owl $'
# Provenance is written before the harness executes, and exactly one dataset
# is staged for the point.
assert_file_contains "$STUB_LOG" '^metadata_present_at_invocation=1$'
assert_eq 1 "$(grep -c '^run_id=' "$STUB_LOG" | tr -d '[:space:]')" "harness invoked exactly once"

# The EL profile adds elk to the ADR 0025 engine set.
: > "$STUB_LOG"
run_wrapper bash "$LADDER_RUN" EL 1 3 run
assert_file_contains "$STUB_LOG" '^run_id=owl2bench_cmp_p3_el_u1$'
assert_file_contains "$STUB_LOG" '^reasoners=tableauxx,hermit,konclude,elk$'
assert_file_contains "$STUB_LOG" '^staged=.*UNIV-BENCH-OWL2EL-seed1-universities1\.owl $'

# TIMEOUT_SECONDS stays overridable for operators but defaults to the
# contract value of 300.
: > "$STUB_LOG"
run_wrapper env TIMEOUT_SECONDS=60 bash "$LADDER_RUN" DL 1 2 run
assert_file_contains "$STUB_LOG" '^timeout=60$'

# ---------------------------------------------------------------------------
# Ladder run metadata: provenance and lane declaration.
# ---------------------------------------------------------------------------

META="$HISTORY/owl2bench_cmp_p1_dl_u2/owl2bench_ladder_metadata.json"
jq -e \
    --arg manifest_path "$TMP_DIR/ladder-manifest.json" \
    --arg revision "$GENERATOR_REVISION" \
    '
    .lane == "owl2bench_consistency_scalability"
    and .lane_terms == "execution performance and completability evidence only (ADR 0024/0026)"
    and .run_id == "owl2bench_cmp_p1_dl_u2"
    and .profile == "DL"
    and .universities == 2
    and .seed == 1
    and .pass == 1
    and .operation == "consistency"
    and .timeout_seconds == 300
    and (.engines == ["tableauxx", "hermit", "konclude"])
    and .harness_action == "all"
    and .dataset.filename == "UNIV-BENCH-OWL2DL-seed1-universities2.owl"
    and (.dataset.sha256 | startswith("sha256:"))
    and .dataset.bytes > 0
    and .dataset_manifest.path == $manifest_path
    and .dataset_manifest.identity_sha256 == "sha256:fixture"
    and .dataset_manifest.point_invocation == ("kracr/owl2bench@" + $revision + ":OWL2Bench.jar 2 DL 1")
    and .dataset_manifest.generator_revision == $revision
    and (.dataset_manifest.generator_jar_sha256 | startswith("sha256:"))
    and .statistic == "per-case repetition median of container-inclusive wall time across three serial passes (ADR 0025)"
    ' "$META" >/dev/null

# Performance-lane wording only: no correctness vocabulary, no gate.
assert_file_not_contains "$META" 'oracle|passed|wrong|correctness|\bgate\b'

# Reusing a run directory that already holds artifacts must fail closed.
assert_command_fails_with "run directory reuse must fail closed" \
    "run directory already exists" \
    run_wrapper bash "$LADDER_RUN" DL 2 1 all

# ---------------------------------------------------------------------------
# Dry run: resolves and records everything but never invokes the harness.
# ---------------------------------------------------------------------------

: > "$STUB_LOG"
run_wrapper env OWL2BENCH_LADDER_DRY_RUN=1 bash "$LADDER_RUN" DL 2 2 all
assert_eq 0 "$(wc -l < "$STUB_LOG" | tr -d '[:space:]')" "dry run must not invoke the harness"
run_wrapper env OWL2BENCH_LADDER_DRY_RUN=1 bash "$LADDER_RUN" DL 2 3 all > "$TMP_DIR/dry_run.log" 2>&1
assert_file_contains "$TMP_DIR/dry_run.log" 'owl2bench_cmp_p3_dl_u2'
assert_file_contains "$TMP_DIR/dry_run.log" 'tableauxx,hermit,konclude'

# ---------------------------------------------------------------------------
# The lane metadata file must stay invisible to the shared harness report
# paths: run_benchmarks.sh result scans and the benchmark result count
# contract must ignore owl2bench_ladder_metadata.json exactly as they
# ignore ore_metadata.json.
# ---------------------------------------------------------------------------

source "$ROOT_DIR/benchmarks/competitors/scripts/run_benchmarks.sh"

SEAM_RUN_ID="ladder-meta-seam"
SEAM_RESULTS_DIR="$TMP_DIR/harness_results"
SEAM_RUN_DIR="$SEAM_RESULTS_DIR/history/$SEAM_RUN_ID"
RUN_ID="$SEAM_RUN_ID"
RESULTS_DIR="$SEAM_RESULTS_DIR"
RUN_DIR="$SEAM_RUN_DIR"
RUN_REPORT_FILE="$RUN_DIR/benchmark_report.md"
RUN_CSV_FILE="$RUN_DIR/results.csv"
RUN_PAPER_MD="$RUN_DIR/paper_table.md"
RUN_PAPER_TEX="$RUN_DIR/paper_table.tex"
RUN_ONTOLOGY_DIR="$RUN_DIR/ontologies"
LATEST_LINK="$SEAM_RESULTS_DIR/latest"
ONTOLOGY_SUITE="ladderseam"
OPERATION="consistency"
TIMEOUT_SECONDS=300
REASONERS=("tableauxx" "hermit")

mkdir -p "$RUN_DIR"
printf '%s\n' 'UNIV-BENCH-OWL2DL-seed1-universities2.owl' > "$RUN_DIR/ontology_manifest.txt"
cp "$META" "$RUN_DIR/owl2bench_ladder_metadata.json"
printf '%s\n' '{"reasoner": "fixture", "version": "1.0"}' > "$TMP_DIR/engine.output"

write_benchmark_result \
    "$RUN_DIR/tableauxx_UNIV-BENCH-OWL2DL-seed1-universities2.json" \
    "$SEAM_RUN_ID" "$ONTOLOGY_SUITE" "ladderseam:UNIV-BENCH-OWL2DL-seed1-universities2.owl:consistency" \
    "tableauxx" "UNIV-BENCH-OWL2DL-seed1-universities2.owl" "consistency" \
    "success" "consistent" "not_evaluated" "not_applicable" "" "" "" \
    2100 2000 -1 -1 "$TMP_DIR/engine.output"

write_benchmark_result \
    "$RUN_DIR/hermit_UNIV-BENCH-OWL2DL-seed1-universities2.json" \
    "$SEAM_RUN_ID" "$ONTOLOGY_SUITE" "ladderseam:UNIV-BENCH-OWL2DL-seed1-universities2.owl:consistency" \
    "hermit" "UNIV-BENCH-OWL2DL-seed1-universities2.owl" "consistency" \
    "success" "consistent" "not_evaluated" "not_applicable" "" "" "" \
    9000 8900 -1 -1 "$TMP_DIR/engine.output"

generate_report >"$TMP_DIR/seam_report.log" 2>&1

assert_eq 3 "$(wc -l < "$RUN_CSV_FILE" | tr -d '[:space:]')" \
    "results.csv holds the header plus the two engine results only"
assert_file_not_contains "$RUN_CSV_FILE" '^.*"unknown".*$'
assert_file_not_contains "$RUN_REPORT_FILE" '\| null \|'
assert_file_not_contains "$RUN_PAPER_MD" '\| null \|'
assert_file_contains "$RUN_PAPER_MD" 'UNIV-BENCH-OWL2DL-seed1-universities2'

if validate_benchmark_result_count "$RUN_DIR" 2; then
    :
else
    echo "assertion failed: lane metadata must not count as a benchmark result" >&2
    exit 1
fi
if validate_benchmark_result_count "$RUN_DIR" 3; then
    echo "assertion failed: result count validation must not overcount with lane metadata present" >&2
    exit 1
fi

echo "owl2bench ladder run tests passed"
