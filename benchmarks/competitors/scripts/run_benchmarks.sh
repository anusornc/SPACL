#!/usr/bin/env bash
#
# OWL reasoner head-to-head benchmark harness.
# Includes: SPACL, HermiT, Konclude, Openllet, ELK, JFact, Pellet.
#
# Fairness policy:
# - Every reasoner runs the same operation (`consistency` by default).
# - Primary metric is wall time measured around container execution.
# - Every invocation writes into an isolated run directory.
#

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BENCHMARK_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
PROJECT_ROOT="$(cd "$BENCHMARK_DIR/../.." && pwd)"

RESULTS_DIR="$BENCHMARK_DIR/results"
ONTOLOGIES_DIR="${ONTOLOGIES_DIR_OVERRIDE:-$BENCHMARK_DIR/ontologies}"
LARGE_ONTOLOGIES_DIR="${LARGE_ONTOLOGIES_DIR_OVERRIDE:-$PROJECT_ROOT/benchmarks/ontologies/other}"

TIMEOUT_SECONDS="${TIMEOUT_SECONDS:-300}"
OPERATION="${OPERATION:-consistency}"
ONTOLOGY_SUITE="${ONTOLOGY_SUITE:-standard}"   # standard | large | all
INCLUDE_CHEBI="${INCLUDE_CHEBI:-1}"            # include 773MB ChEBI when suite has large
ONTOLOGY_REGEX="${ONTOLOGY_REGEX:-}"           # optional filename regex filter

RUN_ID_PROVIDED=0
if [[ "${RUN_ID+x}" == "x" ]]; then
    RUN_ID_PROVIDED=1
fi
RUN_ID="${RUN_ID:-$(date +%Y%m%d_%H%M%S)}"
CLEAN_STALE_CONTAINERS="${CLEAN_STALE_CONTAINERS:-1}"

RUN_DIR="$RESULTS_DIR/history/$RUN_ID"
RUN_REPORT_FILE="$RUN_DIR/benchmark_report.md"
RUN_CSV_FILE="$RUN_DIR/results.csv"
RUN_PAPER_MD="$RUN_DIR/paper_table.md"
RUN_PAPER_TEX="$RUN_DIR/paper_table.tex"
RUN_ONTOLOGY_DIR="$RUN_DIR/ontologies"
LATEST_LINK="$RESULTS_DIR/latest"

DEFAULT_REASONERS=("tableauxx" "hermit" "konclude" "openllet" "elk" "jfact" "pellet")
if [[ "${INCLUDE_FACTPP:-0}" == "1" ]]; then
    DEFAULT_REASONERS+=("factpp")
fi

if [[ -n "${REASONERS_OVERRIDE:-}" ]]; then
    REASONERS_OVERRIDE_NORMALIZED="${REASONERS_OVERRIDE//,/ }"
    read -r -a REASONERS <<< "$REASONERS_OVERRIDE_NORMALIZED"
    if [[ "${#REASONERS[@]}" -eq 0 ]]; then
        log_warn "REASONERS_OVERRIDE is set but empty after parsing; using defaults."
        REASONERS=("${DEFAULT_REASONERS[@]}")
    fi
else
    REASONERS=("${DEFAULT_REASONERS[@]}")
fi

declare -A REASONER_BUILD_STATUS

FORWARDED_OWL_ENV_VARS=(
    OWL2_REASONER_LARGE_PARSE
    OWL2_REASONER_MAX_FILE_SIZE
    OWL2_REASONER_AUTO_CACHE
    OWL2_REASONER_FORCE_TEXT
    OWL2_REASONER_BIN_ONLY
    OWL2_REASONER_DISABLE_PARSE_FALLBACK
    OWL2_REASONER_ENABLE_PARSE_FALLBACK
    OWL2_REASONER_PARSE_PROGRESS_EVERY
    OWL2_REASONER_PARSE_IO_PROGRESS_BYTES
    OWL2_REASONER_EXPERIMENTAL_XML_PARSER
    OWL2_REASONER_EXPERIMENTAL_XML_STRICT
    OWL2_REASONER_EXPERIMENTAL_XML_BATCH
    OWL2_REASONER_EXPERIMENTAL_XML_QUEUE
    OWL2_REASONER_EXPERIMENTAL_XML_WORKERS
    OWL2_REASONER_EXPERIMENTAL_XML_CACHE
    OWL2_REASONER_STRUCTURAL_XML_PARSER
    OWL2_REASONER_STRUCTURAL_XML_AUTO
    OWL2_REASONER_STRUCTURAL_XML_AUTO_THRESHOLD
    OWL2_REASONER_STRUCTURAL_XML_INTERNER
    OWL2_REASONER_STRUCTURAL_BREAKDOWN
    OWL2_REASONER_LARGE_PROFILE_AUTO
    OWL2_REASONER_LARGE_PROFILE_THRESHOLD
    OWL2_REASONER_STAGE_TIMING
)

