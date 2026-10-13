#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
TMP_DIR="$(mktemp -d "${TMPDIR:-/tmp}/tableauxx-profile-query-smoke-test.XXXXXX")"
trap 'rm -rf "$TMP_DIR"' EXIT
STACK_DIR="$TMP_DIR/stack"
RESULTS_ROOT="$TMP_DIR/results"
mkdir -p "$STACK_DIR"

python3 - "$TMP_DIR" "$SCRIPT_DIR" <<'PY'
import hashlib
import json
import pathlib
import sys

root = pathlib.Path(sys.argv[1])
sys.path.insert(0, sys.argv[2])
from rdf_canonicalization import canonicalize_file, write_canonical
query = root / "query.rq"
query.write_text("SELECT ?x WHERE { ?x <urn:query> ?o }\n", encoding="utf-8")
query_manifest = root / "query-manifest.tsv"
rows = ["staged_query\tsource_query\tquery_id\tprofile\tworkload\tsource_corpus\tsource_revision"]
for profile, count in (("DL", 22), ("EL", 10), ("QL", 10), ("RL", 18)):
    for index in range(1, count + 1):
        rows.append(f"{query}\tofficial-{profile}-{index}\t{profile.lower()}-{index:03d}\t{profile}\tofficial-text\ttest-corpus\ttest-revision")
query_manifest.write_text("\n".join(rows) + "\n", encoding="utf-8")

def digest(path):
    return "sha256:" + hashlib.sha256(path.read_bytes()).hexdigest()

query_file_hash = digest(query)

profiles = {}
materializations = {}
for index, profile in enumerate(("DL", "EL", "QL", "RL"), start=1):
    context = root / "contexts" / profile
    context.mkdir(parents=True)
    source = context / "source.nq"
    graph = context / "graph.nq"
    component_artifact = context / "component.bin"
    exporter_artifact = context / "exporter.bin"
    query_artifact = context / "query.bin"
    config_artifact = context / "config.properties"
    content = (
        f"<urn:{profile}:i> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <urn:{profile}:C> .\n"
        f"<urn:{profile}:i> <urn:{profile}:p> <urn:{profile}:j> .\n"
        f"<urn:{profile}:i> <urn:{profile}:dp> \"value\" .\n"
        f"<urn:{profile}:C> <http://www.w3.org/2000/01/rdf-schema#subClassOf> <urn:{profile}:D> .\n"
        f"<urn:{profile}:p> <http://www.w3.org/2000/01/rdf-schema#subPropertyOf> <urn:{profile}:q> .\n"
    )
    source.write_text(content, encoding="utf-8")
    graph.write_text(content, encoding="utf-8")
    write_canonical(graph, canonicalize_file(graph))
    component_artifact.write_text(f"component {profile}\n", encoding="utf-8")
    exporter_artifact.write_text(f"exporter {profile}\n", encoding="utf-8")
    query_artifact.write_text(f"query {profile}\n", encoding="utf-8")
    config_artifact.write_text(f"config {profile}\n", encoding="utf-8")
    source_hash = digest(source)
    graph_hash = digest(graph)
    component_hash = digest(component_artifact)
    exporter_hash = digest(exporter_artifact)
    query_hash = digest(query_artifact)
    config_hash = digest(config_artifact)
    component = {"name": f"{profile}-materializer", "version": "1.0", "revision": f"{profile}-r1", "artifact_digest": component_hash, "path": str(component_artifact)}
    exporter = {"name": f"{profile}-exporter", "version": "1.0", "revision": f"{profile}-exporter-r1", "artifact_digest": exporter_hash, "path": str(exporter_artifact)}
    query_backend = {"name": f"{profile}-query", "version": "1.0", "revision": f"{profile}-query-r1", "artifact_digest": query_hash, "path": str(query_artifact), "read_only": True}
    runtime = {"id": f"runtime-{profile}", "dependencies": [{"name": "runtime", "version": "1.0", "digest": component_hash}]}
    configuration = {"id": f"config-{profile}", "path": str(config_artifact), "sha256": config_hash}
    ruleset = {"id": f"ruleset-{profile}", "version": "1.0", "revision": f"{profile}-ruleset-r1", "artifact_digest": component_hash, "conformance_boundary": "fixture"}
    profiles[profile] = {
        "components": {
            "engine": exporter,
            "materializer": component,
            "query": query_backend,
        },
        "ruleset": ruleset,
        "configuration": {"id": configuration["id"], "path": configuration["path"], "digest": config_hash},
        "runtime": runtime,
        "credentials": {"status": "not_required"},
        "license": {"status": "not_required"},
        "capability": {"status": "supported", "supports_query": True},
        "resource_limits": {"timeout_seconds": 60, "memory": "1GiB", "threads": 1, "cpu": "unspecified", "disk": "unspecified", "network": "test-local", "variant": "owl2bench-test-1GiB"},
        "dataset": {"id": f"dataset-{profile}", "sha256": source_hash, "tbox_id": f"tbox-{profile}", "tbox_sha256": component_hash, "abox_id": f"abox-{profile}", "abox_sha256": config_hash},
        "generator": {"id": "OWL2Bench", "revision": "generator-r1", "seed": 1, "configuration_id": "generator-config", "configuration_sha256": config_hash},
        "profile_context": {"id": f"context-{profile}", "directory": str(context)},
    }
    materializations[profile] = {
        "profile": profile,
        "status": "complete",
        "source_ontology": {"id": f"source-{profile}", "path": str(source), "sha256": source_hash},
        "candidate_graph": {"id": f"candidate-{profile}", "path": str(graph), "sha256": graph_hash, "immutable": True},
        "component": component,
        "exporter": exporter,
        "query_backend": query_backend,
        "ruleset": ruleset if profile in {"QL", "RL"} else {},
        "configuration": configuration,
        "runtime": runtime,
        "output_graph": {"id": f"graph-{profile}", "path": str(graph), "sha256": graph_hash, "immutable": True},
        "derivation": {"version": "fixture-materialization/1", "revision": f"{profile}-derivation-r1"},
        "resource_limits": {"timeout_seconds": 60, "memory": "1GiB", "threads": 1, "cpu": "unspecified", "disk": "unspecified", "network": "test-local", "variant": "owl2bench-test-1GiB"},
        "input_sha256": source_hash,
        "output_sha256": graph_hash,
        "profile_context": {"id": f"context-{profile}", "directory": str(context)},
        "query_workload": {"id": f"fixture-query-workload-{profile}", "query_count": 1, "queries": [{"path": str(query), "sha256": query_file_hash}]},
        "graph_contract": {"canonicalizer": {"name": "URDNA2015", "version": "2015", "serialization": "N-Quads"}, "closure_scope": "finite_query_complete", "source_included": True, "required_assertion_families": ["class", "object_property", "data_property", "tbox", "rbox"], "anonymous_witness_policy": "not_exported", "independent_obligation_check_required": True, "two_run_determinism_required": True},
        "determinism": {"status": "complete", "runs": 2, "first_graph": {"path": str(graph), "sha256": graph_hash}, "second_graph": {"path": str(graph), "sha256": graph_hash}},
    }

