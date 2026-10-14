#!/usr/bin/env bash
#
# OWL reasoner head-to-head benchmark harness.
# Includes: Tableauxx, HermiT, Konclude, Openllet, ELK, JFact, Pellet.
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

source "$SCRIPT_DIR/benchmark_result.sh"

RESULTS_DIR="${RESULTS_DIR_OVERRIDE:-$BENCHMARK_DIR/results}"
ONTOLOGIES_DIR="${ONTOLOGIES_DIR_OVERRIDE:-$BENCHMARK_DIR/ontologies}"
LARGE_ONTOLOGIES_DIR="${LARGE_ONTOLOGIES_DIR_OVERRIDE:-$PROJECT_ROOT/benchmarks/ontologies/other}"

TIMEOUT_SECONDS="${TIMEOUT_SECONDS:-300}"
OPERATION="${OPERATION:-consistency}"
ONTOLOGY_SUITE="${ONTOLOGY_SUITE:-standard}"   # standard | large | all
INCLUDE_CHEBI="${INCLUDE_CHEBI:-1}"            # include 773MB ChEBI when suite has large
ONTOLOGY_REGEX="${ONTOLOGY_REGEX:-}"           # optional filename regex filter
ORACLE_STATUS="${ORACLE_STATUS:-not_applicable}"
ORACLE_SOURCE="${ORACLE_SOURCE:-}"
CORRECTNESS_STATUS="${CORRECTNESS_STATUS:-not_evaluated}"

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
RUN_KONCLUDE_INPUT_DIR="$RUN_DIR/konclude_inputs"
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

# Engine provenance (ADR 0025): every executed engine is pinned in run
# metadata by its docker image digest and its engine-reported version.
declare -A ENGINE_IMAGE_DIGEST
declare -A ENGINE_VERSION
declare -A ENGINE_EXECUTED

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
    SPACL_SCHED_MODE
    SPACL_BRANCH_POLICY
    SPACL_RRN_MODEL_PATH
    SPACL_BRANCH_SNAPSHOT_FILE
    SPACL_NOGOOD
    SPACL_EMIT_STATS
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

prepare_konclude_input() {
    local ontology_path="$1"
    local ontology_name prepared_name prepared_path
    ontology_name="$(basename "$ontology_path")"
    prepared_name="${ontology_name%.owl}.konclude.owlxml.owl"
    prepared_path="$RUN_KONCLUDE_INPUT_DIR/$prepared_name"

    mkdir -p "$RUN_KONCLUDE_INPUT_DIR"

    if [[ -f "$prepared_path" ]]; then
        printf '%s\n' "$prepared_path"
        return 0
    fi

    log_info "Preprocessing Konclude input for $ontology_name (merge imports + OWL/XML)"
    if docker run --rm --entrypoint java \
        -v "$PROJECT_ROOT:/work:ro" \
        -v "$RUN_KONCLUDE_INPUT_DIR:/out" \
        owl-reasoner-openllet \
        -cp "/opt/reasoner/bin:/opt/reasoner/target/dependency/*" \
        OwlapiFormatConverter \
        "/work/${ontology_path#$PROJECT_ROOT/}" \
        "/out/$prepared_name" \
        owlxml \
        merge >/dev/null 2>&1; then
        printf '%s\n' "$prepared_path"
        return 0
    fi

    return 1
}

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

truthy() {
    local value
    value="$(printf '%s' "${1:-}" | tr '[:upper:]' '[:lower:]' | xargs)"
    case "$value" in
        ""|"0"|"false"|"no") return 1 ;;
        *) return 0 ;;
    esac
}

looks_like_rdfxml() {
    local file="$1"
    if [[ ! -f "$file" ]]; then
        return 1
    fi
    local head_sample
    head_sample="$(head -c 4096 "$file" 2>/dev/null || true)"
    [[ "$head_sample" == *"<?xml"* || "$head_sample" == *"<rdf:RDF"* ]]
}

