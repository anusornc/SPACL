#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
VALIDATOR="$SCRIPT_DIR/profile_result_contract.py"
TMP_DIR="$(mktemp -d)"
trap 'rm -rf "$TMP_DIR"' EXIT

BASE="$TMP_DIR/base.json"
python3 - "$BASE" <<'PY'
import json
import hashlib
import pathlib
import sys

base_dir = pathlib.Path(sys.argv[1]).parent
oracle_answer = base_dir / "oracle-answer.out"
oracle_provenance = base_dir / "oracle-provenance.json"
oracle_proof = base_dir / "oracle-proof.json"
oracle_audit = base_dir / "oracle-audit.json"
oracle_answer.write_text("answer\n", encoding="utf-8")
oracle_provenance.write_text("{\"source\":\"test-oracle\"}\n", encoding="utf-8")
oracle_proof.write_text("{\"proof\":\"test\"}\n", encoding="utf-8")
oracle_audit.write_text("{\"audit\":\"test\"}\n", encoding="utf-8")

def file_hash(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()

def text_hash(value):
    return hashlib.sha256(value.encode("utf-8")).hexdigest()

canonical_payload = {
    "canonicalizer_version": "owl2bench-query-canonicalizer/2",
    "projection": ["x"],
    "select_modifier": "distinct",
    "distinct": True,
    "ordered": False,
    "row_order": "typed_tuple",
    "rows": [[{"type": "iri", "value": "http://example.test/value"}]],
}
canonical = "sha256:" + hashlib.sha256(
    json.dumps(canonical_payload, sort_keys=True, separators=(",", ":")).encode("utf-8")
).hexdigest()
canonical_result = base_dir / "canonical-result.json"
canonical_result.write_text(
    json.dumps({**canonical_payload, "format": "tsv", "row_count": 1, "result_hash": canonical}) + "\n",
    encoding="utf-8",
)

oracle_provenance_text = "test-oracle-provenance"
record = {
    "schema_version": 1,
    "case_id": "query-001__QL",
    "query_id": "query-001",
    "query_profile": "QL",
    "status": "success",
    "execution_status": "success",
    "execution_detail": "success",
    "result_status": "oracle_unavailable",
    "semantic_result": canonical,
    "reasoning_result": canonical,
    "correctness_status": "not_evaluated",
    "output": "?x\n<http://example.test/value>\n",
    "diagnostics_file": "raw/query-001.diagnostics",
    "load_time_ms": -1,
    "query_time_ms": 12,
    "source_query": "official-source#query-001",
    "source_query_sha256": "3" * 64,
    "source_corpus": "owl2bench-test",
    "source_revision": "test-revision",
    "dataset_identity": {"dataset_id": "dataset-ql", "dataset_sha256": "9" * 64},
    "canonicalization": {
        "status": "available",
        "canonicalizer": "owl2bench-query-canonicalizer",
        "canonicalizer_version": "owl2bench-query-canonicalizer/2",
        "format": "tsv",
        "canonical_format": "typed-rdf-results-v1",
        "result_hash": canonical,
        "canonical_result_file": str(canonical_result),
    },
    "oracle": {
        "status": "unavailable",
        "match_reason": "identity_mismatch:dataset",
        "case_id": "query-001__QL",
        "query_id": "query-001",
        "profile": "QL",
        "source_query_sha256": "3" * 64,
        "source_corpus": "owl2bench-test",
        "source_revision": "test-revision",
        "dataset_id": "dataset-ql",
        "dataset_sha256": "9" * 64,
        "profile_context_identity": "profile=QL;directory=/data/QL",
        "canonicalizer_version": "owl2bench-query-canonicalizer/2",
        "source": str(oracle_answer),
        "provenance": oracle_provenance_text,
        "provenance_sha256": text_hash(oracle_provenance_text),
        "provenance_artifact": str(oracle_provenance),
        "provenance_artifact_sha256": file_hash(oracle_provenance),
        "artifact_sha256": file_hash(oracle_answer),
        "proof_artifact": str(oracle_proof),
        "proof_artifact_sha256": file_hash(oracle_proof),
        "audit_artifact": str(oracle_audit),
        "audit_artifact_sha256": file_hash(oracle_audit),
    },
    "differential_evidence": [{"class": "differential", "engine": "other-engine"}],
    "profile_context": {"profile": "QL", "identity": "profile=QL;directory=/data/QL"},
}
pathlib.Path(sys.argv[1]).write_text(json.dumps(record, indent=2) + "\n", encoding="utf-8")
PY

run_valid_variant() {
  local name="$1"
  local filter="$2"
  local result_file="$TMP_DIR/$name.json"
  jq "$filter" "$BASE" > "$result_file"
  python3 "$VALIDATOR" validate --result "$result_file" --output "$TMP_DIR/$name-validation.json"
  jq -e '.status == "valid" and (.errors | length) == 0' "$TMP_DIR/$name-validation.json" >/dev/null
}

run_invalid_variant() {
  local name="$1"
  local filter="$2"
  local result_file="$TMP_DIR/$name.json"
  jq "$filter" "$BASE" > "$result_file"
  if python3 "$VALIDATOR" validate --result "$result_file" --output "$TMP_DIR/$name-validation.json"; then
    echo "assertion failed: $name unexpectedly passed contract validation" >&2
    exit 1
  fi
  jq -e '.status == "invalid" and (.errors | length) > 0' "$TMP_DIR/$name-validation.json" >/dev/null
}

run_valid_variant no-oracle '.result_status = "oracle_unavailable" | .correctness_status = "not_evaluated"'
run_valid_variant differential '.result_status = "oracle_unavailable" | .oracle.status = "unavailable" | .differential_evidence = [{class:"differential",engine:"agreement-only"}]'
run_valid_variant passed '
  .result_status = "passed"
  | .correctness_status = "passed"
  | .semantic_result as $semantic
  | .oracle = (.oracle
      | .status = "available"
      | .class = "official"
      | .oracle_id = "official:query-001:QL"
      | .match_reason = "accepted"
      | .canonical_result_hash = $semantic)
'
run_valid_variant wrong '
  .result_status = "wrong"
  | .correctness_status = "wrong"
  | .oracle = (.oracle
      | .status = "available"
      | .class = "official"
      | .oracle_id = "official:query-001:QL"
      | .match_reason = "accepted"
      | .canonical_result_hash = ("sha256:" + ("2" * 64)))
'
run_valid_variant derived '
  .result_status = "passed"
  | .correctness_status = "passed"
  | .semantic_result as $semantic
  | .oracle = (.oracle
      | .status = "available"
      | .class = "derived"
      | .oracle_id = "derived:query-001:QL"
      | .match_reason = "accepted"
      | .canonical_result_hash = $semantic
      | .derivation_deterministic = true
      | .sound_complete = true
      | .audit_status = "independently_audited"
      | .oracle_pipeline_version = "derived-pipeline/1")
'
run_invalid_variant derived-unreviewed '
  .result_status = "passed"
  | .correctness_status = "passed"
  | .oracle = {
      status:"available",
      class:"derived",
      oracle_id:"derived:query-001:QL",
      provenance:"unreviewed-derived-oracle",
      provenance_sha256:("4" * 64),
      provenance_artifact:"provenance/derived.json",
      provenance_artifact_sha256:("5" * 64),
      artifact_sha256:("6" * 64),
      canonical_result_hash:("sha256:" + ("1" * 64)),
      match_reason:"accepted"
    }
'

for status in unsupported not_available failed timeout; do
  run_valid_variant "$status" \
    ".execution_status = \"$status\" | .status = \"$status\" | .result_status = \"$status\" | .canonicalization.status = \"not_applicable\" | .oracle.status = \"not_applicable\""
done

run_valid_variant malformed-output '
  .execution_status = "failed"
  | .status = "failed"
  | .result_status = "failed"
  | .canonicalization = {status:"failed",canonicalizer_version:"owl2bench-query-canonicalizer/2",result_hash:null}
  | .oracle.status = "not_applicable"
'
QUERY006="$TMP_DIR/query-006.json"
jq '
  .case_id = "owl2bench_query_006__DL"
  | .query_id = "owl2bench_query_006"
  | .query_profile = "DL"
  | .profile_context.profile = "DL"
  | .profile_context.identity = "profile=DL;directory=/data/DL"
  | .execution_status = "failed"
  | .status = "failed"
  | .result_status = "failed"
  | .output = "<sparql><head><variable name=\"x\"/></head><results/></sparql>"
  | .canonicalization = {status:"failed",canonicalizer_version:"owl2bench-query-canonicalizer/2",result_hash:null}
  | .oracle.status = "not_applicable"
' "$BASE" > "$QUERY006"
python3 "$VALIDATOR" validate --result "$QUERY006" --output "$TMP_DIR/query-006-validation.json"
jq -e '
  .status == "valid"
  and .errors == []
' "$TMP_DIR/query-006-validation.json" >/dev/null
jq -e '
  .result_status == "failed"
  and (.output | contains("<variable name=\"x\"/>") )
  and .canonicalization.status == "failed"
  and .canonicalization.result_hash == null
' "$QUERY006" >/dev/null

run_invalid_variant profile-mismatch '.component_manifest = {profile:"DL",status:"available"}'
run_invalid_variant unknown-profile '.query_profile = "BOGUS" | .execution_status = "failed" | .status = "failed" | .result_status = "failed" | .canonicalization.status = "not_applicable" | .oracle.status = "not_applicable"'
run_invalid_variant passed-without-oracle '.result_status = "passed" | .correctness_status = "passed"'
run_invalid_variant correctness-overclaim '.correctness_status = "passed"'
run_invalid_variant answer-key '.nested = {answer_key:{case_id:"not-permitted"}}'

run_invalid_file_variant() {
  local name="$1"
  local source_file="$2"
  local filter="$3"
  local result_file="$TMP_DIR/$name.json"
  jq "$filter" "$source_file" > "$result_file"
  if python3 "$VALIDATOR" validate --result "$result_file" --output "$TMP_DIR/$name-validation.json"; then
    echo "assertion failed: $name unexpectedly passed contract validation" >&2
    exit 1
  fi
  jq -e '.status == "invalid" and (.errors | length) > 0' "$TMP_DIR/$name-validation.json" >/dev/null
}

for identity_field in case_id query_id profile dataset_id canonicalizer_version; do
  case "$identity_field" in
    case_id) mismatch_filter='.oracle.case_id = "other-case"' ;;
    query_id) mismatch_filter='.oracle.query_id = "other-query"' ;;
    profile) mismatch_filter='.oracle.profile = "DL"' ;;
    dataset_id) mismatch_filter='.oracle.dataset_id = "other-dataset"' ;;
    canonicalizer_version) mismatch_filter='.oracle.canonicalizer_version = "other-canonicalizer/1"' ;;
  esac
  run_invalid_file_variant "oracle-$identity_field-mismatch" "$TMP_DIR/passed.json" "$mismatch_filter"
