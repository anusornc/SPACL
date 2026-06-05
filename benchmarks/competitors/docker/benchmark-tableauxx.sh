#!/bin/bash
set -euo pipefail

ONTOLOGY_FILE="${1:-}"
OPERATION="${2:-consistency}"

if [ -z "$ONTOLOGY_FILE" ]; then
    echo "Usage: benchmark-tableauxx.sh <ontology.owl> [operation]" >&2
    exit 2
fi

LARGE_PARSE="${OWL2_REASONER_LARGE_PARSE:-1}"
MAX_FILE_SIZE="${OWL2_REASONER_MAX_FILE_SIZE:-0}"
AUTO_CACHE="${OWL2_REASONER_AUTO_CACHE:-0}"
FORCE_TEXT="${OWL2_REASONER_FORCE_TEXT:-}"
BIN_ONLY="${OWL2_REASONER_BIN_ONLY:-}"
DISABLE_PARSE_FALLBACK="${OWL2_REASONER_DISABLE_PARSE_FALLBACK:-}"
ENABLE_PARSE_FALLBACK="${OWL2_REASONER_ENABLE_PARSE_FALLBACK:-}"
PARSE_PROGRESS_EVERY="${OWL2_REASONER_PARSE_PROGRESS_EVERY:-}"
PARSE_IO_PROGRESS_BYTES="${OWL2_REASONER_PARSE_IO_PROGRESS_BYTES:-}"
EXPERIMENTAL_XML_PARSER="${OWL2_REASONER_EXPERIMENTAL_XML_PARSER:-}"
EXPERIMENTAL_XML_STRICT="${OWL2_REASONER_EXPERIMENTAL_XML_STRICT:-}"
EXPERIMENTAL_XML_BATCH="${OWL2_REASONER_EXPERIMENTAL_XML_BATCH:-}"
EXPERIMENTAL_XML_QUEUE="${OWL2_REASONER_EXPERIMENTAL_XML_QUEUE:-}"
EXPERIMENTAL_XML_WORKERS="${OWL2_REASONER_EXPERIMENTAL_XML_WORKERS:-}"
EXPERIMENTAL_XML_CACHE="${OWL2_REASONER_EXPERIMENTAL_XML_CACHE:-}"
STRUCTURAL_XML_AUTO="${OWL2_REASONER_STRUCTURAL_XML_AUTO:-}"
STRUCTURAL_XML_AUTO_THRESHOLD="${OWL2_REASONER_STRUCTURAL_XML_AUTO_THRESHOLD:-}"
LARGE_PROFILE_AUTO="${OWL2_REASONER_LARGE_PROFILE_AUTO:-}"
LARGE_PROFILE_THRESHOLD="${OWL2_REASONER_LARGE_PROFILE_THRESHOLD:-}"
STAGE_TIMING="${OWL2_REASONER_STAGE_TIMING:-}"
SPACL_SCHED_MODE="${SPACL_SCHED_MODE:-}"
SPACL_BRANCH_POLICY="${SPACL_BRANCH_POLICY:-}"
SPACL_RRN_MODEL_PATH="${SPACL_RRN_MODEL_PATH:-}"
SPACL_BRANCH_SNAPSHOT_FILE="${SPACL_BRANCH_SNAPSHOT_FILE:-}"
SPACL_NOGOOD="${SPACL_NOGOOD:-}"
SPACL_EMIT_STATS="${SPACL_EMIT_STATS:-}"

