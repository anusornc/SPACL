#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
RUNNER="$SCRIPT_DIR/run.sh"
CANONICALIZER="$SCRIPT_DIR/canonicalize_result.py"
TMP_DIR="$(mktemp -d)"
cleanup() {
  if [[ "${KEEP_ORACLE_TEST_DIR:-0}" == "1" ]]; then
    echo "[test-owl2bench-oracle] retained test directory: $TMP_DIR" >&2
  else
    rm -rf "$TMP_DIR"
  fi
}
trap cleanup EXIT

QUERY="$TMP_DIR/query.rq"
printf 'SELECT ?x WHERE { ?x ?p ?o }\n' > "$QUERY"
STAGED_DIR="$TMP_DIR/staged"
mkdir -p "$STAGED_DIR"
printf 'staged_query\tsource_query\tquery_id\tprofile\tworkload\tsource_corpus\tsource_revision\n%s\t%s\toracle-query\tEL\ttest-workload\ttest-corpus\ttest-revision\n' \
  "$QUERY" "$TMP_DIR/source#oracle-query" > "$STAGED_DIR/manifest.tsv"

ENGINE="$TMP_DIR/fake-engine"
cat > "$ENGINE" <<'ENGINE'
#!/usr/bin/env bash
set -euo pipefail
[[ "${1:-}" == "query" ]]
printf '?x\n<http://example.test/value>\n'
ENGINE
chmod +x "$ENGINE"
CAPABILITIES="$TMP_DIR/capabilities.json"
printf '{"supports_query":true,"profiles":["EL"],"data_mode":"query-file-only"}\n' > "$CAPABILITIES"
DATASET_METADATA="$TMP_DIR/datasets.json"
printf '{"EL":{"dataset_id":"anchor-el","dataset_sha256":"dataset-sha-el"}}\n' > "$DATASET_METADATA"

canonicalize_answer() {
  local answer_file="$1"
  local canonical_file="$2"
  python3 "$CANONICALIZER" --query "$QUERY" --input "$answer_file" --output "$canonical_file"
}

ORACLE_DIR="$TMP_DIR/oracles"
mkdir -p "$ORACLE_DIR/EL" "$ORACLE_DIR/provenance" "$ORACLE_DIR/proof" "$ORACLE_DIR/audit"
OFFICIAL_ANSWER="$ORACLE_DIR/EL/oracle-query.out"
printf '?x\n<http://example.test/value>\n' > "$OFFICIAL_ANSWER"
OFFICIAL_ARTIFACT_SHA256="$(sha256sum "$OFFICIAL_ANSWER" | awk '{print $1}')"
canonicalize_answer "$OFFICIAL_ANSWER" "$TMP_DIR/official-canonical.json"
OFFICIAL_CANONICAL_HASH="$(jq -r '.result_hash' "$TMP_DIR/official-canonical.json")"
QUERY_SHA256="$(sha256sum "$QUERY" | awk '{print $1}')"
OFFICIAL_PROVENANCE="$ORACLE_DIR/provenance/official.json"
printf '{"source":"official-test-key"}\n' > "$OFFICIAL_PROVENANCE"
OFFICIAL_PROVENANCE_SHA256="$(sha256sum "$OFFICIAL_PROVENANCE" | awk '{print $1}')"
OFFICIAL_PROVENANCE_ID_SHA256="$(printf 'official-test-key' | sha256sum | awk '{print $1}')"

write_manifest() {
  local manifest_file="$1"
  local rows_json="$2"
  jq -n \
    --arg source_revision "test-revision" \
    --argjson oracles "$rows_json" \
    '{schema_version:1, source_revision:$source_revision, oracles:$oracles}' > "$manifest_file"
}

