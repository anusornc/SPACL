#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
VALIDATOR="$SCRIPT_DIR/materialization_manifest.py"
TMP_DIR="$(mktemp -d)"
trap 'rm -rf "$TMP_DIR"' EXIT

QUERY_FILE="$TMP_DIR/query.rq"
printf 'SELECT DISTINCT ?x WHERE { ?x ?p ?o }\n' > "$QUERY_FILE"
QUERY_MANIFEST="$TMP_DIR/query-manifest.tsv"
printf 'staged_query\tsource_query\tquery_id\tprofile\tworkload\tsource_corpus\tsource_revision\n' > "$QUERY_MANIFEST"
printf '%s\tquery-source\tmaterialization-query\tDL,EL,QL,RL\tofficial-text\ttest-corpus\ttest-revision\n' \
  "$QUERY_FILE" >> "$QUERY_MANIFEST"

MATERIALIZATION_MANIFEST="$TMP_DIR/materialization.json"
python3 - "$MATERIALIZATION_MANIFEST" "$TMP_DIR" <<'PY'
import hashlib
import json
import pathlib
import sys

root = pathlib.Path(sys.argv[2])
profiles = {}
for index, profile in enumerate(("DL", "EL", "QL", "RL"), start=1):
    source = root / f"{profile.lower()}-source.owl"
    output = root / f"{profile.lower()}-closure.ttl"
    source.write_text(f"source {profile}\n", encoding="utf-8")
    output.write_text(f"closure {profile}\n", encoding="utf-8")
    source_hash = "sha256:" + hashlib.sha256(source.read_bytes()).hexdigest()
    output_hash = "sha256:" + hashlib.sha256(output.read_bytes()).hexdigest()
    component_hash = f"sha256:{index:064x}"
    profiles[profile] = {
        "status": "complete",
        "profile": profile,
        "source_ontology": {
            "id": f"source-{profile.lower()}",
            "path": str(source),
            "sha256": source_hash,
        },
        "component": {
            "name": f"{profile.lower()}-materializer",
            "version": "1.0.0",
            "revision": f"{profile.lower()}-materializer-revision",
            "artifact_digest": component_hash,
        },
        **(
            {
                "ruleset": {
                    "id": f"{profile.lower()}-ruleset",
                    "version": "1.0.0",
                    "revision": f"{profile.lower()}-ruleset-revision",
                    "artifact_digest": component_hash,
                    "conformance_boundary": "test",
                }
            }
            if profile in {"QL", "RL"}
            else {}
        ),
        "configuration": {
            "id": f"{profile.lower()}-materialization-config",
            "sha256": component_hash,
        },
        "output_graph": {
            "id": f"closure-{profile.lower()}",
            "path": str(output),
            "sha256": output_hash,
        },
        "derivation": {
            "version": "materialization-1.0",
            "revision": f"{profile.lower()}-derivation-revision",
        },
        "resource_limits": {
            "timeout_seconds": 600,
            "memory": "4GiB",
            "threads": 1,
            "cpu": "unspecified",
            "disk": "unspecified",
            "network": "test-local",
            "variant": "owl2bench-test-4GiB",
        },
        "input_sha256": source_hash,
        "output_sha256": output_hash,
        "profile_context": {
            "id": f"context-{profile.lower()}",
            "directory": f"/data/{profile}",
        },
    }

document = {
    "schema_version": 1,
    "manifest_id": "test-materialization-manifest",
    "profiles": profiles,
}
pathlib.Path(sys.argv[1]).write_text(json.dumps(document, indent=2) + "\n", encoding="utf-8")
PY

COMPONENT_MANIFEST="$TMP_DIR/components.json"
STACK_QUERY_MANIFEST="$TMP_DIR/stack-query-manifest.tsv"
python3 - "$COMPONENT_MANIFEST" "$MATERIALIZATION_MANIFEST" "$TMP_DIR" "$STACK_QUERY_MANIFEST" <<'PY'
import hashlib
import json
import pathlib
import sys

