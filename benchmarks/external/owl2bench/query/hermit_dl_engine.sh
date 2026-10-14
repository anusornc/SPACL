#!/usr/bin/env bash
set -euo pipefail

ADAPTER_NAME="${OWL2BENCH_PROFILE_ADAPTER_NAME:-hermit-dl}"
PROFILE="${OWL2BENCH_PROFILE:-DL}"
EXPECTED_ENGINE_NAME="${OWL2BENCH_PROFILE_ENGINE_NAME:-HermiT}"
PROFILE_BIN="${OWL2BENCH_PROFILE_BIN:-${HERMIT_DL_BIN:-}}"
PROFILE_CONFIG="${OWL2BENCH_PROFILE_CONFIG:-${HERMIT_DL_CONFIG:-}}"
PROFILE_QUERY_BIN="${OWL2BENCH_PROFILE_QUERY_BIN:-${HERMIT_DL_QUERY_BIN:-}}"
PROFILE_STATE_DIR="${OWL2BENCH_PROFILE_STATE_DIR:-.hermit-dl}"
ENGINE_KEY="$(printf '%s' "$EXPECTED_ENGINE_NAME" | tr '[:upper:]' '[:lower:]' | tr -cd '[:alnum:]_-')"
NOT_AVAILABLE_EXIT_CODE=125
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
COMPONENT_VALIDATOR="$SCRIPT_DIR/split_stack_manifest.py"
MATERIALIZATION_VALIDATOR="$SCRIPT_DIR/materialization_manifest.py"
UNSUPPORTED_EXIT_CODE=123

usage() {
  echo "usage: $0 load <$PROFILE-profile-context> | query <query-file> <$PROFILE-profile-context>" >&2
  exit 2
}

not_available() {
  printf '[%s][not_available] %s\n' "$ADAPTER_NAME" "$1" >&2
  exit "$NOT_AVAILABLE_EXIT_CODE"
}

unsupported() {
  printf '[%s][unsupported] %s\n' "$ADAPTER_NAME" "$1" >&2
  exit "$UNSUPPORTED_EXIT_CODE"
}

adapter_failed() {
  printf '[%s][failed] %s\n' "$ADAPTER_NAME" "$1" >&2
  exit 1
}

if [[ "${OWL2BENCH_PROFILE_CORE_MODE:-0}" != "1" ]]; then
  [[ "$PROFILE" == "DL" && "$ADAPTER_NAME" == "hermit-dl" && "$EXPECTED_ENGINE_NAME" == "HermiT" ]] \
    || adapter_failed "hermit-dl is permanently bound to the DL HermiT profile"
fi

sha256_file() {
  local path="$1"
  [[ -f "$path" ]] || not_available "artifact is missing: $path"
  sha256sum "$path" | awk '{print $1}'
}

resolve_executable() {
  local path="$1"
  [[ -n "$path" && -x "$path" ]] || not_available "executable is missing or not executable: $path"
  realpath "$path" || not_available "cannot resolve executable: $path"
}

resolve_file() {
  local path="$1"
  [[ -n "$path" && -r "$path" ]] || not_available "file is missing or unreadable: $path"
  realpath "$path" || not_available "cannot resolve file: $path"
}

context_paths() {
  local requested_context="$1"
  CONTEXT_DIR="$(realpath "$requested_context")" || not_available "cannot resolve profile context: $requested_context"
  [[ -d "$CONTEXT_DIR" ]] || not_available "profile context directory is missing: $requested_context"
  [[ "$(basename "$CONTEXT_DIR")" == "$PROFILE" ]] || adapter_failed "$ADAPTER_NAME adapter refuses non-$PROFILE profile context: $CONTEXT_DIR"
  STATE_DIR="$CONTEXT_DIR/$PROFILE_STATE_DIR"
  mkdir -p "$STATE_DIR" || not_available "cannot create adapter state directory: $STATE_DIR"
  PROVENANCE_FILE="$STATE_DIR/provenance.json"
}