official_row="$(jq -n \
  --arg case_id 'oracle-query__EL' \
  --arg query_id 'oracle-query' \
  --arg profile 'EL' \
  --arg answer_file 'EL/oracle-query.out' \
  --arg oracle_id 'official:oracle-query:EL' \
  --arg provenance_sha256 "$OFFICIAL_PROVENANCE_ID_SHA256" \
  --arg provenance_artifact 'provenance/official.json' \
  --arg provenance_artifact_sha256 "$OFFICIAL_PROVENANCE_SHA256" \
  --arg query_sha256 "$QUERY_SHA256" \
  --arg dataset_id 'anchor-el' \
  --arg dataset_sha256 'dataset-sha-el' \
  --arg artifact_sha256 "$OFFICIAL_ARTIFACT_SHA256" \
  --arg canonical_hash "$OFFICIAL_CANONICAL_HASH" \
  '{case_id:$case_id,query_id:$query_id,profile:$profile,answer_file:$answer_file,oracle_class:"official",acceptance_status:"accepted",oracle_id:$oracle_id,oracle_provenance:"official-test-key",oracle_provenance_sha256:$provenance_sha256,provenance_artifact:$provenance_artifact,provenance_artifact_sha256:$provenance_artifact_sha256,source_query_sha256:$query_sha256,source_corpus:"test-corpus",source_revision:"test-revision",dataset_id:$dataset_id,dataset_sha256:$dataset_sha256,canonicalizer_version:"owl2bench-query-canonicalizer/2",answer_sha256:$artifact_sha256,canonical_result_hash:$canonical_hash}')"
write_manifest "$ORACLE_DIR/manifest.json" "[$official_row]"

run_case() {
  local run_id="$1"
  local oracle_manifest="$2"
  local engine_bin="${3:-$ENGINE}"
  OWL2BENCH_QUERY_STAGED_DIR="$STAGED_DIR" \
  OWL2BENCH_QUERY_RESULTS_ROOT="$TMP_DIR/results" \
  OWL2BENCH_QUERY_ENGINE_BIN="$engine_bin" \
  OWL2BENCH_QUERY_CAPABILITIES_FILE="$CAPABILITIES" \
  OWL2BENCH_QUERY_ORACLE_DIR="$ORACLE_DIR" \
  OWL2BENCH_QUERY_ORACLE_MANIFEST="$oracle_manifest" \
  OWL2BENCH_QUERY_DATASET_METADATA="$DATASET_METADATA" \
  RUN_ID="$run_id" \
  "$RUNNER" >/dev/null
}

run_case "oracle-official" "$ORACLE_DIR/manifest.json"
jq -e '.query_count == 1 and .result_status_counts.passed == 1 and .correctness_gate == "blocked" and .query_gate == "blocked"' \
  "$TMP_DIR/results/oracle-official/summary.json" >/dev/null
jq -e '.result_status == "passed" and .oracle.status == "available" and .oracle.class == "official" and .oracle.provenance == "official-test-key" and .oracle.match_reason == "accepted" and .oracle.artifact_sha256 == $artifact and .oracle.canonical_result_hash == $canonical and .oracle.provenance_artifact_sha256 == $provenance_artifact and .semantic_result == $canonical and .source_query_sha256 == $query_sha256 and .dataset_identity.dataset_id == "anchor-el" and .dataset_identity.dataset_sha256 == "dataset-sha-el" and .dataset_identity.source == "dataset_metadata_file"' \
  --arg artifact "$OFFICIAL_ARTIFACT_SHA256" --arg canonical "$OFFICIAL_CANONICAL_HASH" \
  --arg provenance_artifact "$OFFICIAL_PROVENANCE_SHA256" --arg query_sha256 "$QUERY_SHA256" \
  "$TMP_DIR/results/oracle-official/oracle-query__EL.json" >/dev/null

STALE_MANIFEST="$TMP_DIR/stale-manifest.json"
stale_row="$(jq '.canonicalizer_version = "old-canonicalizer/0"' <<< "$official_row")"
write_manifest "$STALE_MANIFEST" "[$stale_row]"
run_case "oracle-stale-metadata" "$STALE_MANIFEST"
jq -e '.result_status_counts.oracle_unavailable == 1 and .correctness_gate == "blocked"' \
  "$TMP_DIR/results/oracle-stale-metadata/summary.json" >/dev/null

# A v1-keyed oracle row under the v2 runner fails closed on exactly the
# canonicalizer identity, never comparing across versions (ADR 0023).
V1_KEYED_MANIFEST="$TMP_DIR/v1-keyed-manifest.json"
v1_keyed_row="$(jq '.canonicalizer_version = "owl2bench-query-canonicalizer/1"' <<< "$official_row")"
write_manifest "$V1_KEYED_MANIFEST" "[$v1_keyed_row]"
run_case "oracle-v1-keyed" "$V1_KEYED_MANIFEST"
jq -e '.result_status == "oracle_unavailable" and .oracle.status == "unavailable" and .oracle.match_reason == "identity_mismatch:canonicalizer_version"' \
  "$TMP_DIR/results/oracle-v1-keyed/oracle-query__EL.json" >/dev/null