component_path = pathlib.Path(sys.argv[1])
materialization_path = pathlib.Path(sys.argv[2])
root = pathlib.Path(sys.argv[3])
stack_query_manifest_path = pathlib.Path(sys.argv[4])
profiles = {}
for index, profile in enumerate(("DL", "EL", "QL", "RL"), start=1):
    source = root / f"{profile.lower()}-source.owl"
    source_hash = "sha256:" + hashlib.sha256(source.read_bytes()).hexdigest()
    component_hash = f"sha256:{index:064x}"
    record = {
        "components": {
            "engine": {"name": f"{profile.lower()}-engine", "version": "1.0.0", "revision": f"{profile.lower()}-engine-revision", "artifact_digest": component_hash},
            "materializer": {"name": f"{profile.lower()}-materializer", "version": "1.0.0", "revision": f"{profile.lower()}-materializer-revision", "artifact_digest": component_hash},
            "query": {"name": f"{profile.lower()}-query", "version": "1.0.0", "revision": f"{profile.lower()}-query-revision", "artifact_digest": component_hash},
        },
        "configuration": {"id": f"{profile.lower()}-materialization-config", "digest": component_hash},
        "runtime": {"id": f"{profile.lower()}-runtime", "dependencies": [{"name": "runtime", "version": "1.0.0", "digest": component_hash}]},
        "credentials": {"status": "not_required"},
        "license": {"status": "not_required"},
        "capability": {"status": "supported", "supports_query": True},
        "resource_limits": {"timeout_seconds": 600, "memory": "4GiB", "threads": 1, "cpu": "unspecified", "disk": "unspecified", "network": "test-local", "variant": "owl2bench-test-4GiB"},
        "dataset": {"id": f"dataset-{profile.lower()}", "sha256": source_hash, "tbox_id": f"tbox-{profile.lower()}", "tbox_sha256": source_hash, "abox_id": f"abox-{profile.lower()}", "abox_sha256": source_hash},
        "generator": {"id": "test-generator", "revision": "test-generator-revision", "seed": 1},
        "profile_context": {"id": f"context-{profile.lower()}", "directory": f"/data/{profile}"},
    }
    if profile in {"QL", "RL"}:
        record["ruleset"] = {"id": f"{profile.lower()}-ruleset", "version": "1.0.0", "revision": f"{profile.lower()}-ruleset-revision", "artifact_digest": component_hash, "conformance_boundary": "test"}
    profiles[profile] = record
component_path.write_text(
    json.dumps(
        {
            "schema_version": 1,
            "manifest_id": "test-component-manifest",
            "matrix": {"expansion": "query_profile", "expected_query_count": 60, "expected_profile_case_counts": {"DL": 22, "EL": 10, "QL": 10, "RL": 18}},
            "profiles": profiles,
        },
        indent=2,
    ) + "\n",
    encoding="utf-8",
)
query_file = root / "query.rq"
lines = ["staged_query\tsource_query\tquery_id\tprofile\tworkload\tsource_corpus\tsource_revision"]
for profile, count in (("DL", 22), ("EL", 10), ("QL", 10), ("RL", 18)):
    for index in range(1, count + 1):
        query_id = f"{profile.lower()}-query-{index:02d}"
        lines.append(
            f"{query_file}\tquery-source-{query_id}\t{query_id}\t{profile}\tofficial-text\ttest-corpus\ttest-revision"
        )
stack_query_manifest_path.write_text("\n".join(lines) + "\n", encoding="utf-8")
PY

# The stack verifier must accept only a complete, cross-consistent pair of
# manifests and must retain the materialization identities without creating
# any graph or answer key.
STACK_VALIDATION="$TMP_DIR/stack-validation.json"
python3 "$VALIDATOR" verify-stack \
  --component-manifest "$COMPONENT_MANIFEST" \
  --materialization-manifest "$MATERIALIZATION_MANIFEST" \
  --query-manifest "$STACK_QUERY_MANIFEST" \
  --output "$STACK_VALIDATION"
jq -e '
  .status == "complete"
  and .answer_key_policy == "not_permitted"
  and ([.profiles[].status] | sort) == ["complete","complete","complete","complete"]
  and .profiles.DL.component.manifest_identity
  and .profiles.DL.materialization.materialization_identity
  and .profiles.DL.cross_contract_errors == []
' "$STACK_VALIDATION" >/dev/null

RULESET_MISSING="$TMP_DIR/ruleset-missing.json"
jq 'del(.profiles.QL.ruleset, .profiles.RL.ruleset)' "$MATERIALIZATION_MANIFEST" > "$RULESET_MISSING"
python3 "$VALIDATOR" validate \
  --materialization-manifest "$RULESET_MISSING" \
  --output "$TMP_DIR/ruleset-missing-validation.json"