query_manifest_path() {
  if [[ -n "${OWL2BENCH_QUERY_MANIFEST:-}" ]]; then
    printf '%s\n' "$OWL2BENCH_QUERY_MANIFEST"
  elif [[ -n "${OWL2BENCH_QUERY_STAGED_DIR:-}" ]]; then
    printf '%s/manifest.tsv\n' "${OWL2BENCH_QUERY_STAGED_DIR%/}"
  else
    printf '%s\n' ""
  fi
}

validate_component_manifest() {
  local validation_file="$STATE_DIR/component-manifest-validation.json"
  local manifest="${OWL2BENCH_QUERY_COMPONENT_MANIFEST:-}"
  local query_manifest
  query_manifest="$(query_manifest_path)"
  [[ -n "$manifest" && -r "$manifest" ]] || not_available "OWL2BENCH_QUERY_COMPONENT_MANIFEST is missing"
  [[ -n "$query_manifest" && -r "$query_manifest" ]] || not_available "OWL2BENCH_QUERY_MANIFEST is missing"
  python3 "$COMPONENT_VALIDATOR" validate \
    --component-manifest "$manifest" \
    --query-manifest "$query_manifest" \
    --output "$validation_file" \
    || not_available "component manifest validation failed"
  local profile_status
  profile_status="$(jq -r --arg profile "$PROFILE" '.profiles[$profile].status // "not_available"' "$validation_file")"
  case "$profile_status" in
    available) ;;
    unsupported) unsupported "$PROFILE capability is unsupported by the selected component manifest" ;;
    *) not_available "$PROFILE component manifest status is $profile_status" ;;
  esac
  local manifest_engine_name
  manifest_engine_name="$(jq -r --arg profile "$PROFILE" '.profiles[$profile].components.engine.name // empty' "$validation_file")"
  [[ "${manifest_engine_name,,}" == "${EXPECTED_ENGINE_NAME,,}" ]] \
    || unsupported "component manifest selects $manifest_engine_name for $PROFILE, expected $EXPECTED_ENGINE_NAME"
  if [[ "$PROFILE" == "QL" || "$PROFILE" == "RL" ]]; then
    jq -e --arg profile "$PROFILE" \
      '.profiles[$profile].ruleset | type == "object" and (.id | type == "string" and length > 0) and (.version | type == "string" and length > 0) and (.revision | type == "string" and length > 0) and (.artifact_digest | type == "string" and length > 0) and (.conformance_boundary | type == "string" and length > 0)' \
      "$validation_file" >/dev/null \
      || not_available "$PROFILE ruleset identity or conformance boundary is missing"
  fi
  COMPONENT_MANIFEST_PATH="$(realpath "$manifest")"
  COMPONENT_MANIFEST_VALIDATION="$validation_file"
  COMPONENT_PROFILE_JSON="$(jq -c --arg profile "$PROFILE" '.profiles[$profile]' "$validation_file")"
}

validate_materialization_manifest() {
  local validation_file="$STATE_DIR/materialization-manifest-validation.json"
  local manifest="${OWL2BENCH_QUERY_MATERIALIZATION_MANIFEST:-}"
  [[ -n "$manifest" && -r "$manifest" ]] || not_available "OWL2BENCH_QUERY_MATERIALIZATION_MANIFEST is missing"
  python3 "$MATERIALIZATION_VALIDATOR" validate \
    --materialization-manifest "$manifest" \
    --output "$validation_file" \
    || not_available "materialization manifest validation failed"
  local materialization_status
  materialization_status="$(jq -r --arg profile "$PROFILE" '.profiles[$profile].status // "not_available"' "$validation_file")"
  case "$materialization_status" in
    complete) ;;
    not_available) not_available "$PROFILE materialization is unavailable" ;;
    unsupported) unsupported "$PROFILE materialization capability is unsupported" ;;
    *) adapter_failed "$PROFILE materialization is $materialization_status" ;;
  esac
  MATERIALIZATION_MANIFEST_PATH="$(realpath "$manifest")"
  MATERIALIZATION_MANIFEST_VALIDATION="$validation_file"
  MATERIALIZATION_PROFILE_JSON="$(jq -c --arg profile "$PROFILE" '.profiles[$profile]' "$validation_file")"
}

