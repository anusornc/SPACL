#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/../../.." && pwd)"
source "$SCRIPT_DIR/benchmark_result.sh"
source "$SCRIPT_DIR/run_benchmarks.sh"

TEST_DIR="$(mktemp -d)"
STAGE_RUN_ID="stage-contract-$$-$(date +%s%N)"
STAGE_DIR="$ROOT_DIR/benchmarks/competitors/results/history/$STAGE_RUN_ID"
trap 'rm -rf "$TEST_DIR"; if [[ "$STAGE_DIR" == "$ROOT_DIR/benchmarks/competitors/results/history/stage-contract-"* ]]; then rm -rf "$STAGE_DIR"; fi' EXIT

assert_eq() {
    local expected="$1"
    local actual="$2"
    local message="$3"
    if [[ "$expected" != "$actual" ]]; then
        printf 'assertion failed: %s (expected=%s actual=%s)\n' "$message" "$expected" "$actual" >&2
        exit 1
    fi
}

assert_fails() {
    local message="$1"
    shift
    if "$@" >/dev/null 2>&1; then
        printf 'assertion failed: %s\n' "$message" >&2
        exit 1
    fi
}

write_output() {
    local name="$1"
    local body="$2"
    printf '%s\n' "$body" > "$TEST_DIR/$name.out"
    printf '%s\n' "$TEST_DIR/$name.out"
}

write_output_no_lf() {
    local name="$1"
    local body="$2"
    printf '%s' "$body" > "$TEST_DIR/$name.out"
    printf '%s\n' "$TEST_DIR/$name.out"
}

valid_success='[bench] diagnostic line
{"schema_version":1,"report_type":"tableauxx_check_auto","operation":"consistency","input_file":"/ontologies/consistent.owl","status":"success","reasoning_result":"consistent","correctness_status":"not_evaluated","selected_reasoner":"SimpleReasoner","parse_time_ms":0,"reason_time_ms":3,"duration_ms":4,"error":null}'
success_output="$(write_output success "$valid_success")"
success_report="$TEST_DIR/success-report.json"
benchmark_result_extract_tableauxx_check_auto_report \
    "$success_output" "/ontologies/consistent.owl" "consistency" 0 "$success_report"

assert_eq "success" "$(jq -r '.status' "$success_report")" "valid report status is preserved"
assert_eq "consistent" "$(jq -r '.reasoning_result' "$success_report")" "valid semantic answer is preserved"
assert_eq "0" "$(jq -r '.parse_time_ms' "$success_report")" "zero parse timing is valid"

valid_success_no_lf="$(write_output_no_lf success-no-lf '{"schema_version":1,"report_type":"tableauxx_check_auto","operation":"consistency","input_file":"/ontologies/consistent.owl","status":"success","reasoning_result":"consistent","correctness_status":"not_evaluated","selected_reasoner":"SimpleReasoner","parse_time_ms":0,"reason_time_ms":0,"duration_ms":0,"error":null}')"
benchmark_result_extract_tableauxx_check_auto_report \
    "$valid_success_no_lf" "/ontologies/consistent.owl" "consistency" 0 "$TEST_DIR/success-no-lf-report.json"

false_success_output="$(write_output false-success '[bench] diagnostic line
Error: No such file or directory
{"duration_ms":7,"status":"completed","reasoning_result":"unknown","parse_time_ms":-1,"reason_time_ms":-1}')"
assert_fails "legacy completed/unknown record must not validate as Tableauxx consistency success" \
    benchmark_result_extract_tableauxx_check_auto_report \
    "$false_success_output" "/ontologies/missing.owl" "consistency" 0 "$TEST_DIR/false-success.json"

contradictory_output="$(write_output contradictory '{"schema_version":1,"report_type":"tableauxx_check_auto","operation":"consistency","input_file":"/ontologies/consistent.owl","status":"success","reasoning_result":"consistent","correctness_status":"not_evaluated","selected_reasoner":"SimpleReasoner","parse_time_ms":1,"reason_time_ms":1,"duration_ms":1,"error":null}')"
assert_fails "success report with nonzero exit must fail closed" \
    benchmark_result_extract_tableauxx_check_auto_report \
    "$contradictory_output" "/ontologies/consistent.owl" "consistency" 1 "$TEST_DIR/contradictory.json"

