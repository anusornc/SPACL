#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../../.." && pwd)"
CONTRACT="$ROOT_DIR/benchmarks/competitors/scripts/benchmark_result.sh"
source "$CONTRACT"

STAGED_DIR="${OWL2BENCH_QUERY_STAGED_DIR:-$ROOT_DIR/benchmarks/ontologies/external/owl2bench/query-staged}"
MANIFEST_FILE="${OWL2BENCH_QUERY_MANIFEST:-$STAGED_DIR/manifest.tsv}"
RESULTS_ROOT="${OWL2BENCH_QUERY_RESULTS_ROOT:-$ROOT_DIR/benchmarks/external/owl2bench/query/results/history}"
RUN_ID="${RUN_ID:-owl2bench_query_$(date +%Y%m%d_%H%M%S)}"
RUN_DIR="$RESULTS_ROOT/$RUN_ID"
RAW_DIR="$RUN_DIR/raw"
RESULT_CONTRACT_FILE="$RUN_DIR/provenance/profile_result_contract.json"
ENGINE_BIN="${OWL2BENCH_QUERY_ENGINE_BIN:-}"
DATA_DIR="${OWL2BENCH_QUERY_DATA_DIR:-}"
ORACLE_DIR="${OWL2BENCH_QUERY_ORACLE_DIR:-}"
ORACLE_MANIFEST="${OWL2BENCH_QUERY_ORACLE_MANIFEST:-}"
if [[ -z "$ORACLE_MANIFEST" && -n "$ORACLE_DIR" ]]; then
  if [[ -f "$ORACLE_DIR/manifest.json" ]]; then
    ORACLE_MANIFEST="$ORACLE_DIR/manifest.json"
  elif [[ -f "$ORACLE_DIR/derived-oracle-manifest.json" ]]; then
    ORACLE_MANIFEST="$ORACLE_DIR/derived-oracle-manifest.json"
  fi
fi
DATASET_METADATA_FILE="${OWL2BENCH_QUERY_DATASET_METADATA:-}"
DATASET_ID_FALLBACK="${OWL2BENCH_QUERY_DATASET_ID:-}"
DATASET_SHA256_FALLBACK="${OWL2BENCH_QUERY_DATASET_SHA256:-}"
CAPABILITIES_FILE="${OWL2BENCH_QUERY_CAPABILITIES_FILE:-}"
COMPONENT_MANIFEST="${OWL2BENCH_QUERY_COMPONENT_MANIFEST:-}"
SPLIT_STACK_REQUESTED="${OWL2BENCH_QUERY_SPLIT_STACK:-0}"
SPLIT_STACK_MANIFEST_VALIDATOR="$ROOT_DIR/benchmarks/external/owl2bench/query/split_stack_manifest.py"
MATERIALIZATION_MANIFEST="${OWL2BENCH_QUERY_MATERIALIZATION_MANIFEST:-}"
MATERIALIZATION_VALIDATOR="$ROOT_DIR/benchmarks/external/owl2bench/query/materialization_manifest.py"
GRAPH_MATERIALIZATION_VALIDATOR="${OWL2BENCH_QUERY_GRAPH_MATERIALIZATION_VALIDATOR:-$ROOT_DIR/benchmarks/external/owl2bench/query/materialization_validator.py}"
RESULT_CONTRACT_VALIDATOR="$ROOT_DIR/benchmarks/external/owl2bench/query/profile_result_contract.py"
RESULT_AUGMENTATION_FILTER="$ROOT_DIR/benchmarks/external/owl2bench/query/profile_result_augmentation.jq"
EXECUTION_ONLY="${OWL2BENCH_QUERY_EXECUTION_ONLY:-0}"
TIMEOUT_SECONDS="${TIMEOUT_SECONDS:-600}"
UNSUPPORTED_EXIT_CODE=123
CANONICALIZER="${OWL2BENCH_QUERY_CANONICALIZER:-$ROOT_DIR/benchmarks/external/owl2bench/query/canonicalize_result.py}"
CANONICALIZER_NAME="owl2bench-query-canonicalizer"
ORACLE_LOOKUP="${OWL2BENCH_QUERY_ORACLE_LOOKUP:-$ROOT_DIR/benchmarks/external/owl2bench/query/oracle_lookup.py}"
if [[ ! -r "$CANONICALIZER" ]]; then
  echo "[owl2bench-query-run][error] missing query result canonicalizer: $CANONICALIZER" >&2
  exit 1
fi
CANONICALIZER_VERSION="$(python3 "$CANONICALIZER" --version)"
if [[ ! -r "$ORACLE_LOOKUP" ]]; then
  echo "[owl2bench-query-run][error] missing oracle metadata validator: $ORACLE_LOOKUP" >&2
  exit 1
fi
if [[ "$EXECUTION_ONLY" != "0" && "$EXECUTION_ONLY" != "1" ]]; then
  echo "[owl2bench-query-run][error] OWL2BENCH_QUERY_EXECUTION_ONLY must be 0 or 1" >&2
  exit 1
fi
if [[ ! "$RUN_ID" =~ ^[A-Za-z0-9_-]+$ || "$RUN_ID" == *..* ]]; then
  echo "[owl2bench-query-run][error] unsafe RUN_ID: $RUN_ID" >&2
  exit 1
fi

if [[ ! -f "$MANIFEST_FILE" ]]; then
  echo "[owl2bench-query-run][error] missing query manifest: $MANIFEST_FILE" >&2
  exit 1
fi

if [[ -d "$RUN_DIR" ]]; then
  if [[ ! -f "$RUN_DIR/.owl2bench-query-owned" ]]; then
    echo "[owl2bench-query-run][error] refusing to clean unstamped run directory: $RUN_DIR" >&2
    exit 1
  fi
  find "$RUN_DIR" -maxdepth 1 -type f -name '*.json' ! -name 'run_metadata.json' ! -name 'summary.json' -delete
  rm -f "$RUN_DIR/report.md" "$RUN_DIR/summary.json"
  rm -rf "$RAW_DIR"
fi
mkdir -p "$RUN_DIR" "$RAW_DIR"
touch "$RUN_DIR/.owl2bench-query-owned"

is_known_profile() {
  case "$1" in
    DL|EL|QL|RL) return 0 ;;
    *) return 1 ;;
  esac
}

normalize_profile() {
  printf '%s' "$1" | tr -d '[:space:]' | tr '[:lower:]' '[:upper:]'
}

profile_slot() {
  case "$1" in
    DL) printf '0\n' ;;
    EL) printf '1\n' ;;
    QL) printf '2\n' ;;
    RL) printf '3\n' ;;
    *) return 1 ;;
  esac
}

profile_context_dir_for() {
  local profile="$1"
  [[ -n "$DATA_DIR" ]] || return 0
  printf '%s/%s\n' "${DATA_DIR%/}" "$profile"
}

profile_context_identity_for() {
  local profile="$1"
  local context_dir="$2"
  [[ -n "$context_dir" ]] || return 0
  printf 'profile=%s;directory=%s\n' "$profile" "$context_dir"
}

profile_names=(DL EL QL RL)
profile_context_dirs=()
profile_context_identities=()
profile_context_statuses=()
profile_load_statuses=()
profile_load_times_ms=()
profile_context_provenance_files=()
profile_load_diagnostic_files=()
profile_materialization_graph_statuses=()
profile_materialization_graph_reasons=()
profile_materialization_validation_files=()
profile_materialization_handoff_files=()
profile_materialization_handoff_hashes=()
for profile_name in "${profile_names[@]}"; do
  profile_context_dirs+=("")
  profile_context_identities+=("")
  profile_context_statuses+=("not_applicable")
  profile_load_statuses+=("not_applicable")
  profile_load_times_ms+=(-1)
  profile_context_provenance_files+=("")
  profile_load_diagnostic_files+=("")
  profile_materialization_graph_statuses+=("not_configured")
  profile_materialization_graph_reasons+=("")
  profile_materialization_validation_files+=("")
  profile_materialization_handoff_files+=("")
  profile_materialization_handoff_hashes+=("")
done

current_profile_context_dir=""
current_profile_context_identity=""
current_profile_context_status="not_applicable"
current_profile_context_load_status="not_applicable"
current_profile_context_load_time_ms=-1
current_profile_context_provenance_file=""
current_profile_context_provenance_json="{}"
current_dataset_id=""
current_dataset_sha256=""
current_dataset_identity_source="unavailable"
current_diagnostics_file=""
current_load_diagnostics_file=""
current_oracle_status="unavailable"
current_oracle_class=""
current_oracle_id=""
current_oracle_case_id=""
current_oracle_query_id=""
current_oracle_profile=""
current_oracle_source_query_sha256=""
current_oracle_source_corpus=""
current_oracle_source_revision=""
current_oracle_dataset_id=""
current_oracle_dataset_sha256=""
current_oracle_profile_context_identity=""
current_oracle_canonicalizer_version=""
current_oracle_provenance=""
current_oracle_provenance_sha256=""
current_oracle_provenance_artifact=""
current_oracle_provenance_artifact_sha256=""
current_oracle_artifact_sha256=""
current_oracle_proof_artifact=""
current_oracle_proof_artifact_sha256=""
current_oracle_audit_artifact=""
current_oracle_audit_artifact_sha256=""
current_oracle_audit_status=""
current_oracle_derivation_deterministic="null"
current_oracle_sound_complete="null"
current_oracle_pipeline_version=""
current_oracle_declared_canonical_hash=""
current_oracle_reason="oracle_manifest_unconfigured"
current_differential_evidence_json="[]"
current_canonicalization_status="not_applicable"
current_canonicalization_format=""
current_canonicalization_hash=""
current_canonical_result_file=""
current_canonicalization_error=""
current_source_query_sha256=""
current_component_manifest_profile_json="{}"
current_materialization_profile_json="{}"
current_materialization_handoff_file=""
current_materialization_handoff_sha256=""
current_manifest_row_status=""
current_profile_metadata_status=""