jq -e '
  .status == "failed"
  and .profiles.QL.status == "failed"
  and ([.profiles.QL.reasons[] | select(. == "ruleset_missing")] | length) == 1
  and .profiles.RL.status == "failed"
  and ([.profiles.RL.reasons[] | select(. == "ruleset_missing")] | length) == 1
' "$TMP_DIR/ruleset-missing-validation.json" >/dev/null

RULESET_PARTIAL_MISSING="$TMP_DIR/ruleset-partial-missing.json"
jq '.profiles.RL.status = "partial" | .profiles.RL.reason = "closure incomplete" | del(.profiles.RL.ruleset)' \
  "$MATERIALIZATION_MANIFEST" > "$RULESET_PARTIAL_MISSING"
python3 "$VALIDATOR" validate \
  --materialization-manifest "$RULESET_PARTIAL_MISSING" \
  --output "$TMP_DIR/ruleset-partial-missing-validation.json"
jq -e '
  .profiles.RL.status == "failed"
  and ([.profiles.RL.reasons[] | select(. == "ruleset_missing")] | length) == 1
' "$TMP_DIR/ruleset-partial-missing-validation.json" >/dev/null

RULESET_MISMATCH="$TMP_DIR/ruleset-mismatch.json"
jq '
  .profiles.QL.ruleset.artifact_digest = "sha256:0000000000000000000000000000000000000000000000000000000000000009"
  | .profiles.RL.ruleset.artifact_digest = "sha256:0000000000000000000000000000000000000000000000000000000000000009"
' \
  "$COMPONENT_MANIFEST" > "$RULESET_MISMATCH"
if python3 "$VALIDATOR" verify-stack \
  --component-manifest "$RULESET_MISMATCH" \
  --materialization-manifest "$MATERIALIZATION_MANIFEST" \
  --query-manifest "$STACK_QUERY_MANIFEST" \
  --output "$TMP_DIR/ruleset-mismatch-validation.json"; then
  echo "assertion failed: QL ruleset mismatch passed verification" >&2
  exit 1
fi
jq -e '
  .status == "failed"
  and .profiles.QL.status == "failed"
  and ([.profiles.QL.cross_contract_errors[] | select(startswith("ruleset_identity_mismatch:"))] | length) >= 1
  and .profiles.RL.status == "failed"
  and ([.profiles.RL.cross_contract_errors[] | select(startswith("ruleset_identity_mismatch:"))] | length) >= 1
' "$TMP_DIR/ruleset-mismatch-validation.json" >/dev/null

STACK_MISMATCH="$TMP_DIR/stack-mismatch.json"
jq '.profiles.EL.components.materializer.artifact_digest = "sha256:0000000000000000000000000000000000000000000000000000000000000009"' \
  "$COMPONENT_MANIFEST" > "$STACK_MISMATCH"
if python3 "$VALIDATOR" verify-stack \
  --component-manifest "$STACK_MISMATCH" \
  --materialization-manifest "$MATERIALIZATION_MANIFEST" \
  --query-manifest "$STACK_QUERY_MANIFEST" \
  --output "$TMP_DIR/stack-mismatch-validation.json"; then
  echo "assertion failed: cross-manifest mismatch passed verification" >&2
  exit 1
fi
jq -e '
  .status == "failed"
  and .profiles.EL.status == "failed"
  and ([.profiles.EL.cross_contract_errors[] | select(startswith("materializer_identity_mismatch:"))] | length) >= 1
' "$TMP_DIR/stack-mismatch-validation.json" >/dev/null

DATASET_MISMATCH="$TMP_DIR/dataset-mismatch.json"
jq '.profiles.EL.dataset.sha256 = "sha256:0000000000000000000000000000000000000000000000000000000000000009"' \
  "$COMPONENT_MANIFEST" > "$DATASET_MISMATCH"
if python3 "$VALIDATOR" verify-stack \
  --component-manifest "$DATASET_MISMATCH" \
  --materialization-manifest "$MATERIALIZATION_MANIFEST" \
  --query-manifest "$STACK_QUERY_MANIFEST" \
  --output "$TMP_DIR/dataset-mismatch-validation.json"; then
  echo "assertion failed: dataset/source mismatch passed verification" >&2
  exit 1
fi
jq -e '
  .status == "failed"
  and .profiles.EL.status == "failed"
  and ([.profiles.EL.cross_contract_errors[] | select(. == "dataset_source_identity_mismatch")] | length) == 1
  and ([.profiles.EL.cross_contract_errors[] | select(. == "dataset_input_identity_mismatch")] | length) == 1