done
run_invalid_file_variant passed-without-correctness "$TMP_DIR/passed.json" '.correctness_status = "not_evaluated"'
run_invalid_file_variant wrong-without-correctness "$TMP_DIR/wrong.json" '.correctness_status = "not_evaluated"'
run_invalid_file_variant wrong-canonical-format "$TMP_DIR/passed.json" '.canonicalization.canonical_format = "wrong-format"'
run_invalid_file_variant missing-oracle-artifact "$TMP_DIR/passed.json" '.oracle.provenance_artifact = "/definitely/missing/oracle-provenance.json"'
run_invalid_file_variant missing-canonical-artifact "$TMP_DIR/passed.json" '.canonicalization.canonical_result_file = "/definitely/missing/canonical.json"'
BAD_CANONICAL="$TMP_DIR/bad-canonical.json"
printf '%s\n' '{"result_hash":"sha256:0000000000000000000000000000000000000000000000000000000000000001"}' > "$BAD_CANONICAL"
run_invalid_file_variant bad-canonical-payload "$TMP_DIR/passed.json" ".canonicalization.canonical_result_file = \"$BAD_CANONICAL\""

RUN_DIR="$TMP_DIR/run"
mkdir -p "$RUN_DIR"
cp "$TMP_DIR/no-oracle.json" "$RUN_DIR/no-oracle.json"
cp "$TMP_DIR/passed.json" "$RUN_DIR/passed.json"
cp "$TMP_DIR/unsupported.json" "$RUN_DIR/unsupported.json"
jq '.query_contract_scope = "invalid_manifest_row"' "$TMP_DIR/unknown-profile.json" > "$RUN_DIR/unknown-profile.json"
printf '%s\n' '{
  "schema_version": 1,
  "run_id": "contract-fixture",
  "suite": "owl2bench",
  "lane": "query",
  "component_manifest": {"requested": false},
  "materialization": {"requested": false},
  "result_contract": {
    "validator": "profile_result_contract.py",
    "validation_file": "profile-contract.json",
    "status": "valid",
    "answer_key_policy": "not_permitted"
  }
}' > "$RUN_DIR/run_metadata.json"
python3 "$VALIDATOR" validate-run --run-dir "$RUN_DIR" --output "$TMP_DIR/run-validation.json"
jq -e '
  .status == "valid"
  and .result_count == 3
  and .excluded_invalid_manifest_rows == ["unknown-profile.json"]
  and .result_status_counts.oracle_unavailable == 1
  and .result_status_counts.passed == 1
  and .result_status_counts.unsupported == 1
