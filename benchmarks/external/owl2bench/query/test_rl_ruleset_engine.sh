#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ADAPTER="$SCRIPT_DIR/rl_ruleset_engine.sh"
TMP_DIR="$(mktemp -d)"
trap 'rm -rf "$TMP_DIR"' EXIT

DATA_ROOT="$TMP_DIR/data"
CONTEXT="$DATA_ROOT/RL"
CONFIG="$TMP_DIR/rl-ruleset.properties"
SOURCE="$CONTEXT/source.owl"
GRAPH="$CONTEXT/materialized.ttl"
QUERY="$TMP_DIR/query.rq"
RULESET_BIN="$TMP_DIR/rl-ruleset"
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
context = data_root / "RL"
for profile in ("DL", "EL", "QL", "RL"):
    (data_root / profile).mkdir(parents=True, exist_ok=True)
config = root / "rl-ruleset.properties"
source = context / "source.owl"
graph = context / "materialized.ttl"
query = root / "query.rq"
ruleset = root / "rl-ruleset"
query_backend = root / "query-backend"

config.write_text("ruleset.test=true\n", encoding="utf-8")
source.write_text(
    "<http://example.test/source> <http://example.test/p> <http://example.test/o> .\n",
    encoding="utf-8",
)
graph.write_text(
    "<http://example.test/value> <http://example.test/p> <http://example.test/o> .\n",
    encoding="utf-8",
)
query.write_text("SELECT DISTINCT ?x WHERE { ?x ?p ?o }\n", encoding="utf-8")
ruleset.write_text(
    "#!/usr/bin/env bash\n"
    "set -euo pipefail\n"
    "[[ \"${1:-}\" == \"--version\" ]]\n"
    "printf 'RL-ruleset-engine 1.0.0\\n'\n",
    encoding="utf-8",
)
ruleset.chmod(0o755)
query_backend.write_text(
    "#!/usr/bin/env bash\n"
    "set -euo pipefail\n"
    "[[ \"${1:-}\" == \"query\" && $# -eq 3 ]]\n"
    "printf '%s\\n' \"${2:?}\" >> \"${RL_QUERY_INPUT_TRACE:?}\"\n"
    "printf '%s\\n' \"${3:?}\" >> \"${RL_QUERY_TRACE:?}\"\n"
    "printf '?x\\n<http://example.test/value>\\n'\n",
    encoding="utf-8",
)
query_backend.chmod(0o755)