(root / "stack" / "component.json").write_text(json.dumps({
    "schema_version": 1,
    "manifest_id": "fixture-component-stack",
    "matrix": {"expansion": "query_profile", "expected_query_count": 60, "expected_profile_case_counts": {"DL": 22, "EL": 10, "QL": 10, "RL": 18}},
    "profiles": profiles,
}, indent=2) + "\n", encoding="utf-8")
(root / "stack" / "materialization.json").write_text(json.dumps({"schema_version": 1, "manifest_id": "fixture-materialization-stack", "profiles": materializations}, indent=2) + "\n", encoding="utf-8")
component_path = root / "stack" / "component.json"
materialization_path = root / "stack" / "materialization.json"
dataset_manifest_path = root / "dataset-manifest.json"
dataset_manifest_path.write_text("{\"status\":\"complete\"}\n", encoding="utf-8")
validation_path = root / "stack" / "stack-validation.json"
validation_path.write_text(json.dumps({
    "status": "complete",
    "graph_validations": {
        profile: {"status": "complete"}
        for profile in ("DL", "EL", "QL", "RL")
    },
}, indent=2) + "\n", encoding="utf-8")
(root / "stack" / "stack.json").write_text(json.dumps({
    "schema_version": 1,
    "stack_id": "fixture-profile-stack",
    "assembly_status": "complete",
    "status": "complete",
    "component_manifest": {"path": str(component_path), "sha256": digest(component_path)},
    "materialization_manifest": {"path": str(materialization_path), "sha256": digest(materialization_path)},
    "dataset_manifest": {"path": str(dataset_manifest_path), "sha256": digest(dataset_manifest_path)},
    "validation": {"path": str(validation_path), "sha256": digest(validation_path)},
    "profiles": {
        profile: {
            "component_status": "available",
            "materialization_status": "complete",
            "graph_validation_status": "complete",
            "status": "complete",
        }
        for profile in ("DL", "EL", "QL", "RL")
    },
}, indent=2) + "\n", encoding="utf-8")
PY

