#!/usr/bin/env bash
#
# Comparative execution aggregation (issue #192, ADR 0025).
#
# Takes two or more comparative execution pass run directories of the same
# track (identical ontology_suite and operation) and writes run-directory
# artifacts:
#
#   - comparative_median_times.tsv: per case and engine, the repetition
#     median of container-inclusive wall time (wall_time_ms) across the
#     passes, computed over recorded (completed) passes only.
#   - comparative_paper_table.md: paper-table comparison per case; the best
#     (lowest) median among engines that completed is bolded, ties are all
#     bolded, and non-completed cells carry the standard TO/NA/FAIL legend
#     codes.
#   - comparative_engine_summary.tsv / .md: per-engine completability counts
#     plus median and total over the recorded per-case medians.
#   - aggregation_metadata.json: pass identities and track.
#
# Execution-performance evidence only (ADR 0025): this step adjudicates no
# correctness, consults no oracle, and gates nothing.

set -euo pipefail
export LC_ALL=C

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/benchmark_result.sh"

log_info() { echo "[info] $1"; }
log_ok() { echo "[ok] $1"; }
log_err() { echo "[error] $1" >&2; }

usage() {
    cat <<'EOF'
Usage: run_comparative_aggregation.sh PASS_DIR_1 PASS_DIR_2 [PASS_DIR_3 ...]

Aggregates two or more comparative execution passes of the same track
(identical ontology_suite and operation) into run-directory artifacts.

Env:
  AGGREGATION_OUTPUT_DIR  output run directory (default: an
                          aggregate_<timestamp> sibling of the first pass)
EOF
}