' "$TMP_DIR/dataset-mismatch-validation.json" >/dev/null

if python3 "$VALIDATOR" verify-stack \
  --component-manifest "$TMP_DIR/missing-components.json" \
  --materialization-manifest "$TMP_DIR/missing-materialization.json" \
  --query-manifest "$STACK_QUERY_MANIFEST" \
  --output "$TMP_DIR/stack-missing-validation.json"; then
  echo "assertion failed: missing stack inputs passed verification" >&2
  exit 1
fi
jq -e '
  .status == "not_available"
  and .answer_key_policy == "not_permitted"
' "$TMP_DIR/stack-missing-validation.json" >/dev/null

if python3 "$VALIDATOR" verify-stack \
  --component-manifest "$COMPONENT_MANIFEST" \
  --materialization-manifest "$MATERIALIZATION_MANIFEST" \
  --query-manifest "$TMP_DIR/missing-query-manifest.tsv" \
  --output "$TMP_DIR/stack-query-missing-validation.json"; then
  echo "assertion failed: missing query manifest passed verification" >&2
  exit 1
fi
jq -e '
  .status == "not_available"
  and ([.errors[] | select(startswith("component_query:query_manifest_missing:"))] | length) == 1
' "$TMP_DIR/stack-query-missing-validation.json" >/dev/null

OUTPUT="$TMP_DIR/validation.json"
python3 "$VALIDATOR" validate \
  --materialization-manifest "$MATERIALIZATION_MANIFEST" \
  --output "$OUTPUT"
jq -e '
  .status == "complete"
  and ([.profiles[].status] | sort) == ["complete","complete","complete","complete"]
  and .profiles.DL.profile == "DL"
  and .profiles.DL.source_ontology.id == "source-dl"
  and .profiles.DL.output_graph.id == "closure-dl"
  and .profiles.DL.input_sha256 == .profiles.DL.source_ontology.sha256
  and .profiles.DL.output_sha256 == .profiles.DL.output_graph.sha256
  and .profiles.DL.resource_limits.timeout_seconds == 600
' "$OUTPUT" >/dev/null

# Explicit non-complete states are retained and never upgraded to complete.
for state in partial failed not_available unsupported timeout; do
  STATE_MANIFEST="$TMP_DIR/$state.json"
  jq --arg state "$state" \
    '.profiles.EL.status = $state | .profiles.EL.reason = ("synthetic " + $state)' \
    "$MATERIALIZATION_MANIFEST" > "$STATE_MANIFEST"
  python3 "$VALIDATOR" validate \
    --materialization-manifest "$STATE_MANIFEST" \
    --output "$TMP_DIR/$state-validation.json"
  jq -e --arg state "$state" '
    .profiles.EL.status == $state
    and ([.profiles.DL.status, .profiles.QL.status, .profiles.RL.status] | sort)
        == ["complete","complete","complete"]
  ' "$TMP_DIR/$state-validation.json" >/dev/null
done

GLOBAL_TIMEOUT_MANIFEST="$TMP_DIR/global-timeout.json"
jq '.profiles.EL.status = "timeout" | .profiles.EL.reason = "global timeout precedence"' \
  "$MATERIALIZATION_MANIFEST" > "$GLOBAL_TIMEOUT_MANIFEST"
python3 "$VALIDATOR" validate \
  --materialization-manifest "$GLOBAL_TIMEOUT_MANIFEST" \
  --output "$TMP_DIR/global-timeout-validation.json"
jq -e '.status == "timeout" and .profiles.EL.status == "timeout"' \
  "$TMP_DIR/global-timeout-validation.json" >/dev/null

# A changed output hash is a failed, auditable materialization, not a repaired closure.
HASH_MISMATCH="$TMP_DIR/hash-mismatch.json"
jq '.profiles.DL.output_sha256 = "sha256:0000000000000000000000000000000000000000000000000000000000000000"' \
  "$MATERIALIZATION_MANIFEST" > "$HASH_MISMATCH"
python3 "$VALIDATOR" validate \
  --materialization-manifest "$HASH_MISMATCH" \
  --output "$TMP_DIR/hash-mismatch-validation.json"
jq -e '
  .status == "failed"
  and .profiles.DL.status == "failed"
  and ([.profiles.DL.reasons[] | select(. == "output_hash_mismatch")] | length) == 1
