#!/usr/bin/env bash
set -euo pipefail
export LC_ALL=C

# OWL2Bench scalability ladder view (issue #197, ADR 0025/0026).
#
# Takes the per-dataset comparative aggregation outputs (one per size x
# profile, each aggregating the three serial sweep passes produced by
# ladder_run.sh) and renders one scalability ladder table per profile:
# rows = universities ascending, columns = the ADR 0025 engine set for the
# profile (DL/QL/RL: tableauxx, hermit, konclude; EL adds elk), cells =
# repetition median wall time with completability counts, e.g.
# "2330ms (305/306)", with the paper-table TO/NA/FAIL legend for engines
# that completed nothing. A ladder_metadata.json records the input run ids
# per ladder point.
#
# Missing or malformed inputs fail closed with errors naming what is wrong.
#
# Performance lane (ADR 0024/0026): execution performance and
# completability evidence only.

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
RESULTS_HISTORY="${OWL2BENCH_LADDER_RESULTS_HISTORY:-$ROOT_DIR/benchmarks/competitors/results/history}"

LADDER_RUN_ID_SHAPE='owl2bench_cmp_p<pass>_<profile>_u<universities>'
EXPECTED_PASSES=(1 2 3)
DEFAULT_PROFILES="DL,EL,QL,RL"
DEFAULT_SIZES="1,2,5,10,20,50"

SUMMARY_HEADER=$'engine\tcases\tcompleted\ttimeout\tfailed\tnot_available\tother_execution\tmedian_wall_time_ms\ttotal_wall_time_ms'
MEDIAN_HEADER=$'case_id\tontology\tengine\taggregated_status\trecorded_passes\ttotal_passes\tmedian_wall_time_ms'

# Cell-code precedence for an engine that completed nothing, mirroring the
# paper-table legend.
STATUS_PRECEDENCE=(timeout not_available failed killed orphan non_comparable unsupported)

ladder_log() { printf '[owl2bench-ladder] %s\n' "$*"; }
ladder_err() { printf '[owl2bench-ladder][error] %s\n' "$*" >&2; }

usage() {
    cat <<EOF
Usage: ladder_view.sh AGGREGATION_DIR [AGGREGATION_DIR ...]

Renders one scalability ladder table per profile from the per-dataset
comparative aggregation outputs of the three sweep passes (ADR 0025/0026).
Every expected profile x size point must be present and well formed.

Env:
  OWL2BENCH_LADDER_PROFILES       expected profiles (default: $DEFAULT_PROFILES)
  OWL2BENCH_LADDER_SIZES          expected sizes ascending (default: $DEFAULT_SIZES)
  OWL2BENCH_LADDER_OUTPUT_DIR     output directory (default: a fresh
                                  owl2bench_ladder_<timestamp> under the
                                  comparative results history)
  OWL2BENCH_LADDER_RESULTS_HISTORY  history dir for the default output
EOF
}

ladder_engines_for() {
    case "$1" in
        DL|QL|RL) printf '%s\n' "tableauxx hermit konclude" ;;
        EL) printf '%s\n' "tableauxx hermit konclude elk" ;;
        *) return 1 ;;
    esac
}

numeric_lt() {
    awk -v a="$1" -v b="$2" 'BEGIN { print (a + 0 < b + 0) ? 1 : 0 }'
}

numeric_eq() {
    awk -v a="$1" -v b="$2" 'BEGIN { print (a + 0 == b + 0) ? 1 : 0 }'
}

# Tab-separated field readers: awk keeps empty fields intact (bash `read`
# collapses consecutive tab delimiters), which matters for engines that
# completed nothing and carry an empty median.
summary_field() {
    local file="$1" engine="$2" column="$3"
    awk -F '\t' -v e="$engine" -v col="$column" 'NR > 1 && $1 == e { print $col; exit }' "$file"
}

median_case_count() {
    local file="$1" engine="$2"
    awk -F '\t' -v e="$engine" 'NR > 1 && $3 == e { n++ } END { print n + 0 }' "$file"
}