' "$TMP_DIR/run-validation.json" >/dev/null

INVALID_ANSWER_RUN="$TMP_DIR/invalid-answer-run"
mkdir -p "$INVALID_ANSWER_RUN"
cp "$TMP_DIR/unknown-profile.json" "$INVALID_ANSWER_RUN/invalid-answer.json"
jq '.nested = {answer_key:{case_id:"not-permitted"}} | .query_contract_scope = "invalid_manifest_row"' \
  "$INVALID_ANSWER_RUN/invalid-answer.json" > "$INVALID_ANSWER_RUN/invalid-answer.tmp"
mv "$INVALID_ANSWER_RUN/invalid-answer.tmp" "$INVALID_ANSWER_RUN/invalid-answer.json"
cp "$RUN_DIR/run_metadata.json" "$INVALID_ANSWER_RUN/run_metadata.json"
if python3 "$VALIDATOR" validate-run --run-dir "$INVALID_ANSWER_RUN" --output "$TMP_DIR/invalid-answer-validation.json"; then
  echo "assertion failed: invalid-manifest answer-key artifact passed validation" >&2
  exit 1
fi
jq -e '
  .status == "invalid"
  and ([.errors[] | select(. == "invalid-answer.json:forbidden_field:nested.answer_key")] | length) == 1