declare -a ACTIVE_CONTAINERS=()
declare -A ACTIVE_RESULT_FILE_BY_CONTAINER
declare -A ACTIVE_REASONER_BY_CONTAINER
declare -A ACTIVE_ONTOLOGY_BY_CONTAINER
declare -A ACTIVE_OPERATION_BY_CONTAINER
declare -A ACTIVE_START_NS_BY_CONTAINER
SHUTDOWN_REASON=""

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

log_info() { echo -e "${BLUE}[INFO]${NC} $1"; }
log_ok() { echo -e "${GREEN}[OK]${NC} $1"; }
log_warn() { echo -e "${YELLOW}[WARN]${NC} $1"; }
log_err() { echo -e "${RED}[ERR]${NC} $1"; }

register_active_container() {
    local container_name="$1"
    local result_file="$2"
    local reasoner="$3"
    local ontology_name="$4"
    local operation="$5"
    local start_ns="$6"

    ACTIVE_CONTAINERS+=("$container_name")
    ACTIVE_RESULT_FILE_BY_CONTAINER["$container_name"]="$result_file"
    ACTIVE_REASONER_BY_CONTAINER["$container_name"]="$reasoner"
    ACTIVE_ONTOLOGY_BY_CONTAINER["$container_name"]="$ontology_name"
    ACTIVE_OPERATION_BY_CONTAINER["$container_name"]="$operation"
    ACTIVE_START_NS_BY_CONTAINER["$container_name"]="$start_ns"
}

unregister_active_container() {
    local container_name="$1"
    local i
    local updated=()
    for i in "${ACTIVE_CONTAINERS[@]}"; do
        if [[ "$i" != "$container_name" ]]; then
            updated+=("$i")
        fi
    done
    ACTIVE_CONTAINERS=("${updated[@]}")
    unset ACTIVE_RESULT_FILE_BY_CONTAINER["$container_name"]
    unset ACTIVE_REASONER_BY_CONTAINER["$container_name"]
    unset ACTIVE_ONTOLOGY_BY_CONTAINER["$container_name"]
    unset ACTIVE_OPERATION_BY_CONTAINER["$container_name"]
    unset ACTIVE_START_NS_BY_CONTAINER["$container_name"]
}

require_cmd() {
    if ! command -v "$1" >/dev/null 2>&1; then
        log_err "Missing required command: $1"
        exit 1
    fi
}

ensure_run_dirs() {
    mkdir -p "$RESULTS_DIR" "$RESULTS_DIR/history" "$RUN_DIR" "$RUN_ONTOLOGY_DIR"
}

stage_ontology() {
    local src="$1"
    local dst="$RUN_ONTOLOGY_DIR/$(basename "$src")"
    if [[ ! -f "$src" ]]; then
        return
    fi
    if ! ln -f "$src" "$dst" 2>/dev/null; then
        cp -f "$src" "$dst"
    fi

    # Stage optional binary cache next to the source ontology for fast-load runs.
    local src_bin="${src%.owl}.owlbin"
    if [[ -f "$src_bin" ]]; then
        local dst_bin="$RUN_ONTOLOGY_DIR/$(basename "$src_bin")"
        if ! ln -f "$src_bin" "$dst_bin" 2>/dev/null; then
            cp -f "$src_bin" "$dst_bin"
        fi
    fi
}

prepare_ontologies() {
    ensure_run_dirs

    rm -f "$RUN_ONTOLOGY_DIR"/*.owl "$RUN_ONTOLOGY_DIR"/*.owlbin 2>/dev/null || true

    case "$ONTOLOGY_SUITE" in
        standard|all)
            for owl in "$ONTOLOGIES_DIR"/*.owl; do
                [[ -e "$owl" ]] || continue
                stage_ontology "$owl"
            done
            if [[ -d "$PROJECT_ROOT/tests/data" ]]; then
                for owl in "$PROJECT_ROOT/tests/data"/*.owl; do
                    [[ -e "$owl" ]] || continue
                    stage_ontology "$owl"
                done
            fi
            ;;
    esac

    case "$ONTOLOGY_SUITE" in
        large|all)
            if [[ ! -d "$LARGE_ONTOLOGIES_DIR" ]]; then
                log_err "Large ontology directory not found: $LARGE_ONTOLOGIES_DIR"
                exit 1
            fi
            for owl in "$LARGE_ONTOLOGIES_DIR"/*.owl; do
                [[ -e "$owl" ]] || continue
                if [[ "$(basename "$owl")" == "chebi.owl" && "$INCLUDE_CHEBI" != "1" ]]; then
                    continue
                fi
                stage_ontology "$owl"
            done
            ;;
    esac

    case "$ONTOLOGY_SUITE" in
        standard|large|all) ;;
        *)
            log_err "Unsupported ONTOLOGY_SUITE: $ONTOLOGY_SUITE (use standard|large|all)"
            exit 1
            ;;
    esac

    if [[ -n "$ONTOLOGY_REGEX" ]]; then
        local f base
        for f in "$RUN_ONTOLOGY_DIR"/*.owl; do
            [[ -e "$f" ]] || continue
            base="$(basename "$f")"
            if [[ ! "$base" =~ $ONTOLOGY_REGEX ]]; then
                rm -f "$f"
                rm -f "${f%.owl}.owlbin"
            fi
        done
    fi

    mapfile -t staged_ontologies < <(find "$RUN_ONTOLOGY_DIR" -maxdepth 1 -type f -name "*.owl" | sort)
    if [[ "${#staged_ontologies[@]}" -eq 0 ]]; then
        log_err "No OWL ontologies staged in $RUN_ONTOLOGY_DIR"
        exit 1
    fi

    printf '%s\n' "${staged_ontologies[@]##*/}" > "$RUN_DIR/ontology_manifest.txt"
    log_info "Ontology suite '$ONTOLOGY_SUITE' ready: ${#staged_ontologies[@]} files"
}