tableauxx_expected_check_auto_input() {
    local mount_dir="$1"
    local container_ontology_name="$2"
    local host_input="$mount_dir/$container_ontology_name"
    local selected_name="$container_ontology_name"

    if [[ "$container_ontology_name" == *.owl ]]; then
        local host_bin="${host_input%.owl}.owlbin"
        if [[ "${OWL2_REASONER_BIN_ONLY:-}" == "1" && -f "$host_bin" ]]; then
            selected_name="$(basename "$host_bin")"
        elif [[ "${OWL2_REASONER_BIN_ONLY:-}" != "1" && "${OWL2_REASONER_FORCE_TEXT:-}" != "1" && -f "$host_bin" ]]; then
            local threshold="${OWL2_REASONER_LARGE_PROFILE_THRESHOLD:-4194304}"
            if [[ ! "$threshold" =~ ^[0-9]+$ || "$threshold" -le 0 ]]; then
                threshold=4194304
            fi
            local file_size
            file_size="$(stat -c%s "$host_input" 2>/dev/null || echo 0)"
            if truthy "${OWL2_REASONER_LARGE_PROFILE_AUTO:-1}" \
                && [[ "$file_size" -ge "$threshold" ]] \
                && looks_like_rdfxml "$host_input"; then
                selected_name="$container_ontology_name"
            else
                selected_name="$(basename "$host_bin")"
            fi
        fi
    fi

    printf '/ontologies/%s\n' "$selected_name"
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
        --argjson result_schema_version "$BENCHMARK_RESULT_SCHEMA_VERSION" \
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
        --argjson engine_provenance "$(engine_provenance_json)" \
        '{
            result_schema_version: $result_schema_version,
            run_id: $run_id,
            started_at: $started_at,
            ontology_suite: $ontology_suite,
            ontologies_dir: $ontologies_dir,
            large_ontologies_dir: $large_ontologies_dir,
            operation: $operation,
            timeout_seconds: $timeout_seconds,
            include_chebi: ($include_chebi == 1),
            reasoners: $reasoners,
            engine_provenance: $engine_provenance,
            run_dir: $run_dir,
            fairness_policy: {
              primary_metric: "wall_time_ms",
              operation: "single_operation_per_run",
              isolation: "run_scoped_results_directory",
              correctness_gate: "result_status must be passed before semantic performance claims"
            }
         }' > "$RUN_DIR/run_metadata.json"
}

engine_provenance_json() {
    local provenance_json="{}"
    local reasoner entry
    for reasoner in "${REASONERS[@]}"; do
        [[ -n "${ENGINE_IMAGE_DIGEST[$reasoner]:-}" ]] || continue
        entry="$(jq -n \
            --arg image "owl-reasoner-$reasoner" \
            --arg digest "${ENGINE_IMAGE_DIGEST[$reasoner]}" \
            --arg version "${ENGINE_VERSION[$reasoner]:-unknown}" \
            '{image: $image, image_digest: $digest, engine_version: $version}')"
        provenance_json="$(jq -nc --argjson provenance "$provenance_json" \
            --arg reasoner "$reasoner" --argjson entry "$entry" \
            '$provenance + {($reasoner): $entry}')"
    done
    printf '%s\n' "$provenance_json"
}

record_engine_image_digests() {
    local reasoner digest
    for reasoner in "${REASONERS[@]}"; do
        if [[ "${REASONER_BUILD_STATUS[$reasoner]:-unknown}" != "ready" ]]; then
            continue
        fi
        digest="$(docker image inspect --format '{{.Id}}' "owl-reasoner-$reasoner" 2>/dev/null || true)"
        if [[ "$digest" =~ ^sha256:[0-9a-f]{64}$ ]]; then
            ENGINE_IMAGE_DIGEST["$reasoner"]="$digest"
        else
            log_warn "$reasoner: could not record docker image digest"
        fi
    done
}

record_engine_version_from_output() {
    local reasoner="$1"
    local output_file="$2"
    [[ -n "${ENGINE_VERSION[$reasoner]:-}" ]] && return 0
    [[ -f "$output_file" ]] || return 0
    local version
    version="$(grep -Eo '"version"[[:space:]]*:[[:space:]]*"[^"]+"' "$output_file" 2>/dev/null \
        | head -1 | sed -E 's/.*"version"[[:space:]]*:[[:space:]]*"([^"]+)".*/\1/' || true)"
    if [[ -n "$version" ]]; then
        ENGINE_VERSION["$reasoner"]="$version"
    fi
    return 0
}