' "$TMP_DIR/invalid-answer-validation.json" >/dev/null

MINIMAL_SPLIT_RUN="$TMP_DIR/minimal-split-run"
mkdir -p "$MINIMAL_SPLIT_RUN"
jq '.component_manifest = {profile:"QL",status:"available"} | .materialization = {profile:"QL",status:"complete"}' \
  "$TMP_DIR/no-oracle.json" > "$MINIMAL_SPLIT_RUN/no-oracle.json"
printf '%s\n' '{
  "schema_version": 1,
  "run_id": "minimal-split-fixture",
  "suite": "owl2bench",
  "lane": "query",
  "component_manifest": {"requested": true, "validation_file": "components.json"},
  "materialization": {"requested": true, "validation_file": "materialization.json"},
  "result_contract": {"status": "valid", "answer_key_policy": "not_permitted"}
}' > "$MINIMAL_SPLIT_RUN/run_metadata.json"
if python3 "$VALIDATOR" validate-run --run-dir "$MINIMAL_SPLIT_RUN" --output "$TMP_DIR/minimal-split-validation.json"; then
  echo "assertion failed: minimal split provenance passed contract validation" >&2
  exit 1
fi
jq -e '
  .status == "invalid"
  and ([.errors[] | select(. == "no-oracle.json:component_manifest_identity_incomplete")] | length) == 1
  and ([.errors[] | select(. == "no-oracle.json:materialization_identity_incomplete")] | length) == 1