' "$TMP_DIR/hash-mismatch-validation.json" >/dev/null

# Reusing one output graph across profiles fails only the affected profiles.
MIXED_DATA="$TMP_DIR/mixed-data.json"
jq '
  .profiles.EL.output_graph = .profiles.DL.output_graph
  | .profiles.EL.output_sha256 = .profiles.DL.output_sha256
' "$MATERIALIZATION_MANIFEST" > "$MIXED_DATA"
python3 "$VALIDATOR" validate \
  --materialization-manifest "$MIXED_DATA" \
  --output "$TMP_DIR/mixed-data-validation.json"
jq -e '
  .status == "failed"
  and .profiles.DL.status == "failed"
  and .profiles.EL.status == "failed"
  and .profiles.QL.status == "complete"
  and .profiles.RL.status == "complete"
  and ([.profiles.DL.reasons[] | select(startswith("cross_profile_materialization:"))] | length) >= 1
' "$TMP_DIR/mixed-data-validation.json" >/dev/null

# Answer-key fields are forbidden recursively in materialization records.
ANSWER_KEY="$TMP_DIR/answer-key.json"
jq '.profiles.DL.answer_keys = [{"case_id":"must-not-exist"}]' "$MATERIALIZATION_MANIFEST" > "$ANSWER_KEY"
python3 "$VALIDATOR" validate \
  --materialization-manifest "$ANSWER_KEY" \
  --output "$TMP_DIR/answer-key-validation.json"
jq -e '
  .status == "failed"
  and ([.manifest_errors[] | select(. == "answer_key_field_not_allowed:profiles.DL.answer_keys")] | length) == 1
' "$TMP_DIR/answer-key-validation.json" >/dev/null

# Complete materialization is accepted by the runner; partial closure blocks
# only that profile and does not route it through another profile.
ENGINE="$TMP_DIR/fake-engine"
TRACE="$TMP_DIR/engine.trace"
cat > "$ENGINE" <<'ENGINE'
#!/usr/bin/env bash
set -euo pipefail
case "${1:-}" in
  load)
    context_dir="${2:?}"
    profile="$(basename "$context_dir")"
    input="$(find "$context_dir" -maxdepth 1 -type f -name '*.ofn' -print -quit)"
    mkdir -p "$context_dir/.test-split"
    jq -n \
      --arg profile "$profile" \
      --arg context_dir "$context_dir" \
      --arg input "$input" \
      --arg binary "$0" \
      --arg binary_sha256 "sha256:$(sha256sum "$0" | awk '{print $1}')" \
      --arg dataset_sha256 "$(sha256sum "$input" | awk '{print $1}')" \
      '{adapter:"test-split",profile:$profile,context_dir:$context_dir,context_identity:("profile=" + $profile + ";directory=" + $context_dir),binary_path:$binary,binary_sha256:$binary_sha256,dataset_identity:{dataset_id:("dataset-" + $profile),dataset_sha256:("sha256:" + $dataset_sha256),source:"fixture"}}' \
      > "$context_dir/.test-split/provenance.json"
    ;;
  query)
    query_file="${2:?}"
    if [[ -n "${3:-}" ]]; then
      test -f "${3}/.test-split/provenance.json"
    fi
    printf '%s\n' "$query_file" >> "${TRACE:?}"
    printf '?x\n<http://example.test/value>\n'
    ;;
  *)
    exit 2
    ;;
esac
ENGINE
chmod +x "$ENGINE"
CAPABILITIES="$TMP_DIR/capabilities.json"
printf '{"supports_query":true,"profiles":["DL","EL","QL","RL"],"data_mode":"query-file-only"}\n' > "$CAPABILITIES"
SPLIT_CAPABILITIES="$TMP_DIR/split-capabilities.json"
printf '{"supports_query":true,"profiles":["DL","EL","QL","RL"],"data_mode":"directory","adapter":"test-split","provenance_relative_path":".test-split/provenance.json"}\n' > "$SPLIT_CAPABILITIES"

# Explicit split mode runs the cross-manifest preflight before invoking the
# query engine. A mismatched materializer identity must block its profile and
# leave only the affected profile cases out of the engine trace.
SPLIT_DATA_ROOT="$TMP_DIR/split-data"
for profile in DL EL QL RL; do
  mkdir -p "$SPLIT_DATA_ROOT/$profile"
  printf 'Prefix(:=<https://example.test/%s#>)\nDeclaration(Class(:Anchor))\n' "$profile" > "$SPLIT_DATA_ROOT/$profile/anchor.ofn"