update_engine_provenance_metadata() {
    ensure_run_dirs
    if [[ ! -f "$RUN_DIR/run_metadata.json" ]]; then
        log_err "Cannot record engine provenance: missing $RUN_DIR/run_metadata.json"
        return 1
    fi
    jq --argjson engine_provenance "$(engine_provenance_json)" \
        '.engine_provenance = $engine_provenance' \
        "$RUN_DIR/run_metadata.json" > "${RUN_DIR}/run_metadata.json.tmp" \
        && mv "${RUN_DIR}/run_metadata.json.tmp" "$RUN_DIR/run_metadata.json"

    local -a executed_engines=()
    local reasoner
    for reasoner in "${REASONERS[@]}"; do
        if [[ -n "${ENGINE_EXECUTED[$reasoner]:-}" ]]; then
            executed_engines+=("$reasoner")
            if [[ -z "${ENGINE_VERSION[$reasoner]:-}" ]]; then
                log_warn "$reasoner: engine version not reported in any output; recorded as unknown"
            fi
        fi
    done

    if ! validate_run_metadata_engine_provenance "$RUN_DIR/run_metadata.json" ${executed_engines[@]+"${executed_engines[@]}"}; then
        log_err "Engine provenance validation failed for $RUN_DIR (executed engines: ${executed_engines[*]:-none})"
        return 1
    fi
    log_ok "Engine provenance recorded for ${#executed_engines[@]} executed engine(s)"
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
    local case_id="${ONTOLOGY_SUITE}:${ontology_name}:${operation}"
    local oracle_case_id="${ORACLE_CASE_ID:-$case_id}"
    local expected_semantic_result="${EXPECTED_SEMANTIC_RESULT:-}"

    # The engine wrapper reports its version in the container output; the
    # first observed value per engine pins the run's engine provenance.
    record_engine_version_from_output "$reasoner" "$tmp_output"

    write_benchmark_result \
        "$result_file" \
        "$RUN_ID" \
        "$ONTOLOGY_SUITE" \
        "$case_id" \
        "$reasoner" \
        "$ontology_name" \
        "$operation" \
        "$status" \
        "$reasoning_result" \
        "$CORRECTNESS_STATUS" \
        "$ORACLE_STATUS" \
        "$ORACLE_SOURCE" \
        "$oracle_case_id" \
        "$expected_semantic_result" \
        "$wall_ms" \
        "$reported_ms" \
        "$parse_ms" \
        "$reason_ms" \
        "$tmp_output"

    # Project the check-auto merge-probe posture marker (#168/#171,
    # ADR 0022) into the outer result JSON: skipped_size_gate or
    # budget_exhausted when the report carried one, absent otherwise.
    # Post-patch rather than a signature change: the raw output is the
    # transport the report already rode in on.
    local merge_probe_marker
    if [[ -f "$tmp_output" ]]; then
        merge_probe_marker="$(grep -Eo '"merge_probe"[[:space:]]*:[[:space:]]*"[^"]+"' "$tmp_output" 2>/dev/null | tail -1 | sed -E 's/.*"merge_probe"[[:space:]]*:[[:space:]]*"([^"]+)".*/\1/' || true)"
        if [[ -n "$merge_probe_marker" ]]; then
            jq --arg merge_probe "$merge_probe_marker" \
                '.merge_probe = $merge_probe' "$result_file" > "${result_file}.probe" \
                && mv "${result_file}.probe" "$result_file" \
                || rm -f "${result_file}.probe"
        fi
    fi
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

    # From here on the harness executes this engine, so its provenance must
    # be pinned in run metadata.
    ENGINE_EXECUTED["$reasoner"]=1

    log_info "Running $reasoner on $ontology_name"
    local tmp_output
    tmp_output="$(mktemp)"
    local start_ns end_ns wall_ms cmd_status
    local reported_ms reported_status reasoning_result final_status parse_ms reason_ms
    local report_json
    local container_name
    local exit_code_file wait_status
    local mount_dir="$RUN_ONTOLOGY_DIR"
    local container_ontology_name="$ontology_name"
    container_name="owlbench_${RUN_ID}_${reasoner}_${ontology_name%.owl}"
    container_name="${container_name//[^a-zA-Z0-9_.-]/_}"
    exit_code_file="$(mktemp)"

    wait_status=1
    start_ns="$(date +%s%N)"
    docker rm -f "$container_name" >/dev/null 2>&1 || true

    local -a docker_env_args=()
    local env_name
    for env_name in "${FORWARDED_OWL_ENV_VARS[@]}"; do
        if [[ "${!env_name+x}" == "x" ]]; then
            docker_env_args+=(-e "${env_name}=${!env_name}")
        fi
    done

    if [[ "$reasoner" == "openllet" && -n "${OPENLLET_JAVA_TOOL_OPTIONS:-}" ]]; then
        docker_env_args+=(-e "JAVA_TOOL_OPTIONS=${OPENLLET_JAVA_TOOL_OPTIONS}")
    fi

    if [[ "$reasoner" == "konclude" && "${KONCLUDE_PREPROCESS_INPUTS:-1}" == "1" ]]; then
        local prepared_konclude_input
        if prepared_konclude_input="$(prepare_konclude_input "$ontology_path")"; then
            mount_dir="$RUN_KONCLUDE_INPUT_DIR"
            container_ontology_name="$(basename "$prepared_konclude_input")"
        else
            log_warn "Konclude preprocessing failed for $ontology_name; falling back to raw ontology input"
        fi
    fi

    if docker run -d --name "$container_name" \
        "${docker_env_args[@]}" \
        -v "$mount_dir:/ontologies:ro" \
        "owl-reasoner-$reasoner" \
        "/ontologies/$container_ontology_name" "$operation" >/dev/null 2>&1; then
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

    if [[ "$reasoner" == "tableauxx" && "$operation" == "consistency" && "$wait_status" -eq 0 ]]; then
        local expected_check_auto_input
        expected_check_auto_input="$(tableauxx_expected_check_auto_input "$mount_dir" "$container_ontology_name")"
        report_json="$(mktemp)"
        if benchmark_result_extract_tableauxx_check_auto_report \
            "$tmp_output" \
            "$expected_check_auto_input" \
            "$operation" \
            "$cmd_status" \
            "$report_json"; then
            reported_ms="$(jq -r '.duration_ms' "$report_json")"
            reported_status="$(jq -r '.status' "$report_json")"
            reasoning_result="$(jq -r '.reasoning_result' "$report_json")"
            parse_ms="$(jq -r '.parse_time_ms // -1' "$report_json")"
            reason_ms="$(jq -r '.reason_time_ms // -1' "$report_json")"
            final_status="$reported_status"
        else
            log_warn "Invalid or missing check-auto report for $ontology_name (exit=$cmd_status)"
            reported_ms="$wall_ms"
            reported_status=""
            reasoning_result="error"
            parse_ms=-1
            reason_ms=-1
            if [[ "$cmd_status" -eq 124 ]]; then
                final_status="timeout"
                reasoning_result="unknown"
            else
                final_status="failed"
            fi
        fi
        rm -f "$report_json"
    elif [[ "$reasoner" == "tableauxx" && "$operation" == "consistency" ]]; then
        # No application report can establish completion if waiting failed.
        reported_ms="$wall_ms"
        reported_status=""
        parse_ms=-1
        reason_ms=-1
        if [[ "$cmd_status" -eq 124 ]]; then
            final_status="timeout"
            reasoning_result="unknown"
        else
            final_status="failed"
            reasoning_result="error"
        fi
    else
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
        elif [[ "$reported_status" == "non_comparable" ]]; then
            final_status="non_comparable"
        elif [[ "$reported_status" == "unsupported" || "$reported_status" == "unsupported_operation" ]]; then
            final_status="$reported_status"
        elif [[ "$cmd_status" -ne 0 ]]; then
            final_status="failed"
        elif [[ "$reported_status" == "failed" ]]; then
            final_status="$reported_status"
        else
            final_status="success"
        fi
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
    benchmark_result_status_code "$1"
}