null_reasoner_output="$(write_output null-reasoner '{"schema_version":1,"report_type":"tableauxx_check_auto","operation":"consistency","input_file":"/ontologies/consistent.owl","status":"success","reasoning_result":"consistent","correctness_status":"not_evaluated","selected_reasoner":null,"parse_time_ms":1,"reason_time_ms":1,"duration_ms":1,"error":null}')"
assert_fails "successful report must name the routed Tableauxx engine" \
    benchmark_result_extract_tableauxx_check_auto_report \
    "$null_reasoner_output" "/ontologies/consistent.owl" "consistency" 0 "$TEST_DIR/null-reasoner.json"

unknown_reasoner_output="$(write_output unknown-reasoner '{"schema_version":1,"report_type":"tableauxx_check_auto","operation":"consistency","input_file":"/ontologies/consistent.owl","status":"success","reasoning_result":"consistent","correctness_status":"not_evaluated","selected_reasoner":"OtherReasoner","parse_time_ms":1,"reason_time_ms":1,"duration_ms":1,"error":null}')"
assert_fails "successful report must use a known Tableauxx routed engine name" \
    benchmark_result_extract_tableauxx_check_auto_report \
    "$unknown_reasoner_output" "/ontologies/consistent.owl" "consistency" 0 "$TEST_DIR/unknown-reasoner.json"

missing_error_output="$(write_output missing-error '{"schema_version":1,"report_type":"tableauxx_check_auto","operation":"consistency","input_file":"/ontologies/consistent.owl","status":"success","reasoning_result":"consistent","correctness_status":"not_evaluated","selected_reasoner":"SimpleReasoner","parse_time_ms":1,"reason_time_ms":1,"duration_ms":1}')"
assert_fails "mandatory nullable error field must be present" \
    benchmark_result_extract_tableauxx_check_auto_report \
    "$missing_error_output" "/ontologies/consistent.owl" "consistency" 0 "$TEST_DIR/missing-error.json"

unsupported_output="$(write_output unsupported '{"schema_version":1,"report_type":"tableauxx_check_auto","operation":"consistency","input_file":"/ontologies/unsupported.owl","status":"unsupported","reasoning_result":"unknown","correctness_status":"not_evaluated","selected_reasoner":"SimpleReasoner","parse_time_ms":5,"reason_time_ms":null,"duration_ms":8,"error":"unsupported reasoning"}')"
unsupported_report="$TEST_DIR/unsupported-report.json"
benchmark_result_extract_tableauxx_check_auto_report \
    "$unsupported_output" "/ontologies/unsupported.owl" "consistency" 2 "$unsupported_report"
assert_eq "unsupported" "$(jq -r '.status' "$unsupported_report")" "unsupported is preserved distinctly"

parse_failure_output="$(write_output parse-failure '{"schema_version":1,"report_type":"tableauxx_check_auto","operation":"consistency","input_file":"/ontologies/malformed.owl","status":"failed","reasoning_result":"error","correctness_status":"not_evaluated","selected_reasoner":null,"parse_time_ms":null,"reason_time_ms":null,"duration_ms":1,"error":"load failed before routing"}')"
if ! benchmark_result_extract_tableauxx_check_auto_report \
    "$parse_failure_output" "/ontologies/malformed.owl" "consistency" 1 "$TEST_DIR/parse-failure.json"; then
    echo "valid parse failure before router selection was rejected" >&2
    exit 1
fi

unrouted_parsed_failure="$(write_output unrouted-parsed-failure "$(jq -c '.parse_time_ms = 0' "$parse_failure_output")")"
assert_fails "version1 parsed failure must identify the selected reasoner" \
    benchmark_result_extract_tableauxx_check_auto_report \
    "$unrouted_parsed_failure" "/ontologies/malformed.owl" "consistency" 1 "$TEST_DIR/unrouted-parsed-failure.json"

unsupported_nulls_output="$(write_output unsupported-nulls '{"schema_version":1,"report_type":"tableauxx_check_auto","operation":"consistency","input_file":"/ontologies/unsupported.owl","status":"unsupported","reasoning_result":"unknown","correctness_status":"not_evaluated","selected_reasoner":"SimpleReasoner","parse_time_ms":null,"reason_time_ms":null,"duration_ms":8,"error":null}')"
assert_fails "unsupported report must retain successful parse and nonempty error" \
    benchmark_result_extract_tableauxx_check_auto_report \
    "$unsupported_nulls_output" "/ontologies/unsupported.owl" "consistency" 2 "$TEST_DIR/unsupported-nulls.json"