done
SPLIT_RUN_ROOT="$TMP_DIR/split-runner-results"
SPLIT_TRACE="$TMP_DIR/split-preflight-engine.trace"
TRACE="$SPLIT_TRACE" \
OWL2BENCH_QUERY_STAGED_DIR="$TMP_DIR" \
OWL2BENCH_QUERY_MANIFEST="$STACK_QUERY_MANIFEST" \
OWL2BENCH_QUERY_DATA_DIR="$SPLIT_DATA_ROOT" \
OWL2BENCH_QUERY_RESULTS_ROOT="$SPLIT_RUN_ROOT" \
OWL2BENCH_QUERY_ENGINE_BIN="$ENGINE" \
OWL2BENCH_QUERY_CAPABILITIES_FILE="$SPLIT_CAPABILITIES" \
OWL2BENCH_QUERY_COMPONENT_MANIFEST="$STACK_MISMATCH" \
OWL2BENCH_QUERY_MATERIALIZATION_MANIFEST="$MATERIALIZATION_MANIFEST" \
OWL2BENCH_QUERY_SPLIT_STACK=1 \
RUN_ID="split-preflight-mismatch" \
  "$SCRIPT_DIR/run.sh" >/dev/null
jq -e '
  .query_count == 60
  and .result_status_counts.failed == 10
  and .result_status_counts.not_available == 50
  and .correctness_gate == "blocked"
' "$SPLIT_RUN_ROOT/split-preflight-mismatch/summary.json" >/dev/null
jq -e '
  .split_stack_materialization.status == "failed"
' "$SPLIT_RUN_ROOT/split-preflight-mismatch/run_metadata.json" >/dev/null
test ! -s "$SPLIT_TRACE"

RUN_ROOT="$TMP_DIR/runner-results"
OWL2BENCH_QUERY_STAGED_DIR="$TMP_DIR" \
OWL2BENCH_QUERY_MANIFEST="$QUERY_MANIFEST" \
OWL2BENCH_QUERY_RESULTS_ROOT="$RUN_ROOT" \
OWL2BENCH_QUERY_ENGINE_BIN="$ENGINE" \
OWL2BENCH_QUERY_CAPABILITIES_FILE="$CAPABILITIES" \
OWL2BENCH_QUERY_MATERIALIZATION_MANIFEST="$MATERIALIZATION_MANIFEST" \
TRACE="$TRACE" \
RUN_ID="materialization-complete" \
  "$SCRIPT_DIR/run.sh" >/dev/null
jq -e '
  .query_count == 4
  and .result_status_counts.not_available == 4
  and .correctness_gate == "blocked"
' "$RUN_ROOT/materialization-complete/summary.json" >/dev/null
jq -e '
  .result_status == "not_available"
  and .materialization.status == "complete"
  and .materialization.output_graph.id == "closure-dl"
' "$RUN_ROOT/materialization-complete/materialization-query__DL.json" >/dev/null
test ! -s "$TRACE"

PARTIAL_RUN_ROOT="$TMP_DIR/partial-runner-results"
PARTIAL_TRACE="$TMP_DIR/partial-engine.trace"
PARTIAL_MANIFEST="$TMP_DIR/partial-runner.json"
jq '.profiles.EL.status = "partial" | .profiles.EL.reason = "closure incomplete"' \
  "$MATERIALIZATION_MANIFEST" > "$PARTIAL_MANIFEST"
OWL2BENCH_QUERY_STAGED_DIR="$TMP_DIR" \
OWL2BENCH_QUERY_MANIFEST="$QUERY_MANIFEST" \
OWL2BENCH_QUERY_RESULTS_ROOT="$PARTIAL_RUN_ROOT" \
OWL2BENCH_QUERY_ENGINE_BIN="$ENGINE" \
OWL2BENCH_QUERY_CAPABILITIES_FILE="$CAPABILITIES" \
OWL2BENCH_QUERY_MATERIALIZATION_MANIFEST="$PARTIAL_MANIFEST" \
TRACE="$PARTIAL_TRACE" \
RUN_ID="materialization-partial" \
  "$SCRIPT_DIR/run.sh" >/dev/null
jq -e '
  .query_count == 4
  and .result_status_counts.failed == 1
  and .result_status_counts.not_available == 3
  and .correctness_gate == "blocked"
