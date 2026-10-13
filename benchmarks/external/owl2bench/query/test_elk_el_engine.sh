#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ADAPTER="$SCRIPT_DIR/elk_el_engine.sh"
TMP_DIR="$(mktemp -d)"
trap 'rm -rf "$TMP_DIR"' EXIT

DATA_ROOT="$TMP_DIR/data"
CONTEXT="$DATA_ROOT/EL"
mkdir -p "$CONTEXT" "$DATA_ROOT/DL" "$DATA_ROOT/QL" "$DATA_ROOT/RL"
CONFIG="$TMP_DIR/elk-config.properties"
SOURCE="$CONTEXT/source.owl"
CANDIDATE_GRAPH="$CONTEXT/candidate.nq"
GRAPH="$CONTEXT/materialized.ttl"
QUERY="$TMP_DIR/query.rq"
ELK_BIN="$TMP_DIR/elk"
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
context = data_root / "EL"
for profile in ("DL", "EL", "QL", "RL"):
    (data_root / profile).mkdir(parents=True, exist_ok=True)
config = root / "elk-config.properties"
source = context / "source.owl"
graph = context / "materialized.ttl"
candidate = context / "candidate.nq"
query = root / "query.rq"
elk = root / "elk"
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
elk.write_text(
    "#!/usr/bin/env bash\n"
    "set -euo pipefail\n"
    "[[ \"${1:-}\" == \"--version\" ]]\n"
    "printf 'ELK 0.5.0\\n'\n",
    encoding="utf-8",
)
elk.chmod(0o755)
query_backend.write_text(
    "#!/usr/bin/env bash\n"
    "set -euo pipefail\n"
    "[[ \"${1:-}\" == \"query\" && $# -eq 3 ]]\n"
    "printf '%s\\n' \"${2:?}\" >> \"${ELK_QUERY_INPUT_TRACE:?}\"\n"
    "printf '%s\\n' \"${3:?}\" >> \"${ELK_QUERY_TRACE:?}\"\n"
    "printf '?x\\n<http://example.test/value>\\n'\n",
    encoding="utf-8",
)
query_backend.chmod(0o755)