prepare_context() {
  local engine_bin engine_config query_bin
  validate_component_manifest
  validate_materialization_manifest

  engine_bin="$(resolve_executable "$PROFILE_BIN")"
  engine_config="$(resolve_file "$PROFILE_CONFIG")"
  query_bin="$(resolve_executable "$PROFILE_QUERY_BIN")"

  local expected_engine_hash expected_query_hash expected_config_hash
  expected_engine_hash="$(jq -r '.components.engine.artifact_digest // empty' <<< "$COMPONENT_PROFILE_JSON")"
  expected_query_hash="$(jq -r '.components.query.artifact_digest // empty' <<< "$COMPONENT_PROFILE_JSON")"
  expected_config_hash="$(jq -r '.configuration.digest // empty' <<< "$COMPONENT_PROFILE_JSON")"
  [[ "sha256:$(sha256_file "$engine_bin")" == "$expected_engine_hash" ]] \
    || not_available "$EXPECTED_ENGINE_NAME executable does not match the manifest digest"
  [[ "sha256:$(sha256_file "$query_bin")" == "$expected_query_hash" ]] \
    || not_available "DL query backend does not match the manifest digest"
  [[ "sha256:$(sha256_file "$engine_config")" == "$expected_config_hash" ]] \
    || not_available "$EXPECTED_ENGINE_NAME configuration does not match the manifest digest"

  local expected_version version_probe_status
  expected_version="$(jq -r '.components.engine.version // empty' <<< "$COMPONENT_PROFILE_JSON")"
  set +e
  "$engine_bin" --version > "$STATE_DIR/${PROFILE,,}-version.txt" 2>&1
  version_probe_status="$?"
  set -e
  [[ "$version_probe_status" -eq 0 ]] || not_available "$EXPECTED_ENGINE_NAME runtime version probe failed"
  grep -Fq "$expected_version" "$STATE_DIR/${PROFILE,,}-version.txt" \
    || not_available "$EXPECTED_ENGINE_NAME runtime does not match the pinned version"

  local context_identity materialization_context output_graph source_ontology
  context_identity="$(jq -r '.profile_context.directory // empty' <<< "$MATERIALIZATION_PROFILE_JSON")"
  [[ "$context_identity" == "$CONTEXT_DIR" ]] \
    || adapter_failed "materialization context does not match the $PROFILE profile context"
  output_graph="$(jq -r '.output_graph.path // empty' <<< "$MATERIALIZATION_PROFILE_JSON")"
  source_ontology="$(jq -r '.source_ontology.path // empty' <<< "$MATERIALIZATION_PROFILE_JSON")"
  [[ -r "$output_graph" ]] || not_available "complete $PROFILE output graph is missing"
  [[ -r "$source_ontology" ]] || not_available "$PROFILE source ontology is missing"

  local manifest_hash validation_hash materialization_hash materialization_validation_hash
  manifest_hash="$(sha256_file "$COMPONENT_MANIFEST_PATH")"
  validation_hash="$(sha256_file "$COMPONENT_MANIFEST_VALIDATION")"
  materialization_hash="$(sha256_file "$MATERIALIZATION_MANIFEST_PATH")"
  materialization_validation_hash="$(sha256_file "$MATERIALIZATION_MANIFEST_VALIDATION")"
  jq -n \
    --arg adapter "$ADAPTER_NAME" \
    --arg profile "$PROFILE" \
    --arg engine_name "$EXPECTED_ENGINE_NAME" \
    --arg engine_key "$ENGINE_KEY" \
    --arg engine_version "$expected_version" \
    --arg context_dir "$CONTEXT_DIR" \
    --arg context_identity "profile=$PROFILE;directory=$CONTEXT_DIR" \
    --arg component_manifest "$COMPONENT_MANIFEST_PATH" \
    --arg component_manifest_sha256 "$manifest_hash" \
    --arg component_validation "$COMPONENT_MANIFEST_VALIDATION" \
    --arg component_validation_sha256 "$validation_hash" \
    --arg materialization_manifest "$MATERIALIZATION_MANIFEST_PATH" \
    --arg materialization_manifest_sha256 "$materialization_hash" \
    --arg materialization_validation "$MATERIALIZATION_MANIFEST_VALIDATION" \
    --arg materialization_validation_sha256 "$materialization_validation_hash" \
    --arg engine_bin "$engine_bin" \
    --arg engine_bin_sha256 "sha256:$(sha256_file "$engine_bin")" \
    --arg engine_config "$engine_config" \
    --arg engine_config_sha256 "sha256:$(sha256_file "$engine_config")" \
    --arg query_bin "$query_bin" \
    --arg query_bin_sha256 "sha256:$(sha256_file "$query_bin")" \
    --arg output_graph "$output_graph" \
    --arg output_graph_sha256 "sha256:$(sha256_file "$output_graph")" \
    --arg source_ontology "$source_ontology" \
    --arg source_ontology_sha256 "sha256:$(sha256_file "$source_ontology")" \
    --argjson component "$COMPONENT_PROFILE_JSON" \
    --argjson materialization "$MATERIALIZATION_PROFILE_JSON" \
    --argjson ruleset "$(jq -c --arg profile "$PROFILE" '.profiles[$profile].ruleset // {}' "$COMPONENT_MANIFEST_VALIDATION")" \
    --arg dataset_id "$(jq -r --arg profile "$PROFILE" '.profiles[$profile].dataset.id // empty' "$COMPONENT_MANIFEST_VALIDATION")" \
    --arg dataset_sha256 "$(jq -r --arg profile "$PROFILE" '.profiles[$profile].dataset.sha256 // empty' "$COMPONENT_MANIFEST_VALIDATION")" \
    '{schema_version:1,adapter:$adapter,profile:$profile,context_dir:$context_dir,context_identity:$context_identity,component_manifest:{path:$component_manifest,sha256:$component_manifest_sha256,validation_file:$component_validation,validation_sha256:$component_validation_sha256,profile:$component},materialization_manifest:{path:$materialization_manifest,sha256:$materialization_manifest_sha256,validation_file:$materialization_validation,validation_sha256:$materialization_validation_sha256,profile:$materialization},engine:{name:$engine_name,version:$engine_version,binary_path:$engine_bin,binary_sha256:$engine_bin_sha256,config_path:$engine_config,config_sha256:$engine_config_sha256},($engine_key):{name:$engine_name,version:$engine_version,binary_path:$engine_bin,binary_sha256:$engine_bin_sha256,config_path:$engine_config,config_sha256:$engine_config_sha256},ruleset:$ruleset,query_backend:{path:$query_bin,sha256:$query_bin_sha256,interface:"query <query-file> <materialized-rdf-graph>",backend_result_format:"sparql-results-xml-or-tsv"},canonical_result_format:"typed-rdf-results-v1",source_ontology:{path:$source_ontology,sha256:$source_ontology_sha256},output_graph:{path:$output_graph,sha256:$output_graph_sha256},dataset_identity:{dataset_id:$dataset_id,dataset_sha256:$dataset_sha256,source:"component_manifest"},resource_limits:$component.resource_limits,credential_status:$component.credentials.status,license_status:$component.license.status,runtime_probe:"passed"}' \
    > "$PROVENANCE_FILE"
}