write_run_metadata() {
    ensure_run_dirs
    jq -n \
        --arg run_id "$RUN_ID" \
        --arg started_at "$(date -Iseconds)" \
        --arg ontology_suite "$ONTOLOGY_SUITE" \
        --arg ontologies_dir "$ONTOLOGIES_DIR" \
        --arg large_ontologies_dir "$LARGE_ONTOLOGIES_DIR" \
        --arg operation "$OPERATION" \
        --argjson timeout_seconds "$TIMEOUT_SECONDS" \
        --argjson include_chebi "$INCLUDE_CHEBI" \
        --arg run_dir "$RUN_DIR" \
        --argjson reasoners "$(printf '%s\n' "${REASONERS[@]}" | jq -R . | jq -s .)" \
        '{
            run_id: $run_id,
            started_at: $started_at,
            ontology_suite: $ontology_suite,
            ontologies_dir: $ontologies_dir,
            large_ontologies_dir: $large_ontologies_dir,
            operation: $operation,
            timeout_seconds: $timeout_seconds,
            include_chebi: ($include_chebi == 1),
            reasoners: $reasoners,
            run_dir: $run_dir,
            fairness_policy: {
              primary_metric: "wall_time_ms",
              operation: "single_operation_per_run",
              isolation: "run_scoped_results_directory"
            }
         }' > "$RUN_DIR/run_metadata.json"
}

build_images() {
    log_info "Building benchmark images (timeout per run: ${TIMEOUT_SECONDS}s)"
    cd "$BENCHMARK_DIR"

    for reasoner in "${REASONERS[@]}"; do
        local dockerfile="docker/Dockerfile.${reasoner}"
        if [[ ! -f "$dockerfile" ]]; then
            REASONER_BUILD_STATUS["$reasoner"]="missing_dockerfile"
            log_warn "$reasoner: Dockerfile missing ($dockerfile)"
            continue
        fi

        log_info "Building image: owl-reasoner-${reasoner}"
        if docker build -f "$dockerfile" -t "owl-reasoner-${reasoner}" "$PROJECT_ROOT" >/tmp/benchmark-build-"$reasoner".log 2>&1; then
            REASONER_BUILD_STATUS["$reasoner"]="ready"
            log_ok "$reasoner image built"
        else
            REASONER_BUILD_STATUS["$reasoner"]="build_failed"
            log_err "$reasoner image build failed (see /tmp/benchmark-build-${reasoner}.log)"
        fi
    done
}

detect_existing_images() {
    mapfile -t existing_images < <(docker images --format '{{.Repository}}' | sort -u)
    for reasoner in "${REASONERS[@]}"; do
        if printf '%s\n' "${existing_images[@]}" | grep -qx "owl-reasoner-${reasoner}"; then
            REASONER_BUILD_STATUS["$reasoner"]="ready"
        else
            REASONER_BUILD_STATUS["$reasoner"]="missing_image"
        fi
    done
}

write_result_json() {
    local result_file="$1"
    local reasoner="$2"
    local ontology_name="$3"
    local wall_ms="$4"
    local reported_ms="$5"
    local status="$6"
    local reasoning_result="$7"
    local operation="$8"
    local parse_ms="$9"
    local reason_ms="${10}"
    local tmp_output="${11}"

    jq -n \
        --arg run_id "$RUN_ID" \
        --arg ontology_suite "$ONTOLOGY_SUITE" \
        --arg reasoner "$reasoner" \
        --arg ontology "$ontology_name" \
        --arg operation "$operation" \
        --arg status "$status" \
        --arg reasoning_result "$reasoning_result" \
        --arg timestamp "$(date -Iseconds)" \
        --argjson wall_time_ms "$wall_ms" \
        --argjson reported_time_ms "$reported_ms" \
        --argjson parse_ms "$parse_ms" \
        --argjson reason_ms "$reason_ms" \
        --rawfile output "$tmp_output" \
        '{
            run_id: $run_id,
            ontology_suite: $ontology_suite,
            reasoner: $reasoner,
            ontology: $ontology,
            operation: $operation,
            wall_time_ms: $wall_time_ms,
            reported_time_ms: $reported_time_ms,
            parse_time_ms: $parse_ms,
            reason_time_ms: $reason_ms,
            status: $status,
            reasoning_result: $reasoning_result,
            timestamp: $timestamp,
            output: $output
         }' > "$result_file"
}