select_profile_context() {
  current_profile_context_dir=""
  current_profile_context_identity=""
  current_profile_context_status="not_applicable"
  current_profile_context_load_status="not_applicable"
  current_profile_context_load_time_ms=-1
  current_profile_context_provenance_file=""
  current_profile_context_provenance_json="{}"
  current_dataset_id=""
  current_dataset_sha256=""
  current_dataset_identity_source="unavailable"
  current_load_diagnostics_file=""

  if [[ "$engine_data_mode" == "directory" ]]; then
    is_known_profile "$profile" || return 0

    local slot
    slot="$(profile_slot "$profile")"
    current_profile_context_dir="${profile_context_dirs[$slot]}"
    current_profile_context_identity="${profile_context_identities[$slot]}"
    current_profile_context_status="${profile_context_statuses[$slot]}"
    current_profile_context_load_status="${profile_load_statuses[$slot]}"
    current_profile_context_load_time_ms="${profile_load_times_ms[$slot]}"
    current_profile_context_provenance_file="${profile_context_provenance_files[$slot]}"
    if [[ -n "$current_profile_context_dir" ]]; then
      local provenance_candidate expected_adapter provenance_relative_path candidate_count selected_candidate
      local provenance_candidates=()
      expected_adapter="$(jq -r --arg profile "$profile" '.provenance_adapters[$profile] // .adapter // empty' "$CAPABILITIES_FILE" 2>/dev/null || true)"
      provenance_relative_path="$(jq -r --arg profile "$profile" '.provenance_relative_paths[$profile] // .provenance_relative_path // empty' "$CAPABILITIES_FILE" 2>/dev/null || true)"
      if [[ -n "$provenance_relative_path" \
          && "$provenance_relative_path" != /* \
          && "$provenance_relative_path" != *..* ]]; then
        provenance_candidates+=("$current_profile_context_dir/${provenance_relative_path#./}")
      elif [[ -z "$expected_adapter" ]]; then
        provenance_candidates+=(
          "$current_profile_context_dir/.konclude-unified/provenance.json"
          "$current_profile_context_dir/.hermit-dl/provenance.json"
          "$current_profile_context_dir/.owl2bench/provenance.json"
        )
      fi
      candidate_count=0
      selected_candidate=""
      for provenance_candidate in "${provenance_candidates[@]}"; do
        if [[ -f "$provenance_candidate" ]] \
            && jq -e \
              --arg profile "$profile" \
              --arg context_dir "$current_profile_context_dir" \
              --arg expected_adapter "$expected_adapter" \
              '(.profile == $profile)
               and ((.context_identity == ("profile=" + $profile + ";directory=" + $context_dir))
                    or .context_dir == $context_dir)
               and ($expected_adapter == "" or .adapter == $expected_adapter)' \
              "$provenance_candidate" >/dev/null 2>&1; then
          candidate_count=$((candidate_count + 1))
          selected_candidate="$provenance_candidate"
        fi
      done
      if [[ -n "$selected_candidate" ]] \
          && { [[ -n "$expected_adapter" ]] || [[ "$candidate_count" -eq 1 ]]; }; then
        current_profile_context_provenance_file="$selected_candidate"
        profile_context_provenance_files[$slot]="$selected_candidate"
      else
        current_profile_context_provenance_file=""
        profile_context_provenance_files[$slot]=""
      fi
    fi
    current_load_diagnostics_file="${profile_load_diagnostic_files[$slot]}"
    if [[ ! -f "$current_load_diagnostics_file" ]]; then
      current_load_diagnostics_file=""
    fi
    if [[ -f "$current_profile_context_provenance_file" ]] \
        && jq -e . "$current_profile_context_provenance_file" >/dev/null 2>&1; then
      current_profile_context_provenance_json="$(cat "$current_profile_context_provenance_file")"
      current_dataset_id="$(jq -r '.dataset_identity.dataset_id // .dataset_id // empty' "$current_profile_context_provenance_file")"
      current_dataset_sha256="$(jq -r '.dataset_identity.dataset_sha256 // .dataset_sha256 // .prepared_sha256 // empty' "$current_profile_context_provenance_file")"
      if [[ -n "$current_dataset_id" && -n "$current_dataset_sha256" ]]; then
        current_dataset_identity_source="profile_context_provenance"
      fi
    fi
  fi
  if [[ -f "$DATASET_METADATA_FILE" ]]; then
    metadata_dataset_id="$(jq -r --arg profile "$profile" '.[$profile].dataset_id // empty' "$DATASET_METADATA_FILE" 2>/dev/null || true)"
    metadata_dataset_sha256="$(jq -r --arg profile "$profile" '.[$profile].dataset_sha256 // empty' "$DATASET_METADATA_FILE" 2>/dev/null || true)"
    if [[ -n "$metadata_dataset_id" ]]; then
      current_dataset_id="$metadata_dataset_id"
      current_dataset_sha256="$metadata_dataset_sha256"
      current_dataset_identity_source="dataset_metadata_file"
    fi
  fi
  if [[ -n "$DATASET_ID_FALLBACK" ]]; then
    current_dataset_id="$DATASET_ID_FALLBACK"
    current_dataset_sha256="$DATASET_SHA256_FALLBACK"
    current_dataset_identity_source="environment_fallback"
  fi
  if [[ -z "$current_dataset_id" || -z "$current_dataset_sha256" ]]; then
    current_dataset_identity_source="unavailable"
  fi
}

select_component_manifest_profile() {
  current_component_manifest_profile_json="{}"
  if [[ "$component_manifest_requested" -eq 1 ]] && is_known_profile "$profile"; then
    local slot
    slot="$(profile_slot "$profile")"
    current_component_manifest_profile_json="${component_profile_records[$slot]}"
    if [[ -z "$current_dataset_id" || -z "$current_dataset_sha256" ]]; then
      current_dataset_id="$(jq -r '.dataset.id // empty' <<< "$current_component_manifest_profile_json")"
      current_dataset_sha256="$(jq -r '.dataset.sha256 // empty' <<< "$current_component_manifest_profile_json")"
      if [[ -n "$current_dataset_id" && -n "$current_dataset_sha256" ]]; then
        current_dataset_identity_source="component_manifest"
      fi
    fi
  fi
}

select_materialization_profile() {
  current_materialization_profile_json="{}"
  current_materialization_handoff_file=""
  current_materialization_handoff_sha256=""
  if [[ "$materialization_manifest_requested" -eq 1 ]] && is_known_profile "$profile"; then
    local slot
    slot="$(profile_slot "$profile")"
    current_materialization_profile_json="${materialization_profile_records[$slot]}"
    current_materialization_handoff_file="${profile_materialization_handoff_files[$slot]}"
    current_materialization_handoff_sha256="${profile_materialization_handoff_hashes[$slot]}"
    # Query-file-only adapters still need to retain the profile context that
    # identifies the complete materialized graph.  There is no load operation
    # for such adapters, but dropping the context would make a valid
    # materialization record unverifiable in the result contract.
    if [[ "$engine_data_mode" != "directory" && -z "$current_profile_context_identity" ]]; then
      local materialization_context_dir materialization_status
      materialization_context_dir="$(jq -r '.profile_context.directory // empty' <<< "$current_materialization_profile_json")"
      if [[ -n "$materialization_context_dir" ]]; then
        current_profile_context_dir="$materialization_context_dir"
        current_profile_context_identity="$(profile_context_identity_for "$profile" "$materialization_context_dir")"
        materialization_status="$(jq -r '.status // "not_available"' <<< "$current_materialization_profile_json")"
        case "$materialization_status" in
          complete)
            current_profile_context_status="available"
            ;;
          partial|failed)
            current_profile_context_status="failed"
            ;;
          unsupported)
            current_profile_context_status="unsupported"
            ;;
          *)
            current_profile_context_status="not_available"
            ;;
        esac
        current_profile_context_load_status="not_applicable"
      fi
    fi
  fi
}

materialized_query_provenance() {
  # Query-file-only adapters do not have a directory adapter provenance file.
  # Carry the validated component/materialization identities forward instead
  # of letting a successful query become an unidentifiable result record.
  [[ "$engine_data_mode" != "directory" ]] || return 0
  [[ "$current_profile_context_provenance_json" == "{}" ]] || return 0
  [[ "$(jq -r '.status // "not_available"' <<< "$current_materialization_profile_json")" == "complete" ]] \
    || return 0

  local dataset_id dataset_sha engine_name engine_version query_engine_sha
  dataset_id="$(jq -r '.dataset.id // empty' <<< "$current_component_manifest_profile_json")"
  dataset_sha="$(jq -r '.dataset.sha256 // empty' <<< "$current_component_manifest_profile_json")"
  engine_name="$(jq -r '.engine // .adapter // "materialized-graph-query"' "$CAPABILITIES_FILE" 2>/dev/null || printf '%s' 'materialized-graph-query')"
  engine_version="$(jq -r '.engine_version // "manifest-selected"' "$CAPABILITIES_FILE" 2>/dev/null || printf '%s' 'manifest-selected')"
  query_engine_sha=""
  if [[ -f "$ENGINE_BIN" ]]; then
    query_engine_sha="sha256:$(sha256_file "$ENGINE_BIN")"
  fi
  current_dataset_id="$dataset_id"
  current_dataset_sha256="$dataset_sha"
  current_dataset_identity_source="component_manifest"
  current_profile_context_provenance_json="$(jq -n \
    --arg profile "$profile" \
    --arg engine_name "$engine_name" \
    --arg engine_version "$engine_version" \
    --arg query_engine "$ENGINE_BIN" \
    --arg query_engine_sha "$query_engine_sha" \
    --arg dataset_id "$dataset_id" \
    --arg dataset_sha "$dataset_sha" \
    --arg context_dir "$current_profile_context_dir" \
    --arg context_identity "$current_profile_context_identity" \
    --arg handoff "$current_materialization_handoff_file" \
    --arg handoff_sha "$current_materialization_handoff_sha256" \
    --argjson component "$current_component_manifest_profile_json" \
    --argjson materialization "$current_materialization_profile_json" \
    '{schema_version:1,adapter:$engine_name,profile:$profile,engine:{name:$engine_name,version:$engine_version,binary_path:$query_engine,binary_sha256:(if $query_engine_sha == "" then null else $query_engine_sha end),read_only:true},query_backend:{path:$query_engine,sha256:(if $query_engine_sha == "" then null else $query_engine_sha end),interface:"query <query-file>",backend_result_format:"sparql-results-xml-or-tsv",read_only:true},dataset_identity:{dataset_id:$dataset_id,dataset_sha256:$dataset_sha,source:"component_manifest"},source_ontology:($materialization.source_ontology // null),output_graph:($materialization.output_graph // null),component_manifest:$component,materialization_manifest:$materialization,materialization_handoff:{path:$handoff,graph_sha256:$handoff_sha,read_only:true},profile_context:{profile:$profile,directory:$context_dir,identity:$context_identity},resource_limits:($component.resource_limits // $materialization.resource_limits // {}),runtime_probe:"passed",reasoning_mode:"pre-materialized-graph-read-only"}')"
}

reset_canonicalization() {
  current_canonicalization_status="not_applicable"
  current_canonicalization_format=""
  current_canonicalization_hash=""
  current_canonical_result_file=""
  current_canonicalization_error=""
}

CASE_MANIFEST="$RUN_DIR/query_cases.tsv"
printf 'query_path\tsource_query\tquery_id\tquery_profile\tworkload\tsource_corpus\tsource_revision\tcase_id\tprofile_metadata_status\tmanifest_row_status\toracle_scope\n' > "$CASE_MANIFEST"
expected_case_count=0
manifest_line_number=0
seen_case_ids=()
case_id_is_seen() {
  local candidate="$1"
  local seen_case_id
  for seen_case_id in "${seen_case_ids[@]}"; do
    [[ "$seen_case_id" == "$candidate" ]] && return 0
  done
  return 1
}

while IFS= read -r manifest_line; do
  manifest_line_number=$((manifest_line_number + 1))
  [[ -n "$manifest_line" ]] || continue
  manifest_record="${manifest_line//$'\t'/$'\x1f'}"
  IFS=$'\x1f' read -r -a manifest_fields <<< "$manifest_record"
  query_path="${manifest_fields[0]-}"
  source_query="${manifest_fields[1]-}"
  query_id="${manifest_fields[2]-}"
  profile="${manifest_fields[3]-}"
  workload="${manifest_fields[4]-}"
  source_corpus="${manifest_fields[5]-}"
  source_revision="${manifest_fields[6]-}"
  manifest_row_status="valid"
  if [[ "$manifest_line" == *$'\t' || "${#manifest_fields[@]}" -ne 7 ]]; then
    manifest_row_status="invalid"
  elif [[ -z "$query_path" || ! -r "$query_path" || -z "$source_query" || -z "$query_id" \
      || -z "$workload" || -z "$source_corpus" || -z "$source_revision" ]]; then
    manifest_row_status="invalid"
  fi

  if [[ "$manifest_row_status" != "valid" ]]; then
    invalid_query_id="${query_id:-invalid_manifest_row_${manifest_line_number}}"
    invalid_profile="$(normalize_profile "${profile%%,*}")"
    invalid_profile="${invalid_profile:-unknown}"
    case_id="${invalid_query_id}__${invalid_profile}__invalid${manifest_line_number}"
    while case_id_is_seen "$case_id"; do
      case_id="${case_id}x"
    done
    seen_case_ids+=("$case_id")
    printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\tinvalid\tinvalid\tprofile-qualified\n' \
      "$query_path" "$source_query" "$invalid_query_id" "$invalid_profile" "$workload" "$source_corpus" \
      "$source_revision" "$case_id" >> "$CASE_MANIFEST"
    expected_case_count=$((expected_case_count + 1))
    continue
  fi

  IFS=',' read -r -a profile_values <<< "${profile:-}"
  normalized_profiles=()
  for profile_value in "${profile_values[@]}"; do
    normalized_profile="$(normalize_profile "$profile_value")"
    [[ -n "$normalized_profile" ]] || continue
    normalized_profiles+=("$normalized_profile")
  done
  emitted_profile=0
  seen_profiles=()
  normalized_profile_count="${#normalized_profiles[@]}"
  for normalized_profile in "${normalized_profiles[@]}"; do
    emitted_profile=1
    profile_metadata_status="valid"
    if ! is_known_profile "$normalized_profile"; then
      profile_metadata_status="invalid"
    fi
    profile_occurrence=0
    for seen_profile in "${seen_profiles[@]}"; do
      [[ "$seen_profile" == "$normalized_profile" ]] && profile_occurrence=$((profile_occurrence + 1))
    done
    seen_profiles+=("$normalized_profile")
    case_id_base="${query_id}__${normalized_profile}"
    case_id="$case_id_base"
    if [[ "$profile_occurrence" -gt 0 ]]; then
      profile_metadata_status="invalid"
      duplicate_index=2
      while case_id_is_seen "${case_id_base}__duplicate${duplicate_index}"; do
        duplicate_index=$((duplicate_index + 1))
      done
      case_id="${case_id_base}__duplicate${duplicate_index}"
    fi
    if case_id_is_seen "$case_id"; then
      profile_metadata_status="invalid"
      duplicate_index=2
      while case_id_is_seen "${case_id_base}__duplicate${duplicate_index}"; do
        duplicate_index=$((duplicate_index + 1))
      done
      case_id="${case_id_base}__duplicate${duplicate_index}"
    fi
    seen_case_ids+=("$case_id")
    oracle_scope="profile-qualified"
    if [[ "$normalized_profile_count" -eq 1 && "$profile_metadata_status" == "valid" ]]; then
      oracle_scope="legacy-compatible-single"
    fi
    printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n' \
      "$query_path" "$source_query" "$query_id" "$normalized_profile" "$workload" \
      "$source_corpus" "$source_revision" "$case_id" "$profile_metadata_status" "$manifest_row_status" "$oracle_scope" \
      >> "$CASE_MANIFEST"
    expected_case_count=$((expected_case_count + 1))
  done
  if [[ "$emitted_profile" -eq 0 ]]; then
    case_id="${query_id}__unknown"
    if case_id_is_seen "$case_id"; then
      case_id="${case_id}__duplicate"
    fi
    seen_case_ids+=("$case_id")
    printf '%s\t%s\t%s\tunknown\t%s\t%s\t%s\t%s\tinvalid\tvalid\tprofile-qualified\n' \
      "$query_path" "$source_query" "$query_id" "$workload" "$source_corpus" \
      "$source_revision" "$case_id" >> "$CASE_MANIFEST"
    expected_case_count=$((expected_case_count + 1))
  fi
done < <(tail -n +2 "$MANIFEST_FILE")

if [[ "$expected_case_count" -eq 0 ]]; then
  echo "[owl2bench-query-run][error] query manifest produced no query/profile cases" >&2
  exit 1
fi

component_manifest_requested=0
component_manifest_status="not_configured"
component_manifest_validation_file=""
component_profile_records=("{}" "{}" "{}" "{}")
component_manifest_profiles_json="{}"
component_manifest_required_by_capability=0
if jq -e '.component_manifest_required == true' "$CAPABILITIES_FILE" >/dev/null 2>&1; then
  component_manifest_required_by_capability=1
fi
if [[ "$SPLIT_STACK_REQUESTED" == "1" || -n "$COMPONENT_MANIFEST" || "$component_manifest_required_by_capability" == "1" ]]; then
  component_manifest_requested=1
  component_manifest_validation_file="$RUN_DIR/provenance/split_stack_manifest.json"
  component_manifest_input="$COMPONENT_MANIFEST"
  if [[ -z "$component_manifest_input" ]]; then
    component_manifest_input="$RUN_DIR/provenance/missing-component-manifest.json"
  fi
  mkdir -p "$(dirname "$component_manifest_validation_file")"
  if [[ -r "$SPLIT_STACK_MANIFEST_VALIDATOR" ]]; then
    python3 "$SPLIT_STACK_MANIFEST_VALIDATOR" validate \
      --component-manifest "$component_manifest_input" \
      --query-manifest "$MANIFEST_FILE" \
      --output "$component_manifest_validation_file"
  else
    jq -n \
      --arg manifest "$COMPONENT_MANIFEST" \
      '{schema_version:1,validator:"owl2bench-split-stack-manifest/1",status:"failed",manifest:$manifest,manifest_errors:["validator_missing"],answer_key_policy:"not_permitted"}' \
      > "$component_manifest_validation_file"
  fi
  component_manifest_status="$(jq -r '.status // "failed"' "$component_manifest_validation_file" 2>/dev/null || printf '%s' 'failed')"
  for profile_name in "${profile_names[@]}"; do
    slot="$(profile_slot "$profile_name")"
    component_profile_records[$slot]="$(jq -c --arg profile "$profile_name" \
      '.profiles[$profile] // {profile:$profile,status:"not_available",reasons:["component_manifest_profile_missing"],manifest_identity:null}' \
      "$component_manifest_validation_file" 2>/dev/null || printf '%s' '{}')"
  done
  component_manifest_profiles_json="$(jq -n \
    --argjson dl "${component_profile_records[0]}" \
    --argjson el "${component_profile_records[1]}" \
    --argjson ql "${component_profile_records[2]}" \
    --argjson rl "${component_profile_records[3]}" \
    '{DL:$dl,EL:$el,QL:$ql,RL:$rl}')"
fi

materialization_manifest_requested=0
materialization_manifest_status="not_configured"
materialization_manifest_fatal="false"
materialization_manifest_validation_file=""
materialization_profile_records=("{}" "{}" "{}" "{}")
materialization_profiles_json="{}"
materialization_manifest_required_by_capability=0
if jq -e '.materialization_manifest_required == true' "$CAPABILITIES_FILE" >/dev/null 2>&1; then
  materialization_manifest_required_by_capability=1
fi
if [[ "$SPLIT_STACK_REQUESTED" == "1" || -n "$MATERIALIZATION_MANIFEST" || "$materialization_manifest_required_by_capability" == "1" ]]; then
  materialization_manifest_requested=1
  materialization_manifest_validation_file="$RUN_DIR/provenance/materialization_manifest.json"
  materialization_manifest_input="$MATERIALIZATION_MANIFEST"
  if [[ -z "$materialization_manifest_input" ]]; then
    materialization_manifest_input="$RUN_DIR/provenance/missing-materialization-manifest.json"
  fi
  mkdir -p "$(dirname "$materialization_manifest_validation_file")"
  if [[ -r "$MATERIALIZATION_VALIDATOR" ]]; then
    python3 "$MATERIALIZATION_VALIDATOR" validate \
      --materialization-manifest "$materialization_manifest_input" \
      --output "$materialization_manifest_validation_file"
  else
    jq -n \
      --arg manifest "$materialization_manifest_input" \
      '{schema_version:1,validator:"owl2bench-materialization-manifest/1",status:"failed",fatal:true,manifest:$manifest,manifest_errors:["validator_missing"],answer_key_policy:"not_permitted"}' \
      > "$materialization_manifest_validation_file"
  fi
  materialization_manifest_status="$(jq -r '.status // "failed"' "$materialization_manifest_validation_file" 2>/dev/null || printf '%s' 'failed')"
  materialization_manifest_fatal="$(jq -r 'if .fatal == true then "true" else "false" end' "$materialization_manifest_validation_file" 2>/dev/null || printf '%s' 'true')"
  for profile_name in "${profile_names[@]}"; do
    slot="$(profile_slot "$profile_name")"
    materialization_profile_records[$slot]="$(jq -c --arg profile "$profile_name" \
      '.profiles[$profile] // {profile:$profile,status:"not_available",reasons:["materialization_manifest_profile_missing"],materialization_identity:null}' \
      "$materialization_manifest_validation_file" 2>/dev/null || printf '%s' '{}')"
  done
  materialization_profiles_json="$(jq -n \
    --argjson dl "${materialization_profile_records[0]}" \
    --argjson el "${materialization_profile_records[1]}" \
    --argjson ql "${materialization_profile_records[2]}" \
    --argjson rl "${materialization_profile_records[3]}" \
    '{DL:$dl,EL:$el,QL:$ql,RL:$rl}')"
fi

validate_graph_materialization_handoffs() {
  local profile slot source_graph candidate_graph output_graph validation_file handoff_file
  local validation_status validation_exit handoff_status handoff_exit reason graph_hash
  local adapter_managed provenance_relative provenance_file handoff_file_managed graph_file graph_hash_expected
  [[ -n "${materialization_manifest_input:-}" && -r "$materialization_manifest_input" ]] || return 0

  if [[ ! -r "$GRAPH_MATERIALIZATION_VALIDATOR" ]]; then
    for profile in "${profile_names[@]}"; do
      slot="$(profile_slot "$profile")"
      if jq -e --arg profile "$profile" \
        '.profiles[$profile] | type == "object" and has("graph_contract")' \
        "$materialization_manifest_input" >/dev/null 2>&1; then
        profile_materialization_graph_statuses[$slot]="not_available"
        profile_materialization_graph_reasons[$slot]="graph_materialization_validator_missing"
      fi
    done
    return 0
  fi

  # ADR 0027: a stack whose directory adapter owns its materialization handoff
  # (load materializes, writes the graph plus a ready handoff, and query
  # re-validates the graph hash before delegating) may declare
  # adapter_managed_materialization instead of supplying a canonical export
  # graph contract.  The runner still fail-closes per profile: the pinned
  # adapter provenance, a ready handoff, and a matching graph hash are
  # verified here, the materialization manifest validator re-checks every
  # record identity, and the result contract re-validates the records kept
  # in each result row.
  adapter_managed=0
  if [[ -n "$DATA_DIR" ]] \
      && jq -e '.adapter_managed_materialization == true' "$CAPABILITIES_FILE" >/dev/null 2>&1; then
    adapter_managed=1
  fi

  for profile in "${profile_names[@]}"; do
    slot="$(profile_slot "$profile")"
    if ! jq -e --arg profile "$profile" \
      '.profiles[$profile] | type == "object" and has("graph_contract") and has("candidate_graph") and has("output_graph")' \
      "$materialization_manifest_input" >/dev/null 2>&1; then
      if [[ "$adapter_managed" -eq 1 ]]; then
        provenance_relative="$(jq -r --arg profile "$profile" '.provenance_relative_paths[$profile] // empty' "$CAPABILITIES_FILE" 2>/dev/null || true)"
        if [[ -n "$provenance_relative" && "$provenance_relative" != /* && "$provenance_relative" != *..* ]]; then
          provenance_file="${DATA_DIR%/}/$profile/${provenance_relative#./}"
          handoff_file_managed="$(dirname "$provenance_file")/handoff.json"
          graph_file=""
          graph_hash_expected=""
          if [[ -f "$provenance_file" && -f "$handoff_file_managed" ]] \
              && jq -e . "$provenance_file" >/dev/null 2>&1 \
              && [[ "$(jq -r '.status // ""' "$handoff_file_managed" 2>/dev/null || printf '%s' '')" == "ready" ]]; then
            graph_file="$(jq -r '.graph_path // ""' "$handoff_file_managed" 2>/dev/null || printf '%s' '')"
            graph_hash_expected="$(jq -r '.graph_sha256 // ""' "$handoff_file_managed" 2>/dev/null || printf '%s' '')"
          fi
          if [[ -n "$graph_file" && -f "$graph_file" ]] \
              && [[ "$graph_hash_expected" =~ ^sha256:[0-9a-fA-F]{64}$ ]] \
              && [[ "$graph_hash_expected" == "sha256:$(sha256sum "$graph_file" | awk '{print $1}')" ]]; then
            profile_materialization_graph_statuses[$slot]="complete"
            profile_materialization_graph_reasons[$slot]=""
          else
            profile_materialization_graph_statuses[$slot]="failed"
            profile_materialization_graph_reasons[$slot]="adapter_managed_handoff_not_verified"
          fi
        fi
      fi
      continue
    fi

    source_graph="$(jq -r --arg profile "$profile" '.profiles[$profile].source_ontology.path // empty' "$materialization_manifest_input")"
    candidate_graph="$(jq -r --arg profile "$profile" '.profiles[$profile].candidate_graph.path // empty' "$materialization_manifest_input")"
    output_graph="$(jq -r --arg profile "$profile" '.profiles[$profile].output_graph.path // empty' "$materialization_manifest_input")"
    validation_file="$RUN_DIR/provenance/materialization_graph_${profile,,}_validation.json"
    handoff_file="$RUN_DIR/provenance/materialization_graph_${profile,,}_handoff.json"
    profile_materialization_validation_files[$slot]="$validation_file"
    profile_materialization_handoff_files[$slot]=""
    profile_materialization_handoff_hashes[$slot]=""

    set +e
    python3 "$GRAPH_MATERIALIZATION_VALIDATOR" validate \
      --profile "$profile" \
      --source "$source_graph" \
      --candidate-graph "$candidate_graph" \
      --graph "$output_graph" \
      --manifest "$materialization_manifest_input" \
      --output "$validation_file"
    validation_exit="$?"
    set -e
    validation_status="$(jq -r '.status // "failed"' "$validation_file" 2>/dev/null || printf '%s' 'failed')"
    reason="$(jq -r '((.reasons // []) | join(";"))' "$validation_file" 2>/dev/null || printf '%s' 'graph_validator_failed')"
    if [[ "$validation_exit" -ne 0 && "$validation_status" == "complete" ]]; then
      validation_status="failed"
      reason="graph_validator_exit_status:$validation_exit"
    fi

    if [[ "$validation_status" == "complete" ]]; then
      set +e
      python3 "$GRAPH_MATERIALIZATION_VALIDATOR" handoff \
        --validation "$validation_file" \
        --graph "$output_graph" \
        --output "$handoff_file"
      handoff_exit="$?"
      set -e
      handoff_status="$(jq -r '.status // "failed"' "$handoff_file" 2>/dev/null || printf '%s' 'failed')"
      if [[ "$handoff_exit" -eq 0 && "$handoff_status" == "ready" ]]; then
        graph_hash="$(jq -r '.graph_sha256 // empty' "$handoff_file")"
        profile_materialization_graph_statuses[$slot]="complete"
        profile_materialization_graph_reasons[$slot]=""
        profile_materialization_handoff_files[$slot]="$handoff_file"
        profile_materialization_handoff_hashes[$slot]="$graph_hash"
        continue
      fi
      validation_status="$handoff_status"
      reason="query_handoff:$handoff_status"
    fi

    profile_materialization_graph_statuses[$slot]="$validation_status"
    profile_materialization_graph_reasons[$slot]="${reason:-graph_materialization_unavailable}"
  done
}

validate_graph_materialization_handoffs

split_stack_materialization_validation_file=""
split_stack_materialization_status="not_configured"
split_stack_materialization_global_error="0"
split_stack_materialization_profile_statuses=("not_configured" "not_configured" "not_configured" "not_configured")
split_stack_materialization_profile_reasons=("" "" "" "")
split_stack_materialization_profiles_json="{}"
if [[ "$SPLIT_STACK_REQUESTED" == "1" ]]; then
  split_stack_materialization_validation_file="$RUN_DIR/provenance/split_stack_materialization.json"
  mkdir -p "$(dirname "$split_stack_materialization_validation_file")"
  set +e
  python3 "$MATERIALIZATION_VALIDATOR" verify-stack \
    --component-manifest "${component_manifest_input:-$RUN_DIR/provenance/missing-component-manifest.json}" \
    --materialization-manifest "${materialization_manifest_input:-$RUN_DIR/provenance/missing-materialization-manifest.json}" \
    --query-manifest "$MANIFEST_FILE" \
    --output "$split_stack_materialization_validation_file"
  split_stack_materialization_exit_status="$?"
  set -e
  split_stack_materialization_status="$(jq -r '.status // "failed"' \
    "$split_stack_materialization_validation_file" 2>/dev/null || printf '%s' 'failed')"
  if [[ ! -s "$split_stack_materialization_validation_file" ]]; then
    split_stack_materialization_global_error="1"
  fi
  if [[ "$split_stack_materialization_exit_status" -ne 0 && "$split_stack_materialization_status" == "complete" ]]; then
    split_stack_materialization_status="failed"
    split_stack_materialization_global_error="1"
  fi
  for profile_name in "${profile_names[@]}"; do
    slot="$(profile_slot "$profile_name")"
    split_stack_materialization_profile_statuses[$slot]="$(jq -r --arg profile "$profile_name" \
      '.profiles[$profile].status // "not_available"' \
      "$split_stack_materialization_validation_file" 2>/dev/null || printf '%s' 'not_available')"
    split_stack_materialization_profile_reasons[$slot]="$(jq -r --arg profile "$profile_name" \
      '((.profiles[$profile].cross_contract_errors // []) + (.profiles[$profile].component.reasons // []) + (.profiles[$profile].materialization.reasons // []) | join("; "))' \
      "$split_stack_materialization_validation_file" 2>/dev/null || printf '%s' '')"
  done
  split_stack_materialization_profiles_json="$(jq -n \
    --arg dl_status "${split_stack_materialization_profile_statuses[0]}" \
    --arg dl_reason "${split_stack_materialization_profile_reasons[0]}" \
    --arg el_status "${split_stack_materialization_profile_statuses[1]}" \
    --arg el_reason "${split_stack_materialization_profile_reasons[1]}" \
    --arg ql_status "${split_stack_materialization_profile_statuses[2]}" \
    --arg ql_reason "${split_stack_materialization_profile_reasons[2]}" \
    --arg rl_status "${split_stack_materialization_profile_statuses[3]}" \
    --arg rl_reason "${split_stack_materialization_profile_reasons[3]}" \
    '{DL:{status:$dl_status,reason:$dl_reason},EL:{status:$el_status,reason:$el_reason},QL:{status:$ql_status,reason:$ql_reason},RL:{status:$rl_status,reason:$rl_reason}}')"
  if [[ "$(jq -r 'if (.errors // []) | length > 0 then "1" else "0" end' \
    "$split_stack_materialization_validation_file" 2>/dev/null || printf '%s' '1')" == "1" ]]; then
    split_stack_materialization_global_error="1"
  fi
fi

engine_data_mode=""
jq -n \
  --arg run_id "$RUN_ID" \
  --arg manifest "$MANIFEST_FILE" \
  --arg case_manifest "$CASE_MANIFEST" \
  --arg engine "$ENGINE_BIN" \
  --arg data_dir "$DATA_DIR" \
  --arg oracle_dir "$ORACLE_DIR" \
  --arg oracle_manifest "$ORACLE_MANIFEST" \
  --arg dataset_metadata_file "$DATASET_METADATA_FILE" \
  --arg dataset_id_fallback "$DATASET_ID_FALLBACK" \
  --arg dataset_sha256_fallback "$DATASET_SHA256_FALLBACK" \
  --arg capabilities_file "$CAPABILITIES_FILE" \
  --arg canonicalizer_name "$CANONICALIZER_NAME" \
  --arg canonicalizer_version "$CANONICALIZER_VERSION" \
  --arg canonicalizer_path "$CANONICALIZER" \
  --arg component_manifest "$COMPONENT_MANIFEST" \
  --arg split_stack_requested "$SPLIT_STACK_REQUESTED" \
  --arg component_manifest_validation_file "$component_manifest_validation_file" \
  --arg component_manifest_status "$component_manifest_status" \
  --argjson component_manifest_profiles "$component_manifest_profiles_json" \
  --arg materialization_manifest "$MATERIALIZATION_MANIFEST" \
  --arg materialization_manifest_validation_file "$materialization_manifest_validation_file" \
  --arg materialization_manifest_status "$materialization_manifest_status" \
  --arg materialization_manifest_fatal "$materialization_manifest_fatal" \
  --argjson component_manifest_requested "$component_manifest_requested" \
  --argjson materialization_manifest_requested "$materialization_manifest_requested" \
  --argjson materialization_profiles "$materialization_profiles_json" \
  --arg split_stack_materialization_validation_file "$split_stack_materialization_validation_file" \
  --arg split_stack_materialization_status "$split_stack_materialization_status" \
  --arg split_stack_materialization_global_error "$split_stack_materialization_global_error" \
  --argjson split_stack_materialization_profiles "$split_stack_materialization_profiles_json" \
  --arg result_contract_validator "$RESULT_CONTRACT_VALIDATOR" \
  --arg result_contract_file "$RESULT_CONTRACT_FILE" \
  --arg data_mode "$engine_data_mode" \
  --argjson query_case_count "$expected_case_count" \
  --argjson timeout_seconds "$TIMEOUT_SECONDS" \
  '{schema_version:1,run_id:$run_id, suite:"owl2bench", lane:"query", manifest:$manifest, case_manifest:$case_manifest, query_case_count:$query_case_count, engine:$engine, data_dir:$data_dir, oracle_dir:$oracle_dir, oracle_manifest:(if $oracle_manifest == "" then null else $oracle_manifest end), dataset_metadata_file:(if $dataset_metadata_file == "" then null else $dataset_metadata_file end), dataset_identity_fallback:(if $dataset_id_fallback == "" then null else {dataset_id:$dataset_id_fallback, dataset_sha256:$dataset_sha256_fallback, source:"environment_fallback"} end), capabilities_file:$capabilities_file, canonicalizer:{name:$canonicalizer_name,version:$canonicalizer_version,path:$canonicalizer_path}, component_manifest:{requested:($component_manifest_requested == 1),status:$component_manifest_status,manifest:(if $component_manifest == "" then null else $component_manifest end),validation_file:(if $component_manifest_validation_file == "" then null else $component_manifest_validation_file end),profiles:$component_manifest_profiles,answer_key_policy:"not_permitted"}, materialization:{requested:($materialization_manifest_requested == 1),status:$materialization_manifest_status,fatal:($materialization_manifest_fatal == "true"),manifest:(if $materialization_manifest == "" then null else $materialization_manifest end),validation_file:(if $materialization_manifest_validation_file == "" then null else $materialization_manifest_validation_file end),profiles:$materialization_profiles,answer_key_policy:"not_permitted"}, split_stack_materialization:{requested:($split_stack_requested == "1"),status:$split_stack_materialization_status,global_error:($split_stack_materialization_global_error == "1"),profiles:$split_stack_materialization_profiles,validation_file:(if $split_stack_materialization_validation_file == "" then null else $split_stack_materialization_validation_file end),answer_key_policy:"not_permitted"}, result_contract:{validator:$result_contract_validator,validation_file:$result_contract_file,status:"pending",answer_key_policy:"not_permitted"}, oracle_policy:{requires_exact_identity:true, accepted_classes:["official","derived"], differential_is_evidence_only:true}, data_mode:$data_mode, timeout_seconds:$timeout_seconds, query_correctness_requires_explicit_oracle:true}' \
  > "$RUN_DIR/run_metadata.json"

now_ms() { python3 -c 'import time; print(int(time.time() * 1000))'; }
sha256_file() { sha256sum "$1" | awk '{print $1}'; }
canonicalize_result_file() {
  local query_file="$1"
  local result_file="$2"
  local canonical_file="$3"
  python3 "$CANONICALIZER" --query "$query_file" --input "$result_file" --output "$canonical_file"
}

reset_oracle_match() {
  current_oracle_status="unavailable"
  current_oracle_class=""
  current_oracle_id=""
  current_oracle_case_id=""
  current_oracle_query_id=""
  current_oracle_profile=""
  current_oracle_source_query_sha256=""
  current_oracle_source_corpus=""
  current_oracle_source_revision=""
  current_oracle_dataset_id=""
  current_oracle_dataset_sha256=""
  current_oracle_profile_context_identity=""
  current_oracle_canonicalizer_version=""
  current_oracle_provenance=""
  current_oracle_provenance_sha256=""
  current_oracle_provenance_artifact=""
  current_oracle_provenance_artifact_sha256=""
  current_oracle_artifact_sha256=""
  current_oracle_proof_artifact=""
  current_oracle_proof_artifact_sha256=""
  current_oracle_audit_artifact=""
  current_oracle_audit_artifact_sha256=""
  current_oracle_audit_status=""
  current_oracle_derivation_deterministic="null"
  current_oracle_sound_complete="null"
  current_oracle_pipeline_version=""
  current_oracle_declared_canonical_hash=""
  current_oracle_reason="oracle_manifest_unconfigured"
  current_differential_evidence_json="[]"
}

lookup_oracle_for_case() {
  reset_oracle_match
  if [[ -z "$ORACLE_DIR" || -z "$ORACLE_MANIFEST" ]]; then
    return 0
  fi
  if [[ -z "$current_dataset_id" || -z "$current_dataset_sha256" ]]; then
    current_oracle_reason="dataset_identity_missing"
    return 0
  fi

  local query_sha256 lookup_output lookup_error_file lookup_status
  local lookup_args=()
  query_sha256="$current_source_query_sha256"
  lookup_args+=(--oracle-dir "$ORACLE_DIR" --case-id "$case_id" --query-id "$query_id" --profile "$profile")
  lookup_args+=(--source-query-sha256 "$query_sha256" --source-corpus "$source_corpus" --source-revision "$source_revision")
  lookup_args+=(--dataset-id "$current_dataset_id" --dataset-sha256 "$current_dataset_sha256")
  if [[ -n "$current_profile_context_identity" ]]; then
    lookup_args+=(--profile-context-identity "$current_profile_context_identity")
  fi
  lookup_args+=(--canonicalizer-version "$CANONICALIZER_VERSION" --manifest "$ORACLE_MANIFEST")
  lookup_error_file="$RAW_DIR/${safe_id}.oracle-lookup.stderr"
  set +e
  lookup_output="$(python3 "$ORACLE_LOOKUP" "${lookup_args[@]}" 2> "$lookup_error_file")"
  lookup_status="$?"
  set -e
  if [[ "$lookup_status" -ne 0 ]] || ! jq -e . <<< "$lookup_output" >/dev/null 2>&1; then
    current_oracle_reason="oracle_lookup_failed"
    current_differential_evidence_json="[]"
    return 0
  fi
  current_oracle_status="$(jq -r '.status // "unavailable"' <<< "$lookup_output")"
  current_oracle_class="$(jq -r '.oracle_class // empty' <<< "$lookup_output")"
  current_oracle_id="$(jq -r '.oracle_id // empty' <<< "$lookup_output")"
  current_oracle_case_id="$(jq -r '.case_id // empty' <<< "$lookup_output")"
  current_oracle_query_id="$(jq -r '.query_id // empty' <<< "$lookup_output")"
  current_oracle_profile="$(jq -r '.profile // empty' <<< "$lookup_output")"
  current_oracle_source_query_sha256="$(jq -r '.source_query_sha256 // empty' <<< "$lookup_output")"
  current_oracle_source_corpus="$(jq -r '.source_corpus // empty' <<< "$lookup_output")"
  current_oracle_source_revision="$(jq -r '.source_revision // empty' <<< "$lookup_output")"
  current_oracle_dataset_id="$(jq -r '.dataset_id // empty' <<< "$lookup_output")"
  current_oracle_dataset_sha256="$(jq -r '.dataset_sha256 // empty' <<< "$lookup_output")"
  current_oracle_profile_context_identity="$(jq -r '.profile_context_identity // empty' <<< "$lookup_output")"
  current_oracle_canonicalizer_version="$(jq -r '.canonicalizer_version // empty' <<< "$lookup_output")"
  current_oracle_provenance="$(jq -r '.oracle_provenance // empty' <<< "$lookup_output")"
  current_oracle_provenance_sha256="$(jq -r '.oracle_provenance_sha256 // empty' <<< "$lookup_output")"
  current_oracle_provenance_artifact="$(jq -r '.provenance_artifact // empty' <<< "$lookup_output")"
  current_oracle_provenance_artifact_sha256="$(jq -r '.provenance_artifact_sha256 // empty' <<< "$lookup_output")"
  current_oracle_artifact_sha256="$(jq -r '.oracle_artifact_sha256 // empty' <<< "$lookup_output")"
  current_oracle_proof_artifact="$(jq -r '.proof_artifact // empty' <<< "$lookup_output")"
  current_oracle_proof_artifact_sha256="$(jq -r '.proof_artifact_sha256 // empty' <<< "$lookup_output")"
  current_oracle_audit_artifact="$(jq -r '.audit_artifact // empty' <<< "$lookup_output")"
  current_oracle_audit_artifact_sha256="$(jq -r '.audit_artifact_sha256 // empty' <<< "$lookup_output")"
  current_oracle_audit_status="$(jq -r '.audit_status // empty' <<< "$lookup_output")"
  current_oracle_derivation_deterministic="$(jq -c '.derivation_deterministic // null' <<< "$lookup_output")"
  current_oracle_sound_complete="$(jq -c '.sound_complete // null' <<< "$lookup_output")"
  current_oracle_pipeline_version="$(jq -r '.oracle_pipeline_version // empty' <<< "$lookup_output")"
  current_oracle_declared_canonical_hash="$(jq -r '.canonical_result_hash // empty' <<< "$lookup_output")"
  current_oracle_reason="$(jq -r '.reason // "oracle_unavailable"' <<< "$lookup_output")"
  current_differential_evidence_json="$(jq -c '.differential_evidence // []' <<< "$lookup_output")"
  oracle_file="$(jq -r '.oracle_file // empty' <<< "$lookup_output")"
}

profile_supported() {
  local profile_list="$1"
  local query_profile
  local query_profiles=()
  IFS=',' read -r -a query_profiles <<< "$profile_list"
  for query_profile in "${query_profiles[@]}"; do
    query_profile="$(printf '%s' "$query_profile" | tr -d '[:space:]')"
    if [[ -n "$query_profile" ]] \
      && jq -e --arg profile "$query_profile" \
        '(.profiles | index("ALL") != null or index($profile) != null)' \
        "$CAPABILITIES_FILE" >/dev/null 2>&1; then
      return 0
    fi
  done
  return 1
}

augment_query_result() {
  local result_file="$1"
  local profile="$2"
  local source_query="$3"
  local query_id="$4"
  local workload="$5"
  local source_corpus="$6"
  local source_revision="$7"
  local load_time_ms="$8"
  local query_time_ms="$9"
  local engine_detail="${10}"
  local load_status="${11}"
  local temporary_file
  temporary_file="$(mktemp)"
  jq \
    --arg profile "$profile" \
    --arg source_query "$source_query" \
    --arg query_id "$query_id" \
    --arg workload "$workload" \
    --arg source_corpus "$source_corpus" \
    --arg source_revision "$source_revision" \
    --arg engine_detail "$engine_detail" \
    --arg load_status "$load_status" \
    --arg profile_context_dir "$current_profile_context_dir" \
    --arg profile_context_identity "$current_profile_context_identity" \
    --arg profile_context_status "$current_profile_context_status" \
    --arg profile_context_load_status "$current_profile_context_load_status" \
    --arg profile_context_provenance_file "$current_profile_context_provenance_file" \
    --arg diagnostics_file "$current_diagnostics_file" \
    --arg load_diagnostics_file "$current_load_diagnostics_file" \
    --argjson engine_provenance "$current_profile_context_provenance_json" \
    --argjson component_manifest_profile "$current_component_manifest_profile_json" \
    --argjson materialization_profile "$current_materialization_profile_json" \
    --arg materialization_handoff_file "$current_materialization_handoff_file" \
    --arg materialization_handoff_sha256 "$current_materialization_handoff_sha256" \
    --arg execution_only "$EXECUTION_ONLY" \
    --arg oracle_class "$current_oracle_class" \
    --arg oracle_id "$current_oracle_id" \
    --arg oracle_matched_case_id "$current_oracle_case_id" \
    --arg oracle_query_id "$current_oracle_query_id" \
    --arg oracle_profile "$current_oracle_profile" \
    --arg oracle_source_query_sha256 "$current_oracle_source_query_sha256" \
    --arg oracle_source_corpus "$current_oracle_source_corpus" \
    --arg oracle_source_revision "$current_oracle_source_revision" \
    --arg oracle_dataset_id "$current_oracle_dataset_id" \
    --arg oracle_dataset_sha256 "$current_oracle_dataset_sha256" \
    --arg oracle_profile_context_identity "$current_oracle_profile_context_identity" \
    --arg oracle_canonicalizer_version "$current_oracle_canonicalizer_version" \
    --arg oracle_provenance "$current_oracle_provenance" \
    --arg oracle_provenance_sha256 "$current_oracle_provenance_sha256" \
    --arg oracle_provenance_artifact "$current_oracle_provenance_artifact" \
    --arg oracle_provenance_artifact_sha256 "$current_oracle_provenance_artifact_sha256" \
    --arg oracle_artifact_sha256 "$current_oracle_artifact_sha256" \
    --arg oracle_proof_artifact "$current_oracle_proof_artifact" \
    --arg oracle_proof_artifact_sha256 "$current_oracle_proof_artifact_sha256" \
    --arg oracle_audit_artifact "$current_oracle_audit_artifact" \
    --arg oracle_audit_artifact_sha256 "$current_oracle_audit_artifact_sha256" \
    --arg oracle_audit_status "$current_oracle_audit_status" \
    --argjson oracle_derivation_deterministic "$current_oracle_derivation_deterministic" \
    --argjson oracle_sound_complete "$current_oracle_sound_complete" \
    --arg oracle_pipeline_version "$current_oracle_pipeline_version" \
    --arg oracle_declared_canonical_hash "$current_oracle_declared_canonical_hash" \
    --arg oracle_match_reason "$current_oracle_reason" \
    --arg oracle_manifest "$ORACLE_MANIFEST" \
    --argjson differential_evidence "$current_differential_evidence_json" \
    --arg dataset_id "$current_dataset_id" \
    --arg dataset_sha256 "$current_dataset_sha256" \
    --arg dataset_identity_source "$current_dataset_identity_source" \
    --arg canonicalizer_name "$CANONICALIZER_NAME" \
    --arg canonicalizer_version "$CANONICALIZER_VERSION" \
    --arg canonicalization_status "$current_canonicalization_status" \
    --arg canonicalization_format "$current_canonicalization_format" \
    --arg canonicalization_hash "$current_canonicalization_hash" \
    --arg canonical_result_file "$current_canonical_result_file" \
    --arg canonicalization_error "$current_canonicalization_error" \
    --arg source_query_sha256 "$current_source_query_sha256" \
    --arg manifest_row_status "$current_manifest_row_status" \
    --arg profile_metadata_status "$current_profile_metadata_status" \
    --argjson load_time_ms "$load_time_ms" \
    --argjson query_time_ms "$query_time_ms" \
    --argjson profile_context_load_time_ms "$current_profile_context_load_time_ms" \
    -f "$RESULT_AUGMENTATION_FILTER" \
    "$result_file" > "$temporary_file"
  mv "$temporary_file" "$result_file"
}

engine_available=0
engine_detail="not_available"
if [[ -n "$ENGINE_BIN" && -x "$ENGINE_BIN" ]]; then
  if [[ -f "$CAPABILITIES_FILE" ]] && jq -e '.supports_query == true and (.profiles | type == "array") and (.data_mode | IN("query-file-only", "directory"))' "$CAPABILITIES_FILE" >/dev/null 2>&1; then
    engine_available=1
    engine_detail="available"
    engine_data_mode="$(jq -r '.data_mode' "$CAPABILITIES_FILE")"
  else
    engine_detail="capability_manifest_missing_or_invalid"
  fi
fi

if [[ "$engine_available" -eq 1 && "$engine_data_mode" == "directory" && ! -d "$DATA_DIR" ]]; then
  engine_available=0
  engine_detail="missing_data_directory"
fi

initialize_profile_contexts() {
  local profile slot context_dir
  for profile in "${profile_names[@]}"; do
    slot="$(profile_slot "$profile")"
    profile_context_dirs[$slot]=""
    profile_context_identities[$slot]=""
    profile_context_statuses[$slot]="not_applicable"
    profile_load_statuses[$slot]="not_applicable"
    profile_load_times_ms[$slot]=-1
    profile_context_provenance_files[$slot]=""
    profile_load_diagnostic_files[$slot]=""

    [[ "$engine_data_mode" == "directory" ]] || continue
    if [[ -z "$DATA_DIR" ]]; then
      profile_context_statuses[$slot]="not_available"
      profile_load_statuses[$slot]="not_available"
      continue
    fi

    context_dir="$(profile_context_dir_for "$profile")"
    profile_context_dirs[$slot]="$context_dir"
    profile_context_identities[$slot]="$(profile_context_identity_for "$profile" "$context_dir")"
    profile_context_provenance_files[$slot]="$context_dir/.konclude-unified/provenance.json"
    profile_load_diagnostic_files[$slot]="$RAW_DIR/load_${profile}.diagnostics"
    if [[ ! -d "$context_dir" ]]; then
      profile_context_statuses[$slot]="not_available"
      profile_load_statuses[$slot]="not_available"
    elif [[ "$engine_available" -eq 1 ]]; then
      profile_context_statuses[$slot]="available"
      profile_load_statuses[$slot]="not_loaded"
    else
      profile_context_statuses[$slot]="available"
      profile_load_statuses[$slot]="not_applicable"
    fi
  done
}

profile_contexts_json() {
  jq -n \
    --arg dl_directory "${profile_context_dirs[0]}" \
    --arg dl_identity "${profile_context_identities[0]}" \
    --arg dl_status "${profile_context_statuses[0]}" \
    --arg dl_load_status "${profile_load_statuses[0]}" \
    --arg dl_provenance_file "${profile_context_provenance_files[0]}" \
    --argjson dl_load_time_ms "${profile_load_times_ms[0]}" \
    --arg el_directory "${profile_context_dirs[1]}" \
    --arg el_identity "${profile_context_identities[1]}" \
    --arg el_status "${profile_context_statuses[1]}" \
    --arg el_load_status "${profile_load_statuses[1]}" \
    --arg el_provenance_file "${profile_context_provenance_files[1]}" \
    --argjson el_load_time_ms "${profile_load_times_ms[1]}" \
    --arg ql_directory "${profile_context_dirs[2]}" \
    --arg ql_identity "${profile_context_identities[2]}" \
    --arg ql_status "${profile_context_statuses[2]}" \
    --arg ql_load_status "${profile_load_statuses[2]}" \
    --arg ql_provenance_file "${profile_context_provenance_files[2]}" \
    --argjson ql_load_time_ms "${profile_load_times_ms[2]}" \
    --arg rl_directory "${profile_context_dirs[3]}" \
    --arg rl_identity "${profile_context_identities[3]}" \
    --arg rl_status "${profile_context_statuses[3]}" \
    --arg rl_load_status "${profile_load_statuses[3]}" \
    --arg rl_provenance_file "${profile_context_provenance_files[3]}" \
    --argjson rl_load_time_ms "${profile_load_times_ms[3]}" \
    'def context($profile; $directory; $identity; $status; $load_status; $load_time_ms; $provenance_file):
       {profile: $profile,
        directory: (if $directory == "" then null else $directory end),
        identity: (if $identity == "" then null else $identity end),
        status: $status,
        load_status: $load_status,
        load_time_ms: $load_time_ms,
        provenance_file: (if $provenance_file == "" then null else $provenance_file end)};
     {DL: context("DL"; $dl_directory; $dl_identity; $dl_status; $dl_load_status; $dl_load_time_ms; $dl_provenance_file),
      EL: context("EL"; $el_directory; $el_identity; $el_status; $el_load_status; $el_load_time_ms; $el_provenance_file),
      QL: context("QL"; $ql_directory; $ql_identity; $ql_status; $ql_load_status; $ql_load_time_ms; $ql_provenance_file),
      RL: context("RL"; $rl_directory; $rl_identity; $rl_status; $rl_load_status; $rl_load_time_ms; $rl_provenance_file)}'
}

update_run_metadata() {
  local profile_contexts
  profile_contexts="$(profile_contexts_json)"
  jq \
    --arg data_mode "$engine_data_mode" \
    --arg engine_detail "$engine_detail" \
    --arg engine_image "$engine_image" \
    --arg engine_image_digest "$engine_image_digest" \
    --argjson profile_contexts "$profile_contexts" \
    '. + {data_mode: $data_mode, engine_status: $engine_detail, engine_image: $engine_image, engine_image_digest: $engine_image_digest, profile_contexts: $profile_contexts}' \
    "$RUN_DIR/run_metadata.json" > "$RUN_DIR/.run_metadata.tmp"
  mv "$RUN_DIR/.run_metadata.tmp" "$RUN_DIR/run_metadata.json"
}

load_profile_context() {
  local profile="$1"
  local slot context_dir load_start_ms load_cmd_status
  slot="$(profile_slot "$profile")"
  if [[ "${profile_context_statuses[$slot]}" != "available" || "${profile_load_statuses[$slot]}" != "not_loaded" ]]; then
    return 0
  fi

  context_dir="${profile_context_dirs[$slot]}"
  load_start_ms="$(now_ms)"
  set +e
  timeout "$TIMEOUT_SECONDS" "$ENGINE_BIN" load "$context_dir" > "$RAW_DIR/load_${profile}.output" 2> "$RAW_DIR/load_${profile}.diagnostics"
  load_cmd_status="$?"
  set -e
  profile_load_times_ms[$slot]="$(( $(now_ms) - load_start_ms ))"
  if [[ "$load_cmd_status" -eq 0 ]]; then
    profile_load_statuses[$slot]="success"
  elif [[ "$load_cmd_status" -eq 124 ]]; then
    profile_load_statuses[$slot]="timeout"
    profile_context_statuses[$slot]="timeout"
  elif [[ "$load_cmd_status" -eq "$UNSUPPORTED_EXIT_CODE" ]]; then
    profile_load_statuses[$slot]="unsupported"
    profile_context_statuses[$slot]="unsupported"
  elif [[ "$load_cmd_status" -eq 125 ]]; then
    profile_load_statuses[$slot]="not_available"
    profile_context_statuses[$slot]="not_available"
  else
    profile_load_statuses[$slot]="failed"
    profile_context_statuses[$slot]="failed"
  fi
}

engine_image="$(jq -r '.engine_image // ""' "$CAPABILITIES_FILE" 2>/dev/null || true)"
engine_image_digest="$(jq -r '.engine_image_digest // ""' "$CAPABILITIES_FILE" 2>/dev/null || true)"
initialize_profile_contexts
update_run_metadata

while IFS= read -r case_line; do
  case_record="${case_line//$'\t'/$'\x1f'}"
  IFS=$'\x1f' read -r query_path source_query query_id profile workload source_corpus source_revision case_id profile_metadata_status manifest_row_status oracle_scope <<< "$case_record"
  [[ -n "${case_id:-}" ]] || continue
  current_manifest_row_status="$manifest_row_status"
  current_profile_metadata_status="$profile_metadata_status"
  select_profile_context
  select_component_manifest_profile
  select_materialization_profile
  materialized_query_provenance
  load_time_ms="$current_profile_context_load_time_ms"
  load_status="$current_profile_context_load_status"
  reset_canonicalization
  reset_oracle_match
  current_source_query_sha256=""
  if [[ -f "$query_path" ]]; then
    current_source_query_sha256="$(sha256_file "$query_path")"
  fi
  safe_id="$(printf '%s' "$case_id" | tr -c 'A-Za-z0-9._-' '_')"
  result_file="$RUN_DIR/${safe_id}.json"
  output_file="$RAW_DIR/${safe_id}.output"
  current_diagnostics_file="$RAW_DIR/${safe_id}.diagnostics"
  : > "$current_diagnostics_file"
  oracle_file=""

  if [[ "$manifest_row_status" != "valid" ]]; then
    printf '%s\n' "invalid or incomplete OWL2Bench manifest row" > "$output_file"
    write_benchmark_result "$result_file" "$RUN_ID" "owl2bench-query" "$case_id" "tableauxx-query" "$query_path" "query" \
      "failed" "unknown" "not_evaluated" "not_applicable" "owl2bench-query" "$case_id" "" \
      -1 -1 -1 -1 "$output_file"
    augment_query_result "$result_file" "$profile" "$source_query" "$query_id" "$workload" "$source_corpus" "$source_revision" "$load_time_ms" -1 "$engine_detail" "$load_status"
    continue
  fi

  if [[ "$profile_metadata_status" != "valid" ]]; then
    printf '%s\n' "invalid or missing OWL2Bench profile applicability metadata" > "$output_file"
    write_benchmark_result "$result_file" "$RUN_ID" "owl2bench-query" "$case_id" "tableauxx-query" "$query_path" "query" \
      "failed" "unknown" "not_evaluated" "not_applicable" "owl2bench-query" "$case_id" "" \
      -1 -1 -1 -1 "$output_file"
    augment_query_result "$result_file" "$profile" "$source_query" "$query_id" "$workload" "$source_corpus" "$source_revision" "$load_time_ms" -1 "$engine_detail" "$load_status"
    continue
  fi

  # The stack status is an aggregate gate, not a reason to suppress healthy
  # profiles.  A partial stack must still execute every profile whose own
  # materialization is complete; only a shared-manifest error or this
  # profile's status blocks the case.
  if [[ "$SPLIT_STACK_REQUESTED" == "1" ]]; then
    split_stack_profile_slot="$(profile_slot "$profile" 2>/dev/null || printf '%s' '')"
    split_stack_profile_status="not_available"
    split_stack_profile_reason=""
    if [[ -n "$split_stack_profile_slot" ]]; then
      split_stack_profile_status="${split_stack_materialization_profile_statuses[$split_stack_profile_slot]}"
      split_stack_profile_reason="${split_stack_materialization_profile_reasons[$split_stack_profile_slot]}"
    fi
    if [[ "$split_stack_materialization_global_error" == "1" || "$split_stack_profile_status" != "complete" ]]; then
      split_stack_case_status="failed"
      split_stack_case_correctness="not_evaluated"
      case "$split_stack_profile_status" in
        not_available)
          split_stack_case_status="not_available"
          ;;
        unsupported)
          split_stack_case_status="unsupported"
          split_stack_case_correctness="unsupported"
          ;;
        timeout)
          split_stack_case_status="timeout"
          ;;
      esac
      if [[ -z "$split_stack_profile_reason" ]]; then
        split_stack_profile_reason="$split_stack_profile_status"
      fi
      printf '%s\n' "split-stack materialization preflight ($profile): $split_stack_profile_status; $split_stack_profile_reason" > "$output_file"
      write_benchmark_result "$result_file" "$RUN_ID" "owl2bench-query" "$case_id" "tableauxx-query" "$query_path" "query" \
        "$split_stack_case_status" "unknown" "$split_stack_case_correctness" "not_applicable" "owl2bench-query" "$case_id" "" \
        -1 -1 -1 -1 "$output_file"
      augment_query_result "$result_file" "$profile" "$source_query" "$query_id" "$workload" "$source_corpus" "$source_revision" "$load_time_ms" -1 "$engine_detail" "$load_status"
      continue
    fi
  fi

  component_case_status="available"
  component_case_reason=""
  if [[ "$component_manifest_requested" -eq 1 ]]; then
    if [[ "$component_manifest_status" == "failed" ]]; then
      component_case_status="failed"
      component_case_reason="component_manifest_invalid"
      current_component_manifest_profile_json="$(jq \
        --arg reason "$component_case_reason" \
        '.status = "failed" | .reasons = ((.reasons // []) + [$reason])' \
        <<< "$current_component_manifest_profile_json")"
    else
      component_case_status="$(jq -r '.status // "not_available"' <<< "$current_component_manifest_profile_json")"
      component_case_reason="$(jq -r '((.reasons // ["component_manifest_profile_unavailable"]) | join(";"))' \
        <<< "$current_component_manifest_profile_json")"
    fi
    if [[ "$component_case_status" == "failed" ]]; then
      printf '%s\n' "split-stack component manifest: $component_case_reason" > "$output_file"
      write_benchmark_result "$result_file" "$RUN_ID" "owl2bench-query" "$case_id" "tableauxx-query" "$query_path" "query" \
        "failed" "unknown" "not_evaluated" "not_applicable" "owl2bench-query" "$case_id" "" \
        -1 -1 -1 -1 "$output_file"
      augment_query_result "$result_file" "$profile" "$source_query" "$query_id" "$workload" "$source_corpus" "$source_revision" "$load_time_ms" -1 "$engine_detail" "$load_status"
      continue
    fi
    if [[ "$component_case_status" == "not_available" ]]; then
      printf '%s\n' "split-stack component manifest: $component_case_reason" > "$output_file"
      write_benchmark_result "$result_file" "$RUN_ID" "owl2bench-query" "$case_id" "tableauxx-query" "$query_path" "query" \
        "not_available" "unknown" "not_evaluated" "not_applicable" "owl2bench-query" "$case_id" "" \
        -1 -1 -1 -1 "$output_file"
      augment_query_result "$result_file" "$profile" "$source_query" "$query_id" "$workload" "$source_corpus" "$source_revision" "$load_time_ms" -1 "$engine_detail" "$load_status"
      continue
    fi
    if [[ "$component_case_status" == "unsupported" ]]; then
      printf '%s\n' "split-stack component capability: $component_case_reason" > "$output_file"
      write_benchmark_result "$result_file" "$RUN_ID" "owl2bench-query" "$case_id" "tableauxx-query" "$query_path" "query" \
        "unsupported" "unknown" "unsupported" "not_applicable" "owl2bench-query" "$case_id" "" \
        -1 -1 -1 -1 "$output_file"
      augment_query_result "$result_file" "$profile" "$source_query" "$query_id" "$workload" "$source_corpus" "$source_revision" "$load_time_ms" -1 "$engine_detail" "$load_status"
      continue
    fi
  fi

  materialization_case_status="available"
  materialization_case_reason=""
  materialization_graph_status="not_configured"
  if [[ "$materialization_manifest_requested" -eq 1 ]] && is_known_profile "$profile"; then
    materialization_graph_slot="$(profile_slot "$profile")"
    materialization_graph_status="${profile_materialization_graph_statuses[$materialization_graph_slot]}"
    materialization_case_reason="${profile_materialization_graph_reasons[$materialization_graph_slot]}"
  fi
  if [[ "$materialization_manifest_requested" -eq 1 ]]; then
    if [[ "$materialization_manifest_fatal" == "true" ]]; then
      materialization_case_status="failed"
      materialization_case_reason="materialization_manifest_invalid"
      current_materialization_profile_json="$(jq \
        --arg reason "$materialization_case_reason" \
        '.status = "failed" | .reasons = ((.reasons // []) + [$reason])' \
        <<< "$current_materialization_profile_json")"
    elif [[ "$materialization_graph_status" != "not_configured" ]]; then
      materialization_declared_status="$materialization_graph_status"
      if [[ "$materialization_declared_status" == "complete" ]]; then
        materialization_case_status="available"
      else
        materialization_case_status="$materialization_declared_status"
      fi
    else
      materialization_declared_status="$(jq -r '.status // "not_available"' <<< "$current_materialization_profile_json")"
      materialization_case_reason="$(jq -r '((.reasons // ["materialization_profile_unavailable"]) | join(";"))' \
        <<< "$current_materialization_profile_json")"
      case "$materialization_declared_status" in
        complete)
          materialization_case_status="not_available"
          materialization_case_reason="validated graph handoff is not configured"
          ;;
        partial|failed)
          materialization_case_status="failed"
          ;;
        timeout)
          materialization_case_status="timeout"
          ;;
        not_available)
          materialization_case_status="not_available"
          ;;
        unsupported)
          materialization_case_status="unsupported"
          ;;
        *)
          materialization_case_status="failed"
          materialization_case_reason="materialization_status_invalid"
          ;;
      esac
    fi
    if [[ "$materialization_case_status" == "failed" ]]; then
      printf '%s\n' "profile materialization: $materialization_case_reason" > "$output_file"
      write_benchmark_result "$result_file" "$RUN_ID" "owl2bench-query" "$case_id" "tableauxx-query" "$query_path" "query" \
        "failed" "unknown" "not_evaluated" "not_applicable" "owl2bench-query" "$case_id" "" \
        -1 -1 -1 -1 "$output_file"
      augment_query_result "$result_file" "$profile" "$source_query" "$query_id" "$workload" "$source_corpus" "$source_revision" "$load_time_ms" -1 "$engine_detail" "$load_status"
      continue
    fi
    if [[ "$materialization_case_status" == "timeout" ]]; then
      printf '%s\n' "profile materialization: $materialization_case_reason" > "$output_file"
      write_benchmark_result "$result_file" "$RUN_ID" "owl2bench-query" "$case_id" "tableauxx-query" "$query_path" "query" \
        "timeout" "unknown" "not_evaluated" "not_applicable" "owl2bench-query" "$case_id" "" \
        -1 -1 -1 -1 "$output_file"
      augment_query_result "$result_file" "$profile" "$source_query" "$query_id" "$workload" "$source_corpus" "$source_revision" "$load_time_ms" -1 "$engine_detail" "$load_status"
      continue
    fi
    if [[ "$materialization_case_status" == "not_available" ]]; then
      printf '%s\n' "profile materialization: $materialization_case_reason" > "$output_file"
      write_benchmark_result "$result_file" "$RUN_ID" "owl2bench-query" "$case_id" "tableauxx-query" "$query_path" "query" \
        "not_available" "unknown" "not_evaluated" "not_applicable" "owl2bench-query" "$case_id" "" \
        -1 -1 -1 -1 "$output_file"
      augment_query_result "$result_file" "$profile" "$source_query" "$query_id" "$workload" "$source_corpus" "$source_revision" "$load_time_ms" -1 "$engine_detail" "$load_status"
      continue
    fi
    if [[ "$materialization_case_status" == "unsupported" ]]; then
      printf '%s\n' "profile materialization capability: $materialization_case_reason" > "$output_file"
      write_benchmark_result "$result_file" "$RUN_ID" "owl2bench-query" "$case_id" "tableauxx-query" "$query_path" "query" \
        "unsupported" "unknown" "unsupported" "not_applicable" "owl2bench-query" "$case_id" "" \
        -1 -1 -1 -1 "$output_file"
      augment_query_result "$result_file" "$profile" "$source_query" "$query_id" "$workload" "$source_corpus" "$source_revision" "$load_time_ms" -1 "$engine_detail" "$load_status"
      continue
    fi
  fi

  if [[ "$engine_available" -eq 0 ]]; then
    printf '%s\n' "$engine_detail" > "$output_file"
    write_benchmark_result "$result_file" "$RUN_ID" "owl2bench-query" "$case_id" "tableauxx-query" "$query_path" "query" \
      "not_available" "unknown" "not_evaluated" "not_applicable" "owl2bench-query" "$case_id" "" \
      -1 -1 -1 -1 "$output_file"
    augment_query_result "$result_file" "$profile" "$source_query" "$query_id" "$workload" "$source_corpus" "$source_revision" "$load_time_ms" -1 "$engine_detail" "$load_status"
    continue
  fi

  if [[ "$engine_data_mode" == "directory" && -z "$DATA_DIR" ]]; then
    printf '%s\n' 'query engine requires a data directory' > "$output_file"
    write_benchmark_result "$result_file" "$RUN_ID" "owl2bench-query" "$case_id" "tableauxx-query" "$query_path" "query" \
      "not_available" "unknown" "not_evaluated" "not_applicable" "owl2bench-query" "$case_id" "" \
      -1 -1 -1 -1 "$output_file"
    augment_query_result "$result_file" "$profile" "$source_query" "$query_id" "$workload" "$source_corpus" "$source_revision" "$load_time_ms" -1 "$engine_detail" "$load_status"
    continue
  fi

  if [[ "$engine_data_mode" == "directory" && "$current_profile_context_status" == "not_available" ]]; then
    printf '%s\n' "profile data context not available" > "$output_file"
    write_benchmark_result "$result_file" "$RUN_ID" "owl2bench-query" "$case_id" "tableauxx-query" "$query_path" "query" \
      "not_available" "unknown" "not_evaluated" "not_applicable" "owl2bench-query" "$case_id" "" \
      -1 -1 -1 -1 "$output_file"
    augment_query_result "$result_file" "$profile" "$source_query" "$query_id" "$workload" "$source_corpus" "$source_revision" "$load_time_ms" -1 "$engine_detail" "$load_status"
    continue
  fi

  if ! profile_supported "$profile"; then
    printf '%s\n' "profile unsupported by query engine" > "$output_file"
    write_benchmark_result "$result_file" "$RUN_ID" "owl2bench-query" "$case_id" "tableauxx-query" "$query_path" "query" \
      "unsupported" "unknown" "unsupported" "not_applicable" "owl2bench-query" "$case_id" "" \
      -1 -1 -1 -1 "$output_file"
    augment_query_result "$result_file" "$profile" "$source_query" "$query_id" "$workload" "$source_corpus" "$source_revision" "$load_time_ms" -1 "$engine_detail" "$load_status"
    continue
  fi

  if [[ "$engine_data_mode" == "directory" ]]; then
    load_profile_context "$profile"
    select_profile_context
    select_component_manifest_profile
    load_time_ms="$current_profile_context_load_time_ms"
    load_status="$current_profile_context_load_status"
  fi

  if [[ "$engine_data_mode" == "directory" && "$current_profile_context_status" == "not_available" ]]; then
    printf '%s\n' "profile data context or adapter runtime not available" > "$output_file"
    write_benchmark_result "$result_file" "$RUN_ID" "owl2bench-query" "$case_id" "tableauxx-query" "$query_path" "query" \
      "not_available" "unknown" "not_evaluated" "not_applicable" "owl2bench-query" "$case_id" "" \
      -1 -1 -1 -1 "$output_file"
    augment_query_result "$result_file" "$profile" "$source_query" "$query_id" "$workload" "$source_corpus" "$source_revision" "$load_time_ms" -1 "$engine_detail" "$load_status"
    continue
  fi

  if [[ "$load_status" == "failed" || "$load_status" == "timeout" || "$load_status" == "unsupported" ]]; then
    printf '%s\n' "query engine load: $load_status" > "$output_file"
    write_benchmark_result "$result_file" "$RUN_ID" "owl2bench-query" "$case_id" "tableauxx-query" "$query_path" "query" \
      "$load_status" "unknown" "not_evaluated" "not_applicable" "owl2bench-query" "$case_id" "" \
      "$load_time_ms" "$load_time_ms" -1 "$load_time_ms" "$output_file"
    augment_query_result "$result_file" "$profile" "$source_query" "$query_id" "$workload" "$source_corpus" "$source_revision" "$load_time_ms" -1 "$engine_detail" "$load_status"
    continue
  fi

  if [[ "$EXECUTION_ONLY" == "1" ]]; then
    reset_oracle_match
  else
    lookup_oracle_for_case
  fi
  start_ms="$(now_ms)"
  set +e
  if [[ "$engine_data_mode" == "directory" ]]; then
    if [[ -n "$current_materialization_handoff_file" ]]; then
      OWL2BENCH_QUERY_MATERIALIZATION_HANDOFF="$current_materialization_handoff_file" \
      OWL2BENCH_QUERY_MATERIALIZATION_GRAPH_SHA256="$current_materialization_handoff_sha256" \
        timeout "$TIMEOUT_SECONDS" "$ENGINE_BIN" query "$query_path" "$current_profile_context_dir" > "$output_file" 2> "$current_diagnostics_file"
    else
      timeout "$TIMEOUT_SECONDS" "$ENGINE_BIN" query "$query_path" "$current_profile_context_dir" > "$output_file" 2> "$current_diagnostics_file"
    fi
  else
    if [[ -n "$current_materialization_handoff_file" ]]; then
      OWL2BENCH_QUERY_MATERIALIZATION_HANDOFF="$current_materialization_handoff_file" \
      OWL2BENCH_QUERY_MATERIALIZATION_GRAPH_SHA256="$current_materialization_handoff_sha256" \
        timeout "$TIMEOUT_SECONDS" "$ENGINE_BIN" query "$query_path" > "$output_file" 2> "$current_diagnostics_file"
    else
      timeout "$TIMEOUT_SECONDS" "$ENGINE_BIN" query "$query_path" > "$output_file" 2> "$current_diagnostics_file"
    fi
  fi
  cmd_status="$?"
  set -e
  wall_ms="$(( $(now_ms) - start_ms ))"

  if [[ "$cmd_status" -eq 124 ]]; then
    execution_detail="timeout"
  elif [[ "$cmd_status" -eq "$UNSUPPORTED_EXIT_CODE" ]]; then
    execution_detail="unsupported"
  elif [[ "$cmd_status" -eq 125 ]]; then
    execution_detail="not_available"
  elif [[ "$cmd_status" -ne 0 ]]; then
    execution_detail="failed"
  else
    execution_detail="success"
  fi

  semantic_result="unknown"
  correctness_status="not_evaluated"
  oracle_status="$current_oracle_status"
  oracle_source="${oracle_file:-${ORACLE_MANIFEST:-${ORACLE_DIR:-unconfigured}}}"
  expected_digest=""
  if [[ "$execution_detail" == "success" ]]; then
    current_canonical_result_file="$RAW_DIR/${safe_id}.canonical.json"
    canonicalizer_error_file="$RAW_DIR/${safe_id}.canonicalizer.stderr"
    set +e
    canonicalize_result_file "$query_path" "$output_file" "$current_canonical_result_file" \
      > /dev/null 2> "$canonicalizer_error_file"
    canonicalizer_cmd_status="$?"
    set -e
    current_canonicalization_error="$(tr -s '[:space:]' ' ' < "$canonicalizer_error_file" | sed 's/^ //; s/ $//' || true)"
    current_canonicalization_format="$(jq -r '.format // empty' "$current_canonical_result_file" 2>/dev/null || true)"
    current_canonicalization_hash="$(jq -r '.result_hash // empty' "$current_canonical_result_file" 2>/dev/null || true)"
    if [[ "$canonicalizer_cmd_status" -ne 0 || ! -s "$current_canonical_result_file" \
        || ! "$current_canonicalization_hash" =~ ^sha256:[0-9a-f]{64}$ ]]; then
      execution_detail="failed"
      current_canonicalization_status="failed"
      current_canonical_result_file=""
      current_canonicalization_format=""
      current_canonicalization_hash=""
      if [[ -z "$current_canonicalization_error" ]]; then
        current_canonicalization_error="canonicalizer exited with status $canonicalizer_cmd_status"
      fi
      oracle_status="not_applicable"
      current_oracle_reason="execution_result_canonicalization_failed"
    else
      current_canonicalization_status="available"
      semantic_result="$current_canonicalization_hash"
      if [[ "$current_oracle_status" == "available" && -f "$oracle_file" ]]; then
        oracle_canonical_file="$RAW_DIR/${safe_id}.oracle.canonical.json"
        oracle_canonicalizer_error_file="$RAW_DIR/${safe_id}.oracle.canonicalizer.stderr"
        set +e
        canonicalize_result_file "$query_path" "$oracle_file" "$oracle_canonical_file" \
          > /dev/null 2> "$oracle_canonicalizer_error_file"
        oracle_canonicalizer_cmd_status="$?"
        set -e
        expected_digest="$(jq -r '.result_hash // empty' "$oracle_canonical_file" 2>/dev/null || true)"
        if [[ "$oracle_canonicalizer_cmd_status" -eq 0 && -s "$oracle_canonical_file" \
            && "$expected_digest" =~ ^sha256:[0-9a-f]{64}$ \
            && "$expected_digest" == "$current_oracle_declared_canonical_hash" ]]; then
          oracle_status="available"
          oracle_source="$oracle_file"
          correctness_status="not_evaluated"
        else
          expected_digest=""
          oracle_status="unavailable"
          oracle_source="$oracle_file"
          if [[ "$expected_digest" != "$current_oracle_declared_canonical_hash" ]]; then
            current_oracle_reason="oracle_canonical_result_hash_mismatch"
          else
            current_oracle_reason="oracle_result_malformed"
          fi
        fi
      fi
    fi
  else
    oracle_status="not_applicable"
  fi

  write_benchmark_result "$result_file" "$RUN_ID" "owl2bench-query" "$case_id" "tableauxx-query" "$query_path" "query" \
    "$execution_detail" "$semantic_result" "$correctness_status" "$oracle_status" "$oracle_source" "$case_id" "$expected_digest" \
    "$wall_ms" "$wall_ms" -1 "$wall_ms" "$output_file"
  augment_query_result "$result_file" "$profile" "$source_query" "$query_id" "$workload" "$source_corpus" "$source_revision" "$load_time_ms" "$wall_ms" "$engine_detail" "$load_status"
done < <(tail -n +2 "$CASE_MANIFEST")

mkdir -p "$(dirname "$RESULT_CONTRACT_FILE")"
if [[ ! -r "$RESULT_CONTRACT_VALIDATOR" ]]; then
  echo "[owl2bench-query-run][error] missing profile result contract validator: $RESULT_CONTRACT_VALIDATOR" >&2
  exit 1
fi
set +e
python3 "$RESULT_CONTRACT_VALIDATOR" validate-run \
  --run-dir "$RUN_DIR" \
  --output "$RESULT_CONTRACT_FILE" \
  > "$RUN_DIR/provenance/profile_result_contract.stdout" \
  2> "$RUN_DIR/provenance/profile_result_contract.stderr"
result_contract_status="$?"
set -e
if [[ "$result_contract_status" -ne 0 ]] || ! jq -e '.status == "valid"' "$RESULT_CONTRACT_FILE" >/dev/null 2>&1; then
  echo "[owl2bench-query-run][error] profile result contract validation failed: $RESULT_CONTRACT_FILE" >&2
  exit 1
fi
jq \
  --arg result_contract_file "$RESULT_CONTRACT_FILE" \
  '.result_contract.status = "valid" | .result_contract.validation_file = $result_contract_file' \
  "$RUN_DIR/run_metadata.json" > "$RUN_DIR/.run_metadata.tmp"
mv "$RUN_DIR/.run_metadata.tmp" "$RUN_DIR/run_metadata.json"

update_run_metadata
validate_benchmark_result_count "$RUN_DIR" "$expected_case_count"
"$(dirname "${BASH_SOURCE[0]}")/report.sh" "$RUN_ID"
echo "[owl2bench-query-run] run_id=$RUN_ID"
echo "[owl2bench-query-run] run_dir=$RUN_DIR"