' "$TMP_DIR/minimal-split-validation.json" >/dev/null

BAD_METADATA_RUN="$TMP_DIR/bad-metadata-run"
mkdir -p "$BAD_METADATA_RUN"
cp "$TMP_DIR/no-oracle.json" "$BAD_METADATA_RUN/no-oracle.json"
printf '{"schema_version":' > "$BAD_METADATA_RUN/run_metadata.json"
if python3 "$VALIDATOR" validate-run --run-dir "$BAD_METADATA_RUN" --output "$TMP_DIR/bad-metadata-validation.json"; then
  echo "assertion failed: malformed run metadata passed contract validation" >&2
  exit 1
fi
jq -e '
  .status == "invalid"
  and ([.errors[] | select(. == "run_metadata_invalid")] | length) == 1
' "$TMP_DIR/bad-metadata-validation.json" >/dev/null

LIST_RESULT_RUN="$TMP_DIR/list-result-run"
mkdir -p "$LIST_RESULT_RUN"
printf '%s\n' '[{"schema_version":1,"case_id":"not-an-object"}]' > "$LIST_RESULT_RUN/list-result.json"
cp "$RUN_DIR/run_metadata.json" "$LIST_RESULT_RUN/run_metadata.json"
if python3 "$VALIDATOR" validate-run --run-dir "$LIST_RESULT_RUN" --output "$TMP_DIR/list-result-validation.json"; then
  echo "assertion failed: list-shaped result record passed contract validation" >&2
  exit 1
fi
jq -e '
  .status == "invalid"
  and ([.errors[] | select(. == "list-result.json:record_not_object")] | length) == 1
' "$TMP_DIR/list-result-validation.json" >/dev/null

BAD_SCHEMA_RUN="$TMP_DIR/bad-schema-run"
mkdir -p "$BAD_SCHEMA_RUN"
cp "$TMP_DIR/no-oracle.json" "$BAD_SCHEMA_RUN/no-oracle.json"
printf '%s\n' '{
  "schema_version": 999,
  "run_id": "contract-fixture",
  "suite": "owl2bench",
  "lane": "query",
  "component_manifest": {"requested": false},
  "materialization": {"requested": false},
  "result_contract": {
    "validator": "profile_result_contract.py",
    "validation_file": "profile-contract.json",
    "status": "valid",
    "answer_key_policy": "not_permitted"
  }
}' > "$BAD_SCHEMA_RUN/run_metadata.json"
if python3 "$VALIDATOR" validate-run --run-dir "$BAD_SCHEMA_RUN" --output "$TMP_DIR/bad-schema-validation.json"; then
  echo "assertion failed: invalid run metadata schema passed contract validation" >&2
  exit 1
fi
jq -e '
  .status == "invalid"
  and ([.errors[] | select(. == "run_metadata_schema_version_invalid")] | length) == 1
' "$TMP_DIR/bad-schema-validation.json" >/dev/null

echo "[test-owl2bench-profile-result-contract] ok"