truthy() {
    local value
    value="$(echo "${1:-}" | tr '[:upper:]' '[:lower:]' | xargs)"
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

INPUT_FILE="$ONTOLOGY_FILE"
if [[ "$ONTOLOGY_FILE" == *.owl ]]; then
    BIN_CANDIDATE="${ONTOLOGY_FILE%.owl}.owlbin"
    if [ "$BIN_ONLY" = "1" ] && [ ! -f "$BIN_CANDIDATE" ]; then
        echo "{\"duration_ms\": -1, \"status\": \"failed\", \"reasoning_result\": \"unknown\", \"error\": \"bin_only_requested_but_missing\"}"
        exit 1
    fi
    if [ "$BIN_ONLY" = "1" ] && [ -f "$BIN_CANDIDATE" ]; then
        INPUT_FILE="$BIN_CANDIDATE"
    elif [ "$FORCE_TEXT" != "1" ] && [ -f "$BIN_CANDIDATE" ]; then
        # Auto large-profile default: prefer text RDF/XML for large files.
        threshold="${LARGE_PROFILE_THRESHOLD:-4194304}"
        if [[ "$threshold" =~ ^[0-9]+$ ]] && [ "$threshold" -gt 0 ]; then
            :
        else
            threshold=4194304
        fi
        file_size="$(stat -c%s "$ONTOLOGY_FILE" 2>/dev/null || echo 0)"
        auto_enabled=1
        if [ -n "$LARGE_PROFILE_AUTO" ] && ! truthy "$LARGE_PROFILE_AUTO"; then
            auto_enabled=0
        fi
        if [ "$auto_enabled" -eq 1 ] && [ "$file_size" -ge "$threshold" ] && looks_like_rdfxml "$ONTOLOGY_FILE"; then
            INPUT_FILE="$ONTOLOGY_FILE"
        else
            INPUT_FILE="$BIN_CANDIDATE"
        fi
    fi
fi

echo "{\"reasoner\": \"Tableauxx\", \"version\": \"0.2.0\", \"operation\": \"$OPERATION\", \"ontology\": \"$ONTOLOGY_FILE\", \"input_file\": \"$INPUT_FILE\"}"
echo "[bench] tableauxx input_file=$INPUT_FILE" >&2

tmp_output=$(mktemp)
start_time=$(date +%s%N)

run_and_capture() {
    local output_file="$1"
    shift
    set +e
    "$@" 2>&1 | tee "$output_file"
    local status="${PIPESTATUS[0]}"
    set -e
    return "$status"
}

if [ "$OPERATION" = "consistency" ]; then
    if run_and_capture "$tmp_output" env \
        OWL2_REASONER_LARGE_PARSE="$LARGE_PARSE" \
        OWL2_REASONER_MAX_FILE_SIZE="$MAX_FILE_SIZE" \
        OWL2_REASONER_AUTO_CACHE="$AUTO_CACHE" \
        OWL2_REASONER_FORCE_TEXT="$FORCE_TEXT" \
        OWL2_REASONER_BIN_ONLY="$BIN_ONLY" \
        OWL2_REASONER_DISABLE_PARSE_FALLBACK="$DISABLE_PARSE_FALLBACK" \
        OWL2_REASONER_ENABLE_PARSE_FALLBACK="$ENABLE_PARSE_FALLBACK" \
        OWL2_REASONER_PARSE_PROGRESS_EVERY="$PARSE_PROGRESS_EVERY" \
        OWL2_REASONER_PARSE_IO_PROGRESS_BYTES="$PARSE_IO_PROGRESS_BYTES" \
        OWL2_REASONER_EXPERIMENTAL_XML_PARSER="$EXPERIMENTAL_XML_PARSER" \
        OWL2_REASONER_EXPERIMENTAL_XML_STRICT="$EXPERIMENTAL_XML_STRICT" \
        OWL2_REASONER_EXPERIMENTAL_XML_BATCH="$EXPERIMENTAL_XML_BATCH" \
        OWL2_REASONER_EXPERIMENTAL_XML_QUEUE="$EXPERIMENTAL_XML_QUEUE" \
        OWL2_REASONER_EXPERIMENTAL_XML_WORKERS="$EXPERIMENTAL_XML_WORKERS" \
        OWL2_REASONER_EXPERIMENTAL_XML_CACHE="$EXPERIMENTAL_XML_CACHE" \
        OWL2_REASONER_STRUCTURAL_XML_AUTO="$STRUCTURAL_XML_AUTO" \
        OWL2_REASONER_STRUCTURAL_XML_AUTO_THRESHOLD="$STRUCTURAL_XML_AUTO_THRESHOLD" \
        OWL2_REASONER_LARGE_PROFILE_AUTO="$LARGE_PROFILE_AUTO" \
        OWL2_REASONER_LARGE_PROFILE_THRESHOLD="$LARGE_PROFILE_THRESHOLD" \
        OWL2_REASONER_STAGE_TIMING="$STAGE_TIMING" \
        SPACL_SCHED_MODE="$SPACL_SCHED_MODE" \
        SPACL_BRANCH_POLICY="$SPACL_BRANCH_POLICY" \
        SPACL_RRN_MODEL_PATH="$SPACL_RRN_MODEL_PATH" \
        SPACL_BRANCH_SNAPSHOT_FILE="$SPACL_BRANCH_SNAPSHOT_FILE" \
        SPACL_NOGOOD="$SPACL_NOGOOD" \
        SPACL_EMIT_STATS="$SPACL_EMIT_STATS" \
        /opt/reasoner/owl2-reasoner check-auto "$INPUT_FILE"
    then
        cmd_status=0
    else
        cmd_status=$?
    fi
elif [ "$OPERATION" = "classification" ]; then
    if run_and_capture "$tmp_output" /opt/reasoner/owl2-reasoner stats "$ONTOLOGY_FILE"; then
        cmd_status=0
    else
        cmd_status=$?
    fi
else
    echo "{\"duration_ms\": -1, \"status\": \"unsupported_operation\", \"reasoning_result\": \"unknown\"}"
    exit 2
fi

end_time=$(date +%s%N)
duration_ms=$(( (end_time - start_time) / 1000000 ))

reasoning_result="unknown"
if grep -qi "INCONSISTENT" "$tmp_output"; then
    reasoning_result="inconsistent"
elif grep -qi "CONSISTENT" "$tmp_output"; then
    reasoning_result="consistent"
fi

parse_ms="$(grep -Eo '\[phase\][[:space:]]+parse_done[[:space:]]+ms=[0-9]+' "$tmp_output" | tail -1 | grep -Eo '[0-9]+' || true)"
reason_ms="$(grep -Eo '\[phase\][[:space:]]+reason_done[[:space:]]+ms=[0-9]+' "$tmp_output" | tail -1 | grep -Eo '[0-9]+' || true)"
if [[ -z "$parse_ms" ]]; then
    parse_ms=-1
fi
if [[ -z "$reason_ms" ]]; then
    reason_ms=-1
fi

if [ "$cmd_status" -eq 0 ]; then
    echo "{\"duration_ms\": $duration_ms, \"status\": \"completed\", \"reasoning_result\": \"$reasoning_result\", \"parse_time_ms\": $parse_ms, \"reason_time_ms\": $reason_ms}"
else
    echo "{\"duration_ms\": $duration_ms, \"status\": \"failed\", \"reasoning_result\": \"$reasoning_result\", \"parse_time_ms\": $parse_ms, \"reason_time_ms\": $reason_ms}"
fi

rm -f "$tmp_output"
exit "$cmd_status"