timeout_output="$(write_output timeout '{"schema_version":1,"report_type":"tableauxx_check_auto","operation":"consistency","input_file":"/ontologies/timeout.owl","status":"timeout","reasoning_result":"unknown","correctness_status":"not_evaluated","selected_reasoner":"SpeculativeTableauxReasoner","parse_time_ms":6,"reason_time_ms":null,"duration_ms":124000,"error":"timed out"}')"
benchmark_result_extract_tableauxx_check_auto_report \
    "$timeout_output" "/ontologies/timeout.owl" "consistency" 124 "$TEST_DIR/timeout-report.json"

bad_identity_type_output="$(write_output bad-identity-type '{"schema_version":1,"report_type":"tableauxx_check_auto","operation":"consistency","input_file":123,"status":"success","reasoning_result":"consistent","correctness_status":"not_evaluated","selected_reasoner":"SimpleReasoner","parse_time_ms":1,"reason_time_ms":1,"duration_ms":1,"error":null}')"
assert_fails "input identity must be a string matching the mounted input" \
    benchmark_result_extract_tableauxx_check_auto_report \
    "$bad_identity_type_output" "/ontologies/consistent.owl" "consistency" 0 "$TEST_DIR/bad-identity-type.json"

identity_dir="$TEST_DIR/input-identity"
mkdir -p "$identity_dir"
printf '<rdf:RDF></rdf:RDF>\n' > "$identity_dir/case.owl"
printf 'binary-cache\n' > "$identity_dir/case.owlbin"
OWL2_REASONER_FORCE_TEXT=0
OWL2_REASONER_BIN_ONLY=0
OWL2_REASONER_LARGE_PROFILE_AUTO=0
assert_eq "/ontologies/case.owlbin" \
    "$(tableauxx_expected_check_auto_input "$identity_dir" "case.owl")" \
    "host report identity follows selected binary-cache input"
OWL2_REASONER_FORCE_TEXT=1
assert_eq "/ontologies/case.owl" \
    "$(tableauxx_expected_check_auto_input "$identity_dir" "case.owl")" \
    "force-text host report identity stays on mounted text input"
unset OWL2_REASONER_FORCE_TEXT OWL2_REASONER_BIN_ONLY OWL2_REASONER_LARGE_PROFILE_AUTO

assert_eq "/ontologies/case.owlbin" \
    "$(OWL2_REASONER_BIN_ONLY=1 OWL2_REASONER_FORCE_TEXT=1 tableauxx_expected_check_auto_input "$identity_dir" case.owl)" \
    "explicit binary-only selection precedes force-text at the wrapper boundary"
assert_eq "/ontologies/case.owl" \
    "$(OWL2_REASONER_LARGE_PROFILE_AUTO=1 OWL2_REASONER_LARGE_PROFILE_THRESHOLD=1 tableauxx_expected_check_auto_input "$identity_dir" case.owl)" \
    "large RDFXML auto-profile chooses source text despite an existing cache"
assert_eq "/ontologies/case.owlbin" \
    "$(OWL2_REASONER_LARGE_PROFILE_AUTO=1 OWL2_REASONER_LARGE_PROFILE_THRESHOLD=999999 tableauxx_expected_check_auto_input "$identity_dir" case.owl)" \
    "small-file auto-profile keeps the existing binary cache"
assert_eq "/ontologies/case.owlbin" \
    "$(OWL2_REASONER_LARGE_PROFILE_THRESHOLD=invalid tableauxx_expected_check_auto_input "$identity_dir" case.owl)" \
    "invalid profile threshold uses the existing large-file default"
printf 'not an RDFXML signature\n' > "$identity_dir/nonxml.owl"
printf 'binary-cache\n' > "$identity_dir/nonxml.owlbin"
assert_eq "/ontologies/nonxml.owlbin" \
    "$(OWL2_REASONER_LARGE_PROFILE_AUTO=1 OWL2_REASONER_LARGE_PROFILE_THRESHOLD=1 tableauxx_expected_check_auto_input "$identity_dir" nonxml.owl)" \
    "non-RDFXML input keeps its existing binary cache"
printf '<rdf:RDF></rdf:RDF>\n' > "$identity_dir/uncached.owl"
assert_eq "/ontologies/uncached.owl" \
    "$(tableauxx_expected_check_auto_input "$identity_dir" uncached.owl)" \
    "source text is selected when no binary cache exists"
