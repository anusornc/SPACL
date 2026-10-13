#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
MATERIALIZATION="$SCRIPT_DIR/openllet_ql_materialization.py"
RUNTIME_IDENTITY="$SCRIPT_DIR/openllet_ql_runtime_identity.json"
LICENSE_IDENTITY="$SCRIPT_DIR/openllet_ql_license_identity.json"
DIAGNOSTIC_DIR="$SCRIPT_DIR/openllet_ql_diagnostic"
TMP_DIR="$(mktemp -d)"
cleanup() {
  if [[ -n "${OPENLLET_TEST_STATE_PATH:-}" \
      && "$(basename "$OPENLLET_TEST_STATE_PATH")" == .openllet-ql-test-* \
      && "$(dirname "$OPENLLET_TEST_STATE_PATH")" == "${DATASET_ROOT:-}/profiles/QL" ]]; then
    rm -rf -- "$OPENLLET_TEST_STATE_PATH"
  fi
  if [[ "${KEEP_OPENLLET_QL_TEST_DIR:-0}" == "1" ]]; then
    echo "[test-openllet-ql-materialization] retained test directory: $TMP_DIR" >&2
  else
    rm -rf "$TMP_DIR"
  fi
}
trap cleanup EXIT

DATASET_ROOT="${OWL2BENCH_PROFILE_DATASET_ROOT:-}"
QUERY_MANIFEST="${OWL2BENCH_QUERY_MANIFEST:-}"
OUTPUT_DIR="$TMP_DIR/output"
FAKE_DOCKER="$TMP_DIR/docker"
FAKE_QUERY_BACKEND="$TMP_DIR/query-backend"
DOCKER_LOG="$TMP_DIR/docker.log"
IMAGE_INSPECT="$TMP_DIR/image-inspect.json"
RUN_ACCEPTED_E2E=0
if [[ -n "$DATASET_ROOT" && -f "$DATASET_ROOT/profiles/QL/profile-identity.json" \
    && -n "$QUERY_MANIFEST" && -f "$QUERY_MANIFEST" ]]; then
  RUN_ACCEPTED_E2E=1
fi
jq -e '
  .status == "accepted"
  and .profile == "QL"
  and .execution_variant == "openllet-ql"
  and .materialization_revision == "openllet-ql-materialization-v1"
  and .query_workload.query_count == 10
  and (.query_workload.queries | length) == 10
' "$SCRIPT_DIR/openllet_ql_acceptance_lock.json" >/dev/null

PYTHONPATH="$SCRIPT_DIR" python3 - "$TMP_DIR" <<'PY'
import copy
import pathlib
import sys

from openllet_ql_contract import (
    OpenlletQlUnavailable,
    digest,
    validate_acceptance_contract,
)

root = pathlib.Path(sys.argv[1]) / "acceptance-contract"
root.mkdir()
paths = {}
for key in (
    "profile_identity",
    "source",
    "dataset",
    "tbox",
    "abox",
    "generator",
    "generator_config",
):
    path = root / key
    path.write_text(key + "\n", encoding="utf-8")
    paths[key] = path
query_path = root / "query.rq"
query_path.write_text("SELECT ?subject WHERE { ?subject <urn:test:p> ?object . }\n", encoding="utf-8")
query_manifest = root / "manifest.tsv"
query_manifest.write_text("locked manifest\n", encoding="utf-8")
queries = [
    {
        "id": "locked_query",
        "path": str(query_path),
        "sha256": digest(query_path),
        "source_corpus": "locked-corpus",
        "source_revision": "locked-query-r1",
    }
]
frozen = {
    "source_revision": "locked-profile-r1",
    "identity_sha256": "sha256:" + "1" * 64,
    "dataset": {"id": "locked-profile"},
}
lock = {
    "profile_identity": {
        "id": "locked-profile",
        "source_revision": "locked-profile-r1",
        "identity_sha256": "sha256:" + "1" * 64,
    },
    "artifacts": {
        "source": digest(paths["source"]),
        "generated_dataset": digest(paths["dataset"]),
        "profile_tbox": digest(paths["tbox"]),
        "profile_abox": digest(paths["abox"]),
        "generator": digest(paths["generator"]),
        "generator_configuration": digest(paths["generator_config"]),
    },
    "query_workload": {
        "manifest_sha256": digest(query_manifest),
        "source_corpus": "locked-corpus",
        "source_revision": "locked-query-r1",
        "query_count": 1,
        "queries": [{"id": "locked_query", "sha256": digest(query_path)}],
    },
}
validate_acceptance_contract(lock, paths, frozen, query_manifest, queries)