' "$PARTIAL_RUN_ROOT/materialization-partial/summary.json" >/dev/null
jq -e '
  .result_status == "failed"
  and .materialization.status == "partial"
' "$PARTIAL_RUN_ROOT/materialization-partial/materialization-query__EL.json" >/dev/null
test ! -s "$PARTIAL_TRACE"

FAILED_RUN_ROOT="$TMP_DIR/failed-runner-results"
FAILED_TRACE="$TMP_DIR/failed-engine.trace"
FAILED_MANIFEST="$TMP_DIR/failed-runner.json"
jq '.profiles.QL.status = "failed" | .profiles.QL.reason = "materializer process failed"' \
  "$MATERIALIZATION_MANIFEST" > "$FAILED_MANIFEST"
OWL2BENCH_QUERY_STAGED_DIR="$TMP_DIR" \
OWL2BENCH_QUERY_MANIFEST="$QUERY_MANIFEST" \
OWL2BENCH_QUERY_RESULTS_ROOT="$FAILED_RUN_ROOT" \
OWL2BENCH_QUERY_ENGINE_BIN="$ENGINE" \
OWL2BENCH_QUERY_CAPABILITIES_FILE="$CAPABILITIES" \
OWL2BENCH_QUERY_MATERIALIZATION_MANIFEST="$FAILED_MANIFEST" \
TRACE="$FAILED_TRACE" \
RUN_ID="materialization-failed" \
  "$SCRIPT_DIR/run.sh" >/dev/null
jq -e '
  .query_count == 4
  and .result_status_counts.failed == 1
  and .result_status_counts.not_available == 3
  and .correctness_gate == "blocked"
' "$FAILED_RUN_ROOT/materialization-failed/summary.json" >/dev/null
jq -e '
  .result_status == "failed"
  and .materialization.status == "failed"
' "$FAILED_RUN_ROOT/materialization-failed/materialization-query__QL.json" >/dev/null
test ! -s "$FAILED_TRACE"

TIMEOUT_RUN_ROOT="$TMP_DIR/timeout-runner-results"
TIMEOUT_TRACE="$TMP_DIR/timeout-engine.trace"
TIMEOUT_MANIFEST="$TMP_DIR/timeout-runner.json"
jq '.profiles.RL.status = "timeout" | .profiles.RL.reason = "materializer exceeded deadline"' \
  "$MATERIALIZATION_MANIFEST" > "$TIMEOUT_MANIFEST"
OWL2BENCH_QUERY_STAGED_DIR="$TMP_DIR" \
OWL2BENCH_QUERY_MANIFEST="$QUERY_MANIFEST" \
OWL2BENCH_QUERY_RESULTS_ROOT="$TIMEOUT_RUN_ROOT" \
OWL2BENCH_QUERY_ENGINE_BIN="$ENGINE" \
OWL2BENCH_QUERY_CAPABILITIES_FILE="$CAPABILITIES" \
OWL2BENCH_QUERY_MATERIALIZATION_MANIFEST="$TIMEOUT_MANIFEST" \
TRACE="$TIMEOUT_TRACE" \
RUN_ID="materialization-timeout" \
  "$SCRIPT_DIR/run.sh" >/dev/null
jq -e '
  .query_count == 4
  and .result_status_counts.timeout == 1
  and .result_status_counts.not_available == 3
  and .correctness_gate == "blocked"
' "$TIMEOUT_RUN_ROOT/materialization-timeout/summary.json" >/dev/null
jq -e '
  .result_status == "timeout"
  and .materialization.status == "timeout"
' "$TIMEOUT_RUN_ROOT/materialization-timeout/materialization-query__RL.json" >/dev/null
test ! -s "$TIMEOUT_TRACE"

UNSUPPORTED_RUN_ROOT="$TMP_DIR/unsupported-runner-results"
UNSUPPORTED_TRACE="$TMP_DIR/unsupported-engine.trace"
UNSUPPORTED_MANIFEST="$TMP_DIR/unsupported-runner.json"
jq '.profiles.RL.status = "unsupported" | .profiles.RL.reason = "materializer capability gap"' \
  "$MATERIALIZATION_MANIFEST" > "$UNSUPPORTED_MANIFEST"