# RDF 1.1 answer preservation (ADR 0023): a simple literal and the same
# literal typed xsd:string are one term, so an oracle stored one way and an
# engine serializing the other way are the same answer in both directions.
PLAIN_LITERAL_ANSWER="$ORACLE_DIR/EL/literal-plain.out"
TYPED_LITERAL_ANSWER="$ORACLE_DIR/EL/literal-typed.out"
printf '?x\n"x"\n' > "$PLAIN_LITERAL_ANSWER"
printf '?x\n"x"^^<http://www.w3.org/2001/XMLSchema#string>\n' > "$TYPED_LITERAL_ANSWER"
canonicalize_answer "$PLAIN_LITERAL_ANSWER" "$TMP_DIR/literal-plain-canonical.json"
canonicalize_answer "$TYPED_LITERAL_ANSWER" "$TMP_DIR/literal-typed-canonical.json"
LITERAL_CANONICAL_HASH="$(jq -r '.result_hash' "$TMP_DIR/literal-plain-canonical.json")"
test "$LITERAL_CANONICAL_HASH" = "$(jq -r '.result_hash' "$TMP_DIR/literal-typed-canonical.json")"
literal_row() {
  local answer_file="$1"
  jq -n \
    --arg answer_file "$answer_file" \
    --arg artifact_sha256 "$(sha256sum "$ORACLE_DIR/$answer_file" | awk '{print $1}')" \
    --arg canonical_hash "$LITERAL_CANONICAL_HASH" \
    --argjson row "$official_row" \
    '$row | .answer_file=$answer_file | .answer_sha256=$artifact_sha256 | .canonical_result_hash=$canonical_hash'
}
TYPED_ENGINE="$TMP_DIR/typed-engine"
cat > "$TYPED_ENGINE" <<'ENGINE'
#!/usr/bin/env bash
set -euo pipefail
[[ "${1:-}" == "query" ]]
printf '?x\n"x"^^<http://www.w3.org/2001/XMLSchema#string>\n'
ENGINE
chmod +x "$TYPED_ENGINE"
PLAIN_ORACLE_MANIFEST="$TMP_DIR/plain-oracle-manifest.json"
write_manifest "$PLAIN_ORACLE_MANIFEST" "[$(literal_row "EL/literal-plain.out")]"
run_case "oracle-literal-typed-engine" "$PLAIN_ORACLE_MANIFEST" "$TYPED_ENGINE"
jq -e '.result_status == "passed" and .oracle.status == "available"' \
  "$TMP_DIR/results/oracle-literal-typed-engine/oracle-query__EL.json" >/dev/null

PLAIN_ENGINE="$TMP_DIR/plain-engine"
cat > "$PLAIN_ENGINE" <<'ENGINE'
#!/usr/bin/env bash
set -euo pipefail
[[ "${1:-}" == "query" ]]
printf '?x\n"x"\n'
ENGINE
chmod +x "$PLAIN_ENGINE"
TYPED_ORACLE_MANIFEST="$TMP_DIR/typed-oracle-manifest.json"
write_manifest "$TYPED_ORACLE_MANIFEST" "[$(literal_row "EL/literal-typed.out")]"
run_case "oracle-literal-plain-engine" "$TYPED_ORACLE_MANIFEST" "$PLAIN_ENGINE"
jq -e '.result_status == "passed" and .oracle.status == "available"' \
  "$TMP_DIR/results/oracle-literal-plain-engine/oracle-query__EL.json" >/dev/null

STALE_ARTIFACT_MANIFEST="$TMP_DIR/stale-artifact-manifest.json"
stale_artifact_row="$(jq '.answer_sha256 = "stale"' <<< "$official_row")"
write_manifest "$STALE_ARTIFACT_MANIFEST" "[$stale_artifact_row]"
run_case "oracle-stale-artifact" "$STALE_ARTIFACT_MANIFEST"
jq -e '.result_status_counts.oracle_unavailable == 1' "$TMP_DIR/results/oracle-stale-artifact/summary.json" >/dev/null

