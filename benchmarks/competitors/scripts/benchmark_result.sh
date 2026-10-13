#!/usr/bin/env bash

# Shared benchmark result contract. This file is sourced by benchmark runners
# and test scripts; it intentionally does not enable or disable shell options.

BENCHMARK_RESULT_SCHEMA_VERSION=1

# Paper-table cell code for a result or execution status. Shared by the
# harness paper tables, the comparative aggregation, and the ORE differential
# appendix so every artifact renders the same status vocabulary.
benchmark_result_status_code() {
    case "${1:-}" in
        passed) printf '%s\n' "PASS" ;;
        wrong) printf '%s\n' "WRONG" ;;
        unsupported) printf '%s\n' "UNSUP" ;;
        oracle_unavailable) printf '%s\n' "OU" ;;
        not_evaluated) printf '%s\n' "NE" ;;
        timeout) printf '%s\n' "TO" ;;
        not_available) printf '%s\n' "NA" ;;
        non_comparable) printf '%s\n' "NC" ;;
        killed) printf '%s\n' "KIL" ;;
        orphan) printf '%s\n' "ORPH" ;;
        failed|unsupported_operation) printf '%s\n' "FAIL" ;;
        *) printf '%s\n' "?" ;;
    esac
}

# Engine provenance contract (ADR 0025): run metadata must pin every executed
# engine by docker image digest and engine-reported version. Fails when the
# metadata lacks the fields for any engine named on the command line.
validate_run_metadata_engine_provenance() {
    local metadata_file="$1"
    local reasoner

    [[ -f "$metadata_file" ]] || return 1

    for reasoner in "${@:2}"; do
        jq -e --arg reasoner "$reasoner" '
            has("engine_provenance")
            and (.engine_provenance | type == "object")
            and (.engine_provenance[$reasoner] | type == "object")
            and (.engine_provenance[$reasoner].image_digest
                 | type == "string" and test("^sha256:[0-9a-f]{64}$"))
            and (.engine_provenance[$reasoner].engine_version
                 | type == "string" and length > 0)
        ' "$metadata_file" >/dev/null || return 1
    done
    return 0
}

benchmark_result_normalize_execution_status() {
    case "${1:-}" in
        completed|success)
            printf '%s\n' "success"
            ;;
        unsupported_operation|unsupported)
            printf '%s\n' "unsupported"
            ;;
        not_available|non_comparable|missing_image|missing_dockerfile|build_failed)
            printf '%s\n' "not_available"
            ;;
        timeout|failed|killed|orphan)
            printf '%s\n' "$1"
            ;;
        *)
            printf '%s\n' "failed"
            ;;
    esac
}

benchmark_result_normalize_oracle_status() {
    case "${1:-}" in
        available|not_applicable|unavailable|not_configured|malformed)
            printf '%s\n' "$1"
            ;;
        *)
            printf '%s\n' "malformed"
            ;;
    esac
}

benchmark_result_normalize_correctness_status() {
    case "${1:-}" in
        passed|wrong|unsupported|not_evaluated|oracle_unavailable)
            printf '%s\n' "$1"
            ;;
        *)
            printf '%s\n' "oracle_unavailable"
            ;;
    esac
}

benchmark_result_is_present() {
    [[ -n "${1:-}" && "${1:-}" != "null" ]]
}

benchmark_result_normalize_comparable_value() {
    printf '%s' "${1:-}" | tr '[:upper:]' '[:lower:]' | tr -d '[:space:]'
}

benchmark_result_derive_correctness_status() {
    local semantic_result="$1"
    local oracle_status="$2"
    local expected_semantic_result="$3"
    local requested_status="$4"

    if [[ "$requested_status" == "unsupported" ]]; then
        if [[ "$oracle_status" == "available" ]]; then
            printf '%s\n' "not_evaluated"
        else
            printf '%s\n' "unsupported"
        fi
        return 0
    fi

    if [[ "$oracle_status" != "available" ]]; then
        printf '%s\n' "not_evaluated"
        return 0
    fi

    if ! benchmark_result_is_present "$expected_semantic_result"; then
        printf '%s\n' "oracle_unavailable"
        return 0
    fi

    if [[ "$(benchmark_result_normalize_comparable_value "$semantic_result")" == \
        "$(benchmark_result_normalize_comparable_value "$expected_semantic_result")" ]]; then
        printf '%s\n' "passed"
    else
        printf '%s\n' "wrong"
    fi
}