result_execution_status() {
    jq -r '.execution_status // .status' "$1"
}

result_execution_detail() {
    jq -r '.execution_detail // .status' "$1"
}

result_overall_status() {
    jq -r 'if .schema_version == 1 then .result_status else (if .status == "success" then "not_evaluated" else .status end) end' "$1"
}

result_display_status() {
    jq -r 'if .schema_version == 1 then .result_status else .status end' "$1"
}

result_correctness_status() {
    jq -r '.correctness_status // "not_evaluated"' "$1"
}

result_oracle_status() {
    jq -r '.oracle.status // "not_configured"' "$1"
}

result_semantic_result() {
    jq -r '.semantic_result // .reasoning_result // "unknown"' "$1"
}

result_case_id() {
    jq -r '(.case_id // ((.ontology_suite // "unknown") + ":" + (.ontology // "unknown") + ":" + (.operation // "unknown")))' "$1"
}

result_performance_eligible() {
    local result_file="$1"
    local schema_version execution_status result_status

    schema_version="$(jq -r '.schema_version // 0' "$result_file")"
    execution_status="$(result_execution_status "$result_file")"
    result_status="$(result_overall_status "$result_file")"

    [[ "$execution_status" == "success" ]] || return 1
    [[ "$schema_version" != "1" || "$result_status" == "passed" ]]
}