# Median of the given numbers; whole values print as integers, otherwise one
# decimal place. Deterministic for any pass count.
aggregate_median() {
    if [[ $# -eq 0 ]]; then
        return 0
    fi
    printf '%s\n' "$@" | sort -n | awk '
        { values[NR] = $1 }
        END {
            if (NR % 2 == 1) {
                m = values[(NR + 1) / 2]
            } else {
                m = (values[NR / 2] + values[NR / 2 + 1]) / 2
            }
            if (m == int(m)) { printf "%d\n", m } else { printf "%.1f\n", m }
        }'
}

aggregate_format_number() {
    awk -v n="$1" 'BEGIN { if (n == int(n)) printf "%d\n", n; else printf "%.1f\n", n }'
}

aggregate_sum() {
    if [[ $# -eq 0 ]]; then
        printf '%s\n' 0
        return 0
    fi
    printf '%s\n' "$@" | awk '{ s += $1 } END { printf "%.1f\n", s }'
}

aggregate_numeric_lt() {
    awk -v a="$1" -v b="$2" 'BEGIN { print (a + 0 < b + 0) ? 1 : 0 }'
}

aggregate_numeric_eq() {
    awk -v a="$1" -v b="$2" 'BEGIN { print (a + 0 == b + 0) ? 1 : 0 }'
}

# Majority status across passes. Ties break by fixed precedence (timeout,
# failed, killed, orphan, not_available, unsupported) so the rendered code is
# deterministic; callers only invoke this with zero completed passes.
aggregate_majority_status() {
    if [[ $# -eq 0 ]]; then
        return 0
    fi
    printf '%s\n' "$@" | awk '
        function rank(s) { return index("|timeout|failed|killed|orphan|not_available|unsupported|", "|" s "|") }
        { count[$1]++ }
        END {
            best = ""; best_count = -1; best_rank = 999
            for (s in count) {
                r = rank(s)
                if (r == 0) { r = 999 }
                if (count[s] > best_count || (count[s] == best_count && r < best_rank)) {
                    best = s; best_count = count[s]; best_rank = r
                }
            }
            print best
        }'
}

if [[ "${1:-}" == "-h" || "${1:-}" == "--help" ]]; then
    usage
    exit 0
fi

if [[ $# -lt 2 ]]; then
    log_err "two or more pass run directories are required"
    usage >&2
    exit 1
fi

PASS_DIRS=("$@")
for pass_dir in "${PASS_DIRS[@]}"; do
    if [[ ! -d "$pass_dir" ]]; then
        log_err "Pass directory not found: $pass_dir"
        exit 1
    fi
    if [[ ! -f "$pass_dir/run_metadata.json" ]]; then
        log_err "Missing run_metadata.json in pass directory: $pass_dir"
        exit 1
    fi
done

TRACK_SUITE="$(jq -r '.ontology_suite // ""' "${PASS_DIRS[0]}/run_metadata.json")"
TRACK_OPERATION="$(jq -r '.operation // ""' "${PASS_DIRS[0]}/run_metadata.json")"
TRACK_TIMEOUT="$(jq -r '.timeout_seconds // ""' "${PASS_DIRS[0]}/run_metadata.json")"
if [[ -z "$TRACK_SUITE" || -z "$TRACK_OPERATION" ]]; then
    log_err "First pass metadata lacks ontology_suite/operation: ${PASS_DIRS[0]}"
    exit 1
fi

for pass_dir in "${PASS_DIRS[@]:1}"; do
    pass_suite="$(jq -r '.ontology_suite // ""' "$pass_dir/run_metadata.json")"
    pass_operation="$(jq -r '.operation // ""' "$pass_dir/run_metadata.json")"
    if [[ "$pass_suite" != "$TRACK_SUITE" || "$pass_operation" != "$TRACK_OPERATION" ]]; then
        log_err "Pass $pass_dir is from a different track ($pass_suite/$pass_operation); expected $TRACK_SUITE/$TRACK_OPERATION"
        exit 1
    fi
    pass_timeout="$(jq -r '.timeout_seconds // ""' "$pass_dir/run_metadata.json")"
    if [[ -n "$TRACK_TIMEOUT" && -n "$pass_timeout" && "$pass_timeout" != "$TRACK_TIMEOUT" ]]; then
        log_err "Pass $pass_dir records timeout_seconds=$pass_timeout; expected $TRACK_TIMEOUT"
        exit 1
    fi
done

AGGREGATION_OUTPUT_DIR="${AGGREGATION_OUTPUT_DIR:-$(dirname "${PASS_DIRS[0]}")/aggregate_$(date +%Y%m%d_%H%M%S)}"
mkdir -p "$AGGREGATION_OUTPUT_DIR"

# Index every pass result by case and engine.
declare -A RESULT_STATUS=()
declare -A RESULT_WALL=()
declare -A CASE_ONTOLOGY=()
declare -A CASE_PRESENT=()
declare -A ENGINE_PRESENT=()

pass_index=0
for pass_dir in "${PASS_DIRS[@]}"; do
    while IFS= read -r -d '' result_file; do
        IFS=$'\t' read -r case_id reasoner ontology execution_status wall_ms < <(
            jq -r '[
                (.case_id // ((.ontology_suite // "unknown") + ":" + (.ontology // "unknown") + ":" + (.operation // "unknown"))),
                (.reasoner // "unknown"),
                (.ontology // "unknown"),
                (.execution_status // .status),
                (.wall_time_ms | tostring)
            ] | @tsv' "$result_file"
        )
        [[ -n "$case_id" && "$case_id" != "null" && "$case_id" != "unknown:unknown:unknown" ]] || continue
        [[ -n "$reasoner" && "$reasoner" != "null" && "$reasoner" != "unknown" ]] || continue
        RESULT_STATUS["${pass_index}|${case_id}|${reasoner}"]="$execution_status"
        RESULT_WALL["${pass_index}|${case_id}|${reasoner}"]="$wall_ms"
        CASE_ONTOLOGY["$case_id"]="$ontology"
        CASE_PRESENT["$case_id"]=1
        ENGINE_PRESENT["$reasoner"]=1
    done < <(
        find "$pass_dir" -maxdepth 1 -type f -name '*.json' \
            ! -name 'run_metadata.json' ! -name 'ore_metadata.json' ! -name 'summary.json' \
            ! -name 'aggregation_metadata.json' ! -name 'owl2bench_ladder_metadata.json' \
            -print0 | sort -z
    )
    pass_index=$((pass_index + 1))
done

if [[ "${#CASE_PRESENT[@]}" -eq 0 ]]; then
    log_err "No result JSON files found across the given pass directories"
    exit 1
fi

mapfile -t CASES < <(printf '%s\n' "${!CASE_PRESENT[@]}" | sort)
mapfile -t ENGINES < <(printf '%s\n' "${!ENGINE_PRESENT[@]}" | sort)

log_info "Aggregating ${#CASES[@]} case(s) over ${#ENGINES[@]} engine(s) across ${#PASS_DIRS[@]} pass(es)"

# Aggregate each case/engine pair across passes.
declare -A AGG_STATUS=()
declare -A AGG_MEDIAN=()
declare -A AGG_RECORDED=()
declare -A AGG_TOTAL=()

for case_id in "${CASES[@]}"; do
    for engine in "${ENGINES[@]}"; do
        statuses=()
        walls=()
        total_results=0
        for ((pass_index = 0; pass_index < ${#PASS_DIRS[@]}; pass_index++)); do
            status="${RESULT_STATUS["${pass_index}|${case_id}|${engine}"]:-}"
            [[ -n "$status" ]] || continue
            total_results=$((total_results + 1))
            normalized="$(benchmark_result_normalize_execution_status "$status")"
            statuses+=("$normalized")
            wall_ms="${RESULT_WALL["${pass_index}|${case_id}|${engine}"]:-}"
            if [[ "$normalized" == "success" && "$wall_ms" =~ ^[0-9]+$ && "$wall_ms" -ge 0 ]]; then
                walls+=("$wall_ms")
            fi
        done
        [[ "$total_results" -gt 0 ]] || continue
        AGG_TOTAL["${case_id}|${engine}"]="$total_results"
        if [[ "${#walls[@]}" -gt 0 ]]; then
            AGG_STATUS["${case_id}|${engine}"]="success"
            AGG_MEDIAN["${case_id}|${engine}"]="$(aggregate_median ${walls[@]+"${walls[@]}"})"
            AGG_RECORDED["${case_id}|${engine}"]="${#walls[@]}"
        else
            AGG_STATUS["${case_id}|${engine}"]="$(aggregate_majority_status ${statuses[@]+"${statuses[@]}"})"
            AGG_MEDIAN["${case_id}|${engine}"]=""
            AGG_RECORDED["${case_id}|${engine}"]=0
        fi
    done
done

MEDIAN_TSV="$AGGREGATION_OUTPUT_DIR/comparative_median_times.tsv"
PAPER_MD="$AGGREGATION_OUTPUT_DIR/comparative_paper_table.md"
SUMMARY_TSV="$AGGREGATION_OUTPUT_DIR/comparative_engine_summary.tsv"
SUMMARY_MD="$AGGREGATION_OUTPUT_DIR/comparative_engine_summary.md"
METADATA_JSON="$AGGREGATION_OUTPUT_DIR/aggregation_metadata.json"

pass_run_ids_md=""
for pass_dir in "${PASS_DIRS[@]}"; do
    run_id="$(basename "$pass_dir")"
    if [[ -n "$pass_run_ids_md" ]]; then
        pass_run_ids_md="$pass_run_ids_md, \`$run_id\`"
    else
        pass_run_ids_md="\`$run_id\`"
    fi
done
# Pass order is chronological evidence; keep the invocation order.
mapfile -t PASS_RUN_IDS < <(printf '%s\n' "${PASS_DIRS[@]##*/}")

{
    printf 'case_id\tontology\tengine\taggregated_status\trecorded_passes\ttotal_passes\tmedian_wall_time_ms\n'
    for case_id in "${CASES[@]}"; do
        for engine in "${ENGINES[@]}"; do
            status="${AGG_STATUS["${case_id}|${engine}"]:-}"
            [[ -n "$status" ]] || continue
            printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\n' \
                "$case_id" \
                "${CASE_ONTOLOGY["$case_id"]}" \
                "$engine" \
                "$status" \
                "${AGG_RECORDED["${case_id}|${engine}"]}" \
                "${AGG_TOTAL["${case_id}|${engine}"]}" \
                "${AGG_MEDIAN["${case_id}|${engine}"]}"
        done
    done
} > "$MEDIAN_TSV"
log_ok "Median TSV written: $MEDIAN_TSV"

{
    echo "# Comparative Repetition Median Wall Time (ms)"
    echo
    echo "- Pass runs: $pass_run_ids_md"
    echo "- Passes aggregated: ${#PASS_DIRS[@]}"
    echo "- Suite: \`$TRACK_SUITE\`"
    echo "- Operation: \`$TRACK_OPERATION\`"
    echo "- Statistic: per-case median of wall time in milliseconds across passes (container-level; parse + reasoning + startup; recorded passes only)"
    echo
    printf '| Ontology |'
    for engine in "${ENGINES[@]}"; do
        printf ' %s |' "$engine"
    done
    echo
    printf '|---|'
    for _ in "${ENGINES[@]}"; do
        printf -- '---:|'
    done
    echo

    for case_id in "${CASES[@]}"; do
        best=""
        for engine in "${ENGINES[@]}"; do
            median="${AGG_MEDIAN["${case_id}|${engine}"]:-}"
            [[ -n "$median" ]] || continue
            if [[ -z "$best" ]] || [[ "$(aggregate_numeric_lt "$median" "$best")" == 1 ]]; then
                best="$median"
            fi
        done

        printf '| %s |' "${CASE_ONTOLOGY["$case_id"]}"
        for engine in "${ENGINES[@]}"; do
            median="${AGG_MEDIAN["${case_id}|${engine}"]:-}"
            status="${AGG_STATUS["${case_id}|${engine}"]:-}"
            if [[ -n "$median" ]]; then
                if [[ -n "$best" && "$(aggregate_numeric_eq "$median" "$best")" == 1 ]]; then
                    printf ' **%s** |' "$median"
                else
                    printf ' %s |' "$median"
                fi
            elif [[ -n "$status" ]]; then
                printf ' %s |' "$(benchmark_result_status_code "$status")"
            else
                printf ' - |'
            fi
        done
        echo
    done

    echo
    echo "Legend: **best median wall time among engines that completed**, TO=timeout, NA=not available, NC=non-comparable parser/runtime incompatibility, KIL=interrupted cleanup kill, ORPH=container orphan cleanup, FAIL=hard runtime failure."
} > "$PAPER_MD"
log_ok "Paper table written: $PAPER_MD"

{
    printf 'engine\tcases\tcompleted\ttimeout\tfailed\tnot_available\tother_execution\tmedian_wall_time_ms\ttotal_wall_time_ms\n'
    for engine in "${ENGINES[@]}"; do
        cases=0
        completed=0
        timeout_count=0
        failed_count=0
        not_available_count=0
        other_count=0
        medians=()
        for case_id in "${CASES[@]}"; do
            status="${AGG_STATUS["${case_id}|${engine}"]:-}"
            [[ -n "$status" ]] || continue
            cases=$((cases + 1))
            case "$status" in
                success) completed=$((completed + 1)) ;;
                timeout) timeout_count=$((timeout_count + 1)) ;;
                failed) failed_count=$((failed_count + 1)) ;;
                not_available) not_available_count=$((not_available_count + 1)) ;;
                *) other_count=$((other_count + 1)) ;;
            esac
            median="${AGG_MEDIAN["${case_id}|${engine}"]:-}"
            if [[ -n "$median" ]]; then
                medians+=("$median")
            fi
        done
        median_of_medians=""
        total_ms=""
        if [[ "${#medians[@]}" -gt 0 ]]; then
            median_of_medians="$(aggregate_median ${medians[@]+"${medians[@]}"})"
            total_ms="$(aggregate_format_number "$(aggregate_sum ${medians[@]+"${medians[@]}"})")"
        fi
        printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n' \
            "$engine" "$cases" "$completed" "$timeout_count" "$failed_count" \
            "$not_available_count" "$other_count" "$median_of_medians" "$total_ms"
    done
} > "$SUMMARY_TSV"
log_ok "Engine summary TSV written: $SUMMARY_TSV"

{
    echo "# Comparative Engine Completability and Timing Summary"
    echo
    echo "- Pass runs: $pass_run_ids_md"
    echo "- Suite: \`$TRACK_SUITE\`"
    echo "- Operation: \`$TRACK_OPERATION\`"
    echo "- Statistic: per-engine counts over aggregated per-case outcomes; median and total over the recorded per-case repetition medians."
    echo
    echo "| Engine | Cases | Completed | Timeout | Failed | Not Available | Other Execution | Median Wall (ms) | Total Wall (ms) |"
    echo "|---|---:|---:|---:|---:|---:|---:|---:|---:|"
    tail -n +2 "$SUMMARY_TSV" | while IFS=$'\t' read -r engine cases completed timeout_count failed_count not_available_count other_count median_of_medians total_ms; do
        echo "| $engine | $cases | $completed | $timeout_count | $failed_count | $not_available_count | $other_count | $median_of_medians | $total_ms |"
    done
} > "$SUMMARY_MD"
log_ok "Engine summary markdown written: $SUMMARY_MD"

jq -n \
    --arg aggregation_type "comparative_repetition_median" \
    --arg ontology_suite "$TRACK_SUITE" \
    --arg operation "$TRACK_OPERATION" \
    --argjson pass_count "${#PASS_DIRS[@]}" \
    --argjson pass_run_ids "$(printf '%s\n' "${PASS_RUN_IDS[@]}" | jq -R . | jq -s .)" \
    --argjson engines "$(printf '%s\n' "${ENGINES[@]}" | jq -R . | jq -s .)" \
    --arg statistic "per-case median container-inclusive wall time across passes (recorded passes only)" \
    '{
        aggregation_type: $aggregation_type,
        ontology_suite: $ontology_suite,
        operation: $operation,
        pass_count: $pass_count,
        pass_run_ids: $pass_run_ids,
        engines: $engines,
        statistic: $statistic,
        artifacts: {
            median_times: "comparative_median_times.tsv",
            paper_table: "comparative_paper_table.md",
            engine_summary_tsv: "comparative_engine_summary.tsv",
            engine_summary_md: "comparative_engine_summary.md"
        }
    }' > "$METADATA_JSON"
log_ok "Aggregation metadata written: $METADATA_JSON"

log_ok "Comparative aggregation complete: $AGGREGATION_OUTPUT_DIR"
