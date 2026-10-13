#!/usr/bin/env bash
set -euo pipefail

ADAPTER_NAME="konclude-unified"
ENGINE_VERSION="v0.7.0-1138"
RELEASE_COMMIT="0002e80635403960a7df5d93bd0e8f994d4952d0"
RELEASE_ASSET_SHA256="37216c18f45a80165aec1e76aeefd62d933d6a3f8f134c5aca0707c676fda76e"
EXECUTABLE_SHA256="5484f16dcff71486a5deed9cf9cea8a0f7febf115aaa6915ad2e8c1cf16965e3"
CONFIG_SHA256="03686d52957c07781745b069df1ecd1a9ae17ba4efd11cd82d21a0e22a6a3829"
CONVERTER_SHA256="83dd76ee81a5de75fb31ebed584270715b4404e2ac4b36db65836df1b1c18fe7"
VARIANT="${KONCLUDE_UNIFIED_VARIANT:-$ADAPTER_NAME}"
THREADS="${KONCLUDE_UNIFIED_THREADS:-1}"
MEMORY_LIMIT="${KONCLUDE_UNIFIED_MEMORY_LIMIT:-24GiB}"
NOT_AVAILABLE_EXIT_CODE=125
RDF_PREFIX="http://www.w3.org/1999/02/22-rdf-syntax-ns#"
OWL2BENCH_PREFIX="https://kracr.iiitd.edu.in/OWL2Bench#"

usage() {
  echo "usage: $0 load <profile-context-dir> | query <query-file> <profile-context-dir>" >&2
  exit 2
}

not_available() {
  printf '[%s][not_available] %s\n' "$ADAPTER_NAME" "$1" >&2
  exit "$NOT_AVAILABLE_EXIT_CODE"
}

adapter_failed() {
  printf '[%s][failed] %s\n' "$ADAPTER_NAME" "$1" >&2
  exit 1
}

require_command() {
  command -v "$1" >/dev/null 2>&1 || not_available "required command is missing: $1"
}

sha256_file() {
  local file="$1"
  require_command sha256sum
  sha256sum "$file" | awk '{print $1}'
}

resolve_binary() {
  local candidate="${KONCLUDE_UNIFIED_BIN:-${KONCLUDE_BIN:-}}"
  if [[ -z "$candidate" && -n "${KONCLUDE_HOME:-}" ]]; then
    candidate="$KONCLUDE_HOME/Konclude"
  fi
  if [[ -z "$candidate" ]]; then
    candidate="$(command -v Konclude || true)"
  fi
  if [[ "$candidate" != */* ]]; then
    candidate="$(command -v "$candidate" || true)"
  fi
  [[ -n "$candidate" && -x "$candidate" ]] || not_available "Konclude executable is missing or not executable"
  realpath "$candidate" || not_available "cannot resolve Konclude executable: $candidate"
}

resolve_config() {
  local binary_path="$1"
  local candidate="${KONCLUDE_UNIFIED_CONFIG:-${KONCLUDE_CONFIG:-}}"
  if [[ -z "$candidate" && -n "${KONCLUDE_HOME:-}" ]]; then
    candidate="$KONCLUDE_HOME/Configs/querying-config.xml"
  fi
  if [[ -z "$candidate" ]]; then
    candidate="$(dirname "$binary_path")/Configs/querying-config.xml"
  fi
  [[ -r "$candidate" ]] || not_available "Konclude querying config is missing or unreadable: $candidate"
  realpath "$candidate" || not_available "cannot resolve Konclude querying config: $candidate"
}

resolve_converter() {
  local candidate="${KONCLUDE_UNIFIED_CONVERTER:-${KONCLUDE_CONVERTER:-}}"
  [[ -n "$candidate" ]] || not_available "RDF/XML input requires KONCLUDE_UNIFIED_CONVERTER"
  if [[ "$candidate" != */* ]]; then
    candidate="$(command -v "$candidate" || true)"
  fi
  [[ -n "$candidate" && -x "$candidate" ]] || not_available "Konclude converter is missing or not executable"
  realpath "$candidate" || not_available "cannot resolve Konclude converter: $candidate"
}

check_expected_hash() {
  local actual="$1"
  local expected="$2"
  local label="$3"
  [[ -n "$expected" ]] || not_available "$label pin is not declared for variant: $VARIANT"
  if [[ "$actual" != "$expected" ]]; then
    not_available "$label hash does not match the pinned value"
  fi
}