cleanup_active_containers() {
    local reason="$1"
    if [[ "${#ACTIVE_CONTAINERS[@]}" -eq 0 ]]; then
        return
    fi

    local container_name result_file reasoner ontology_name operation start_ns now_ns wall_ms
    local tmp_output
    for container_name in "${ACTIVE_CONTAINERS[@]}"; do
        result_file="${ACTIVE_RESULT_FILE_BY_CONTAINER[$container_name]:-}"
        reasoner="${ACTIVE_REASONER_BY_CONTAINER[$container_name]:-unknown}"
        ontology_name="${ACTIVE_ONTOLOGY_BY_CONTAINER[$container_name]:-unknown.owl}"
        operation="${ACTIVE_OPERATION_BY_CONTAINER[$container_name]:-$OPERATION}"
        start_ns="${ACTIVE_START_NS_BY_CONTAINER[$container_name]:-0}"
        now_ns="$(date +%s%N)"

        if [[ "$start_ns" =~ ^[0-9]+$ ]] && [[ "$start_ns" -gt 0 ]]; then
            wall_ms=$(( (now_ns - start_ns) / 1000000 ))
        else
            wall_ms=-1
        fi

        tmp_output="$(mktemp)"
        docker logs "$container_name" > "$tmp_output" 2>&1 || true
        docker rm -f "$container_name" >/dev/null 2>&1 || true

        if [[ -n "$result_file" && ! -f "$result_file" ]]; then
            write_result_json \
                "$result_file" \
                "$reasoner" \
                "$ontology_name" \
                "$wall_ms" \
                "$wall_ms" \
                "$reason" \
                "unknown" \
                "$operation" \
                -1 \
                -1 \
                "$tmp_output"
            log_warn "$reasoner on $ontology_name: marked as $reason during cleanup"
        fi
        rm -f "$tmp_output"
    done
    ACTIVE_CONTAINERS=()
}

on_exit() {
    local exit_code="$1"
    local reason=""
    if [[ "$exit_code" -eq 0 ]]; then
        reason="orphan"
    elif [[ "$SHUTDOWN_REASON" == "interrupted" ]]; then
        reason="killed"
    else
        reason="failed"
    fi
    cleanup_active_containers "$reason"
}

on_interrupt() {
    SHUTDOWN_REASON="interrupted"
    exit 130
}

cleanup_stale_run_containers() {
    if [[ "$CLEAN_STALE_CONTAINERS" != "1" ]]; then
        return
    fi

    local prefix="owlbench_${RUN_ID}_"
    local names=()
    mapfile -t names < <(docker ps -a --format '{{.Names}}' | grep -E "^${prefix}" || true)
    if [[ "${#names[@]}" -eq 0 ]]; then
        return
    fi

    log_warn "Found ${#names[@]} stale containers for RUN_ID=$RUN_ID; removing..."
    local name
    for name in "${names[@]}"; do
        docker rm -f "$name" >/dev/null 2>&1 || true
    done
}