WRONG_ANSWER="$ORACLE_DIR/EL/wrong.out"
printf '?x\n<http://example.test/wrong>\n' > "$WRONG_ANSWER"
WRONG_ARTIFACT_SHA256="$(sha256sum "$WRONG_ANSWER" | awk '{print $1}')"
canonicalize_answer "$WRONG_ANSWER" "$TMP_DIR/wrong-canonical.json"
WRONG_CANONICAL_HASH="$(jq -r '.result_hash' "$TMP_DIR/wrong-canonical.json")"
WRONG_PROVENANCE="$ORACLE_DIR/provenance/official-wrong.json"
printf '{"source":"official-wrong-key"}\n' > "$WRONG_PROVENANCE"
WRONG_PROVENANCE_SHA256="$(sha256sum "$WRONG_PROVENANCE" | awk '{print $1}')"
WRONG_PROVENANCE_ID_SHA256="$(printf 'official-wrong-key' | sha256sum | awk '{print $1}')"
wrong_row="$(jq --arg answer_file 'EL/wrong.out' --arg artifact "$WRONG_ARTIFACT_SHA256" --arg canonical "$WRONG_CANONICAL_HASH" --arg provenance_sha256 "$WRONG_PROVENANCE_ID_SHA256" --arg provenance_artifact 'provenance/official-wrong.json' --arg provenance_artifact_sha256 "$WRONG_PROVENANCE_SHA256" '.answer_file=$answer_file | .answer_sha256=$artifact | .canonical_result_hash=$canonical | .oracle_id="official:oracle-query:EL:wrong" | .oracle_provenance="official-wrong-key" | .oracle_provenance_sha256=$provenance_sha256 | .provenance_artifact=$provenance_artifact | .provenance_artifact_sha256=$provenance_artifact_sha256' <<< "$official_row")"
WRONG_MANIFEST="$TMP_DIR/wrong-manifest.json"
write_manifest "$WRONG_MANIFEST" "[$wrong_row]"
run_case "oracle-wrong" "$WRONG_MANIFEST"
jq -e '.result_status_counts.wrong == 1 and .correctness_gate == "blocked"' "$TMP_DIR/results/oracle-wrong/summary.json" >/dev/null

DERIVED_ANSWER="$ORACLE_DIR/EL/derived.out"
cp "$OFFICIAL_ANSWER" "$DERIVED_ANSWER"
DERIVED_ARTIFACT_SHA256="$(sha256sum "$DERIVED_ANSWER" | awk '{print $1}')"
DERIVED_PROVENANCE="$ORACLE_DIR/provenance/derived.json"
printf '{"source":"derived-test-proof"}\n' > "$DERIVED_PROVENANCE"
DERIVED_PROVENANCE_SHA256="$(sha256sum "$DERIVED_PROVENANCE" | awk '{print $1}')"
DERIVED_PROVENANCE_ID_SHA256="$(printf 'derived-test-proof' | sha256sum | awk '{print $1}')"
PROOF_ARTIFACT="$ORACLE_DIR/proof/test-certificate.json"
printf '{"proof":"deterministic"}\n' > "$PROOF_ARTIFACT"
PROOF_ARTIFACT_SHA256="$(sha256sum "$PROOF_ARTIFACT" | awk '{print $1}')"
AUDIT_ARTIFACT="$ORACLE_DIR/audit/test-audit.json"
printf '{"audit":"independent"}\n' > "$AUDIT_ARTIFACT"
AUDIT_ARTIFACT_SHA256="$(sha256sum "$AUDIT_ARTIFACT" | awk '{print $1}')"
derived_row="$(jq --arg answer_file 'EL/derived.out' --arg artifact "$DERIVED_ARTIFACT_SHA256" --arg oracle_id 'derived:oracle-query:EL' --arg provenance_sha256 "$DERIVED_PROVENANCE_ID_SHA256" --arg provenance_artifact 'provenance/derived.json' --arg provenance_artifact_sha256 "$DERIVED_PROVENANCE_SHA256" --arg proof_sha256 "$PROOF_ARTIFACT_SHA256" --arg audit_sha256 "$AUDIT_ARTIFACT_SHA256" '.answer_file=$answer_file | .answer_sha256=$artifact | .oracle_class="derived" | .oracle_id=$oracle_id | .oracle_provenance="derived-test-proof" | .oracle_provenance_sha256=$provenance_sha256 | .provenance_artifact=$provenance_artifact | .provenance_artifact_sha256=$provenance_artifact_sha256 | .audit_status="independently_audited" | .derivation_deterministic=true | .sound_complete=true | .proof_artifact="proof/test-certificate.json" | .proof_artifact_sha256=$proof_sha256 | .audit_artifact="audit/test-audit.json" | .audit_artifact_sha256=$audit_sha256 | .oracle_pipeline_version="derived-pipeline/1"' <<< "$official_row")"
DERIVED_MANIFEST="$TMP_DIR/derived-manifest.json"
write_manifest "$DERIVED_MANIFEST" "[$derived_row]"
run_case "oracle-derived" "$DERIVED_MANIFEST"
jq -e '.result_status == "passed" and .oracle.class == "derived"' "$TMP_DIR/results/oracle-derived/oracle-query__EL.json" >/dev/null
jq -e '.oracle.match_reason == "accepted" and .oracle.audit_status == "independently_audited" and .oracle.derivation_deterministic == true and .oracle.sound_complete == true and .oracle.oracle_pipeline_version == "derived-pipeline/1" and .oracle.proof_artifact_sha256 == $proof and .oracle.audit_artifact_sha256 == $audit' \
  --arg proof "$PROOF_ARTIFACT_SHA256" --arg audit "$AUDIT_ARTIFACT_SHA256" \
  "$TMP_DIR/results/oracle-derived/oracle-query__EL.json" >/dev/null