assert_eq "/ontologies/case.owlbin" \
    "$(tableauxx_expected_check_auto_input "$identity_dir" case.owlbin)" \
    "an explicitly supplied binary path is not rewritten"

duplicate_output="$(write_output duplicate '{"schema_version":1,"report_type":"tableauxx_check_auto","operation":"consistency","input_file":"/ontologies/consistent.owl","status":"success","reasoning_result":"consistent","correctness_status":"not_evaluated","selected_reasoner":"SimpleReasoner","parse_time_ms":1,"reason_time_ms":1,"duration_ms":1,"error":null}
{"schema_version":1,"report_type":"tableauxx_check_auto","operation":"consistency","input_file":"/ontologies/consistent.owl","status":"success","reasoning_result":"consistent","correctness_status":"not_evaluated","selected_reasoner":"SimpleReasoner","parse_time_ms":1,"reason_time_ms":1,"duration_ms":1,"error":null}')"
assert_fails "duplicate terminal reports must fail closed" \
    benchmark_result_extract_tableauxx_check_auto_report \
    "$duplicate_output" "/ontologies/consistent.owl" "consistency" 0 "$TEST_DIR/duplicate.json"

same_line_duplicate_output="$(write_output same-line-duplicate '{"schema_version":1,"report_type":"tableauxx_check_auto","operation":"consistency","input_file":"/ontologies/consistent.owl","status":"success","reasoning_result":"consistent","correctness_status":"not_evaluated","selected_reasoner":"SimpleReasoner","parse_time_ms":1,"reason_time_ms":1,"duration_ms":1,"error":null}{"schema_version":1,"report_type":"tableauxx_check_auto","operation":"consistency","input_file":"/ontologies/consistent.owl","status":"success","reasoning_result":"consistent","correctness_status":"not_evaluated","selected_reasoner":"SimpleReasoner","parse_time_ms":1,"reason_time_ms":1,"duration_ms":1,"error":null}')"
assert_fails "two JSON objects on one terminal line must fail closed" \
    benchmark_result_extract_tableauxx_check_auto_report \
    "$same_line_duplicate_output" "/ontologies/consistent.owl" "consistency" 0 "$TEST_DIR/same-line-duplicate.json"

trailing_output="$(write_output trailing '{"schema_version":1,"report_type":"tableauxx_check_auto","operation":"consistency","input_file":"/ontologies/consistent.owl","status":"success","reasoning_result":"consistent","correctness_status":"not_evaluated","selected_reasoner":"SimpleReasoner","parse_time_ms":1,"reason_time_ms":1,"duration_ms":1,"error":null}
later malformed line')"
assert_fails "trailing output after a report must fail closed" \
    benchmark_result_extract_tableauxx_check_auto_report \
    "$trailing_output" "/ontologies/consistent.owl" "consistency" 0 "$TEST_DIR/trailing.json"

empty_output="$(write_output empty '')"
assert_fails "empty output must fail closed" \
    benchmark_result_extract_tableauxx_check_auto_report \
    "$empty_output" "/ontologies/consistent.owl" "consistency" 0 "$TEST_DIR/empty.json"

truncated_output="$(write_output_no_lf truncated '{"schema_version":1,"report_type":"tableauxx_check_auto","operation":"consistency"')"
assert_fails "truncated terminal JSON must fail closed" \
    benchmark_result_extract_tableauxx_check_auto_report \
    "$truncated_output" "/ontologies/consistent.owl" "consistency" 0 "$TEST_DIR/truncated.json"

mkdir -p "$STAGE_DIR"
stage_output="$(write_output stage 'synthetic stage output')"
write_benchmark_result \
    "$STAGE_DIR/tableauxx_consistent.json" \
    "$STAGE_RUN_ID" "contract" "contract:consistent.owl:consistency" \
    "tableauxx" "consistent.owl" "consistency" \
    "success" "consistent" "not_evaluated" "not_applicable" "" "" "" \
    11 9 0 0 "$stage_output"

write_benchmark_result \
    "$STAGE_DIR/tableauxx_unknown.json" \
    "$STAGE_RUN_ID" "contract" "contract:unknown.owl:consistency" \
    "tableauxx" "unknown.owl" "consistency" \
    "success" "unknown" "not_evaluated" "not_applicable" "" "" "" \
    11 9 -1 -1 "$stage_output"