run_single_benchmark() {
    local reasoner="$1"
    local ontology_path="$2"
    local operation="${3:-consistency}"
    local ontology_name
    ontology_name="$(basename "$ontology_path")"
    local result_file="$RUN_DIR/${reasoner}_${ontology_name%.owl}.json"

    if [[ "${REASONER_BUILD_STATUS[$reasoner]:-unknown}" != "ready" ]]; then
        local build_state="${REASONER_BUILD_STATUS[$reasoner]:-unknown}"
        local tmp_msg
        tmp_msg="$(mktemp)"
        echo "Reasoner unavailable: ${build_state}" > "$tmp_msg"
        write_result_json "$result_file" "$reasoner" "$ontology_name" -1 -1 "not_available" "unknown" "$operation" -1 -1 "$tmp_msg"
        rm -f "$tmp_msg"
        log_warn "$reasoner on $ontology_name: not available ($build_state)"
        return
    fi

    log_info "Running $reasoner on $ontology_name"
    local tmp_output
    tmp_output="$(mktemp)"
    local start_ns end_ns wall_ms cmd_status
    local reported_ms reported_status reasoning_result final_status parse_ms reason_ms
    local container_name
    local exit_code_file wait_status
    container_name="owlbench_${RUN_ID}_${reasoner}_${ontology_name%.owl}"
    container_name="${container_name//[^a-zA-Z0-9_.-]/_}"
    exit_code_file="$(mktemp)"

    start_ns="$(date +%s%N)"
    docker rm -f "$container_name" >/dev/null 2>&1 || true

    local -a docker_env_args=()
    local env_name
    for env_name in "${FORWARDED_OWL_ENV_VARS[@]}"; do
        if [[ "${!env_name+x}" == "x" ]]; then
            docker_env_args+=(-e "${env_name}=${!env_name}")
        fi
    done

    if docker run -d --name "$container_name" \
        "${docker_env_args[@]}" \
        -v "$RUN_ONTOLOGY_DIR:/ontologies:ro" \
        "owl-reasoner-$reasoner" \
        "/ontologies/$ontology_name" "$operation" >/dev/null 2>&1; then
        register_active_container "$container_name" "$result_file" "$reasoner" "$ontology_name" "$operation" "$start_ns"

        if timeout --kill-after=5s "$TIMEOUT_SECONDS" docker wait "$container_name" > "$exit_code_file" 2>/dev/null; then
            wait_status=0
            cmd_status="$(tr -d '[:space:]' < "$exit_code_file")"
            if [[ -z "$cmd_status" || ! "$cmd_status" =~ ^[0-9]+$ ]]; then
                cmd_status=1
            fi
            docker logs "$container_name" > "$tmp_output" 2>&1 || true
            docker rm "$container_name" >/dev/null 2>&1 || true
            unregister_active_container "$container_name"
        else
            wait_status=$?
            if [[ "$wait_status" -eq 124 || "$wait_status" -eq 137 || "$wait_status" -eq 143 ]]; then
                cmd_status=124
            else
                cmd_status=1
            fi
            docker logs "$container_name" > "$tmp_output" 2>&1 || true
            docker rm -f "$container_name" >/dev/null 2>&1 || true
            unregister_active_container "$container_name"
        fi
    else
        cmd_status=1
        echo "Failed to start container $container_name" > "$tmp_output"
    fi
    end_ns="$(date +%s%N)"
    wall_ms=$(( (end_ns - start_ns) / 1000000 ))
    rm -f "$exit_code_file"

    reported_ms="$(grep -Eo '"duration_ms"[[:space:]]*:[[:space:]]*-?[0-9]+' "$tmp_output" | tail -1 | grep -Eo -- '-?[0-9]+' || true)"
    reported_status="$(grep -Eo '"status"[[:space:]]*:[[:space:]]*"[^"]+"' "$tmp_output" | tail -1 | sed -E 's/.*"status"[[:space:]]*:[[:space:]]*"([^"]+)".*/\1/' || true)"
    reasoning_result="$(grep -Eo '"reasoning_result"[[:space:]]*:[[:space:]]*"[^"]+"' "$tmp_output" | tail -1 | sed -E 's/.*"reasoning_result"[[:space:]]*:[[:space:]]*"([^"]+)".*/\1/' || true)"
    parse_ms="$(grep -Eo '"parse_time_ms"[[:space:]]*:[[:space:]]*-?[0-9]+' "$tmp_output" | tail -1 | grep -Eo -- '-?[0-9]+' || true)"
    reason_ms="$(grep -Eo '"reason_time_ms"[[:space:]]*:[[:space:]]*-?[0-9]+' "$tmp_output" | tail -1 | grep -Eo -- '-?[0-9]+' || true)"

    if [[ -z "$reported_ms" || ! "$reported_ms" =~ ^-?[0-9]+$ ]]; then
        reported_ms="$wall_ms"
    fi
    if [[ -z "$reasoning_result" ]]; then
        reasoning_result="unknown"
    fi
    if [[ -z "$parse_ms" || ! "$parse_ms" =~ ^-?[0-9]+$ ]]; then
        parse_ms=-1
    fi
    if [[ -z "$reason_ms" || ! "$reason_ms" =~ ^-?[0-9]+$ ]]; then
        reason_ms=-1
    fi
    if [[ "$parse_ms" -lt 0 ]]; then
        parse_ms="$(grep -Eo '\[phase\][[:space:]]+parse_done[[:space:]]+ms=[0-9]+' "$tmp_output" | tail -1 | grep -Eo '[0-9]+' || true)"
        if [[ -z "$parse_ms" || ! "$parse_ms" =~ ^[0-9]+$ ]]; then
            parse_ms=-1
        fi
    fi
    if [[ "$reason_ms" -lt 0 ]]; then
        reason_ms="$(grep -Eo '\[phase\][[:space:]]+reason_done[[:space:]]+ms=[0-9]+' "$tmp_output" | tail -1 | grep -Eo '[0-9]+' || true)"
        if [[ -z "$reason_ms" || ! "$reason_ms" =~ ^[0-9]+$ ]]; then
            reason_ms=-1
        fi
    fi

    if [[ "$cmd_status" -eq 124 ]]; then
        final_status="timeout"
    elif [[ "$reported_status" == "not_available" ]]; then
        final_status="not_available"
    elif [[ "$cmd_status" -ne 0 ]]; then
        final_status="failed"
    elif [[ "$reported_status" == "failed" || "$reported_status" == "unsupported_operation" ]]; then
        final_status="$reported_status"
    else
        final_status="success"
    fi

    write_result_json "$result_file" "$reasoner" "$ontology_name" "$wall_ms" "$reported_ms" "$final_status" "$reasoning_result" "$operation" "$parse_ms" "$reason_ms" "$tmp_output"
    rm -f "$tmp_output"

    case "$final_status" in
        success)
            log_ok "$reasoner on $ontology_name: ${wall_ms}ms (reported ${reported_ms}ms)"
            ;;
        not_available)
            log_warn "$reasoner on $ontology_name: not available"
            ;;
        timeout)
            log_warn "$reasoner on $ontology_name: timed out after ${TIMEOUT_SECONDS}s"
            ;;
        *)
            log_err "$reasoner on $ontology_name: $final_status"
            ;;
    esac
}