cat > "$TMP_DIR/fake-query-engine" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
[[ "${1:-}" == query && $# -eq 2 && -r "${2:-}" ]]
printf '?x\n<urn:fixture>\n'
EOF
chmod +x "$TMP_DIR/fake-query-engine"
cat > "$TMP_DIR/capabilities.json" <<'EOF'
{"supports_query":true,"profiles":["ALL"],"data_mode":"query-file-only","engine":"fixture-query","query_result_format":"tsv","component_manifest_required":true,"materialization_manifest_required":true}
EOF

RUN_ID=profile_smoke_fixture \
OWL2BENCH_PROFILE_STACK_DIR="$STACK_DIR" \
OWL2BENCH_PROFILE_QUERY_MANIFEST="$TMP_DIR/query-manifest.tsv" \
OWL2BENCH_PROFILE_RESULTS_ROOT="$RESULTS_ROOT" \
OWL2BENCH_QUERY_ENGINE_BIN="$TMP_DIR/fake-query-engine" \
OWL2BENCH_QUERY_CAPABILITIES_FILE="$TMP_DIR/capabilities.json" \
  "$SCRIPT_DIR/run_profile_query_smoke.sh"

RUN_DIR="$RESULTS_ROOT/profile_smoke_fixture"
jq -e '
  .query_count == 60
  and .profile_case_counts == {DL:22,EL:10,QL:10,RL:18}
  and .query_gate == "blocked"
  and .performance_claims_allowed == false
  and (.result_status_counts.passed // 0) == 0
  and (.result_status_counts.wrong // 0) == 0
  and .result_status_counts.oracle_unavailable == 60
' "$RUN_DIR/summary.json" >/dev/null
for profile in DL EL QL RL; do
  test -s "$RUN_DIR/profiles/$profile/report.md"
  test -s "$RUN_DIR/profiles/$profile/summary.json"
done
jq -e '.status == "valid" and .result_count == 60' "$RUN_DIR/provenance/profile_result_contract.json" >/dev/null

BAD_STACK_DIR="$TMP_DIR/bad-stack"
mkdir -p "$BAD_STACK_DIR"
cp "$STACK_DIR/component.json" "$BAD_STACK_DIR/component.json"
cp "$STACK_DIR/materialization.json" "$BAD_STACK_DIR/materialization.json"
cp "$STACK_DIR/stack-validation.json" "$BAD_STACK_DIR/stack-validation.json"
cp "$TMP_DIR/dataset-manifest.json" "$BAD_STACK_DIR/dataset-manifest.json"
BAD_COMPONENT_SHA256="sha256:$(sha256sum "$BAD_STACK_DIR/component.json" | awk '{print $1}')"
BAD_MATERIALIZATION_SHA256="sha256:$(sha256sum "$BAD_STACK_DIR/materialization.json" | awk '{print $1}')"
BAD_VALIDATION_SHA256="sha256:$(sha256sum "$BAD_STACK_DIR/stack-validation.json" | awk '{print $1}')"
BAD_DATASET_MANIFEST_SHA256="sha256:$(sha256sum "$BAD_STACK_DIR/dataset-manifest.json" | awk '{print $1}')"
jq \
  --arg component_path "$BAD_STACK_DIR/component.json" \
  --arg component_sha256 "$BAD_COMPONENT_SHA256" \
  --arg materialization_path "$BAD_STACK_DIR/materialization.json" \
  --arg materialization_sha256 "$BAD_MATERIALIZATION_SHA256" \
  --arg validation_path "$BAD_STACK_DIR/stack-validation.json" \
  --arg validation_sha256 "$BAD_VALIDATION_SHA256" \
  --arg dataset_manifest_path "$BAD_STACK_DIR/dataset-manifest.json" \
  --arg dataset_manifest_sha256 "$BAD_DATASET_MANIFEST_SHA256" \
  '.component_manifest = {path:$component_path,sha256:$component_sha256}
   | .materialization_manifest = {path:$materialization_path,sha256:$materialization_sha256}
   | .validation = {path:$validation_path,sha256:$validation_sha256}
   | .dataset_manifest = {path:$dataset_manifest_path,sha256:$dataset_manifest_sha256}
   | .profiles.RL.graph_validation_status = "failed"
   | .profiles.RL.status = "failed"' \
  "$STACK_DIR/stack.json" > "$BAD_STACK_DIR/stack.json"
if RUN_ID=bad_profile_smoke \
  OWL2BENCH_PROFILE_STACK_DIR="$BAD_STACK_DIR" \
  OWL2BENCH_PROFILE_QUERY_MANIFEST="$TMP_DIR/query-manifest.tsv" \
  OWL2BENCH_PROFILE_RESULTS_ROOT="$TMP_DIR/bad-results" \
  OWL2BENCH_QUERY_ENGINE_BIN="$TMP_DIR/fake-query-engine" \
  OWL2BENCH_QUERY_CAPABILITIES_FILE="$TMP_DIR/capabilities.json" \
    "$SCRIPT_DIR/run_profile_query_smoke.sh" >/dev/null 2>&1; then
  echo "unvalidated complete stack unexpectedly reached query runner" >&2
  exit 1
fi

echo "[test-run-profile-query-smoke] ok"
