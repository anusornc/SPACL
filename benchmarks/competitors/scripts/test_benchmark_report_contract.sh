#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/run_benchmarks.sh"

TEST_DIR="$(mktemp -d)"
trap 'rm -rf "$TEST_DIR"' EXIT

RUN_ID="contract-report"
RESULTS_DIR="$TEST_DIR/results"
RUN_DIR="$RESULTS_DIR/history/$RUN_ID"
RUN_REPORT_FILE="$RUN_DIR/benchmark_report.md"
RUN_CSV_FILE="$RUN_DIR/results.csv"
RUN_PAPER_MD="$RUN_DIR/paper_table.md"
RUN_PAPER_TEX="$RUN_DIR/paper_table.tex"
LATEST_LINK="$RESULTS_DIR/latest"
ONTOLOGY_SUITE="contract"
OPERATION="consistency"
TIMEOUT_SECONDS=30
REASONERS=("tableauxx")

mkdir -p "$RUN_DIR"
printf '%s\n' 'case.owl' 'passed.owl' > "$RUN_DIR/ontology_manifest.txt"
output_file="$TEST_DIR/output.txt"
printf '%s\n' 'synthetic benchmark output' > "$output_file"

assert_eq() {
    local expected="$1"
    local actual="$2"
    local message="$3"
    if [[ "$expected" != "$actual" ]]; then
        printf 'assertion failed: %s (expected=%s actual=%s)\n' "$message" "$expected" "$actual" >&2
        exit 1
    fi
}

write_benchmark_result \
    "$RUN_DIR/tableauxx_case.json" \
    "$RUN_ID" "$ONTOLOGY_SUITE" "contract:case.owl:consistency" \
    "tableauxx" "case.owl" "$OPERATION" \
    "success" "true" "not_evaluated" "not_applicable" "" "" "" \
    10 8 -1 -1 "$output_file"

write_benchmark_result \
    "$RUN_DIR/tableauxx_passed.json" \
    "$RUN_ID" "$ONTOLOGY_SUITE" "contract:passed.owl:consistency" \
    "tableauxx" "passed.owl" "$OPERATION" \
    "success" "true" "passed" "available" "synthetic-oracle" "passed-case" "true" \
    20 18 -1 -1 "$output_file"

generate_report

grep -F '| Reasoner | Executed | Passed | Wrong | Unsupported | Oracle Unavailable | Not Evaluated |' "$RUN_REPORT_FILE" >/dev/null
grep -F '| tableauxx | 2 | 1 | 0 | 0 | 0 | 1 |' "$RUN_REPORT_FILE" >/dev/null
grep -F '| tableauxx | case.owl | contract:case.owl:consistency | 10 | 8 | success | success | not_evaluated | not_evaluated | not_applicable | true |' "$RUN_REPORT_FILE" >/dev/null
grep -F '| passed.owl | **20** |' "$RUN_PAPER_MD" >/dev/null
grep -F 'case_id,reasoner,ontology,operation,status,execution_status,execution_detail,result_status' "$RUN_CSV_FILE" >/dev/null
grep -F ',"not_evaluated","not_evaluated","not_applicable",' "$RUN_CSV_FILE" >/dev/null

# Engine provenance contract (#192, ADR 0025): run metadata must record the
# docker image digest and the engine-reported version for every executed
# engine, and validation must fail when either field is absent.

PROVENANCE_DIGEST="sha256:$(printf 'a%.0s' $(seq 1 64))"
write_run_metadata

version_probe_output="$TEST_DIR/version_probe.output"
printf '%s\n' '{"reasoner": "Tableauxx", "version": "0.2.0", "operation": "consistency", "ontology": "/ontologies/case.owl"}' > "$version_probe_output"
record_engine_version_from_output "tableauxx" "$version_probe_output"
assert_eq "0.2.0" "${ENGINE_VERSION[tableauxx]:-}" "engine version is extracted from engine output"

ENGINE_IMAGE_DIGEST[tableauxx]="$PROVENANCE_DIGEST"
ENGINE_EXECUTED[tableauxx]=1
update_engine_provenance_metadata

METADATA_FILE="$RUN_DIR/run_metadata.json"
assert_eq "$PROVENANCE_DIGEST" "$(jq -r '.engine_provenance.tableauxx.image_digest' "$METADATA_FILE")" "run metadata records the executed engine image digest"
assert_eq "0.2.0" "$(jq -r '.engine_provenance.tableauxx.engine_version' "$METADATA_FILE")" "run metadata records the executed engine version"
assert_eq "owl-reasoner-tableauxx" "$(jq -r '.engine_provenance.tableauxx.image' "$METADATA_FILE")" "run metadata records the executed engine image tag"

if validate_run_metadata_engine_provenance "$METADATA_FILE" "tableauxx"; then
    :
else
    echo "assertion failed: provenance for an executed engine should validate" >&2
    exit 1
fi

jq 'del(.engine_provenance.tableauxx.image_digest)' "$METADATA_FILE" > "$TEST_DIR/metadata-no-digest.json"
if validate_run_metadata_engine_provenance "$TEST_DIR/metadata-no-digest.json" "tableauxx"; then
    echo "assertion failed: missing image digest must fail provenance validation" >&2
    exit 1
fi

jq 'del(.engine_provenance.tableauxx.engine_version)' "$METADATA_FILE" > "$TEST_DIR/metadata-no-version.json"
if validate_run_metadata_engine_provenance "$TEST_DIR/metadata-no-version.json" "tableauxx"; then
    echo "assertion failed: missing engine version must fail provenance validation" >&2
    exit 1
fi

jq 'del(.engine_provenance)' "$METADATA_FILE" > "$TEST_DIR/metadata-no-provenance.json"
if validate_run_metadata_engine_provenance "$TEST_DIR/metadata-no-provenance.json" "tableauxx"; then
    echo "assertion failed: absent engine provenance must fail validation for executed engines" >&2
    exit 1
fi

jq '.engine_provenance.tableauxx.image_digest = "latest"' "$METADATA_FILE" > "$TEST_DIR/metadata-unpinned.json"
if validate_run_metadata_engine_provenance "$TEST_DIR/metadata-unpinned.json" "tableauxx"; then
    echo "assertion failed: unpinned image tag must fail provenance validation" >&2
    exit 1
fi

# Engines that were never executed make no provenance claim and must not fail.
if validate_run_metadata_engine_provenance "$METADATA_FILE"; then
    :
else
    echo "assertion failed: provenance validation with no executed engines should pass" >&2
    exit 1
fi

echo "benchmark report contract tests passed"