expected_hash_for() {
  local artifact="$1"
  if [[ "$VARIANT" == "$ADAPTER_NAME" ]]; then
    case "$artifact" in
      binary) printf '%s\n' "$EXECUTABLE_SHA256" ;;
      config) printf '%s\n' "$CONFIG_SHA256" ;;
      converter) printf '%s\n' "$CONVERTER_SHA256" ;;
    esac
    return 0
  fi

  case "$artifact" in
    binary) printf '%s\n' "${KONCLUDE_UNIFIED_EXPECTED_BINARY_SHA256:-}" ;;
    config) printf '%s\n' "${KONCLUDE_UNIFIED_EXPECTED_CONFIG_SHA256:-}" ;;
    converter) printf '%s\n' "${KONCLUDE_UNIFIED_EXPECTED_CONVERTER_SHA256:-}" ;;
  esac
}

probe_runtime() {
  local binary_path="$1"
  local state_dir="$2"
  local probe_status
  set +e
  "$binary_path" --version > "$state_dir/runtime.stdout" 2> "$state_dir/runtime.stderr"
  probe_status="$?"
  set -e
  if [[ "$probe_status" -ne 0 ]] \
      || ! grep -Fq "$ENGINE_VERSION" "$state_dir/runtime.stdout" "$state_dir/runtime.stderr"; then
    not_available "Konclude runtime probe failed or reported a different version"
  fi
}