write_benchmark_result \
    "$STAGE_DIR/tableauxx_unsupported.json" \
    "$STAGE_RUN_ID" "contract" "contract:unsupported.owl:consistency" \
    "tableauxx" "unsupported.owl" "consistency" \
    "unsupported" "unknown" "not_evaluated" "not_applicable" "" "" "" \
    11 9 0 -1 "$stage_output"

cp "$STAGE_DIR/tableauxx_consistent.json" "$STAGE_DIR/tableauxx_inconsistentstatus.json"
jq '.execution_status = "failed"' "$STAGE_DIR/tableauxx_inconsistentstatus.json" > "$STAGE_DIR/tableauxx_inconsistentstatus.tmp"
mv "$STAGE_DIR/tableauxx_inconsistentstatus.tmp" "$STAGE_DIR/tableauxx_inconsistentstatus.json"

jq '.status = "failed"' "$STAGE_DIR/tableauxx_consistent.json" > "$STAGE_DIR/tableauxx_reverseconflict.json"
jq '.reasoning_result = "unknown"' "$STAGE_DIR/tableauxx_consistent.json" > "$STAGE_DIR/tableauxx_verdictconflict.json"

cp "$STAGE_DIR/tableauxx_consistent.json" "$STAGE_DIR/tableauxx_multiobject.json"
cat "$STAGE_DIR/tableauxx_consistent.json" >> "$STAGE_DIR/tableauxx_multiobject.json"

RUN_ID="$STAGE_RUN_ID" "$SCRIPT_DIR/run_stage_benchmark.sh" --validate-result consistent.owl >/dev/null
assert_fails "stage gate must reject success/unknown/-1 timing rows" \
    env RUN_ID="$STAGE_RUN_ID" "$SCRIPT_DIR/run_stage_benchmark.sh" --validate-result unknown.owl
assert_fails "stage gate must reject unsupported rows" \
    env RUN_ID="$STAGE_RUN_ID" "$SCRIPT_DIR/run_stage_benchmark.sh" --validate-result unsupported.owl
assert_fails "stage gate must reject multi-object result files" \
    env RUN_ID="$STAGE_RUN_ID" "$SCRIPT_DIR/run_stage_benchmark.sh" --validate-result multiobject.owl
assert_fails "stage gate must reject inconsistent status fields" \
    env RUN_ID="$STAGE_RUN_ID" "$SCRIPT_DIR/run_stage_benchmark.sh" --validate-result inconsistentstatus.owl
assert_fails "stage gate must not let execution_status hide a conflicting status" \
    env RUN_ID="$STAGE_RUN_ID" "$SCRIPT_DIR/run_stage_benchmark.sh" --validate-result reverseconflict.owl
assert_fails "stage gate must not let semantic_result hide a conflicting reasoning_result" \
    env RUN_ID="$STAGE_RUN_ID" "$SCRIPT_DIR/run_stage_benchmark.sh" --validate-result verdictconflict.owl
assert_fails "stage gate must reject missing result files" \
    env RUN_ID="$STAGE_RUN_ID" "$SCRIPT_DIR/run_stage_benchmark.sh" --validate-result missing.owl

fakebin="$TEST_DIR/fakebin"
mkdir -p "$fakebin"
cat > "$fakebin/docker" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
state="${FAKE_DOCKER_STATE:?}"
case "${1:-}" in
  rm)
    exit 0
    ;;
  run)
    name=""
    while [[ $# -gt 0 ]]; do
      if [[ "$1" == "--name" ]]; then
        name="$2"
        shift 2
        continue
      fi
      shift
    done
    printf '%s\n' "$name" > "$state/name"
    exit 0
    ;;
  wait)
    cat "$state/exit"
    exit 0
    ;;
  logs)
    cat "$state/logs"
    exit 0
    ;;
  *)
    exit 0
    ;;
esac
EOF
chmod +x "$fakebin/docker"

cat > "$fakebin/timeout" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
if [[ "${FAKE_TIMEOUT_STATUS:-0}" != "0" ]]; then
  exit "$FAKE_TIMEOUT_STATUS"
fi
shift
if [[ "${1:-}" == --kill-after=* ]]; then
  shift
fi
shift
"$@"
EOF
chmod +x "$fakebin/timeout"