run_all_benchmarks() {
    mapfile -t ontologies < <(find "$RUN_ONTOLOGY_DIR" -maxdepth 1 -type f -name "*.owl" | sort)
    if [[ "${#ontologies[@]}" -eq 0 ]]; then
        log_err "No OWL ontologies found in $RUN_ONTOLOGY_DIR"
        exit 1
    fi

    log_info "Run ID: $RUN_ID"
    log_info "Suite: $ONTOLOGY_SUITE, operation: $OPERATION, timeout: ${TIMEOUT_SECONDS}s"
    log_info "Ontologies: ${#ontologies[@]}, reasoners: ${#REASONERS[@]}"
    for ontology in "${ontologies[@]}"; do
        for reasoner in "${REASONERS[@]}"; do
            run_single_benchmark "$reasoner" "$ontology" "$OPERATION"
        done
    done
}

status_code_for_cell() {
    local status="$1"
    case "$status" in
        timeout) echo "TO" ;;
        not_available) echo "NA" ;;
        killed) echo "KIL" ;;
        orphan) echo "ORPH" ;;
        failed|unsupported_operation) echo "FAIL" ;;
        *) echo "?" ;;
    esac
}

generate_csv() {
    local csv_file="$RUN_CSV_FILE"
    echo "run_id,ontology_suite,reasoner,ontology,operation,status,reasoning_result,wall_time_ms,reported_time_ms,parse_time_ms,reason_time_ms,timestamp" > "$csv_file"
    local file
    mapfile -t result_files < <(find "$RUN_DIR" -maxdepth 1 -type f -name "*.json" ! -name "run_metadata.json" | sort)
    for file in "${result_files[@]}"; do
        jq -r '[.run_id,.ontology_suite,.reasoner,.ontology,.operation,.status,.reasoning_result,.wall_time_ms,.reported_time_ms,.parse_time_ms,.reason_time_ms,.timestamp] | @csv' "$file" >> "$csv_file"
    done
    log_ok "CSV generated: $csv_file"
}