benchmark_result_overall_status() {
    local execution_status="$1"
    local correctness_status="$2"
    local oracle_status="$3"
    local expected_semantic_result="$4"

    if [[ "$execution_status" != "success" ]]; then
        printf '%s\n' "$execution_status"
        return 0
    fi

    if [[ "$correctness_status" == "unsupported" ]]; then
        printf '%s\n' "unsupported"
        return 0
    fi

    if [[ "$oracle_status" == "not_applicable" && "$correctness_status" == "not_evaluated" ]]; then
        printf '%s\n' "not_evaluated"
        return 0
    fi

    if [[ "$oracle_status" == "available" \
        && ("$correctness_status" == "passed" || "$correctness_status" == "wrong") \
        && -n "$expected_semantic_result" \
        && "$expected_semantic_result" != "null" ]]; then
        printf '%s\n' "$correctness_status"
        return 0
    fi

    printf '%s\n' "oracle_unavailable"
}

benchmark_result_numeric_or_minus_one() {
    if [[ "${1:-}" =~ ^-?[0-9]+$ ]]; then
        printf '%s\n' "$1"
    else
        printf '%s\n' "-1"
    fi
}

benchmark_result_extract_tableauxx_check_auto_report() {
    local output_file="$1"
    local expected_input_file="$2"
    local expected_operation="$3"
    local expected_exit_code="$4"
    local destination_file="$5"

    [[ -f "$output_file" ]] || return 1
    [[ "$expected_exit_code" =~ ^[0-9]+$ ]] || return 1
    case "$expected_exit_code" in
        0|1|2|124) ;;
        *) return 1 ;;
    esac

    local terminal_line
    terminal_line="$(awk 'NF { line = $0 } END { print line }' "$output_file")"
    [[ -n "$terminal_line" ]] || return 1

    local temporary_report
    temporary_report="$(mktemp)"
    if ! printf '%s\n' "$terminal_line" | jq -se 'length == 1 and (.[0] | type == "object")' >/dev/null 2>&1; then
        rm -f "$temporary_report"
        return 1
    fi
    if ! printf '%s\n' "$terminal_line" | jq -e '.' > "$temporary_report" 2>/dev/null; then
        rm -f "$temporary_report"
        return 1
    fi

    local report_count=0
    local line_report_count
    local line
    while IFS= read -r line || [[ -n "$line" ]]; do
        [[ -n "$line" ]] || continue
        line_report_count="$(printf '%s\n' "$line" \
            | jq -sr '[.[] | select(type == "object" and .report_type == "tableauxx_check_auto")] | length' 2>/dev/null \
            || printf '%s\n' 0)"
        if [[ "$line_report_count" =~ ^[0-9]+$ ]]; then
            report_count=$((report_count + line_report_count))
        fi
    done < "$output_file"
    if [[ "$report_count" -ne 1 ]]; then
        rm -f "$temporary_report"
        return 1
    fi

    if ! jq -e \
        --arg expected_input_file "$expected_input_file" \
        --arg expected_operation "$expected_operation" \
        --argjson expected_exit_code "$expected_exit_code" \
        '
        def nonnegative_integer:
            . as $value
            | (($value | type) == "number")
            and (($value | floor) == $value)
            and ($value >= 0);
        def nullable_nonnegative_integer:
            . == null or nonnegative_integer;
        def has_all_required_fields:
            has("schema_version")
            and has("report_type")
            and has("operation")
            and has("input_file")
            and has("status")
            and has("reasoning_result")
            and has("correctness_status")
            and has("selected_reasoner")
            and has("parse_time_ms")
            and has("reason_time_ms")
            and has("duration_ms")
            and has("error");
        def selected_tableauxx_reasoner:
            type == "string" and IN("SimpleReasoner", "SpeculativeTableauxReasoner");
        def nonempty_error:
            type == "string" and length > 0;

        . as $r
        | ($r | has_all_required_fields)
        and ($r.schema_version == 1)
        and ($r.report_type == "tableauxx_check_auto")
        and ($r.operation == $expected_operation)
        and (($r.input_file | type) == "string")
        and ($r.input_file == $expected_input_file)
        and ($r.correctness_status == "not_evaluated")
        and ($r.selected_reasoner == null or ($r.selected_reasoner | selected_tableauxx_reasoner))
        and ($r.duration_ms | nonnegative_integer)
        and (
            if $r.status == "success" then
                ($expected_exit_code == 0)
                and ($r.selected_reasoner | selected_tableauxx_reasoner)
                and ($r.reasoning_result | IN("consistent", "inconsistent"))
                and ($r.parse_time_ms | nonnegative_integer)
                and ($r.reason_time_ms | nonnegative_integer)
                and ($r.error == null)
            elif $r.status == "unsupported" then
                ($expected_exit_code == 2)
                and ($r.selected_reasoner | selected_tableauxx_reasoner)
                and ($r.reasoning_result == "unknown")
                and ($r.parse_time_ms | nonnegative_integer)
                and ($r.reason_time_ms == null)
                and ($r.error | nonempty_error)
            elif $r.status == "timeout" then
                ($expected_exit_code == 124)
                and ($r.selected_reasoner | selected_tableauxx_reasoner)
                and ($r.reasoning_result == "unknown")
                and ($r.parse_time_ms | nonnegative_integer)
                and ($r.reason_time_ms == null)
                and ($r.error | nonempty_error)
            elif $r.status == "failed" then
                ($expected_exit_code == 1)
                and ($r.reasoning_result == "error")
                and ($r.parse_time_ms | nullable_nonnegative_integer)
                and ($r.reason_time_ms == null)
                and (if $r.parse_time_ms == null then $r.selected_reasoner == null else ($r.selected_reasoner | selected_tableauxx_reasoner) end)
                and ($r.error | nonempty_error)
            else
                false
            end
        )
        ' "$temporary_report" >/dev/null; then
        rm -f "$temporary_report"
        return 1
    fi

    cp "$temporary_report" "$destination_file"
    rm -f "$temporary_report"
}

