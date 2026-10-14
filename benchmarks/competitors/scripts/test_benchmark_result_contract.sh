#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/benchmark_result.sh"

TEST_DIR="$(mktemp -d)"
trap 'rm -rf "$TEST_DIR"' EXIT

assert_eq() {
    local expected="$1"
    local actual="$2"
    local message="$3"
    if [[ "$expected" != "$actual" ]]; then
        printf 'assertion failed: %s (expected=%s actual=%s)\n' "$message" "$expected" "$actual" >&2
        exit 1
    fi
}

write_output() {
    local name="$1"
    local contents="$2"
    printf '%s\n' "$contents" > "$TEST_DIR/$name.output"
    printf '%s\n' "$TEST_DIR/$name.output"
}

matched_output="$(write_output matched '{"duration_ms":7,"status":"completed","reasoning_result":"true"}')"
write_benchmark_result \
    "$TEST_DIR/matched.json" \
    "run-1" "w3c" "case-1" "tableauxx" "case.owl" "entailment" \
    "success" "true" "passed" "available" "w3c-approved" "case-1" "true" \
    10 7 -1 -1 "$matched_output"

assert_eq "passed" "$(jq -r '.result_status' "$TEST_DIR/matched.json")" "matching oracle result passes"
assert_eq "success" "$(jq -r '.execution_status' "$TEST_DIR/matched.json")" "execution status is preserved"
assert_eq "success" "$(jq -r '.execution_detail' "$TEST_DIR/matched.json")" "execution detail is preserved"
assert_eq "passed" "$(jq -r '.correctness_status' "$TEST_DIR/matched.json")" "correctness status is explicit"
assert_eq "w3c-approved" "$(jq -r '.oracle.source' "$TEST_DIR/matched.json")" "oracle provenance is preserved"
assert_eq "case-1" "$(jq -r '.case_id' "$TEST_DIR/matched.json")" "case identity is preserved"

mismatch_output="$(write_output mismatch '{"duration_ms":7,"status":"completed","reasoning_result":"true"}')"
write_benchmark_result \
    "$TEST_DIR/mismatch.json" \
    "run-1" "w3c" "case-mismatch" "tableauxx" "case.owl" "entailment" \
    "success" "true" "passed" "available" "w3c-approved" "case-mismatch" "false" \
    10 7 -1 -1 "$mismatch_output"

assert_eq "wrong" "$(jq -r '.result_status' "$TEST_DIR/mismatch.json")" "semantic mismatch overrides a caller-supplied pass"
assert_eq "wrong" "$(jq -r '.correctness_status' "$TEST_DIR/mismatch.json")" "semantic mismatch is classified as wrong"

unsupported_with_oracle_output="$(write_output unsupported-with-oracle '{"duration_ms":7,"status":"unsupported","reasoning_result":"unknown"}')"
write_benchmark_result \
    "$TEST_DIR/unsupported-with-oracle.json" \
    "run-1" "w3c" "case-unsupported" "tableauxx" "case.owl" "entailment" \
    "unsupported" "unknown" "unsupported" "available" "w3c-approved" "case-unsupported" "true" \
    10 7 -1 -1 "$unsupported_with_oracle_output"

assert_eq "unsupported" "$(jq -r '.result_status' "$TEST_DIR/unsupported-with-oracle.json")" "unsupported execution remains non-passing"
assert_eq "not_evaluated" "$(jq -r '.correctness_status' "$TEST_DIR/unsupported-with-oracle.json")" "unsupported execution does not claim a semantic verdict"
if validate_benchmark_result "$TEST_DIR/unsupported-with-oracle.json"; then
    :
else
    echo "assertion failed: unsupported result with available oracle should validate" >&2
    exit 1
fi

failed_with_oracle_output="$(write_output failed-with-oracle 'parser failed before a semantic verdict')"
write_benchmark_result \
    "$TEST_DIR/failed-with-oracle.json" \
    "run-1" "w3c" "case-failed" "tableauxx" "case.owl" "consistency" \
    "failed" "error" "passed" "available" "w3c-approved" "case-failed" "consistent" \
    10 -1 -1 -1 "$failed_with_oracle_output"

assert_eq "failed" "$(jq -r '.result_status' "$TEST_DIR/failed-with-oracle.json")" "failed execution remains failed"
assert_eq "not_evaluated" "$(jq -r '.correctness_status' "$TEST_DIR/failed-with-oracle.json")" "failed execution does not claim a semantic mismatch"
if validate_benchmark_result "$TEST_DIR/failed-with-oracle.json"; then
    :
else
    echo "assertion failed: failed result with available oracle should validate" >&2
    exit 1
fi

unavailable_output="$(write_output unavailable '{"duration_ms":8,"status":"completed","reasoning_result":"true"}')"
write_benchmark_result \
    "$TEST_DIR/unavailable.json" \
    "run-1" "head-to-head" "case-2" "tableauxx" "case.owl" "consistency" \
    "success" "true" "not_evaluated" "not_applicable" "" "" "" \
    11 8 -1 -1 "$unavailable_output"

assert_eq "not_evaluated" "$(jq -r '.result_status' "$TEST_DIR/unavailable.json")" "performance-only result is not a correctness pass"
assert_eq "not_applicable" "$(jq -r '.oracle.status' "$TEST_DIR/unavailable.json")" "non-conformance oracle state is explicit"

malformed_output="$(write_output malformed '{"duration_ms":9,"status":"completed","reasoning_result":"true"}')"
write_benchmark_result \
    "$TEST_DIR/malformed.json" \
    "run-1" "w3c" "case-3" "tableauxx" "case.owl" "entailment" \
    "success" "true" "passed" "available" "" "case-3" "" \
    12 9 -1 -1 "$malformed_output"

assert_eq "oracle_unavailable" "$(jq -r '.result_status' "$TEST_DIR/malformed.json")" "malformed oracle metadata cannot pass"
assert_eq "malformed" "$(jq -r '.oracle.status' "$TEST_DIR/malformed.json")" "malformed oracle metadata is classified"

non_comparable_output="$(write_output non-comparable 'parser format mismatch')"
write_benchmark_result \
    "$TEST_DIR/non-comparable.json" \
    "run-1" "head-to-head" "case-4" "tableauxx" "case.owl" "consistency" \
    "non_comparable" "unknown" "not_evaluated" "not_applicable" "" "" "" \
    -1 -1 -1 -1 "$non_comparable_output"

assert_eq "not_available" "$(jq -r '.execution_status' "$TEST_DIR/non-comparable.json")" "high-level unavailable status is claim-safe"
assert_eq "non_comparable" "$(jq -r '.execution_detail' "$TEST_DIR/non-comparable.json")" "infrastructure cause remains visible"

if validate_benchmark_result "$TEST_DIR/malformed.json"; then
    echo "assertion failed: malformed result should fail validation" >&2
    exit 1
fi

mkdir -p "$TEST_DIR/valid-results"
cp "$TEST_DIR/matched.json" "$TEST_DIR/valid-results/"
cp "$TEST_DIR/unavailable.json" "$TEST_DIR/valid-results/"

if validate_benchmark_result_count "$TEST_DIR/valid-results" 2; then
    :
else
    echo "assertion failed: result count should match" >&2
    exit 1
fi

if validate_benchmark_result_count "$TEST_DIR/valid-results" 1; then
    echo "assertion failed: incorrect result count should fail" >&2
    exit 1
fi

echo "benchmark result contract tests passed"