for section, field in (
    ("profile_identity", "identity_sha256"),
    ("artifacts", "source"),
    ("query_workload", "manifest_sha256"),
):
    mismatched = copy.deepcopy(lock)
    mismatched[section][field] = "sha256:" + "0" * 64
    try:
        validate_acceptance_contract(
            mismatched, paths, frozen, query_manifest, queries
        )
    except OpenlletQlUnavailable:
        continue
    raise AssertionError(f"acceptance mismatch unexpectedly passed: {section}.{field}")
PY

python3 - "$RUNTIME_IDENTITY" "$IMAGE_INSPECT" <<'PY'
import json
import pathlib
import sys

identity = json.loads(pathlib.Path(sys.argv[1]).read_text(encoding="utf-8"))
runtime = identity["runtime"]
pathlib.Path(sys.argv[2]).write_text(
    json.dumps(
        [
            {
                "Id": runtime["image"]["digest"],
                "Architecture": "amd64",
                "Os": "linux",
                "RepoDigests": [runtime["image"]["reference"]],
                "Config": {
                    "Labels": {
                        "org.opencontainers.image.revision": runtime["variant_revision"]
                    }
                },
            }
        ]
    )
    + "\n",
    encoding="utf-8",
)
PY

cat > "$FAKE_DOCKER" <<'SH'
#!/usr/bin/env bash
set -euo pipefail
printf '%q ' "$@" >> "${FAKE_DOCKER_LOG:?}"
printf '\n' >> "$FAKE_DOCKER_LOG"

if [[ "${1:-}" == "image" && "${2:-}" == "inspect" ]]; then
  if [[ "${OPENLLET_QL_TEST_IMAGE_MISSING:-0}" == "1" ]]; then
    exit 1
  fi
  cat "${OPENLLET_QL_TEST_IMAGE_INSPECT:?}"
  exit 0
fi

if [[ "${1:-}" != "run" ]]; then
  exit 2
fi

source_file=""
query_iris=""
while (($#)); do
  if [[ "$1" == "--volume" || "$1" == "-v" ]]; then
    mount="${2:?}"
    case "$mount" in
      *:/input/source.nq:ro) source_file="${mount%:/input/source.nq:ro}" ;;
      *:/input/query-iris.txt:ro) query_iris="${mount%:/input/query-iris.txt:ro}" ;;
    esac
    shift 2
    continue
  fi
  shift
done

[[ -r "$source_file" && -r "$query_iris" ]]
run_number=1
if [[ -n "${FAKE_DOCKER_STATE:-}" ]]; then
  if [[ -f "$FAKE_DOCKER_STATE" ]]; then
    run_number="$(( $(<"$FAKE_DOCKER_STATE") + 1 ))"
  fi
  printf '%s\n' "$run_number" > "$FAKE_DOCKER_STATE"
fi
if grep -Fq 'https://example.test/Agent' "$query_iris"; then
  printf '%s\n' '<https://example.test/alice> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <https://example.test/Agent> .'
fi
if grep -Fq 'https://example.test/related' "$query_iris" \
    && [[ "${OPENLLET_QL_TEST_NONDETERMINISTIC:-0}:$run_number" != "1:2" ]]; then
  printf '%s\n' '<https://example.test/alice> <https://example.test/related> <https://example.test/bob> .'
