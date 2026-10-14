#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ADAPTER="$SCRIPT_DIR/hermit_dl_engine.sh"
TMP_DIR="$(mktemp -d)"
trap 'rm -rf "$TMP_DIR"' EXIT

DATA_ROOT="$TMP_DIR/data"
CONTEXT="$DATA_ROOT/DL"
CONFIG="$TMP_DIR/hermit-config.properties"
SOURCE="$CONTEXT/source.owl"
CANDIDATE_GRAPH="$CONTEXT/candidate.nq"
GRAPH="$CONTEXT/materialized.ttl"
QUERY="$TMP_DIR/query.rq"
HERMIT_BIN="$TMP_DIR/hermit"
QUERY_BIN="$TMP_DIR/query-backend"
QUERY_TRACE="$TMP_DIR/query-backend.trace"
QUERY_INPUT_TRACE="$TMP_DIR/query-backend-input.trace"
QUERY_MANIFEST="$TMP_DIR/query-manifest.tsv"
COMPONENT_MANIFEST="$TMP_DIR/components.json"
MATERIALIZATION_MANIFEST="$TMP_DIR/materialization.json"

python3 - "$TMP_DIR" <<'PY'
import hashlib
import json
import pathlib
import sys

root = pathlib.Path(sys.argv[1])
data_root = root / "data"
context = data_root / "DL"
for profile in ("DL", "EL", "QL", "RL"):
    (data_root / profile).mkdir(parents=True, exist_ok=True)
context.mkdir(parents=True, exist_ok=True)
config = root / "hermit-config.properties"
source = context / "source.owl"
graph = context / "materialized.ttl"
candidate = context / "candidate.nq"
query = root / "query.rq"
hermit = root / "hermit"
query_backend = root / "query-backend"

config.write_text("reasoner.test=true\n", encoding="utf-8")
graph_lines = sorted(
    [
        "<http://example.test/value> <http://example.test/p> <http://example.test/o> .",
        "<http://example.test/value> <http://example.test/dp> \"literal\" .",
        "<http://example.test/Person> <http://www.w3.org/2000/01/rdf-schema#subClassOf> <http://example.test/Agent> .",
        "<http://example.test/p> <http://www.w3.org/2000/01/rdf-schema#subPropertyOf> <http://example.test/related> .",
        "<http://example.test/value> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <http://example.test/Person> .",
    ]
)
graph_text = "\n".join(graph_lines) + "\n"
source.write_text(graph_text, encoding="utf-8")
candidate.write_text(graph_text, encoding="utf-8")
graph.write_text(graph_text, encoding="utf-8")
query.write_text(
    "SELECT DISTINCT ?x WHERE { ?x <http://example.test/p> ?o }\n",
    encoding="utf-8",
)
hermit.write_text(
    "#!/usr/bin/env bash\n"
    "set -euo pipefail\n"
    "[[ \"${1:-}\" == \"--version\" ]]\n"
    "printf 'HermiT 1.4.5.519\\n'\n",
    encoding="utf-8",
)
hermit.chmod(0o755)
query_backend.write_text(
    "#!/usr/bin/env bash\n"
    "set -euo pipefail\n"
    "[[ \"${1:-}\" == \"query\" && $# -eq 3 ]]\n"
    "printf '%s\\n' \"${2:?}\" >> \"${HERMIT_QUERY_INPUT_TRACE:?}\"\n"
    "printf '%s\\n' \"${3:?}\" >> \"${HERMIT_QUERY_TRACE:?}\"\n"
    "printf '?x\\n<http://example.test/value>\\n'\n",
    encoding="utf-8",
)
query_backend.chmod(0o755)