generate_csv() {
    local csv_file="$RUN_CSV_FILE"
    echo "run_id,ontology_suite,case_id,reasoner,ontology,operation,status,execution_status,execution_detail,result_status,correctness_status,oracle_status,oracle_source,oracle_case_id,oracle_expected,semantic_result,reasoning_result,wall_time_ms,reported_time_ms,parse_time_ms,reason_time_ms,timestamp" > "$csv_file"
    local file
    mapfile -t result_files < <(
        find "$RUN_DIR" -maxdepth 1 -type f -name "*.json" \
            ! -name "run_metadata.json" ! -name "ore_metadata.json" ! -name "summary.json" \
            ! -name "owl2bench_ladder_metadata.json" | sort
    )
    for file in "${result_files[@]}"; do
        jq -r '[
            .run_id,
            .ontology_suite,
            (.case_id // ((.ontology_suite // "unknown") + ":" + (.ontology // "unknown") + ":" + (.operation // "unknown"))),
            .reasoner,
            .ontology,
            .operation,
            .status,
            (.execution_status // .status),
            (.execution_detail // .status),
            (if .schema_version == 1 then .result_status else (if .status == "success" then "not_evaluated" else .status end) end),
            (.correctness_status // "not_evaluated"),
            (.oracle.status // "not_configured"),
            (.oracle.source // ""),
            (.oracle.case_id // ""),
            (.oracle.expected // ""),
            (.semantic_result // .reasoning_result // "unknown"),
            (.reasoning_result // .semantic_result // "unknown"),
            .wall_time_ms,
            .reported_time_ms,
            .parse_time_ms,
            .reason_time_ms,
            .timestamp
        ] | @csv' "$file" >> "$csv_file"
    done
    log_ok "CSV generated: $csv_file"
}

generate_paper_tables() {
    local md_file="$RUN_PAPER_MD"
    local tex_file="$RUN_PAPER_TEX"

    mapfile -t ontologies < <(
        find "$RUN_DIR" -maxdepth 1 -type f -name "*.json" \
            ! -name "run_metadata.json" ! -name "ore_metadata.json" ! -name "summary.json" \
            ! -name "owl2bench_ladder_metadata.json" \
            -print0 | xargs -0 -I{} jq -r '.ontology' "{}" | sort -u
    )
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
                    status="$(result_display_status "$file")"
                    wall_candidate="$(jq -r '.wall_time_ms' "$file")"
                    if result_performance_eligible "$file" \
                        && [[ "$wall_candidate" =~ ^[0-9]+$ ]] \
                        && [[ "$wall_candidate" -lt "$best" ]]; then
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
                status="$(result_display_status "$file")"
                wall="$(jq -r '.wall_time_ms' "$file")"
                if result_performance_eligible "$file"; then
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
        echo "Legend: **best wall time**, TO=timeout, NA=not available, NC=non-comparable parser/runtime incompatibility, KIL=interrupted cleanup kill, ORPH=container orphan cleanup, FAIL=hard runtime failure."
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
                    status="$(result_display_status "$file")"
                    wall="$(jq -r '.wall_time_ms' "$file")"
                    if result_performance_eligible "$file" \
                        && [[ "$wall" =~ ^[0-9]+$ ]] \
                        && [[ "$wall" -lt "$best" ]]; then
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
                status="$(result_display_status "$file")"
                wall="$(jq -r '.wall_time_ms' "$file")"
                if result_performance_eligible "$file"; then
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

    mapfile -t result_files < <(
        find "$RUN_DIR" -maxdepth 1 -type f -name "*.json" \
            ! -name "run_metadata.json" ! -name "ore_metadata.json" ! -name "summary.json" \
            ! -name "owl2bench_ladder_metadata.json" | sort
    )
    if [[ "${#result_files[@]}" -eq 0 ]]; then
        log_err "No result JSON files found in $RUN_DIR"
        exit 1
    fi

    local contract_result_count=0
    local result_file
    local expected_result_count
    local -a expected_ontologies=()
    for result_file in "${result_files[@]}"; do
        if jq -e '.schema_version == 1' "$result_file" >/dev/null 2>&1; then
            contract_result_count=$((contract_result_count + 1))
            if ! validate_benchmark_result "$result_file"; then
                log_err "Invalid benchmark result contract: $result_file"
                exit 1
            fi
        fi
    done
    if [[ "$contract_result_count" -eq "${#result_files[@]}" ]]; then
        if [[ ! -f "$RUN_DIR/ontology_manifest.txt" ]]; then
            log_err "Missing ontology manifest for schema-v1 result validation in $RUN_DIR"
            exit 1
        fi
        mapfile -t expected_ontologies < "$RUN_DIR/ontology_manifest.txt"
        expected_result_count=$(( ${#expected_ontologies[@]} * ${#REASONERS[@]} ))
        if ! validate_benchmark_result_count "$RUN_DIR" "$expected_result_count"; then
            log_err "Benchmark result count or contract validation failed in $RUN_DIR"
            exit 1
        fi
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
        echo "| Reasoner | Executed | Passed | Wrong | Unsupported | Oracle Unavailable | Not Evaluated | Non-Comparable | Failed | Timeout | Not Available | Killed | Orphan | Avg Wall Time (ms) |"
        echo "|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|"

        local reasoner
        for reasoner in "${REASONERS[@]}"; do
            mapfile -t files < <(find "$RUN_DIR" -maxdepth 1 -type f -name "${reasoner}_*.json" | sort)
            if [[ "${#files[@]}" -eq 0 ]]; then
                continue
            fi

            local executed=0 passed=0 wrong=0 unsupported=0 oracle_unavailable=0 not_evaluated=0
            local non_comparable=0 failed=0 timeout=0 not_available=0 killed=0 orphan=0
            local total_ms=0 total_count=0
            local f execution_status result_status schema_version ms
            for f in "${files[@]}"; do
                execution_status="$(result_execution_status "$f")"
                result_status="$(result_overall_status "$f")"
                schema_version="$(jq -r '.schema_version // 0' "$f")"
                ms="$(jq -r '.wall_time_ms' "$f")"
                case "$execution_status" in
                    success) executed=$((executed + 1)) ;;
                    non_comparable) non_comparable=$((non_comparable + 1)) ;;
                    timeout) timeout=$((timeout + 1)) ;;
                    not_available) not_available=$((not_available + 1)) ;;
                    killed) killed=$((killed + 1)) ;;
                    orphan) orphan=$((orphan + 1)) ;;
                    *) failed=$((failed + 1)) ;;
                esac

                case "$result_status" in
                    passed) passed=$((passed + 1)) ;;
                    wrong) wrong=$((wrong + 1)) ;;
                    unsupported) unsupported=$((unsupported + 1)) ;;
                    oracle_unavailable) oracle_unavailable=$((oracle_unavailable + 1)) ;;
                    not_evaluated) not_evaluated=$((not_evaluated + 1)) ;;
                esac

                if [[ "$execution_status" == "success" ]] \
                    && [[ "$ms" =~ ^[0-9]+$ ]] \
                    && [[ "$ms" -ge 0 ]] \
                    && { [[ "$schema_version" != "1" ]] || [[ "$result_status" == "passed" ]]; }; then
                    total_ms=$((total_ms + ms))
                    total_count=$((total_count + 1))
                fi
            done

            local avg="N/A"
            if [[ "$total_count" -gt 0 ]]; then
                avg=$((total_ms / total_count))
            fi
            echo "| $reasoner | $executed | $passed | $wrong | $unsupported | $oracle_unavailable | $not_evaluated | $non_comparable | $failed | $timeout | $not_available | $killed | $orphan | $avg |"
        done

        echo
        echo "## Detailed Results"
        echo
        echo "| Reasoner | Ontology | Case | Wall (ms) | Reported (ms) | Execution | Execution Detail | Result | Correctness | Oracle | Semantic Result |"
        echo "|---|---|---|---:|---:|---|---|---|---|---|---|"

        local file reasoner ontology case_id wall reported execution_status execution_detail result_status correctness_status oracle_status result
        for file in "${result_files[@]}"; do
            reasoner="$(jq -r '.reasoner' "$file")"
            ontology="$(jq -r '.ontology' "$file")"
            case_id="$(result_case_id "$file")"
            wall="$(jq -r '.wall_time_ms' "$file")"
            reported="$(jq -r '.reported_time_ms' "$file")"
            execution_status="$(result_execution_status "$file")"
            execution_detail="$(result_execution_detail "$file")"
            result_status="$(result_overall_status "$file")"
            correctness_status="$(result_correctness_status "$file")"
            oracle_status="$(result_oracle_status "$file")"
            result="$(result_semantic_result "$file")"
            echo "| $reasoner | $ontology | $case_id | $wall | $reported | $execution_status | $execution_detail | $result_status | $correctness_status | $oracle_status | $result |"
        done

        echo
        echo "## Notes"
        echo
        echo "- Wall time is the primary metric for cross-engine comparison."
        echo "- Execution success does not imply semantic correctness; result status must be passed for a correctness pass."
        echo "- Performance-only runs use not_evaluated when no semantic oracle applies."
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

    local metadata_file="$RUN_DIR/run_metadata.json"
    if [[ -f "$metadata_file" ]]; then
        ONTOLOGY_SUITE="$(jq -r '.ontology_suite // "standard"' "$metadata_file")"
        OPERATION="$(jq -r '.operation // "consistency"' "$metadata_file")"
        TIMEOUT_SECONDS="$(jq -r '.timeout_seconds // 300' "$metadata_file")"
        ONTOLOGIES_DIR="$(jq -r '.ontologies_dir // empty' "$metadata_file")"
        LARGE_ONTOLOGIES_DIR="$(jq -r '.large_ontologies_dir // empty' "$metadata_file")"

        if [[ "$(jq -r '.include_chebi // false' "$metadata_file")" == "true" ]]; then
            INCLUDE_CHEBI=1
        else
            INCLUDE_CHEBI=0
        fi

        mapfile -t REASONERS < <(jq -r '.reasoners[]?' "$metadata_file")
        if [[ "${#REASONERS[@]}" -eq 0 ]]; then
            REASONERS=("${DEFAULT_REASONERS[@]}")
        fi
    else
        log_warn "Missing run metadata for $RUN_ID; regenerating report from current environment."
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
            record_engine_image_digests
            run_all_benchmarks
            update_engine_provenance_metadata
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
            record_engine_image_digests
            run_all_benchmarks
            update_engine_provenance_metadata
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

if [[ "${BASH_SOURCE[0]}" == "$0" ]]; then
    main "$@"
fi