median_status_count() {
    local file="$1" engine="$2" status="$3"
    awk -F '\t' -v e="$engine" -v s="$status" 'NR > 1 && $3 == e && $4 == s { n++ } END { print n + 0 }' "$file"
}

if [[ "${1:-}" == "-h" || "${1:-}" == "--help" ]]; then
    usage
    exit 0
fi

if [[ $# -lt 1 ]]; then
    ladder_err "at least one aggregation directory is required"
    usage >&2
    exit 1
fi

# ---------------------------------------------------------------------------
# Expected grid.
# ---------------------------------------------------------------------------

PROFILES_ARG="${OWL2BENCH_LADDER_PROFILES:-$DEFAULT_PROFILES}"
PROFILE_LIST=()
for token in ${PROFILES_ARG//,/ }; do
    profile="$(printf '%s' "$token" | tr '[:lower:]' '[:upper:]')"
    if [[ ! "$profile" =~ ^(DL|EL|QL|RL)$ ]]; then
        ladder_err "unsupported profile in OWL2BENCH_LADDER_PROFILES: $token (use DL|EL|QL|RL)"
        exit 1
    fi
    seen=0
    for existing in "${PROFILE_LIST[@]:-}"; do
        [[ "$existing" == "$profile" ]] && seen=1
    done
    [[ "$seen" == "1" ]] || PROFILE_LIST+=("$profile")
done
if [[ "${#PROFILE_LIST[@]}" -eq 0 ]]; then
    ladder_err "OWL2BENCH_LADDER_PROFILES must name at least one profile"
    exit 1
fi

SIZES_ARG="${OWL2BENCH_LADDER_SIZES:-$DEFAULT_SIZES}"
SIZE_LIST=()
for token in ${SIZES_ARG//,/ }; do
    if ! [[ "$token" =~ ^[0-9]+$ ]] || [[ "$token" -le 0 ]]; then
        ladder_err "unsupported size in OWL2BENCH_LADDER_SIZES: $token (positive integers only)"
        exit 1
    fi
    seen=0
    for existing in "${SIZE_LIST[@]:-}"; do
        [[ "$existing" == "$token" ]] && seen=1
    done
    [[ "$seen" == "1" ]] || SIZE_LIST+=("$token")
done
if [[ "${#SIZE_LIST[@]}" -eq 0 ]]; then
    ladder_err "OWL2BENCH_LADDER_SIZES must name at least one size"
    exit 1
fi
mapfile -t SIZE_LIST < <(printf '%s\n' "${SIZE_LIST[@]}" | sort -n)

expected_engines_for_profile() {
    ladder_engines_for "$1" | tr ' ' '\n' | sort
}

# Canonical, human-readable engine list for error messages and metadata.
canonical_engines_csv() {
    ladder_engines_for "$1" | sed 's/ /, /g'
}

in_grid() {
    local profile="$1" size="$2" member size_member
    for member in "${PROFILE_LIST[@]}"; do
        [[ "$member" == "$profile" ]] || continue
        for size_member in "${SIZE_LIST[@]}"; do
            [[ "$size_member" == "$size" ]] && return 0
        done
    done
    return 1
}

# ---------------------------------------------------------------------------
# Input validation: every aggregation directory must be a complete, well
# formed ladder point. Fails closed naming what is missing or malformed.
# ---------------------------------------------------------------------------

declare -A POINT_DIR=()
declare -A POINT_RUN_IDS=()
declare -A CELL_CASES=()
declare -A CELL_COMPLETED=()
declare -A CELL_MEDIAN=()

for agg_dir in "$@"; do
    if [[ ! -d "$agg_dir" ]]; then
        ladder_err "aggregation directory not found: $agg_dir"
        exit 1
    fi
    summary_file="$agg_dir/comparative_engine_summary.tsv"
    metadata_file="$agg_dir/aggregation_metadata.json"
    median_file="$agg_dir/comparative_median_times.tsv"
    if [[ ! -f "$summary_file" ]]; then
        ladder_err "missing comparative_engine_summary.tsv: $agg_dir"
        exit 1
    fi
    if [[ ! -f "$metadata_file" ]]; then
        ladder_err "missing aggregation_metadata.json: $agg_dir"
        exit 1
    fi
    if ! jq -e . "$metadata_file" >/dev/null 2>&1; then
        ladder_err "malformed aggregation metadata: $metadata_file"
        exit 1
    fi
    if [[ ! -f "$median_file" ]]; then
        ladder_err "missing comparative_median_times.tsv: $agg_dir"
        exit 1
    fi

    operation="$(jq -r '.operation // ""' "$metadata_file")"
    if [[ "$operation" != "consistency" ]]; then
        ladder_err "$agg_dir is not a consistency aggregation (operation: ${operation:-<empty>})"
        exit 1
    fi
    ontology_suite="$(jq -r '.ontology_suite // ""' "$metadata_file")"
    if [[ -z "$ontology_suite" || "$ontology_suite" == "null" ]]; then
        ladder_err "malformed aggregation metadata (ontology_suite missing): $metadata_file"
        exit 1
    fi

    mapfile -t run_ids < <(jq -r '.pass_run_ids[]?' "$metadata_file")
    if [[ "${#run_ids[@]}" -eq 0 ]]; then
        ladder_err "malformed aggregation metadata (pass_run_ids missing): $metadata_file"
        exit 1
    fi

    point_profile=""
    point_size=""
    point_passes=()
    for run_id in "${run_ids[@]}"; do
        if [[ ! "$run_id" =~ ^owl2bench_cmp_p([0-9]+)_(dl|el|ql|rl)_u([0-9]+)$ ]]; then
            ladder_err "pass run id does not match the ladder run id shape $LADDER_RUN_ID_SHAPE: $run_id (from $agg_dir)"
            exit 1
        fi
        run_pass="${BASH_REMATCH[1]}"
        run_profile="$(printf '%s' "${BASH_REMATCH[2]}" | tr '[:lower:]' '[:upper:]')"
        run_size="${BASH_REMATCH[3]}"
        if [[ -z "$point_profile" ]]; then
            point_profile="$run_profile"
            point_size="$run_size"
        elif [[ "$run_profile" != "$point_profile" || "$run_size" != "$point_size" ]]; then
            ladder_err "run ids in $agg_dir span multiple ladder points: ${run_ids[*]}"
            exit 1
        fi
        duplicate_pass=0
        for existing_pass in "${point_passes[@]:-}"; do
            [[ "$existing_pass" == "$run_pass" ]] && duplicate_pass=1
        done
        [[ "$duplicate_pass" == "1" ]] || point_passes+=("$run_pass")
    done

    mapfile -t sorted_passes < <(printf '%s\n' "${point_passes[@]}" | sort -n)
    expected_passes_joined="$(IFS=,; echo "${EXPECTED_PASSES[*]}")"
    got_passes_joined="$(printf 'p%s\n' "${point_passes[@]}" | paste -sd, -)"
    if [[ "${#sorted_passes[@]}" -ne "${#EXPECTED_PASSES[@]}" ]] \
        || [[ "$(IFS=,; echo "${sorted_passes[*]}")" != "$expected_passes_joined" ]]; then
        ladder_err "$agg_dir must aggregate exactly passes $expected_passes_joined; got: $got_passes_joined"
        exit 1
    fi

    point_key="${point_profile}|${point_size}"
    if [[ -n "${POINT_DIR[$point_key]:-}" ]]; then
        ladder_err "duplicate ladder point $point_profile:$point_size from $agg_dir (already supplied by ${POINT_DIR[$point_key]})"
        exit 1
    fi

    if ! in_grid "$point_profile" "$point_size"; then
        ladder_err "unexpected ladder point $point_profile:$point_size from $agg_dir (expected profiles ${PROFILE_LIST[*]}; sizes ${SIZE_LIST[*]})"
        exit 1
    fi

    # Engine summary: header, engine set, and per-engine counts. Fields are
    # read with awk so empty tab-separated fields (an engine that completed
    # nothing has an empty median) survive parsing.
    actual_header="$(head -n 1 "$summary_file")"
    if [[ "$actual_header" != "$SUMMARY_HEADER" ]]; then
        ladder_err "malformed engine summary header in $summary_file (expected the comparative_engine_summary.tsv header)"
        exit 1
    fi

    expected_engines="$(expected_engines_for_profile "$point_profile")"
    actual_engines="$(tail -n +2 "$summary_file" | awk -F '\t' 'NF >= 9 { print $1 }' | sort)"
    if [[ "$actual_engines" != "$expected_engines" ]]; then
        ladder_err "ladder point $point_profile:$point_size summary engines ($(echo "$actual_engines" | tr '\n' ' ')) do not match the ADR 0025 $point_profile engine set ($(canonical_engines_csv "$point_profile")): $summary_file"
        exit 1
    fi

    for engine in $expected_engines; do
        cases="$(summary_field "$summary_file" "$engine" 2)"
        completed="$(summary_field "$summary_file" "$engine" 3)"
        timeout_count="$(summary_field "$summary_file" "$engine" 4)"
        failed_count="$(summary_field "$summary_file" "$engine" 5)"
        not_available_count="$(summary_field "$summary_file" "$engine" 6)"
        other_count="$(summary_field "$summary_file" "$engine" 7)"
        median="$(summary_field "$summary_file" "$engine" 8)"
        for numeric_field in "$cases" "$completed" "$timeout_count" "$failed_count" "$not_available_count" "$other_count"; do
            if ! [[ "$numeric_field" =~ ^[0-9]+$ ]]; then
                ladder_err "malformed summary row for engine $engine in $summary_file (non-numeric count: $numeric_field)"
                exit 1
            fi
        done
        if [[ "$cases" -le 0 || "$completed" -gt "$cases" ]]; then
            ladder_err "malformed summary row for engine $engine in $summary_file (cases=$cases completed=$completed)"
            exit 1
        fi
        if [[ "$completed" -gt 0 && -z "$median" ]] || [[ "$completed" -eq 0 && -n "$median" ]]; then
            ladder_err "malformed summary row for engine $engine in $summary_file (completed=$completed but median='${median:-<empty>}')"
            exit 1
        fi
        if [[ -n "$median" ]] && ! [[ "$median" =~ ^[0-9]+(\.[0-9]+)?$ ]]; then
            ladder_err "malformed summary row for engine $engine in $summary_file (non-numeric median: $median)"
            exit 1
        fi
        CELL_CASES["$point_key|$engine"]="$cases"
        CELL_COMPLETED["$point_key|$engine"]="$completed"
        CELL_MEDIAN["$point_key|$engine"]="$median"
    done

    # Median TSV: per-case aggregated statuses drive the cell legend codes.
    actual_median_header="$(head -n 1 "$median_file")"
    if [[ "$actual_median_header" != "$MEDIAN_HEADER" ]]; then
        ladder_err "malformed median times header in $median_file (expected the comparative_median_times.tsv header)"
        exit 1
    fi

    median_engines="$(tail -n +2 "$median_file" | awk -F '\t' 'NF >= 4 { print $3 }' | sort -u)"
    if [[ "$median_engines" != "$expected_engines" ]]; then
        ladder_err "ladder point $point_profile:$point_size median times engines ($(echo "$median_engines" | tr '\n' ' ')) do not match the ADR 0025 $point_profile engine set ($(canonical_engines_csv "$point_profile")): $median_file"
        exit 1
    fi

    for engine in $expected_engines; do
        median_rows="$(median_case_count "$median_file" "$engine")"
        success_rows="$(median_status_count "$median_file" "$engine" success)"
        if [[ "$median_rows" != "${CELL_CASES[$point_key|$engine]}" ]] \
            || [[ "$success_rows" != "${CELL_COMPLETED[$point_key|$engine]}" ]]; then
            ladder_err "engine summary and median times disagree for $point_profile:$point_size engine $engine (summary cases=${CELL_CASES[$point_key|$engine]:-<missing>} completed=${CELL_COMPLETED[$point_key|$engine]:-<missing>}; median tsv rows=$median_rows success=$success_rows)"
            exit 1
        fi
    done

    POINT_DIR["$point_key"]="$agg_dir"
    POINT_RUN_IDS["$point_key"]="$(printf '%s\n' "${run_ids[@]}" | jq -R . | jq -s -c .)"
    ladder_log "ladder point $point_profile:$point_size <- $agg_dir"
done

# ---------------------------------------------------------------------------
# Completeness: every expected profile x size point must be present.
# ---------------------------------------------------------------------------

missing_points=()
for profile in "${PROFILE_LIST[@]}"; do
    for size in "${SIZE_LIST[@]}"; do
        if [[ -z "${POINT_DIR[$profile|$size]:-}" ]]; then
            missing_points+=("$profile:$size")
        fi
    done
done
if [[ "${#missing_points[@]}" -gt 0 ]]; then
    ladder_err "missing ladder point input(s): $(IFS=,; echo "${missing_points[*]}")"
    exit 1
fi

# ---------------------------------------------------------------------------
# Rendering.
# ---------------------------------------------------------------------------

OUTPUT_DIR="${OWL2BENCH_LADDER_OUTPUT_DIR:-$RESULTS_HISTORY/owl2bench_ladder_$(date +%Y%m%d_%H%M%S)}"
if [[ -e "$OUTPUT_DIR" ]]; then
    ladder_err "output directory already exists: $OUTPUT_DIR"
    exit 1
fi
mkdir -p "$OUTPUT_DIR"

cell_code_for() {
    local median_file="$1" engine="$2"
    local status count
    for status in "${STATUS_PRECEDENCE[@]}"; do
        count="$(median_status_count "$median_file" "$engine" "$status")"
        if [[ "$count" -gt 0 ]]; then
            case "$status" in
                timeout) printf 'TO' ;;
                not_available) printf 'NA' ;;
                failed) printf 'FAIL' ;;
                killed) printf 'KIL' ;;
                orphan) printf 'ORPH' ;;
                non_comparable) printf 'NC' ;;
                unsupported) printf 'UNSUP' ;;
            esac
            return 0
        fi
    done
    return 1
}