run_fake_tableauxx() {
    local case_name="$1"
    local cli_exit="$2"
    local logs="$3"
    local timeout_status="${4:-0}"
    local fake_state="$TEST_DIR/fake-docker-$case_name"
    mkdir -p "$fake_state"
    printf '%s\n' "$cli_exit" > "$fake_state/exit"
    printf '%s\n' "$logs" > "$fake_state/logs"
    RUN_ID="fake-$case_name"
    RESULTS_DIR="$TEST_DIR/fake-results-$case_name"
    RUN_DIR="$RESULTS_DIR/history/$RUN_ID"
    RUN_ONTOLOGY_DIR="$TEST_DIR/fake-ontologies-$case_name"
    ONTOLOGY_SUITE="contract"
    OPERATION="consistency"
    TIMEOUT_SECONDS=30
    CORRECTNESS_STATUS="not_evaluated"
    ORACLE_STATUS="not_applicable"
    ORACLE_SOURCE=""
    EXPECTED_SEMANTIC_RESULT=""
    ORACLE_CASE_ID=""
    mkdir -p "$RUN_DIR" "$RUN_ONTOLOGY_DIR"
    printf '<rdf:RDF></rdf:RDF>\n' > "$RUN_ONTOLOGY_DIR/$case_name.owl"
    REASONER_BUILD_STATUS[tableauxx]="ready"
    PATH="$fakebin:$PATH" FAKE_DOCKER_STATE="$fake_state" FAKE_TIMEOUT_STATUS="$timeout_status" \
        run_single_benchmark tableauxx "$RUN_ONTOLOGY_DIR/$case_name.owl" consistency >/dev/null 2>&1
}

run_fake_tableauxx fakelegacy 0 '[bench] diagnostic
{"duration_ms":7,"status":"completed","reasoning_result":"unknown","parse_time_ms":-1,"reason_time_ms":-1}'
assert_eq "failed" "$(jq -r '.execution_status' "$TEST_DIR/fake-results-fakelegacy/history/fake-fakelegacy/tableauxx_fakelegacy.json")" \
    "run_single_benchmark rejects old false-success output"

run_fake_tableauxx faketimeout 0 '{"schema_version":1,"report_type":"tableauxx_check_auto","operation":"consistency"' 124
assert_eq "timeout" "$(jq -r '.execution_status' "$TEST_DIR/fake-results-faketimeout/history/fake-faketimeout/tableauxx_faketimeout.json")" \
    "outer timeout wins over missing application report"

run_fake_tableauxx fakeunsupported 2 '{"schema_version":1,"report_type":"tableauxx_check_auto","operation":"consistency","input_file":"/ontologies/fakeunsupported.owl","status":"unsupported","reasoning_result":"unknown","correctness_status":"not_evaluated","selected_reasoner":"SimpleReasoner","parse_time_ms":0,"reason_time_ms":null,"duration_ms":1,"error":"unsupported reasoning"}'
assert_eq "unsupported" "$(jq -r '.execution_status' "$TEST_DIR/fake-results-fakeunsupported/history/fake-fakeunsupported/tableauxx_fakeunsupported.json")" \
    "run_single_benchmark preserves unsupported before generic nonzero failure"

run_fake_tableauxx inner_timeout 124 'missing application result'
assert_eq "timeout" "$(jq -r '.execution_status' "$TEST_DIR/fake-results-inner_timeout/history/fake-inner_timeout/tableauxx_inner_timeout.json")" \
    "timeout process exit must remain timeout when its report is unavailable"
assert_eq "unknown" "$(jq -r '.semantic_result' "$TEST_DIR/fake-results-inner_timeout/history/fake-inner_timeout/tableauxx_inner_timeout.json")" \
    "a timeout without an application answer has unknown semantics"

run_fake_tableauxx waitfailure 2 '{"schema_version":1,"report_type":"tableauxx_check_auto","operation":"consistency","input_file":"/ontologies/waitfailure.owl","status":"unsupported","reasoning_result":"unknown","correctness_status":"not_evaluated","selected_reasoner":"SimpleReasoner","parse_time_ms":0,"reason_time_ms":null,"duration_ms":1,"error":"unsupported reasoning"}' 1
assert_eq "failed" "$(jq -r '.execution_status' "$TEST_DIR/fake-results-waitfailure/history/fake-waitfailure/tableauxx_waitfailure.json")" \
    "application output cannot hide an outer wait failure"

echo "tableauxx status integrity contract tests passed"