read_provenance() {
  [[ -r "$PROVENANCE_FILE" ]] && jq -e . "$PROVENANCE_FILE" >/dev/null 2>&1 \
    || not_available "$ADAPTER_NAME provenance is missing or malformed"
  [[ "$(jq -r '.profile // empty' "$PROVENANCE_FILE")" == "$PROFILE" ]] \
    || adapter_failed "$ADAPTER_NAME provenance has the wrong profile"
  PROFILE_BIN_PATH="$(resolve_executable "$(jq -r '.engine.binary_path // empty' "$PROVENANCE_FILE")")"
  PROFILE_CONFIG_PATH="$(resolve_file "$(jq -r '.engine.config_path // empty' "$PROVENANCE_FILE")")"
  QUERY_BIN="$(resolve_executable "$(jq -r '.query_backend.path // empty' "$PROVENANCE_FILE")")"
  OUTPUT_GRAPH="$(resolve_file "$(jq -r '.output_graph.path // empty' "$PROVENANCE_FILE")")"
  [[ "sha256:$(sha256_file "$PROFILE_BIN_PATH")" == "$(jq -r '.engine.binary_sha256' "$PROVENANCE_FILE")" ]] \
    || not_available "$EXPECTED_ENGINE_NAME executable changed after load"
  [[ "sha256:$(sha256_file "$PROFILE_CONFIG_PATH")" == "$(jq -r '.engine.config_sha256' "$PROVENANCE_FILE")" ]] \
    || not_available "$EXPECTED_ENGINE_NAME configuration changed after load"
  [[ "sha256:$(sha256_file "$QUERY_BIN")" == "$(jq -r '.query_backend.sha256' "$PROVENANCE_FILE")" ]] \
    || not_available "$PROFILE query backend changed after load"
  [[ "sha256:$(sha256_file "$OUTPUT_GRAPH")" == "$(jq -r '.output_graph.sha256' "$PROVENANCE_FILE")" ]] \
    || not_available "$PROFILE materialized graph changed after load"
  if [[ -n "${OWL2BENCH_QUERY_MATERIALIZATION_HANDOFF:-}" ]]; then
    local handoff_file handoff_status handoff_graph handoff_hash
    handoff_file="$(realpath "$OWL2BENCH_QUERY_MATERIALIZATION_HANDOFF")" \
      || not_available "$PROFILE materialization handoff cannot be resolved"
    [[ -r "$handoff_file" ]] || not_available "$PROFILE materialization handoff is missing"
    handoff_status="$(jq -r '.status // "failed"' "$handoff_file" 2>/dev/null || printf '%s' 'failed')"
    [[ "$handoff_status" == "ready" ]] || not_available "$PROFILE materialization handoff is not ready"
    [[ "$(jq -r '.read_only // false' "$handoff_file")" == "true" ]] \
      || adapter_failed "$PROFILE materialization handoff is not read-only"
    handoff_graph="$(jq -r '.graph_path // empty' "$handoff_file")"
    handoff_hash="$(jq -r '.graph_sha256 // empty' "$handoff_file")"
    [[ "$handoff_graph" == "$OUTPUT_GRAPH" ]] \
      || adapter_failed "$PROFILE materialization handoff graph path does not match provenance"
    [[ "$handoff_hash" == "sha256:$(sha256_file "$OUTPUT_GRAPH")" ]] \
      || adapter_failed "$PROFILE materialization handoff graph digest does not match provenance"
  fi
}