profiles_json="{}"
for profile in "${PROFILE_LIST[@]}"; do
    profile_lower="$(printf '%s' "$profile" | tr '[:upper:]' '[:lower:]')"
    engines_list="$(ladder_engines_for "$profile")"
    table_file="ladder_${profile_lower}.md"

    {
        echo "# OWL2Bench $profile Consistency Scalability Ladder"
        echo
        echo "- Lane: execution performance and completability evidence only (ADR 0024/0026)."
        echo "- Statistic: repetition median of container-inclusive wall time per engine across three serial passes; cells show \`<median>ms (completed/cases)\` (ADR 0025)."
        echo "- Rows: universities ascending. Columns: ADR 0025 engine set for profile $profile."
        echo "- Inputs: per-dataset comparative aggregations; run ids per point are recorded in ladder_metadata.json."
        echo
        printf '| Universities |'
        for engine in $engines_list; do
            printf ' %s |' "$engine"
        done
        echo
        printf '|---|'
        for _ in $engines_list; do
            printf -- '---:|'
        done
        echo

        for size in "${SIZE_LIST[@]}"; do
            point_key="$profile|$size"

            best=""
            for engine in $engines_list; do
                median="${CELL_MEDIAN[$point_key|$engine]:-}"
                completed="${CELL_COMPLETED[$point_key|$engine]}"
                [[ "$completed" -gt 0 ]] || continue
                if [[ -z "$best" ]] || [[ "$(numeric_lt "$median" "$best")" == 1 ]]; then
                    best="$median"
                fi
            done

            printf '| %s |' "$size"
            for engine in $engines_list; do
                completed="${CELL_COMPLETED[$point_key|$engine]}"
                cases="${CELL_CASES[$point_key|$engine]}"
                median="${CELL_MEDIAN[$point_key|$engine]:-}"
                cell=""
                if [[ "$completed" -gt 0 ]]; then
                    cell="${median}ms (${completed}/${cases})"
                    if [[ -n "$best" && "$(numeric_eq "$median" "$best")" == 1 ]]; then
                        cell="**${cell}**"
                    fi
                else
                    point_median_file="${POINT_DIR[$point_key]}/comparative_median_times.tsv"
                    if ! code="$(cell_code_for "$point_median_file" "$engine")"; then
                        ladder_err "cannot render a cell code for $profile:$size engine $engine (no per-case execution statuses)"
                        exit 1
                    fi
                    cell="${code} (0/${cases})"
                fi
                printf ' %s |' "$cell"
            done
            echo
        done

        echo
        echo "Legend: \`<median>ms (completed/cases)\`; **bold** = best median among engines that completed; TO=timeout, NA=not available, NC=non-comparable parser/runtime incompatibility, KIL=interrupted cleanup kill, ORPH=container orphan cleanup, FAIL=hard runtime failure."
    } > "$OUTPUT_DIR/$table_file"
    ladder_log "ladder table written: $OUTPUT_DIR/$table_file"

    points_json="["
    first_point=1
    for size in "${SIZE_LIST[@]}"; do
        point_key="$profile|$size"
        point_obj="$(jq -nc \
            --argjson universities "$size" \
            --arg aggregation_dir "${POINT_DIR[$point_key]}" \
            --argjson pass_run_ids "${POINT_RUN_IDS[$point_key]}" \
            '{universities: $universities, aggregation_dir: $aggregation_dir, pass_run_ids: $pass_run_ids}')"
        if [[ "$first_point" == "1" ]]; then
            points_json="$points_json$point_obj"
            first_point=0
        else
            points_json="$points_json,$point_obj"
        fi
    done
    points_json="$points_json]"

    profile_obj="$(jq -nc \
        --argjson engines "$(printf '%s\n' $engines_list | jq -R . | jq -s .)" \
        --arg table "$table_file" \
        --argjson points "$points_json" \
        '{engines: $engines, table: $table, points: $points}')"
    profiles_json="$(jq -nc --argjson profiles "$profiles_json" --arg profile "$profile" --argjson obj "$profile_obj" \
        '$profiles + {($profile): $obj}')"