prepare_context_state() {
  local context_dir="$1"
  local profile="$2"
  local state_dir="$3"
  local binary_path config_path binary_sha256 config_sha256 converter_path converter_sha256
  local source_input source_kind source_extension prepared_input source_sha256 prepared_sha256
  local input_candidates=()
  local candidate

  binary_path="$(resolve_binary)"
  config_path="$(resolve_config "$binary_path")"
  binary_sha256="$(sha256_file "$binary_path")"
  config_sha256="$(sha256_file "$config_path")"
  check_expected_hash "$binary_sha256" "$(expected_hash_for binary)" "Konclude executable"
  check_expected_hash "$config_sha256" "$(expected_hash_for config)" "Konclude config"
  probe_runtime "$binary_path" "$state_dir"

  if [[ -n "${KONCLUDE_UNIFIED_INPUT:-}" ]]; then
    source_input="$KONCLUDE_UNIFIED_INPUT"
    if [[ "$source_input" != /* ]]; then
      source_input="$context_dir/$source_input"
    fi
    [[ -r "$source_input" && -f "$source_input" ]] || not_available "configured context input is missing: $source_input"
  else
    mapfile -t input_candidates < <(
      find "$context_dir" -maxdepth 1 -type f \
        \( -iname '*.ofn' -o -iname '*.omn' -o -iname '*.fss' -o -iname '*.functional' \
        -o -iname '*.owl' -o -iname '*.rdf' -o -iname '*.xml' \) -print | sort
    )
    ((${#input_candidates[@]} > 0)) || not_available "no OWL/Functional input found in profile context: $context_dir"
    source_input=""
    if [[ "${#input_candidates[@]}" -eq 1 ]]; then
      source_input="${input_candidates[0]}"
    fi
    [[ -n "$source_input" ]] || adapter_failed "ambiguous OWL/Functional inputs in profile context: $context_dir"
  fi
  source_input="$(realpath "$source_input")"

    source_extension="${source_input##*.}"
    source_extension="${source_extension,,}"
    case "$source_extension" in
      ofn|omn|fss|functional)
      source_kind="functional-style"
      ;;
    owl)
      source_kind="rdf-xml"
      ;;
    rdf|xml)
      source_kind="rdf-xml"
      ;;
    *)
      adapter_failed "unsupported profile input extension: $source_input"
      ;;
  esac

  source_sha256="$(sha256_file "$source_input")"
  converter_path=""
  converter_sha256=""
  prepared_input="$source_input"
  if [[ "$source_kind" == "rdf-xml" ]]; then
    converter_path="$(resolve_converter)"
    converter_sha256="$(sha256_file "$converter_path")"
    check_expected_hash "$converter_sha256" "$(expected_hash_for converter)" "Konclude converter"
    prepared_input="$state_dir/converted.ofn"
    set +e
    "$converter_path" "$source_input" "$prepared_input" > "$state_dir/converter.stdout" 2> "$state_dir/converter.stderr"
    local converter_status="$?"
    set -e
    if [[ "$converter_status" -eq "$NOT_AVAILABLE_EXIT_CODE" ]]; then
      not_available "converter reported unavailable"
    elif [[ "$converter_status" -ne 0 ]]; then
      adapter_failed "RDF/XML conversion failed with status $converter_status"
    fi
    [[ -s "$prepared_input" ]] || adapter_failed "converter produced no Functional-Style input"
  fi
  prepared_input="$(realpath "$prepared_input")"
  prepared_sha256="$(sha256_file "$prepared_input")"

  jq -n \
    --arg adapter "$ADAPTER_NAME" \
    --arg variant "$VARIANT" \
    --arg profile "$profile" \
    --arg context_dir "$context_dir" \
    --arg context_identity "profile=$profile;directory=$context_dir" \
    --arg engine_version "$ENGINE_VERSION" \
    --arg release_commit "$RELEASE_COMMIT" \
    --arg release_asset_sha256 "$RELEASE_ASSET_SHA256" \
    --arg expected_executable_sha256 "$EXECUTABLE_SHA256" \
    --arg expected_config_sha256 "$CONFIG_SHA256" \
    --arg expected_converter_sha256 "$CONVERTER_SHA256" \
    --arg query_mode "sparqlfile" \
    --arg threads "$THREADS" \
    --arg memory_limit "$MEMORY_LIMIT" \
    --arg source_kind "$source_kind" \
    --arg source_input "$source_input" \
    --arg source_sha256 "$source_sha256" \
    --arg prepared_input "$prepared_input" \
    --arg prepared_sha256 "$prepared_sha256" \
    --arg converter_path "$converter_path" \
    --arg converter_sha256 "$converter_sha256" \
    --arg config_path "$config_path" \
    --arg config_sha256 "$config_sha256" \
    --arg binary_path "$binary_path" \
    --arg binary_sha256 "$binary_sha256" \
    --arg rdf_prefix "$RDF_PREFIX" \
    --arg owl2bench_prefix "$OWL2BENCH_PREFIX" \
    '{adapter:$adapter, variant:$variant, profile:$profile, context_dir:$context_dir, context_identity:$context_identity, engine_version:$engine_version, release_commit:$release_commit, release_asset_sha256:$release_asset_sha256, expected_executable_sha256:$expected_executable_sha256, expected_config_sha256:$expected_config_sha256, expected_converter_sha256:$expected_converter_sha256, query_mode:$query_mode, threads:$threads, memory_limit:$memory_limit, runtime_probe:"passed", source_kind:$source_kind, source_input:$source_input, source_sha256:$source_sha256, prepared_input:$prepared_input, prepared_sha256:$prepared_sha256, converter_path:(if $converter_path == "" then null else $converter_path end), converter_sha256:(if $converter_sha256 == "" then null else $converter_sha256 end), config_path:$config_path, config_sha256:$config_sha256, binary_path:$binary_path, binary_sha256:$binary_sha256, prefix_injection:{rdf:$rdf_prefix, default:$owl2bench_prefix}, conversion_status:(if $source_kind == "rdf-xml" then "converted" else "native" end)}' \
    > "$state_dir/provenance.json"
}

context_paths() {
  local requested_context="$1"
  CONTEXT_DIR="$(realpath "$requested_context")" || not_available "cannot resolve profile context: $requested_context"
  [[ -d "$CONTEXT_DIR" ]] || not_available "profile context directory is missing: $requested_context"
  PROFILE="$(basename "$CONTEXT_DIR")"
  case "$PROFILE" in
    DL|EL|QL|RL) ;;
    *) not_available "profile context must be named DL, EL, QL, or RL: $CONTEXT_DIR" ;;
  esac

  STATE_DIR="$CONTEXT_DIR/.konclude-unified"
  mkdir -p "$STATE_DIR" || not_available "cannot create adapter state directory: $STATE_DIR"
  PROVENANCE_FILE="$STATE_DIR/provenance.json"
}

load_context() {
  local context_dir="$1"
  context_paths "$context_dir"
  prepare_context_state "$CONTEXT_DIR" "$PROFILE" "$STATE_DIR"
  jq -n \
    --arg status "success" \
    --arg profile "$PROFILE" \
    --arg provenance_file "$PROVENANCE_FILE" \
    '{status:$status, profile:$profile, provenance_file:$provenance_file}'
}

read_provenance() {
  local recorded_variant recorded_engine_version recorded_binary_sha256 recorded_config_sha256 recorded_prepared_sha256
  if [[ ! -f "$PROVENANCE_FILE" ]] || ! jq -e . "$PROVENANCE_FILE" >/dev/null 2>&1; then
    not_available "profile context provenance is missing or malformed: $PROVENANCE_FILE"
  fi
  [[ -f "$PROVENANCE_FILE" ]] || not_available "profile context has not been loaded: $CONTEXT_DIR"
  recorded_variant="$(jq -r '.variant // empty' "$PROVENANCE_FILE")"
  recorded_engine_version="$(jq -r '.engine_version // empty' "$PROVENANCE_FILE")"
  BINARY_PATH="$(jq -r '.binary_path // empty' "$PROVENANCE_FILE")"
  CONFIG_PATH="$(jq -r '.config_path // empty' "$PROVENANCE_FILE")"
  PREPARED_INPUT="$(jq -r '.prepared_input // empty' "$PROVENANCE_FILE")"
  recorded_binary_sha256="$(jq -r '.binary_sha256 // empty' "$PROVENANCE_FILE")"
  recorded_config_sha256="$(jq -r '.config_sha256 // empty' "$PROVENANCE_FILE")"
  recorded_prepared_sha256="$(jq -r '.prepared_sha256 // empty' "$PROVENANCE_FILE")"
  recorded_threads="$(jq -r '.threads // empty' "$PROVENANCE_FILE")"
  recorded_memory_limit="$(jq -r '.memory_limit // empty' "$PROVENANCE_FILE")"
  [[ "$recorded_engine_version" == "$ENGINE_VERSION" ]] || not_available "pinned Konclude version is not the selected release"
  [[ -n "$recorded_variant" ]] && VARIANT="$recorded_variant"
  [[ -n "$recorded_binary_sha256" && -n "$recorded_config_sha256" && -n "$recorded_prepared_sha256" ]] \
    || not_available "profile context provenance is missing artifact hashes"
  [[ -n "$recorded_threads" ]] && THREADS="$recorded_threads"
  [[ -n "$recorded_memory_limit" ]] && MEMORY_LIMIT="$recorded_memory_limit"
  [[ -x "$BINARY_PATH" ]] || not_available "pinned Konclude executable is unavailable: $BINARY_PATH"
  [[ -r "$CONFIG_PATH" ]] || not_available "pinned Konclude config is unavailable: $CONFIG_PATH"
  [[ -r "$PREPARED_INPUT" ]] || not_available "prepared profile input is unavailable: $PREPARED_INPUT"
  [[ "$(sha256_file "$BINARY_PATH")" == "$recorded_binary_sha256" ]] \
    || not_available "pinned Konclude executable changed after context load"
  [[ "$(sha256_file "$CONFIG_PATH")" == "$recorded_config_sha256" ]] \
    || not_available "pinned Konclude config changed after context load"
  [[ "$(sha256_file "$PREPARED_INPUT")" == "$recorded_prepared_sha256" ]] \
    || not_available "prepared profile input changed after context load"
}

run_query() {
  local query_file="$1"
  local context_dir="$2"
  [[ -r "$query_file" && -f "$query_file" ]] || not_available "query file is missing or unreadable: $query_file"
  query_file="$(realpath "$query_file")"
  context_paths "$context_dir"
  if [[ ! -f "$PROVENANCE_FILE" ]]; then
    prepare_context_state "$CONTEXT_DIR" "$PROFILE" "$STATE_DIR"
  fi
  read_provenance

  local query_hash query_wrapper result_file diagnostics_file command_status
  query_hash="$(sha256_file "$query_file")"
  query_wrapper="$STATE_DIR/query-${query_hash}.rq"
  result_file="$(mktemp "$STATE_DIR/result.XXXXXX.xml")"
  diagnostics_file="$(mktemp "$STATE_DIR/query.XXXXXX.diagnostics")"
  {
    printf 'PREFIX rdf: <%s>\n' "$RDF_PREFIX"
    printf 'PREFIX : <%s>\n' "$OWL2BENCH_PREFIX"
    cat "$query_file"
  } > "$query_wrapper"

  set +e
  "$BINARY_PATH" sparqlfile \
    -s "$query_wrapper" \
    -i "$PREPARED_INPUT" \
    -o "$result_file" \
    -c "$CONFIG_PATH" \
    -w "$THREADS" \
    -v > "$diagnostics_file" 2>&1
  command_status="$?"
  set -e
  cat "$diagnostics_file" >&2

  if [[ "$command_status" -ne 0 ]]; then
    if grep -Eqi 'error while loading shared libraries|cannot open shared object file|No such file or directory' "$diagnostics_file"; then
      exit "$NOT_AVAILABLE_EXIT_CODE"
    fi
    exit "$command_status"
  fi
  [[ -s "$result_file" ]] || adapter_failed "Konclude completed without a result document"
  cat "$result_file"
}

[[ $# -ge 2 ]] || usage
case "$1" in
  load)
    [[ $# -eq 2 ]] || usage
    load_context "$2"
    ;;
  query)
    [[ $# -eq 3 ]] || usage
    run_query "$2" "$3"
    ;;
  *)
    usage
    ;;
esac