OWL2BENCH_QUERY_STAGED_DIR="$TMP_DIR" \
OWL2BENCH_QUERY_MANIFEST="$QUERY_MANIFEST" \
OWL2BENCH_QUERY_RESULTS_ROOT="$UNSUPPORTED_RUN_ROOT" \
OWL2BENCH_QUERY_ENGINE_BIN="$ENGINE" \
OWL2BENCH_QUERY_CAPABILITIES_FILE="$CAPABILITIES" \
OWL2BENCH_QUERY_MATERIALIZATION_MANIFEST="$UNSUPPORTED_MANIFEST" \
TRACE="$UNSUPPORTED_TRACE" \
RUN_ID="materialization-unsupported" \
  "$SCRIPT_DIR/run.sh" >/dev/null
jq -e '
  .query_count == 4
  and .result_status_counts.unsupported == 1
  and .result_status_counts.not_available == 3
  and .correctness_gate == "blocked"
' "$UNSUPPORTED_RUN_ROOT/materialization-unsupported/summary.json" >/dev/null
jq -e '
  .result_status == "unsupported"
  and .materialization.status == "unsupported"
' "$UNSUPPORTED_RUN_ROOT/materialization-unsupported/materialization-query__RL.json" >/dev/null
test ! -s "$UNSUPPORTED_TRACE"

UNAVAILABLE_RUN_ROOT="$TMP_DIR/unavailable-runner-results"
UNAVAILABLE_TRACE="$TMP_DIR/unavailable-engine.trace"
UNAVAILABLE_MANIFEST="$TMP_DIR/unavailable-runner.json"
jq '.profiles.QL.status = "not_available" | .profiles.QL.reason = "materializer unavailable"' \
  "$MATERIALIZATION_MANIFEST" > "$UNAVAILABLE_MANIFEST"
OWL2BENCH_QUERY_STAGED_DIR="$TMP_DIR" \
OWL2BENCH_QUERY_MANIFEST="$QUERY_MANIFEST" \
OWL2BENCH_QUERY_RESULTS_ROOT="$UNAVAILABLE_RUN_ROOT" \
OWL2BENCH_QUERY_ENGINE_BIN="$ENGINE" \
OWL2BENCH_QUERY_CAPABILITIES_FILE="$CAPABILITIES" \
OWL2BENCH_QUERY_MATERIALIZATION_MANIFEST="$UNAVAILABLE_MANIFEST" \
TRACE="$UNAVAILABLE_TRACE" \
RUN_ID="materialization-unavailable" \
  "$SCRIPT_DIR/run.sh" >/dev/null
jq -e '
  .query_count == 4
  and .result_status_counts.not_available == 4
  and .correctness_gate == "blocked"
' "$UNAVAILABLE_RUN_ROOT/materialization-unavailable/summary.json" >/dev/null
jq -e '
  .result_status == "not_available"
  and .materialization.status == "not_available"
' "$UNAVAILABLE_RUN_ROOT/materialization-unavailable/materialization-query__QL.json" >/dev/null
test ! -s "$UNAVAILABLE_TRACE"

MISSING_MATERIALIZATION="$TMP_DIR/missing-materialization.json"
MISSING_RUN_ROOT="$TMP_DIR/missing-materialization-runner-results"
MISSING_TRACE="$TMP_DIR/missing-materialization-engine.trace"
OWL2BENCH_QUERY_STAGED_DIR="$TMP_DIR" \
OWL2BENCH_QUERY_MANIFEST="$QUERY_MANIFEST" \
OWL2BENCH_QUERY_RESULTS_ROOT="$MISSING_RUN_ROOT" \
OWL2BENCH_QUERY_ENGINE_BIN="$ENGINE" \
OWL2BENCH_QUERY_CAPABILITIES_FILE="$CAPABILITIES" \
OWL2BENCH_QUERY_MATERIALIZATION_MANIFEST="$MISSING_MATERIALIZATION" \
TRACE="$MISSING_TRACE" \
RUN_ID="materialization-missing" \
  "$SCRIPT_DIR/run.sh" >/dev/null
jq -e '
  .query_count == 4
  and .result_status_counts.not_available == 4
  and .correctness_gate == "blocked"
' "$MISSING_RUN_ROOT/materialization-missing/summary.json" >/dev/null
jq -e '
  .materialization.status == "not_available"
  and .materialization.reasons == ["profile_record_missing"]
' "$MISSING_RUN_ROOT/materialization-missing/materialization-query__DL.json" >/dev/null
test ! -s "$MISSING_TRACE"

echo "[test-owl2bench-materialization-manifest] ok"