done

expected_profiles_json="$(printf '%s\n' "${PROFILE_LIST[@]}" | jq -R . | jq -s .)"
expected_sizes_json="$(printf '%s\n' "${SIZE_LIST[@]}" | jq -R . | jq -s . | jq 'map(tonumber)')"

jq -n \
    --arg ladder_type "owl2bench_consistency_scalability" \
    --arg lane_terms "execution performance and completability evidence only (ADR 0024/0026)" \
    --arg statistic "repetition median of container-inclusive wall time per engine across three serial passes; completability = completed/cases" \
    --argjson expected_profiles "$expected_profiles_json" \
    --argjson expected_sizes "$expected_sizes_json" \
    --argjson profiles "$profiles_json" \
    '{
        ladder_type: $ladder_type,
        lane_terms: $lane_terms,
        statistic: $statistic,
        expected: {profiles: $expected_profiles, sizes: $expected_sizes},
        profiles: $profiles,
        legend: [
            "<median>ms (completed/cases)",
            "TO=timeout",
            "NA=not available",
            "NC=non-comparable parser/runtime incompatibility",
            "KIL=interrupted cleanup kill",
            "ORPH=container orphan cleanup",
            "FAIL=hard runtime failure"
        ]
    }' > "$OUTPUT_DIR/ladder_metadata.json"
ladder_log "ladder metadata written: $OUTPUT_DIR/ladder_metadata.json"

ladder_log "ladder view complete: $OUTPUT_DIR"