write_benchmark_result() {
    local result_file="$1"
    local run_id="$2"
    local ontology_suite="$3"
    local case_id="$4"
    local reasoner="$5"
    local ontology="$6"
    local operation="$7"
    local execution_detail="$8"
    local execution_status
    local semantic_result="$9"
    local correctness_status
    local oracle_status
    local oracle_source="${12}"
    local oracle_case_id="${13}"
    local expected_semantic_result="${14}"
    local wall_time_ms
    local reported_time_ms
    local parse_time_ms
    local reason_time_ms
    local output_file="${19}"
    local normalized_oracle_status
    local normalized_correctness_status
    local result_status
    local temporary_output=""

    execution_status="$(benchmark_result_normalize_execution_status "$execution_detail")"
    correctness_status="$(benchmark_result_normalize_correctness_status "${10}")"
    normalized_oracle_status="$(benchmark_result_normalize_oracle_status "${11}")"

    if [[ "$normalized_oracle_status" == "available" ]] \
        && { ! benchmark_result_is_present "$oracle_source" \
            || ! benchmark_result_is_present "$oracle_case_id" \
            || ! benchmark_result_is_present "$expected_semantic_result"; }; then
        normalized_oracle_status="malformed"
    fi

    if [[ "$normalized_oracle_status" == "malformed" ]]; then
        normalized_correctness_status="oracle_unavailable"
    elif [[ "$execution_status" != "success" ]]; then
        normalized_correctness_status="not_evaluated"
    else
        normalized_correctness_status="$(benchmark_result_derive_correctness_status \
            "$semantic_result" \
            "$normalized_oracle_status" \
            "$expected_semantic_result" \
            "$correctness_status")"
    fi

    result_status="$(benchmark_result_overall_status \
        "$execution_status" \
        "$normalized_correctness_status" \
        "$normalized_oracle_status" \
        "$expected_semantic_result")"

    wall_time_ms="$(benchmark_result_numeric_or_minus_one "${15:-}")"
    reported_time_ms="$(benchmark_result_numeric_or_minus_one "${16:-}")"
    parse_time_ms="$(benchmark_result_numeric_or_minus_one "${17:-}")"
    reason_time_ms="$(benchmark_result_numeric_or_minus_one "${18:-}")"

    if [[ ! -f "$output_file" ]]; then
        temporary_output="$(mktemp)"
        output_file="$temporary_output"
    fi

    jq -n \
        --argjson schema_version "$BENCHMARK_RESULT_SCHEMA_VERSION" \
        --arg run_id "$run_id" \
        --arg ontology_suite "$ontology_suite" \
        --arg case_id "$case_id" \
        --arg reasoner "$reasoner" \
        --arg ontology "$ontology" \
        --arg operation "$operation" \
        --arg status "$execution_status" \
        --arg execution_detail "$execution_detail" \
        --arg execution_status "$execution_status" \
        --arg result_status "$result_status" \
        --arg semantic_result "$semantic_result" \
        --arg correctness_status "$normalized_correctness_status" \
        --arg oracle_status "$normalized_oracle_status" \
        --arg oracle_source "$oracle_source" \
        --arg oracle_case_id "$oracle_case_id" \
        --arg oracle_expected "$expected_semantic_result" \
        --argjson wall_time_ms "$wall_time_ms" \
        --argjson reported_time_ms "$reported_time_ms" \
        --argjson parse_time_ms "$parse_time_ms" \
        --argjson reason_time_ms "$reason_time_ms" \
        --arg timestamp "$(date -Iseconds)" \
        --rawfile output "$output_file" \
        '{
            schema_version: $schema_version,
            run_id: $run_id,
            ontology_suite: $ontology_suite,
            case_id: $case_id,
            reasoner: $reasoner,
            ontology: $ontology,
            operation: $operation,
            status: $status,
            execution_status: $execution_status,
            execution_detail: $execution_detail,
            result_status: $result_status,
            semantic_result: $semantic_result,
            reasoning_result: $semantic_result,
            correctness_status: $correctness_status,
            oracle: {
                status: $oracle_status,
                source: $oracle_source,
                case_id: $oracle_case_id,
                expected: (if $oracle_expected == "" or $oracle_expected == "null" then null else $oracle_expected end)
            },
            wall_time_ms: $wall_time_ms,
            reported_time_ms: $reported_time_ms,
            parse_time_ms: $parse_time_ms,
            reason_time_ms: $reason_time_ms,
            timestamp: $timestamp,
            output: $output
        }' > "$result_file"

    if [[ -n "$temporary_output" ]]; then
        rm -f "$temporary_output"
    fi
}