def file_hash(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()

def digest(value):
    return "sha256:" + value

hermit_hash = file_hash(hermit)
query_hash = file_hash(query_backend)
config_hash = file_hash(config)
source_hash = file_hash(source)
graph_hash = file_hash(graph)
candidate_hash = file_hash(candidate)
profiles = ("DL", "EL", "QL", "RL")
counts = {"DL": 22, "EL": 10, "QL": 10, "RL": 18}

query_lines = [
    "staged_query\tsource_query\tquery_id\tprofile\tworkload\tsource_corpus\tsource_revision"
]
for profile in profiles:
    for index in range(1, counts[profile] + 1):
        query_id = f"{profile.lower()}_query_{index:03d}"
        query_lines.append(
            "\t".join(
                [
                    str(query),
                    f"official-source#{query_id}",
                    query_id,
                    profile,
                    "official-text",
                    "owl2bench-test",
                    "test-revision",
                ]
            )
        )
(root / "query-manifest.tsv").write_text("\n".join(query_lines) + "\n", encoding="utf-8")

def component_record(profile):
    number = profiles.index(profile) + 10
    if profile == "DL":
        engine_digest = digest(hermit_hash)
        query_digest = digest(query_hash)
        config_digest = digest(config_hash)
    else:
        engine_digest = f"sha256:{number:064x}"
        query_digest = f"sha256:{number + 1:064x}"
        config_digest = f"sha256:{number + 2:064x}"
    return {
        "components": {
            "engine": {
                "name": "HermiT" if profile == "DL" else f"{profile}-engine",
                "version": "1.4.5.519",
                "revision": f"{profile.lower()}-revision",
                "artifact_digest": engine_digest,
            },
            "materializer": {
                "name": f"{profile.lower()}-materializer",
                "version": "1.0.0",
                "revision": f"{profile.lower()}-materializer-revision",
                "artifact_digest": engine_digest,
            },
            "query": {
                "name": f"{profile.lower()}-query",
                "version": "1.0.0",
                "revision": f"{profile.lower()}-query-revision",
                "artifact_digest": query_digest,
            },
        },
        "configuration": {"id": f"{profile.lower()}-config", "digest": config_digest},
        "runtime": {
            "id": f"{profile.lower()}-runtime",
            "dependencies": [{"name": "java", "version": "11", "digest": engine_digest}],
        },
        "credentials": {"status": "not_required"},
        "license": {"status": "not_required"},
        "capability": {"status": "supported", "supports_query": True},
        "resource_limits": {"timeout_seconds": 600, "memory": "4GiB", "threads": 1, "cpu": "unspecified", "disk": "unspecified", "network": "test-local", "variant": "owl2bench-test-4GiB"},
        "dataset": {
            "id": f"anchor-{profile.lower()}",
            "sha256": config_digest,
            "tbox_id": f"tbox-{profile.lower()}",
            "tbox_sha256": engine_digest,
            "abox_id": f"abox-{profile.lower()}",
            "abox_sha256": query_digest,
        },
        "generator": {"id": "owl2bench-generator", "revision": "test-generator", "seed": 1},
        "profile_context": {
            "id": f"context-{profile.lower()}",
            "directory": str((data_root / profile).resolve()),
        },
    }

component_document = {
    "schema_version": 1,
    "manifest_id": "hermit-dl-test-components",
    "matrix": {
        "expansion": "query_profile",
        "expected_query_count": 60,
        "expected_profile_case_counts": counts,
    },
    "profiles": {profile: component_record(profile) for profile in profiles},
}
(root / "components.json").write_text(
    json.dumps(component_document, indent=2) + "\n", encoding="utf-8"
)

materialization_document = {
    "schema_version": 1,
    "manifest_id": "hermit-dl-test-materialization",
    "profiles": {
        "DL": {
            "status": "complete",
            "profile": "DL",
            "source_ontology": {
                "id": "source-dl",
                "path": str(source),
                "sha256": digest(source_hash),
            },
            "candidate_graph": {
                "id": "candidate-dl",
                "path": str(candidate),
                "sha256": digest(candidate_hash),
                "immutable": True,
            },
            "component": {
                "name": "HermiT-materializer",
                "version": "1.4.5.519",
                "revision": "dl-materializer-revision",
                "artifact_digest": digest(hermit_hash),
                "path": str(hermit),
            },
            "exporter": {
                "name": "hermit-dl-exporter",
                "version": "1.0.0",
                "revision": "dl-exporter-revision",
                "artifact_digest": digest(hermit_hash),
                "path": str(hermit),
            },
            "query_backend": {
                "name": "hermit-dl-query-backend",
                "version": "1.0.0",
                "revision": "dl-query-revision",
                "artifact_digest": digest(query_hash),
                "path": str(query_backend),
                "read_only": True,
            },
            "runtime": {
                "id": "hermit-runtime",
                "dependencies": [
                    {"name": "java", "version": "11", "digest": digest(hermit_hash)}
                ],
            },
            "configuration": {
                "id": "dl-config",
                "path": str(config),
                "sha256": digest(config_hash),
            },
            "output_graph": {
                "id": "closure-dl",
                "path": str(graph),
                "sha256": digest(graph_hash),
                "immutable": True,
            },
            "derivation": {"version": "materialization-1.0", "revision": "dl-derivation-revision"},
            "resource_limits": {"timeout_seconds": 600, "memory": "4GiB", "threads": 1, "cpu": "1", "disk": "10GiB", "network": "test-local", "variant": "owl2bench-test-4GiB"},
            "input_sha256": digest(source_hash),
            "output_sha256": digest(graph_hash),
            "profile_context": {
                "id": "context-dl",
                "directory": str(context.resolve()),
            },
            "query_workload": {
                "id": "hermit-dl-test-workload",
                "query_count": 1,
                "queries": [{"path": str(query), "sha256": digest(file_hash(query))}],
            },
            "graph_contract": {
                "canonicalizer": {"name": "URDNA2015", "version": "2015", "serialization": "N-Quads"},
                "closure_scope": "finite_query_complete",
                "source_included": True,
                "required_assertion_families": ["class", "object_property", "data_property", "tbox", "rbox"],
                "anonymous_witness_policy": "not_exported",
                "independent_obligation_check_required": True,
                "two_run_determinism_required": True,
            },
            "determinism": {
                "status": "complete",
                "runs": 2,
                "first_graph": {"path": str(graph), "sha256": digest(graph_hash)},
                "second_graph": {"path": str(graph), "sha256": digest(graph_hash)},
            },
        }
    },
}
(root / "materialization.json").write_text(
    json.dumps(materialization_document, indent=2) + "\n", encoding="utf-8"
)
PY

export OWL2BENCH_QUERY_MANIFEST="$QUERY_MANIFEST"
export OWL2BENCH_QUERY_COMPONENT_MANIFEST="$COMPONENT_MANIFEST"
export OWL2BENCH_QUERY_MATERIALIZATION_MANIFEST="$MATERIALIZATION_MANIFEST"
export HERMIT_DL_BIN="$HERMIT_BIN"
export HERMIT_DL_CONFIG="$CONFIG"
export HERMIT_DL_QUERY_BIN="$QUERY_BIN"
export HERMIT_QUERY_TRACE="$QUERY_TRACE"
export HERMIT_QUERY_INPUT_TRACE="$QUERY_INPUT_TRACE"

LOAD_OUTPUT="$TMP_DIR/load.json"
"$ADAPTER" load "$CONTEXT" > "$LOAD_OUTPUT"
jq -e '.status == "success" and .profile == "DL" and .adapter == "hermit-dl"' "$LOAD_OUTPUT" >/dev/null
jq -e '
  .profile == "DL"
  and .hermit.binary_sha256 == $hermit_hash
  and .hermit.config_sha256 == $config_hash
  and .query_backend.sha256 == $query_hash
  and .materialization_manifest.profile.status == "complete"
  and .materialization_manifest.profile.output_graph.id == "closure-dl"
  and .credential_status == "not_required"
  and .license_status == "not_required"
' --arg hermit_hash "sha256:$(sha256sum "$HERMIT_BIN" | awk '{print $1}')" \
  --arg config_hash "sha256:$(sha256sum "$CONFIG" | awk '{print $1}')" \
  --arg query_hash "sha256:$(sha256sum "$QUERY_BIN" | awk '{print $1}')" \
  "$CONTEXT/.hermit-dl/provenance.json" >/dev/null

"$ADAPTER" query "$QUERY" "$CONTEXT" > "$TMP_DIR/query.out"
grep -Fxq '<http://example.test/value>' "$TMP_DIR/query.out"
test "$(wc -l < "$QUERY_TRACE" | tr -d '[:space:]')" = "1"
grep -Fxq "$QUERY" "$QUERY_INPUT_TRACE"

HANDOFF="$TMP_DIR/materialization-handoff.json"
GRAPH_HASH="sha256:$(sha256sum "$GRAPH" | awk '{print $1}')"
jq -n --arg graph "$GRAPH" --arg hash "$GRAPH_HASH" \
  '{status:"ready",profile:"DL",graph_path:$graph,graph_sha256:$hash,read_only:true}' \
  > "$HANDOFF"
OWL2BENCH_QUERY_MATERIALIZATION_HANDOFF="$HANDOFF" \
OWL2BENCH_QUERY_MATERIALIZATION_GRAPH_SHA256="$GRAPH_HASH" \
  "$ADAPTER" query "$QUERY" "$CONTEXT" > "$TMP_DIR/handoff-query.out"
grep -Fxq '<http://example.test/value>' "$TMP_DIR/handoff-query.out"

BAD_HANDOFF="$TMP_DIR/bad-materialization-handoff.json"
jq '.graph_sha256 = "sha256:0000000000000000000000000000000000000000000000000000000000000000"' \
  "$HANDOFF" > "$BAD_HANDOFF"
if OWL2BENCH_QUERY_MATERIALIZATION_HANDOFF="$BAD_HANDOFF" \
  "$ADAPTER" query "$QUERY" "$CONTEXT" > /dev/null 2> "$TMP_DIR/bad-handoff.stderr"; then
  echo "assertion failed: adapter accepted an unvalidated graph digest" >&2
  exit 1
else
  test "$?" = "1"
fi

if "$ADAPTER" load "$DATA_ROOT/EL" > /dev/null 2> "$TMP_DIR/wrong-context.stderr"; then
  echo "assertion failed: DL adapter accepted an EL context" >&2
  exit 1
else
  test "$?" = "1"
fi

UNSUPPORTED_COMPONENT="$TMP_DIR/components-unsupported.json"
jq '.profiles.DL.capability.status = "unsupported"' "$COMPONENT_MANIFEST" > "$UNSUPPORTED_COMPONENT"
if OWL2BENCH_QUERY_COMPONENT_MANIFEST="$UNSUPPORTED_COMPONENT" "$ADAPTER" load "$CONTEXT" > /dev/null 2> "$TMP_DIR/unsupported.stderr"; then
  echo "assertion failed: unsupported DL capability was accepted" >&2
  exit 1
else
  test "$?" = "123"
fi

MISSING_PIN_COMPONENT="$TMP_DIR/components-missing-pin.json"
jq '.profiles.DL.components.engine.version = ""' "$COMPONENT_MANIFEST" > "$MISSING_PIN_COMPONENT"
if OWL2BENCH_QUERY_COMPONENT_MANIFEST="$MISSING_PIN_COMPONENT" "$ADAPTER" load "$CONTEXT" > /dev/null 2> "$TMP_DIR/missing-pin.stderr"; then
  echo "assertion failed: missing HermiT pin was accepted" >&2
  exit 1
else
  test "$?" = "125"
fi

MISSING_CREDENTIAL_COMPONENT="$TMP_DIR/components-missing-credential.json"
jq '.profiles.DL.credentials.status = "not_available"' "$COMPONENT_MANIFEST" > "$MISSING_CREDENTIAL_COMPONENT"
if OWL2BENCH_QUERY_COMPONENT_MANIFEST="$MISSING_CREDENTIAL_COMPONENT" "$ADAPTER" load "$CONTEXT" > /dev/null 2> "$TMP_DIR/missing-credential.stderr"; then
  echo "assertion failed: unavailable credentials were accepted" >&2
  exit 1
else
  test "$?" = "125"
fi

MISSING_GRAPH_MATERIALIZATION="$TMP_DIR/materialization-missing-graph.json"
jq '.profiles.DL.output_graph.path = "/path/that/does/not/exist.ttl"' "$MATERIALIZATION_MANIFEST" > "$MISSING_GRAPH_MATERIALIZATION"
if OWL2BENCH_QUERY_MATERIALIZATION_MANIFEST="$MISSING_GRAPH_MATERIALIZATION" "$ADAPTER" load "$CONTEXT" > /dev/null 2> "$TMP_DIR/missing-graph.stderr"; then
  echo "assertion failed: missing materialized graph was accepted" >&2
  exit 1
else
  test "$?" = "125"
fi

mkdir -p "$CONTEXT/.konclude-unified"
jq -n \
  --arg context "$CONTEXT" \
  '{adapter:"konclude-unified",profile:"DL",context_identity:("profile=DL;directory=" + $context)}' \
  > "$CONTEXT/.konclude-unified/provenance.json"

FAILING_QUERY_BIN="$TMP_DIR/query-backend-failing"
cat > "$FAILING_QUERY_BIN" <<'ENGINE'
#!/usr/bin/env bash
set -euo pipefail
exit 7
ENGINE
chmod +x "$FAILING_QUERY_BIN"
FAILING_COMPONENT="$TMP_DIR/components-failing-query.json"
jq --arg digest "sha256:$(sha256sum "$FAILING_QUERY_BIN" | awk '{print $1}')" \
  '.profiles.DL.components.query.artifact_digest = $digest' "$COMPONENT_MANIFEST" > "$FAILING_COMPONENT"
HERMIT_DL_QUERY_BIN="$FAILING_QUERY_BIN" \
OWL2BENCH_QUERY_COMPONENT_MANIFEST="$FAILING_COMPONENT" \
  "$ADAPTER" load "$CONTEXT" > /dev/null
if HERMIT_DL_QUERY_BIN="$FAILING_QUERY_BIN" \
  OWL2BENCH_QUERY_COMPONENT_MANIFEST="$FAILING_COMPONENT" \
  "$ADAPTER" query "$QUERY" "$CONTEXT" > /dev/null 2> "$TMP_DIR/failing-query.stderr"; then
  echo "assertion failed: query process failure was accepted" >&2
  exit 1
else
  test "$?" = "7"
fi

SLOW_QUERY_BIN="$TMP_DIR/query-backend-slow"
cat > "$SLOW_QUERY_BIN" <<'ENGINE'
#!/usr/bin/env bash
set -euo pipefail
sleep 2
printf '?x\n<http://example.test/value>\n'
ENGINE
chmod +x "$SLOW_QUERY_BIN"
SLOW_COMPONENT="$TMP_DIR/components-slow-query.json"
jq --arg digest "sha256:$(sha256sum "$SLOW_QUERY_BIN" | awk '{print $1}')" \
  '.profiles.DL.components.query.artifact_digest = $digest' "$COMPONENT_MANIFEST" > "$SLOW_COMPONENT"
HERMIT_DL_QUERY_BIN="$SLOW_QUERY_BIN" \
OWL2BENCH_QUERY_COMPONENT_MANIFEST="$SLOW_COMPONENT" \
  "$ADAPTER" load "$CONTEXT" > /dev/null
if timeout 1 env \
  HERMIT_DL_QUERY_BIN="$SLOW_QUERY_BIN" \
  OWL2BENCH_QUERY_COMPONENT_MANIFEST="$SLOW_COMPONENT" \
  "$ADAPTER" query "$QUERY" "$CONTEXT" > /dev/null 2> "$TMP_DIR/slow-query.stderr"; then
  echo "assertion failed: query timeout was accepted" >&2
  exit 1
else
  test "$?" = "124"
fi

MALFORMED_QUERY_BIN="$TMP_DIR/query-backend-malformed"
cat > "$MALFORMED_QUERY_BIN" <<'ENGINE'
#!/usr/bin/env bash
set -euo pipefail
printf 'not a SPARQL result document\n'
ENGINE
chmod +x "$MALFORMED_QUERY_BIN"
MALFORMED_COMPONENT="$TMP_DIR/components-malformed-query.json"
jq --arg digest "sha256:$(sha256sum "$MALFORMED_QUERY_BIN" | awk '{print $1}')" \
  '.profiles.DL.components.query.artifact_digest = $digest' "$COMPONENT_MANIFEST" > "$MALFORMED_COMPONENT"
HERMIT_DL_QUERY_BIN="$MALFORMED_QUERY_BIN" \
OWL2BENCH_QUERY_COMPONENT_MANIFEST="$MALFORMED_COMPONENT" \
  "$ADAPTER" load "$CONTEXT" > /dev/null
HERMIT_DL_QUERY_BIN="$MALFORMED_QUERY_BIN" \
OWL2BENCH_QUERY_COMPONENT_MANIFEST="$MALFORMED_COMPONENT" \
  "$ADAPTER" query "$QUERY" "$CONTEXT" > "$TMP_DIR/malformed-query.out"
grep -Fxq 'not a SPARQL result document' "$TMP_DIR/malformed-query.out"
MALFORMED_RUN_ROOT="$TMP_DIR/malformed-query-results"
OWL2BENCH_QUERY_DATA_DIR="$DATA_ROOT" \
OWL2BENCH_QUERY_RESULTS_ROOT="$MALFORMED_RUN_ROOT" \
OWL2BENCH_QUERY_ENGINE_BIN="$ADAPTER" \
OWL2BENCH_QUERY_CAPABILITIES_FILE="$SCRIPT_DIR/hermit-dl-capabilities.json" \
OWL2BENCH_QUERY_COMPONENT_MANIFEST="$MALFORMED_COMPONENT" \
HERMIT_DL_QUERY_BIN="$MALFORMED_QUERY_BIN" \
RUN_ID="hermit-dl-malformed-query" \
  "$SCRIPT_DIR/run.sh" >/dev/null
jq -e '
  .query_count == 60
  and .result_status_counts.failed == 22
  and .result_status_counts.not_available == 38
  and .correctness_gate == "blocked"
' "$MALFORMED_RUN_ROOT/hermit-dl-malformed-query/summary.json" >/dev/null
jq -e '
  .result_status == "failed"
  and .canonicalization.status == "failed"
  and .materialization.status == "complete"
' "$MALFORMED_RUN_ROOT/hermit-dl-malformed-query/dl_query_001__DL.json" >/dev/null

: > "$QUERY_TRACE"
RUN_ROOT="$TMP_DIR/results"
OWL2BENCH_QUERY_DATA_DIR="$DATA_ROOT" \
OWL2BENCH_QUERY_RESULTS_ROOT="$RUN_ROOT" \
OWL2BENCH_QUERY_ENGINE_BIN="$ADAPTER" \
OWL2BENCH_QUERY_CAPABILITIES_FILE="$SCRIPT_DIR/hermit-dl-capabilities.json" \
OWL2BENCH_QUERY_DATASET_METADATA="" \
RUN_ID="hermit-dl-campaign" \
  "$SCRIPT_DIR/run.sh" >/dev/null
jq -e '
  .query_count == 60
  and .result_status_counts.oracle_unavailable == 22
  and .result_status_counts.not_available == 38
  and .correctness_gate == "blocked"
' "$RUN_ROOT/hermit-dl-campaign/summary.json" >/dev/null
jq -e '
  .result_status == "oracle_unavailable"
  and .engine_provenance.adapter == "hermit-dl"
  and .materialization.status == "complete"
  and .component_manifest.status == "available"
  and .profile_context.provenance_file == $provenance
' --arg provenance "$CONTEXT/.hermit-dl/provenance.json" \
  "$RUN_ROOT/hermit-dl-campaign/dl_query_001__DL.json" >/dev/null
jq -e '
  .result_status == "not_available"
  and .materialization.status == "not_available"
' "$RUN_ROOT/hermit-dl-campaign/el_query_001__EL.json" >/dev/null
test "$(wc -l < "$QUERY_TRACE" | tr -d '[:space:]')" = "22"

echo "[test-owl2bench-hermit-dl] ok"