generate_paper_tables() {
    local md_file="$RUN_PAPER_MD"
    local tex_file="$RUN_PAPER_TEX"

    mapfile -t ontologies < <(find "$RUN_DIR" -maxdepth 1 -type f -name "*.json" ! -name "run_metadata.json" -print0 | xargs -0 -I{} jq -r '.ontology' "{}" | sort -u)
    if [[ "${#ontologies[@]}" -eq 0 ]]; then
        log_warn "No result JSON files found for paper table generation"
        return
    fi

    {
        echo "# Head-to-Head Wall Time (ms)"
        echo
        echo "- Run ID: \`$RUN_ID\`"
        echo "- Suite: \`$ONTOLOGY_SUITE\`"
        echo "- Operation: \`$OPERATION\`"
        echo "- Metric: wall time in milliseconds (container-level; parse + reasoning + startup)"
        echo
        printf '| Ontology |'
        for reasoner in "${REASONERS[@]}"; do
            printf ' %s |' "$reasoner"
        done
        echo
        printf '|---|'
        for _ in "${REASONERS[@]}"; do
            printf -- '---:|'
        done
        echo

        local ontology reasoner status wall best wall_candidate
        for ontology in "${ontologies[@]}"; do
            best=9223372036854775807
            for reasoner in "${REASONERS[@]}"; do
                local file="$RUN_DIR/${reasoner}_${ontology%.owl}.json"
                if [[ -f "$file" ]]; then
                    status="$(jq -r '.status' "$file")"
                    wall_candidate="$(jq -r '.wall_time_ms' "$file")"
                    if [[ "$status" == "success" && "$wall_candidate" =~ ^[0-9]+$ && "$wall_candidate" -lt "$best" ]]; then
                        best="$wall_candidate"
                    fi
                fi
            done

            printf '| %s |' "$ontology"
            for reasoner in "${REASONERS[@]}"; do
                local file="$RUN_DIR/${reasoner}_${ontology%.owl}.json"
                if [[ ! -f "$file" ]]; then
                    printf ' - |'
                    continue
                fi
                status="$(jq -r '.status' "$file")"
                wall="$(jq -r '.wall_time_ms' "$file")"
                if [[ "$status" == "success" ]]; then
                    if [[ "$wall" -eq "$best" ]]; then
                        printf ' **%s** |' "$wall"
                    else
                        printf ' %s |' "$wall"
                    fi
                else
                    printf ' %s |' "$(status_code_for_cell "$status")"
                fi
            done
            echo
        done

        echo
        echo "Legend: **best wall time**, TO=timeout, NA=not available, KIL=interrupted cleanup kill, ORPH=container orphan cleanup, FAIL=runtime/parser failure."
    } > "$md_file"
    log_ok "Paper markdown table generated: $md_file"

    {
        echo "\\begin{table}[t]"
        echo "\\centering"
        echo "\\caption{Head-to-head consistency benchmark (wall time, ms)}"
        echo "\\label{tab:head_to_head_wall_ms}"
        printf '\\begin{tabular}{l'
        for _ in "${REASONERS[@]}"; do
            printf 'r'
        done
        echo "}"
        echo "\\toprule"
        printf 'Ontology'
        for reasoner in "${REASONERS[@]}"; do
            printf ' & %s' "$reasoner"
        done
        echo " \\\\"
        echo "\\midrule"

        local ontology reasoner status wall best
        for ontology in "${ontologies[@]}"; do
            best=9223372036854775807
            for reasoner in "${REASONERS[@]}"; do
                local file="$RUN_DIR/${reasoner}_${ontology%.owl}.json"
                if [[ -f "$file" ]]; then
                    status="$(jq -r '.status' "$file")"
                    wall="$(jq -r '.wall_time_ms' "$file")"
                    if [[ "$status" == "success" && "$wall" =~ ^[0-9]+$ && "$wall" -lt "$best" ]]; then
                        best="$wall"
                    fi
                fi
            done

            printf '%s' "${ontology//_/\\_}"
            for reasoner in "${REASONERS[@]}"; do
                local file="$RUN_DIR/${reasoner}_${ontology%.owl}.json"
                if [[ ! -f "$file" ]]; then
                    printf ' & --'
                    continue
                fi
                status="$(jq -r '.status' "$file")"
                wall="$(jq -r '.wall_time_ms' "$file")"
                if [[ "$status" == "success" ]]; then
                    if [[ "$wall" -eq "$best" ]]; then
                        printf ' & \\textbf{%s}' "$wall"
                    else
                        printf ' & %s' "$wall"
                    fi
                else
                    printf ' & %s' "$(status_code_for_cell "$status")"
                fi
            done
            echo " \\\\"
        done

        echo "\\bottomrule"
        echo "\\end{tabular}"
        echo "\\end{table}"
    } > "$tex_file"
    log_ok "Paper LaTeX table generated: $tex_file"
}

generate_report() {
    local report_file="$RUN_REPORT_FILE"

    mapfile -t result_files < <(find "$RUN_DIR" -maxdepth 1 -type f -name "*.json" ! -name "run_metadata.json" | sort)
    if [[ "${#result_files[@]}" -eq 0 ]]; then
        log_err "No result JSON files found in $RUN_DIR"
        exit 1
    fi

    {
        echo "# OWL Reasoner Head-to-Head Report"
        echo
        echo "- Run ID: $RUN_ID"
        echo "- Generated: $(date -Iseconds)"
        echo "- Suite: $ONTOLOGY_SUITE"
        echo "- Operation: $OPERATION"
        echo "- Timeout per run: ${TIMEOUT_SECONDS}s"
        echo "- Reasoners: ${REASONERS[*]}"
        echo "- Primary metric: wall time (ms)"
        echo
        echo "## Summary by Reasoner"
        echo
        echo "| Reasoner | Success | Failed | Timeout | Not Available | Killed | Orphan | Avg Wall Time (ms) |"
        echo "|---|---:|---:|---:|---:|---:|---:|---:|"

        local reasoner
        for reasoner in "${REASONERS[@]}"; do
            mapfile -t files < <(find "$RUN_DIR" -maxdepth 1 -type f -name "${reasoner}_*.json" | sort)
            if [[ "${#files[@]}" -eq 0 ]]; then
                continue
            fi

            local success=0 failed=0 timeout=0 not_available=0 killed=0 orphan=0
            local total_ms=0 total_count=0
            local f st ms
            for f in "${files[@]}"; do
                st="$(jq -r '.status' "$f")"
                ms="$(jq -r '.wall_time_ms' "$f")"
                case "$st" in
                    success) success=$((success + 1)) ;;
                    timeout) timeout=$((timeout + 1)) ;;
                    not_available) not_available=$((not_available + 1)) ;;
                    killed) killed=$((killed + 1)) ;;
                    orphan) orphan=$((orphan + 1)) ;;
                    *) failed=$((failed + 1)) ;;
                esac
                if [[ "$st" == "success" ]] && [[ "$ms" =~ ^[0-9]+$ ]] && [[ "$ms" -ge 0 ]]; then
                    total_ms=$((total_ms + ms))
                    total_count=$((total_count + 1))
                fi
            done

            local avg="N/A"
            if [[ "$total_count" -gt 0 ]]; then
                avg=$((total_ms / total_count))
            fi
            echo "| $reasoner | $success | $failed | $timeout | $not_available | $killed | $orphan | $avg |"
        done

        echo
        echo "## Detailed Results"
        echo
        echo "| Reasoner | Ontology | Wall (ms) | Reported (ms) | Status | Reasoning Result |"
        echo "|---|---|---:|---:|---|---|"

        local file reasoner ontology wall reported status result
        for file in "${result_files[@]}"; do
            reasoner="$(jq -r '.reasoner' "$file")"
            ontology="$(jq -r '.ontology' "$file")"
            wall="$(jq -r '.wall_time_ms' "$file")"
            reported="$(jq -r '.reported_time_ms' "$file")"
            status="$(jq -r '.status' "$file")"
            result="$(jq -r '.reasoning_result' "$file")"
            echo "| $reasoner | $ontology | $wall | $reported | $status | $result |"
        done

        echo
        echo "## Notes"
        echo
        echo "- Wall time is the primary metric for cross-engine comparison."
        echo "- OWLAPI/Konclude internal reported times are kept as secondary diagnostics."
        echo "- \`pellet\` may appear as \`not_available\` when legacy artifact repositories are unavailable."
    } > "$report_file"

    generate_csv
    generate_paper_tables

    ln -sfn "$RUN_DIR" "$LATEST_LINK"
    cp "$report_file" "$RESULTS_DIR/benchmark_report_${RUN_ID}.md"

    log_ok "Report generated: $report_file"
    log_ok "Latest results link updated: $LATEST_LINK"
}