validate_benchmark_result() {
    local result_file="$1"
    [[ -f "$result_file" ]] || return 1

    jq -e '
        (.schema_version == 1)
        and (.run_id | type == "string")
        and (.ontology_suite | type == "string")
        and (.case_id | type == "string")
        and (.operation | type == "string")
        and (.status == .execution_status)
        and (.execution_detail | type == "string" and length > 0)
        and (.execution_status | IN("success", "not_available", "failed", "timeout", "unsupported", "killed", "orphan"))
        and (.result_status | IN("passed", "wrong", "unsupported", "not_evaluated", "oracle_unavailable", "not_available", "failed", "timeout", "killed", "orphan"))
        and (.correctness_status | IN("passed", "wrong", "unsupported", "not_evaluated", "oracle_unavailable"))
        and (
            if .execution_only == true then
                (((.execution_status != "success")
                  or (.materialization_handoff.status == "ready"
                      and .materialization_handoff.read_only == true))
                 and ((has("oracle")) | not)
                 and (.result_status | IN("oracle_unavailable", "not_available", "unsupported", "failed", "timeout")))
            else
                (.oracle.status | IN("available", "not_applicable", "unavailable", "not_configured"))
                and (.oracle.status != "available" or (
                    (.oracle.source | type == "string" and length > 0)
                    and (.oracle.case_id | type == "string" and length > 0)
                    and (.oracle.expected != null)
                    and (.result_status == "passed" or .result_status == "wrong" or .correctness_status == "not_evaluated")
                ))
            end
        )
        and (.result_status != "passed" or (
            .execution_status == "success"
            and .correctness_status == "passed"
            and .oracle.status == "available"
            and .oracle.expected != null
            and ((.semantic_result | ascii_downcase) == (.oracle.expected | tostring | ascii_downcase))
        ))
        and (.result_status != "wrong" or (
            .execution_status == "success"
            and .correctness_status == "wrong"
            and .oracle.status == "available"
            and .oracle.expected != null
            and ((.semantic_result | ascii_downcase) != (.oracle.expected | tostring | ascii_downcase))
        ))
        and (.result_status != "not_evaluated" or .correctness_status == "not_evaluated")
    ' "$result_file" >/dev/null
}

validate_benchmark_result_count() {
    local result_dir="$1"
    local expected_count="$2"
    local result_file
    local actual_count

    [[ "$expected_count" =~ ^[0-9]+$ ]] || return 1
    [[ -d "$result_dir" ]] || return 1

    mapfile -t result_files < <(
        find "$result_dir" -maxdepth 1 -type f -name '*.json' \
            ! -name 'run_metadata.json' ! -name 'ore_metadata.json' ! -name 'summary.json' \
            ! -name 'owl2bench_ladder_metadata.json' | sort
    )
    actual_count="${#result_files[@]}"
    [[ "$actual_count" -eq "$expected_count" ]] || return 1

    for result_file in "${result_files[@]}"; do
        validate_benchmark_result "$result_file" || return 1
    done
}
