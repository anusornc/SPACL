def nullable($value):
  if $value == "" then null else $value end;

. + {
  query_profile: $profile,
  source_query: $source_query,
  source_query_sha256: nullable($source_query_sha256),
  query_id: $query_id,
  workload: $workload,
  source_corpus: $source_corpus,
  source_revision: $source_revision,
  manifest_row_status: nullable($manifest_row_status),
  profile_metadata_status: nullable($profile_metadata_status),
  query_contract_scope: (
    if $manifest_row_status == "invalid" or $profile_metadata_status == "invalid"
    then "invalid_manifest_row"
    else "profile_qualified"
    end
  ),
  engine_detail: $engine_detail,
  load_status: $load_status,
  load_time_ms: $load_time_ms,
  query_time_ms: $query_time_ms,
  diagnostics_file: nullable($diagnostics_file),
  load_diagnostics_file: nullable($load_diagnostics_file),
  dataset_identity: {
    dataset_id: nullable($dataset_id),
    dataset_sha256: nullable($dataset_sha256),
    source: $dataset_identity_source
  },
  engine_provenance: (
    if $engine_provenance == {} then null else $engine_provenance end
  ),
  component_manifest: (
    if $component_manifest_profile == {}
    then null
    else $component_manifest_profile
    end
  ),
  materialization: (
    if $materialization_profile == {}
    then null
    else $materialization_profile
    end
  ),
  materialization_handoff: (
    if $materialization_handoff_file == ""
    then null
    else {
      status: "ready",
      path: $materialization_handoff_file,
      graph_sha256: $materialization_handoff_sha256,
      read_only: true
    }
    end
  ),
  execution_only: ($execution_only == "1"),
  profile_context: {
    profile: $profile,
    directory: nullable($profile_context_dir),
    identity: nullable($profile_context_identity),
    status: $profile_context_status,
    load_status: $profile_context_load_status,
    load_time_ms: $profile_context_load_time_ms,
    provenance_file: nullable($profile_context_provenance_file)
  },
  oracle: (.oracle + {
    class: nullable($oracle_class),
    oracle_id: nullable($oracle_id),
    case_id: nullable($oracle_matched_case_id),
    query_id: nullable($oracle_query_id),
    profile: nullable($oracle_profile),
    source_query_sha256: nullable($oracle_source_query_sha256),
    source_corpus: nullable($oracle_source_corpus),
    source_revision: nullable($oracle_source_revision),
    dataset_id: nullable($oracle_dataset_id),
    dataset_sha256: nullable($oracle_dataset_sha256),
    profile_context_identity: nullable($oracle_profile_context_identity),
    canonicalizer_version: nullable($oracle_canonicalizer_version),
    provenance: nullable($oracle_provenance),
    provenance_sha256: nullable($oracle_provenance_sha256),
    provenance_artifact: nullable($oracle_provenance_artifact),
    provenance_artifact_sha256: nullable($oracle_provenance_artifact_sha256),
    artifact_sha256: nullable($oracle_artifact_sha256),
    proof_artifact: nullable($oracle_proof_artifact),
    proof_artifact_sha256: nullable($oracle_proof_artifact_sha256),
    audit_artifact: nullable($oracle_audit_artifact),
    audit_artifact_sha256: nullable($oracle_audit_artifact_sha256),
    audit_status: nullable($oracle_audit_status),
    derivation_deterministic: $oracle_derivation_deterministic,
    sound_complete: $oracle_sound_complete,
    oracle_pipeline_version: nullable($oracle_pipeline_version),
    canonical_result_hash: nullable($oracle_declared_canonical_hash),
    metadata_file: nullable($oracle_manifest),
    match_reason: $oracle_match_reason
  }),
  differential_evidence: $differential_evidence,
  canonicalization: {
    status: $canonicalization_status,
    canonicalizer: $canonicalizer_name,
    canonicalizer_version: $canonicalizer_version,
    format: nullable($canonicalization_format),
    canonical_format: "typed-rdf-results-v1",
    result_hash: nullable($canonicalization_hash),
    canonical_result_file: nullable($canonical_result_file),
    error: nullable($canonicalization_error)
  }
}
| if $execution_only == "1" then del(.oracle) else . end