load_context() {
  context_paths "$1"
  prepare_context
  jq -n --arg profile "$PROFILE" --arg provenance "$PROVENANCE_FILE" \
    --arg adapter "$ADAPTER_NAME" \
    '{status:"success",profile:$profile,adapter:$adapter,provenance_file:$provenance}'
}

run_query() {
  local query_file="$1"
  context_paths "$2"
  [[ -r "$query_file" && -f "$query_file" ]] || not_available "query file is missing or unreadable: $query_file"
  if [[ ! -f "$PROVENANCE_FILE" ]]; then
    prepare_context
  fi
  read_provenance
  query_file="$(realpath "$query_file")"
  local query_hash result_file diagnostics_file command_status
  query_hash="$(sha256_file "$query_file")"
  result_file="$STATE_DIR/result-${query_hash}.raw"
  diagnostics_file="$STATE_DIR/result-${query_hash}.diagnostics"
  set +e
  "$QUERY_BIN" query "$query_file" "$OUTPUT_GRAPH" > "$result_file" 2> "$diagnostics_file"
  command_status="$?"
  set -e
  cat "$diagnostics_file" >&2
  if [[ "$command_status" -eq "$NOT_AVAILABLE_EXIT_CODE" ]]; then
    not_available "$PROFILE query backend reported unavailable"
  elif [[ "$command_status" -ne 0 ]]; then
    exit "$command_status"
  fi
  [[ -s "$result_file" ]] || adapter_failed "DL query backend returned no result document"
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