def file_hash(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()

def digest(value):
    return "sha256:" + value

elk_hash = file_hash(elk)
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
    if profile == "EL":
        engine_digest = digest(elk_hash)
        query_digest = digest(query_hash)
        config_digest = digest(config_hash)
    else:
        engine_digest = f"sha256:{number:064x}"
        query_digest = f"sha256:{number + 1:064x}"
        config_digest = f"sha256:{number + 2:064x}"
    return {
        "components": {
            "engine": {
                "name": "ELK" if profile == "EL" else f"{profile}-engine",
                "version": "0.5.0",
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
    "manifest_id": "elk-el-test-components",
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
    "manifest_id": "elk-el-test-materialization",
    "profiles": {
        "EL": {
            "status": "complete",
            "profile": "EL",
            "source_ontology": {
                "id": "source-el",
                "path": str(source),
                "sha256": digest(source_hash),
            },
            "candidate_graph": {
                "id": "candidate-el",
                "path": str(candidate),
                "sha256": digest(candidate_hash),
                "immutable": True,
            },
            "component": {
                "name": "ELK-materializer",
                "version": "0.5.0",
                "revision": "el-materializer-revision",
                "artifact_digest": digest(elk_hash),
                "path": str(elk),
            },
            "exporter": {
                "name": "elk-el-exporter",
                "version": "1.0.0",
                "revision": "el-exporter-revision",
                "artifact_digest": digest(elk_hash),
                "path": str(elk),
            },
            "query_backend": {
                "name": "elk-el-query-backend",
                "version": "1.0.0",
                "revision": "el-query-revision",
                "artifact_digest": digest(query_hash),
                "path": str(query_backend),
                "read_only": True,
            },
            "runtime": {
                "id": "elk-runtime",
                "dependencies": [
                    {"name": "java", "version": "11", "digest": digest(elk_hash)}
                ],
            },
            "configuration": {
                "id": "el-config",
                "path": str(config),
                "sha256": digest(config_hash),
            },
            "output_graph": {
                "id": "closure-el",
                "path": str(graph),
                "sha256": digest(graph_hash),
                "immutable": True,
            },
            "derivation": {"version": "materialization-1.0", "revision": "el-derivation-revision"},
            "resource_limits": {"timeout_seconds": 600, "memory": "4GiB", "threads": 1, "cpu": "1", "disk": "10GiB", "network": "test-local", "variant": "owl2bench-test-4GiB"},
            "input_sha256": digest(source_hash),
            "output_sha256": digest(graph_hash),
            "profile_context": {
                "id": "context-el",
                "directory": str(context.resolve()),
            },
            "query_workload": {
                "id": "elk-el-test-workload",
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
export ELK_EL_BIN="$ELK_BIN"
export ELK_EL_CONFIG="$CONFIG"
export ELK_EL_QUERY_BIN="$QUERY_BIN"
export ELK_QUERY_TRACE="$QUERY_TRACE"
export ELK_QUERY_INPUT_TRACE="$QUERY_INPUT_TRACE"

LOAD_OUTPUT="$TMP_DIR/load.json"
"$ADAPTER" load "$CONTEXT" > "$LOAD_OUTPUT"
jq -e '.status == "success" and .profile == "EL" and .adapter == "elk-el"' "$LOAD_OUTPUT" >/dev/null
jq -e '
  .profile == "EL"
  and .engine.name == "ELK"
  and .elk.binary_sha256 == $engine_hash
  and .engine.config_sha256 == $config_hash
  and .query_backend.sha256 == $query_hash
  and .materialization_manifest.profile.status == "complete"
  and .materialization_manifest.profile.output_graph.id == "closure-el"
' --arg engine_hash "sha256:$(sha256sum "$ELK_BIN" | awk '{print $1}')" \
  --arg config_hash "sha256:$(sha256sum "$CONFIG" | awk '{print $1}')" \
  --arg query_hash "sha256:$(sha256sum "$QUERY_BIN" | awk '{print $1}')" \
  "$CONTEXT/.elk-el/provenance.json" >/dev/null

"$ADAPTER" query "$QUERY" "$CONTEXT" > "$TMP_DIR/query.out"
grep -Fxq '<http://example.test/value>' "$TMP_DIR/query.out"
grep -Fxq "$QUERY" "$QUERY_INPUT_TRACE"
test "$(wc -l < "$QUERY_TRACE" | tr -d '[:space:]')" = "1"

if "$ADAPTER" load "$DATA_ROOT/DL" > /dev/null 2> "$TMP_DIR/wrong-context.stderr"; then
  echo "assertion failed: EL adapter accepted a DL context" >&2
  exit 1
else
  test "$?" = "1"
fi

: > "$QUERY_TRACE"
RUN_ROOT="$TMP_DIR/results"
OWL2BENCH_QUERY_DATA_DIR="$DATA_ROOT" \
OWL2BENCH_QUERY_RESULTS_ROOT="$RUN_ROOT" \
OWL2BENCH_QUERY_ENGINE_BIN="$ADAPTER" \
OWL2BENCH_QUERY_CAPABILITIES_FILE="$SCRIPT_DIR/elk-el-capabilities.json" \
OWL2BENCH_QUERY_DATASET_METADATA="" \
RUN_ID="elk-el-campaign" \
  "$SCRIPT_DIR/run.sh" >/dev/null
jq -e '
  .query_count == 60
  and .result_status_counts.oracle_unavailable == 10
  and .result_status_counts.not_available == 50
  and .correctness_gate == "blocked"
' "$RUN_ROOT/elk-el-campaign/summary.json" >/dev/null
jq -e '
  .result_status == "oracle_unavailable"
  and .engine_provenance.adapter == "elk-el"
  and .engine_provenance.engine.name == "ELK"
  and .materialization.status == "complete"
  and .component_manifest.status == "available"
  and .profile_context.provenance_file == $provenance
' --arg provenance "$CONTEXT/.elk-el/provenance.json" \
  "$RUN_ROOT/elk-el-campaign/el_query_001__EL.json" >/dev/null
jq -e '
  .result_status == "not_available"
  and .materialization.status == "not_available"
' "$RUN_ROOT/elk-el-campaign/dl_query_001__DL.json" >/dev/null
test "$(wc -l < "$QUERY_TRACE" | tr -d '[:space:]')" = "10"

echo "[test-owl2bench-elk-el] ok"