def file_hash(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()

def digest(value):
    return "sha256:" + value

ruleset_hash = file_hash(ruleset)
query_hash = file_hash(query_backend)
config_hash = file_hash(config)
source_hash = file_hash(source)
graph_hash = file_hash(graph)
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
    number = profiles.index(profile) + 30
    if profile == "RL":
        engine_digest = digest(ruleset_hash)
        query_digest = digest(query_hash)
        configuration_digest = digest(config_hash)
        engine_name = "RL-ruleset-engine"
        engine_version = "1.0.0"
        ruleset_digest = digest(ruleset_hash)
    else:
        engine_digest = f"sha256:{number:064x}"
        query_digest = f"sha256:{number + 1:064x}"
        configuration_digest = f"sha256:{number + 2:064x}"
        engine_name = f"{profile}-engine"
        engine_version = "1.0.0"
        ruleset_digest = engine_digest
    return {
        "components": {
            "engine": {
                "name": engine_name,
                "version": engine_version,
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
        "ruleset": {
            "id": f"{profile.lower()}-ruleset",
            "version": "1.0.0",
            "revision": f"{profile.lower()}-ruleset-revision",
            "artifact_digest": ruleset_digest,
            "conformance_boundary": f"known-{profile.lower()}-ruleset-boundary",
        },
        "configuration": {"id": f"{profile.lower()}-config", "digest": configuration_digest},
        "runtime": {
            "id": f"{profile.lower()}-runtime",
            "dependencies": [{"name": "ruleset-runtime", "version": "1.0.0", "digest": engine_digest}],
        },
        "credentials": {"status": "not_required"},
        "license": {"status": "not_required"},
        "capability": {"status": "supported", "supports_query": True},
        "resource_limits": {"timeout_seconds": 600, "memory": "4GiB", "threads": 1, "cpu": "unspecified", "disk": "unspecified", "network": "test-local", "variant": "owl2bench-test-4GiB"},
        "dataset": {
            "id": f"anchor-{profile.lower()}",
            "sha256": configuration_digest,
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
    "manifest_id": "rl-ruleset-test-components",
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
    "manifest_id": "rl-ruleset-test-materialization",
    "profiles": {
        "RL": {
            "status": "complete",
            "profile": "RL",
            "source_ontology": {
                "id": "source-rl",
                "path": str(source),
                "sha256": digest(source_hash),
            },
            "component": {
                "name": "RL-ruleset-engine",
                "version": "1.0.0",
                "revision": "rl-materializer-revision",
                "artifact_digest": digest(ruleset_hash),
            },
            "ruleset": {
                "id": "rl-ruleset",
                "version": "1.0.0",
                "revision": "rl-ruleset-revision",
                "artifact_digest": digest(ruleset_hash),
                "conformance_boundary": "known-rl-ruleset-boundary",
            },
            "configuration": {
                "id": "rl-config",
                "path": str(config),
                "sha256": digest(config_hash),
            },
            "output_graph": {
                "id": "closure-rl",
                "path": str(graph),
                "sha256": digest(graph_hash),
            },
            "derivation": {"version": "materialization-1.0", "revision": "rl-derivation-revision"},
            "resource_limits": {"timeout_seconds": 600, "memory": "4GiB", "threads": 1, "cpu": "unspecified", "disk": "unspecified", "network": "test-local", "variant": "owl2bench-test-4GiB"},
            "input_sha256": digest(source_hash),
            "output_sha256": digest(graph_hash),
            "profile_context": {
                "id": "context-rl",
                "directory": str(context.resolve()),
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
export RL_RULESET_BIN="$RULESET_BIN"
export RL_RULESET_CONFIG="$CONFIG"
export RL_RULESET_QUERY_BIN="$QUERY_BIN"
export RL_QUERY_TRACE="$QUERY_TRACE"
export RL_QUERY_INPUT_TRACE="$QUERY_INPUT_TRACE"

LOAD_OUTPUT="$TMP_DIR/load.json"
"$ADAPTER" load "$CONTEXT" > "$LOAD_OUTPUT"
jq -e '.status == "success" and .profile == "RL" and .adapter == "rl-ruleset"' "$LOAD_OUTPUT" >/dev/null
jq -e '
  .profile == "RL"
  and .engine.name == "RL-ruleset-engine"
  and .ruleset.id == "rl-ruleset"
  and .ruleset.conformance_boundary == "known-rl-ruleset-boundary"
  and .materialization_manifest.profile.status == "complete"
  and .materialization_manifest.profile.output_graph.id == "closure-rl"
  and .credential_status == "not_required"
' "$CONTEXT/.rl-ruleset/provenance.json" >/dev/null

"$ADAPTER" query "$QUERY" "$CONTEXT" > "$TMP_DIR/query.out"
grep -Fxq '<http://example.test/value>' "$TMP_DIR/query.out"
grep -Fxq "$QUERY" "$QUERY_INPUT_TRACE"
test "$(wc -l < "$QUERY_TRACE" | tr -d '[:space:]')" = "1"

if OWL2BENCH_PROFILE=QL "$ADAPTER" load "$CONTEXT" > /dev/null 2> "$TMP_DIR/reprofile.stderr"; then
  echo "assertion failed: RL adapter accepted profile override" >&2
  exit 1
else
  test "$?" = "1"
fi

UNSUPPORTED_COMPONENT="$TMP_DIR/components-unsupported.json"
jq '.profiles.RL.capability.status = "unsupported"' "$COMPONENT_MANIFEST" > "$UNSUPPORTED_COMPONENT"
if OWL2BENCH_QUERY_COMPONENT_MANIFEST="$UNSUPPORTED_COMPONENT" "$ADAPTER" load "$CONTEXT" > /dev/null 2> "$TMP_DIR/unsupported.stderr"; then
  echo "assertion failed: unsupported RL capability was accepted" >&2
  exit 1
else
  test "$?" = "123"
fi

MISSING_CREDENTIAL_COMPONENT="$TMP_DIR/components-missing-credential.json"
jq '.profiles.RL.credentials.status = "not_available"' "$COMPONENT_MANIFEST" > "$MISSING_CREDENTIAL_COMPONENT"
if OWL2BENCH_QUERY_COMPONENT_MANIFEST="$MISSING_CREDENTIAL_COMPONENT" "$ADAPTER" load "$CONTEXT" > /dev/null 2> "$TMP_DIR/missing-credential.stderr"; then
  echo "assertion failed: unavailable RL credentials were accepted" >&2
  exit 1
else
  test "$?" = "125"
fi

if "$ADAPTER" load "$DATA_ROOT/QL" > /dev/null 2> "$TMP_DIR/wrong-context.stderr"; then
  echo "assertion failed: RL adapter accepted a QL context" >&2
  exit 1
else
  test "$?" = "1"
fi

: > "$QUERY_TRACE"
RUN_ROOT="$TMP_DIR/results"
OWL2BENCH_QUERY_DATA_DIR="$DATA_ROOT" \
OWL2BENCH_QUERY_RESULTS_ROOT="$RUN_ROOT" \
OWL2BENCH_QUERY_ENGINE_BIN="$ADAPTER" \
OWL2BENCH_QUERY_CAPABILITIES_FILE="$SCRIPT_DIR/rl-ruleset-capabilities.json" \
OWL2BENCH_QUERY_DATASET_METADATA="" \
RUN_ID="rl-ruleset-campaign" \
  "$SCRIPT_DIR/run.sh" >/dev/null
jq -e '
  .query_count == 60
  and .result_status_counts.not_available == 60
  and .correctness_gate == "blocked"
' "$RUN_ROOT/rl-ruleset-campaign/summary.json" >/dev/null
jq -e '
  .result_status == "not_available"
  and .engine_provenance.adapter == "rl-ruleset"
  and .engine_provenance.engine.name == "RL-ruleset-engine"
  and .component_manifest.ruleset.id == "rl-ruleset"
  and .materialization.status == "complete"
  and .component_manifest.status == "available"
' "$RUN_ROOT/rl-ruleset-campaign/rl_query_001__RL.json" >/dev/null
jq -e '
  .result_status == "not_available"
  and .materialization.status == "not_available"
' "$RUN_ROOT/rl-ruleset-campaign/ql_query_001__QL.json" >/dev/null
test ! -s "$QUERY_TRACE"

echo "[test-owl2bench-rl-ruleset] ok"