fi
if [[ "${OPENLLET_QL_TEST_NONDETERMINISTIC:-0}:$run_number" == "1:2" ]] \
    && grep -Fq 'https://kracr.iiitd.edu.in/OWL2Bench#U0C0D0AP0' "$source_file"; then
  printf '%s\n' '<https://kracr.iiitd.edu.in/OWL2Bench#U0C0D0AP0> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <http://benchmark/OWL2Bench#Engineering> .'
fi
if grep -Fq 'http://benchmark/OWL2Bench#EmployeeEvaluationCommittee' "$query_iris"; then
  printf '%s\n' '<urn:tableauxx:wayfinder:67:ql-diagnostic#subject> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <http://benchmark/OWL2Bench#EmployeeEvaluationCommittee> .'
fi
if grep -Fq 'http://benchmark/OWL2Bench#enrollFor' "$query_iris"; then
  printf '%s\n' '<urn:tableauxx:wayfinder:67:ql-diagnostic#subject> <http://benchmark/OWL2Bench#enrollFor> <urn:tableauxx:wayfinder:67:ql-diagnostic#object> .'
fi
SH
chmod +x "$FAKE_DOCKER"

cat > "$FAKE_QUERY_BACKEND" <<'SH'
#!/usr/bin/env bash
set -euo pipefail
[[ "${1:-}" == "query" && $# -ge 2 && $# -le 3 ]]
query_file="$2"
handoff="${OWL2BENCH_QUERY_MATERIALIZATION_HANDOFF:?}"
graph="$(jq -r '.graph_path' "$handoff")"
expected="$(jq -r '.graph_sha256' "$handoff")"
actual="sha256:$(sha256sum "$graph" | awk '{print $1}')"
[[ "$expected" == "$actual" && "$actual" == "${OWL2BENCH_QUERY_MATERIALIZATION_GRAPH_SHA256:?}" ]]
if grep -Fq 'EmployeeEvaluationCommittee' "$query_file"; then
  grep -Fq '<urn:tableauxx:wayfinder:67:ql-diagnostic#subject> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <http://benchmark/OWL2Bench#EmployeeEvaluationCommittee> .' "$graph"
  printf '?subject\n<urn:tableauxx:wayfinder:67:ql-diagnostic#subject>\n'
elif grep -Fq 'enrollFor' "$query_file"; then
  grep -Fq '<urn:tableauxx:wayfinder:67:ql-diagnostic#subject> <http://benchmark/OWL2Bench#enrollFor> <urn:tableauxx:wayfinder:67:ql-diagnostic#object> .' "$graph"
  printf '?subject\t?object\n<urn:tableauxx:wayfinder:67:ql-diagnostic#subject>\t<urn:tableauxx:wayfinder:67:ql-diagnostic#object>\n'
else
  PYTHONPATH="${OPENLLET_QL_TEST_SCRIPT_DIR:?}" python3 - "$query_file" <<'PY'
import pathlib
import sys

from canonicalize_result import parse_query_metadata

metadata = parse_query_metadata(pathlib.Path(sys.argv[1]).read_text(encoding="utf-8"))
projection = metadata["projection"] or ["value"]
print("\t".join("?" + variable for variable in projection))
print("\t".join(f"<urn:openllet:test-result:{index}>" for index, _ in enumerate(projection)))
PY
fi
SH
chmod +x "$FAKE_QUERY_BACKEND"

RUNTIME_ATTESTATION="$TMP_DIR/runtime-attestation.json"
if [[ "$RUN_ACCEPTED_E2E" == "1" ]]; then
SOURCE_HASH_BEFORE="$(sha256sum "$DATASET_ROOT/profiles/QL/source.nq")"
IDENTITY_HASH_BEFORE="$(sha256sum "$DATASET_ROOT/profiles/QL/profile-identity.json")"
FAKE_DOCKER_LOG="$DOCKER_LOG" \
OPENLLET_QL_TEST_IMAGE_INSPECT="$IMAGE_INSPECT" \
python3 "$MATERIALIZATION" run \
  --dataset-root "$DATASET_ROOT" \
  --query-manifest "$QUERY_MANIFEST" \
  --docker-bin "$FAKE_DOCKER" \
  --query-backend "$FAKE_QUERY_BACKEND" \
  --output-dir "$OUTPUT_DIR" \
  > "$TMP_DIR/run.json"

test "$SOURCE_HASH_BEFORE" = "$(sha256sum "$DATASET_ROOT/profiles/QL/source.nq")"
test "$IDENTITY_HASH_BEFORE" = "$(sha256sum "$DATASET_ROOT/profiles/QL/profile-identity.json")"

jq -e '
  .status == "complete"
  and .official.status == "oracle_unavailable"
  and .official.execution_status == "complete"
  and .diagnostic.status == "complete"
  and .claim_boundary.correctness_claim == "not_made"
  and .claim_boundary.performance_claim == "not_made"
' "$TMP_DIR/run.json" >/dev/null

jq -e '
  .profiles.QL.execution_variant == "openllet-ql"
  and .profiles.QL.components.engine.name == "Openllet"
  and .profiles.QL.components.materializer.name == "QL-ruleset-engine"
  and .profiles.QL.components.query.name == "materialized-graph-oxigraph"
  and .profiles.QL.ruleset.id == "openllet-direct-semantics-on-ql-input"
  and .profiles.QL.ruleset.version == "2.6.5"
  and .profiles.QL.ruleset.revision == "openllet-ql-runtime-v1"
  and .profiles.QL.ruleset.conformance_boundary == "Openllet Direct Semantics over OWL 2 QL input"
  and .profiles.QL.runtime.image.digest == "sha256:548e5ecefa8b252f8baec95bfb4e9528363257b653dd79a189aa0f351df937bd"
  and .profiles.QL.capability.status == "supported"
  and .profiles.QL.acceptance_lock.sha256 == $acceptance_lock_digest
  and .profiles.QL.acceptance_lock.materialization_revision == "openllet-ql-materialization-v1"
  and (.profiles.QL.materialization_implementation.modules | length) == 4
  and .profiles.QL.license.status == "available"
  and .profiles.QL.license.id == "AGPL-3.0-only"
  and .profiles.QL.license.digest == $license_digest
  and .profiles.QL.license.distribution_review_status == "pending"
' --arg license_digest "sha256:$(sha256sum "$LICENSE_IDENTITY" | awk '{print $1}')" \
  --arg acceptance_lock_digest "sha256:$(sha256sum "$SCRIPT_DIR/openllet_ql_acceptance_lock.json" | awk '{print $1}')" \
  "$OUTPUT_DIR/component-manifest.json" >/dev/null

python3 "$SCRIPT_DIR/split_stack_manifest.py" validate \
  --component-manifest "$OUTPUT_DIR/component-manifest.json" \
  --query-manifest "$QUERY_MANIFEST" \
  --output "$TMP_DIR/component-validation.json"
if ! jq -e '
    .profiles.QL.status == "available"
    and .profiles.QL.reasons == []
    and .profiles.QL.execution_variant == "openllet-ql"
  ' "$TMP_DIR/component-validation.json" >/dev/null; then
  jq '.profiles.QL' "$TMP_DIR/component-validation.json" >&2
  exit 1
fi

jq -e '
  .profiles.QL.status == "complete"
  and .profiles.QL.graph_contract.source_included == true
  and .profiles.QL.graph_contract.required_assertion_families == ["class","object_property","data_property","tbox","rbox"]
  and .profiles.QL.graph_contract.anonymous_witness_policy == "not_exported"
  and .profiles.QL.graph_contract.two_run_determinism_required == true
  and .profiles.QL.determinism.status == "complete"
  and .profiles.QL.determinism.runs == 2
  and .profiles.QL.determinism.first_graph.sha256 == .profiles.QL.determinism.second_graph.sha256
  and .profiles.QL.derivation.semantics == "Openllet Direct Semantics over OWL 2 QL input"
  and .profiles.QL.acceptance_lock.profile_identity_sha256 == "sha256:24d2236999ae19f855012c7af1784173de180b6619428aecb074eb898b67ddbd"
  and .profiles.QL.acceptance_lock.query_manifest_sha256 == "sha256:dae8f0a57e74a61690cce432915edeadba929ccf7a84c91a2a59e8a841d8f7a7"
  and (.profiles.QL.implementation.modules | length) == 4
  and all(.profiles.QL.implementation.modules[]; .sha256 | test("^sha256:[0-9a-f]{64}$"))
' "$OUTPUT_DIR/official/materialization.json" >/dev/null

python3 "$SCRIPT_DIR/materialization_manifest.py" validate \
  --materialization-manifest "$OUTPUT_DIR/official/materialization.json" \
  --output "$TMP_DIR/materialization-validation.json"
if ! jq -e '
    .profiles.QL.status == "complete"
    and .profiles.QL.reasons == []
    and .profiles.QL.execution_variant == "openllet-ql"
  ' "$TMP_DIR/materialization-validation.json" >/dev/null; then
  jq '.profiles.QL' "$TMP_DIR/materialization-validation.json" >&2
  exit 1
fi

OFFICIAL_GRAPH_HASH="$(jq -r '.graph_sha256' "$OUTPUT_DIR/official/handoff.json")"
test "$OFFICIAL_GRAPH_HASH" = "sha256:$(sha256sum "$OUTPUT_DIR/official/graph.nq" | awk '{print $1}')"
jq -e --arg hash "$OFFICIAL_GRAPH_HASH" '
  .status == "ready"
  and .read_only == true
  and .graph_sha256 == $hash
' "$OUTPUT_DIR/official/handoff.json" >/dev/null

jq -e '
  .status == "complete"
  and .reasons == []
  and .independent_obligations.expected_inferred_count >= 2
  and .independent_obligations.expected_inferred_assertion_counts.class >= 1
  and .independent_obligations.expected_inferred_assertion_counts.object_property >= 1
  and .query_handoff.status == "ready"
' "$OUTPUT_DIR/diagnostic/validation.json" >/dev/null

jq -e '
  .status == "complete"
  and .evidence_scope == "diagnostic_observation_only"
  and .extension.sha256 == $expected
  and (.source.sha256 != .candidate.sha256)
  and .determinism.first_graph_sha256 == .determinism.second_graph_sha256
  and .observations.class.status == "observed"
  and .observations.class.row_count >= 1
  and .observations.object_property.status == "observed"
  and .observations.object_property.row_count >= 1
' --arg expected "sha256:$(sha256sum "$DIAGNOSTIC_DIR/extension.nq" | awk '{print $1}')" \
  "$OUTPUT_DIR/diagnostic/result.json" >/dev/null

if jq -e '[paths(scalars) as $path | ($path | map(tostring) | join(".")) | select(test("answer.?key|semantic.?oracle"; "i"))] | length > 0' \
    "$OUTPUT_DIR/diagnostic/identity.json" \
    "$OUTPUT_DIR/diagnostic/result.json" >/dev/null; then
  echo "diagnostic artifacts unexpectedly contain answer-key or semantic-oracle fields" >&2
  exit 1
fi

test "$(grep -c '^run ' "$DOCKER_LOG")" = "4"
test "$(grep -c -- '--network none' "$DOCKER_LOG")" = "4"
test "$(grep -c -- '--read-only' "$DOCKER_LOG")" = "4"
test "$(grep -c 'tableauxx-openllet-ql@sha256:548e5ecefa8b252f8baec95bfb4e9528363257b653dd79a189aa0f351df937bd' "$DOCKER_LOG")" = "5"
test -z "$(find "$OUTPUT_DIR" -type f -name '*.tar' -print -quit)"
RUNTIME_ATTESTATION="$OUTPUT_DIR/runtime/attestation.json"

PROFILE_CONTEXT="$DATASET_ROOT/profiles/QL"
OPENLLET_TEST_STATE=".openllet-ql-test-${TMP_DIR##*.}"
OPENLLET_TEST_STATE_PATH="$PROFILE_CONTEXT/$OPENLLET_TEST_STATE"
FIRST_QL_QUERY="$(awk -F '\t' 'NR > 1 && $4 ~ /(^|,)QL(,|$)/ {print $1; exit}' "$QUERY_MANIFEST")"
COMMON_ADAPTER_ENV=(
  "OWL2BENCH_QUERY_COMPONENT_MANIFEST=$OUTPUT_DIR/component-manifest.json"
  "OWL2BENCH_QUERY_MATERIALIZATION_MANIFEST=$OUTPUT_DIR/official/materialization.json"
  "OWL2BENCH_QUERY_MANIFEST=$QUERY_MANIFEST"
  "OPENLLET_QL_RUNTIME_ATTESTATION=$RUNTIME_ATTESTATION"
  "OPENLLET_QL_QUERY_BIN=$FAKE_QUERY_BACKEND"
  "OPENLLET_QL_TEST_SCRIPT_DIR=$SCRIPT_DIR"
  "OPENLLET_QL_STATE_DIR=$OPENLLET_TEST_STATE"
  "OWL2BENCH_SPLIT_QL_VARIANT=openllet-ql"
  "OWL2BENCH_OPENLLET_QL_ENGINE=$SCRIPT_DIR/openllet_ql_engine.sh"
)
env "${COMMON_ADAPTER_ENV[@]}" \
  "$SCRIPT_DIR/split_profile_engine.sh" load "$PROFILE_CONTEXT" \
  > "$TMP_DIR/openllet-load.json"
jq -e '
  .status == "success"
  and .profile == "QL"
  and .adapter == "openllet-ql"
' "$TMP_DIR/openllet-load.json" >/dev/null
env "${COMMON_ADAPTER_ENV[@]}" \
  OWL2BENCH_QUERY_MATERIALIZATION_HANDOFF="$OUTPUT_DIR/official/handoff.json" \
  OWL2BENCH_QUERY_MATERIALIZATION_GRAPH_SHA256="$OFFICIAL_GRAPH_HASH" \
  "$SCRIPT_DIR/split_profile_engine.sh" query "$FIRST_QL_QUERY" "$PROFILE_CONTEXT" \
  > "$TMP_DIR/openllet-query.tsv"
test -s "$TMP_DIR/openllet-query.tsv"
jq -e '
  .adapter == "openllet-ql"
  and .profile == "QL"
  and .component_manifest.profile.execution_variant == "openllet-ql"
  and .materialization_manifest.profile.execution_variant == "openllet-ql"
  and .output_graph.sha256 == $graph_hash
  and .ruleset.conformance_boundary == "Openllet Direct Semantics over OWL 2 QL input"
' --arg graph_hash "$OFFICIAL_GRAPH_HASH" \
  "$OPENLLET_TEST_STATE_PATH/provenance.json" >/dev/null
jq -e '
  .provenance_adapters.QL == "openllet-ql"
  and .provenance_relative_paths.QL == ".openllet-ql/provenance.json"
  and .no_profile_fallback == true
' "$SCRIPT_DIR/split-openllet-ql-capabilities.json" >/dev/null

jq --arg ql_provenance "$OPENLLET_TEST_STATE/provenance.json" \
  '.provenance_relative_paths.QL = $ql_provenance' \
  "$SCRIPT_DIR/split-openllet-ql-capabilities.json" \
  > "$TMP_DIR/split-openllet-capabilities.json"
RUN_ID="openllet-ql-generated-manifest"
env "${COMMON_ADAPTER_ENV[@]}" \
  OWL2BENCH_QUERY_STAGED_DIR="$(dirname "$QUERY_MANIFEST")" \
  OWL2BENCH_QUERY_RESULTS_ROOT="$TMP_DIR/results" \
  OWL2BENCH_QUERY_ENGINE_BIN="$SCRIPT_DIR/split_profile_engine.sh" \
  OWL2BENCH_QUERY_CAPABILITIES_FILE="$TMP_DIR/split-openllet-capabilities.json" \
  OWL2BENCH_QUERY_DATA_DIR="$DATASET_ROOT/profiles" \
  OWL2BENCH_QUERY_SPLIT_STACK=1 \
  OWL2BENCH_QUERY_EXECUTION_ONLY=1 \
  RUN_ID="$RUN_ID" \
  "$SCRIPT_DIR/run.sh" >/dev/null
RUN_DIR="$TMP_DIR/results/$RUN_ID"
jq -e '
  .query_count == 60
  and .profile_case_counts == {DL:22,EL:10,QL:10,RL:18}
  and .result_status_counts.oracle_unavailable == 10
  and .result_status_counts.not_available == 50
  and .profile_gates.QL == "blocked"
  and .query_gate == "blocked"
  and .correctness_gate == "blocked"
  and .performance_claims_allowed == false
' "$RUN_DIR/summary.json" >/dev/null
find "$RUN_DIR" -maxdepth 1 -type f -name '*__QL.json' -print0 \
  | xargs -0 jq -s -e '
    length == 10
    and all(.[];
      .query_profile == "QL"
      and .execution_status == "success"
      and .result_status == "oracle_unavailable"
      and .engine_provenance.adapter == "openllet-ql"
      and .component_manifest.execution_variant == "openllet-ql"
      and .materialization.execution_variant == "openllet-ql"
      and .materialization.derivation.semantics == "Openllet Direct Semantics over OWL 2 QL input")
  ' >/dev/null

cp "$QUERY_MANIFEST" "$TMP_DIR/tampered-query-manifest.tsv"
printf '\n' >> "$TMP_DIR/tampered-query-manifest.tsv"
set +e
FAKE_DOCKER_LOG="$DOCKER_LOG" \
OPENLLET_QL_TEST_IMAGE_INSPECT="$IMAGE_INSPECT" \
python3 "$MATERIALIZATION" run \
  --dataset-root "$DATASET_ROOT" \
  --query-manifest "$TMP_DIR/tampered-query-manifest.tsv" \
  --docker-bin "$FAKE_DOCKER" \
  --query-backend "$FAKE_QUERY_BACKEND" \
  --output-dir "$TMP_DIR/tampered-query-output" \
  > "$TMP_DIR/tampered-query.json"
tampered_query_status="$?"
set -e
test "$tampered_query_status" = "125"
jq -e '
  .status == "not_available"
  and (.reason | contains("query manifest does not match acceptance lock"))
' "$TMP_DIR/tampered-query.json" >/dev/null
else
  python3 "$SCRIPT_DIR/openllet_ql_runtime_identity.py" attest \
    --identity "$RUNTIME_IDENTITY" \
    --image-inspect "$IMAGE_INSPECT" \
    --output "$RUNTIME_ATTESTATION"
  echo "[test-openllet-ql-materialization] accepted corpus not configured; real-corpus e2e skipped" >&2
fi

WITNESS_SOURCE="$TMP_DIR/witness-source.nq"
WITNESS_IRIS="$TMP_DIR/witness-query-iris.txt"
cat > "$WITNESS_SOURCE" <<'NQ'
<https://example.test/WitnessOwner> <http://www.w3.org/2000/01/rdf-schema#subClassOf> _:restriction .
_:restriction <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <http://www.w3.org/2002/07/owl#Restriction> .
_:restriction <http://www.w3.org/2002/07/owl#onProperty> <https://example.test/requiresWitness> .
_:restriction <http://www.w3.org/2002/07/owl#someValuesFrom> <https://example.test/WitnessValue> .
<https://example.test/subject> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <https://example.test/WitnessOwner> .
NQ
printf '%s\n' 'https://example.test/requiresWitness' > "$WITNESS_IRIS"
set +e
FAKE_DOCKER_LOG="$DOCKER_LOG" \
OPENLLET_QL_TEST_IMAGE_INSPECT="$IMAGE_INSPECT" \
OPENLLET_QL_DOCKER_BIN="$FAKE_DOCKER" \
OPENLLET_QL_RUNTIME_ATTESTATION="$RUNTIME_ATTESTATION" \
OPENLLET_QL_QUERY_IRIS="$WITNESS_IRIS" \
  "$SCRIPT_DIR/openllet_ql_materializer.sh" export "$WITNESS_SOURCE" \
  > "$TMP_DIR/witness.out" 2> "$TMP_DIR/witness.err"
witness_status="$?"
set -e
test "$witness_status" = "125"
test ! -s "$TMP_DIR/witness.out"
grep -Fq 'generated anonymous witness' "$TMP_DIR/witness.err"

cat >> "$WITNESS_SOURCE" <<'NQ'
<https://example.test/value> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <https://example.test/WitnessValue> .
<https://example.test/subject> <https://example.test/requiresWitness> <https://example.test/value> .
NQ
FAKE_DOCKER_LOG="$DOCKER_LOG" \
OPENLLET_QL_TEST_IMAGE_INSPECT="$IMAGE_INSPECT" \
OPENLLET_QL_DOCKER_BIN="$FAKE_DOCKER" \
OPENLLET_QL_RUNTIME_ATTESTATION="$RUNTIME_ATTESTATION" \
OPENLLET_QL_QUERY_IRIS="$WITNESS_IRIS" \
  "$SCRIPT_DIR/openllet_ql_materializer.sh" export "$WITNESS_SOURCE" \
  > "$TMP_DIR/finite-witness.out" 2> "$TMP_DIR/finite-witness.err"
grep -Fq '<https://example.test/subject> <https://example.test/requiresWitness> <https://example.test/value> .' \
  "$TMP_DIR/finite-witness.out"

if [[ "$RUN_ACCEPTED_E2E" == "1" ]]; then
MISSING_OUTPUT="$TMP_DIR/missing-image-output"
set +e
FAKE_DOCKER_LOG="$DOCKER_LOG" \
OPENLLET_QL_TEST_IMAGE_INSPECT="$IMAGE_INSPECT" \
OPENLLET_QL_TEST_IMAGE_MISSING=1 \
python3 "$MATERIALIZATION" run \
  --dataset-root "$DATASET_ROOT" \
  --query-manifest "$QUERY_MANIFEST" \
  --docker-bin "$FAKE_DOCKER" \
  --query-backend "$FAKE_QUERY_BACKEND" \
  --output-dir "$MISSING_OUTPUT" \
  > "$TMP_DIR/missing-image.json"
missing_image_status="$?"
set -e
test "$missing_image_status" = "125"
jq -e '
  .status == "not_available"
  and (.reason | contains("pinned Openllet image is not available locally"))
' "$TMP_DIR/missing-image.json" >/dev/null

NONDETERMINISTIC_OUTPUT="$TMP_DIR/nondeterministic-output"
set +e
FAKE_DOCKER_LOG="$DOCKER_LOG" \
FAKE_DOCKER_STATE="$TMP_DIR/nondeterministic.state" \
OPENLLET_QL_TEST_IMAGE_INSPECT="$IMAGE_INSPECT" \
OPENLLET_QL_TEST_NONDETERMINISTIC=1 \
python3 "$MATERIALIZATION" run \
  --dataset-root "$DATASET_ROOT" \
  --query-manifest "$QUERY_MANIFEST" \
  --docker-bin "$FAKE_DOCKER" \
  --query-backend "$FAKE_QUERY_BACKEND" \
  --output-dir "$NONDETERMINISTIC_OUTPUT" \
  > "$TMP_DIR/nondeterministic.json"
nondeterministic_status="$?"
set -e
test "$nondeterministic_status" = "1"
jq -e '
  .status == "failed"
  and .official.execution_status == "failed"
  and .official.determinism.status == "failed"
  and (.official.determinism.reason | contains("canonical graph differs"))
' "$TMP_DIR/nondeterministic.json" >/dev/null
fi

echo "[test-openllet-ql-materialization] ok"