MISSING_PROOF_MANIFEST="$TMP_DIR/missing-proof-manifest.json"
missing_proof_row="$(jq '.proof_artifact = "proof/missing-certificate.json"' <<< "$derived_row")"
write_manifest "$MISSING_PROOF_MANIFEST" "[$missing_proof_row]"
run_case "oracle-derived-missing-proof" "$MISSING_PROOF_MANIFEST"
jq -e '.result_status_counts.oracle_unavailable == 1 and .correctness_gate == "blocked"' \
  "$TMP_DIR/results/oracle-derived-missing-proof/summary.json" >/dev/null

DIFFERENTIAL_ANSWER="$ORACLE_DIR/EL/differential.out"
cp "$OFFICIAL_ANSWER" "$DIFFERENTIAL_ANSWER"
DIFFERENTIAL_ARTIFACT_SHA256="$(sha256sum "$DIFFERENTIAL_ANSWER" | awk '{print $1}')"
differential_row="$(jq --arg answer_file 'EL/differential.out' --arg artifact "$DIFFERENTIAL_ARTIFACT_SHA256" '.answer_file=$answer_file | .answer_sha256=$artifact | .oracle_class="differential" | .acceptance_status="evidence" | .oracle_provenance="engine-agreement"' <<< "$official_row")"
DIFFERENTIAL_MANIFEST="$TMP_DIR/differential-manifest.json"
write_manifest "$DIFFERENTIAL_MANIFEST" "[$differential_row]"
run_case "oracle-differential" "$DIFFERENTIAL_MANIFEST"
jq -e '.result_status_counts.oracle_unavailable == 1 and .correctness_gate == "blocked"' "$TMP_DIR/results/oracle-differential/summary.json" >/dev/null
jq -e '.oracle.class == null and (.differential_evidence | length) == 1 and .differential_evidence[0].class == "differential"' "$TMP_DIR/results/oracle-differential/oracle-query__EL.json" >/dev/null

OFFICIAL_WRONG_DERIVED_RIGHT="$TMP_DIR/precedence-manifest.json"
write_manifest "$OFFICIAL_WRONG_DERIVED_RIGHT" "[$wrong_row, $derived_row]"
run_case "oracle-official-precedence" "$OFFICIAL_WRONG_DERIVED_RIGHT"
jq -e '.result_status_counts.wrong == 1 and .correctness_gate == "blocked"' "$TMP_DIR/results/oracle-official-precedence/summary.json" >/dev/null
jq -e '.oracle.class == "official" and .oracle.provenance == "official-wrong-key"' "$TMP_DIR/results/oracle-official-precedence/oracle-query__EL.json" >/dev/null

run_case "oracle-missing-manifest" "$TMP_DIR/does-not-exist.json"
jq -e '.result_status_counts.oracle_unavailable == 1' "$TMP_DIR/results/oracle-missing-manifest/summary.json" >/dev/null

echo "[test-owl2bench-oracle] ok"