report_existing_run() {
    if [[ "$RUN_ID_PROVIDED" -eq 0 ]]; then
        if [[ -L "$LATEST_LINK" ]]; then
            RUN_DIR="$(readlink -f "$LATEST_LINK")"
            RUN_ID="$(basename "$RUN_DIR")"
            RUN_REPORT_FILE="$RUN_DIR/benchmark_report.md"
            RUN_CSV_FILE="$RUN_DIR/results.csv"
            RUN_PAPER_MD="$RUN_DIR/paper_table.md"
            RUN_PAPER_TEX="$RUN_DIR/paper_table.tex"
            RUN_ONTOLOGY_DIR="$RUN_DIR/ontologies"
            log_info "Using latest run: $RUN_ID"
        else
            log_err "No latest run found and RUN_ID not provided."
            exit 1
        fi
    fi
    generate_report
}

main() {
    require_cmd docker
    require_cmd jq
    require_cmd timeout
    trap on_interrupt INT TERM
    trap 'on_exit $?' EXIT

    case "${1:-all}" in
        prepare)
            prepare_ontologies
            write_run_metadata
            ;;
        build)
            prepare_ontologies
            write_run_metadata
            build_images
            ;;
        run)
            cleanup_stale_run_containers
            prepare_ontologies
            write_run_metadata
            if [[ "${SKIP_BUILD:-0}" == "1" ]]; then
                log_info "SKIP_BUILD=1, using existing images only"
                detect_existing_images
            else
                build_images
            fi
            run_all_benchmarks
            ;;
        report)
            report_existing_run
            ;;
        all)
            cleanup_stale_run_containers
            prepare_ontologies
            write_run_metadata
            if [[ "${SKIP_BUILD:-0}" == "1" ]]; then
                log_info "SKIP_BUILD=1, using existing images only"
                detect_existing_images
            else
                build_images
            fi
            run_all_benchmarks
            generate_report
            ;;
        *)
            echo "Usage: $0 {prepare|build|run|report|all}"
            echo "  Optional env:"
            echo "    REASONERS_OVERRIDE=tableauxx,hermit,konclude,openllet,elk,jfact,pellet"
            echo "    INCLUDE_FACTPP=1"
            echo "    TIMEOUT_SECONDS=300"
            echo "    SKIP_BUILD=1"
            echo "    OPERATION=consistency"
            echo "    ONTOLOGY_SUITE=standard|large|all"
            echo "    ONTOLOGY_REGEX='(doid|pato)\\.owl'"
            echo "    INCLUDE_CHEBI=1"
            echo "    ONTOLOGIES_DIR_OVERRIDE=/path/to/standard_owl_dir"
            echo "    LARGE_ONTOLOGIES_DIR_OVERRIDE=/path/to/large_owl_dir"
            echo "    RUN_ID=<custom_run_id>"
            echo "    CLEAN_STALE_CONTAINERS=1"
            echo "    OWL2_REASONER_EXPERIMENTAL_XML_PARSER=1"
            echo "    OWL2_REASONER_EXPERIMENTAL_XML_STRICT=1"
            exit 1
            ;;
    esac
}

main "$@"
