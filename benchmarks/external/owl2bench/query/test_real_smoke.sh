#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/../../../.." && pwd)"
SMOKE_RUNNER="$SCRIPT_DIR/run_real_smoke.sh"
TMP_DIR="$(mktemp -d)"
cleanup() {
  if [[ "${KEEP_REAL_SMOKE_TEST_DIR:-0}" == "1" ]]; then
    echo "[test-owl2bench-real-smoke] retained test directory: $TMP_DIR" >&2
  else
    rm -rf "$TMP_DIR"
  fi
}
trap cleanup EXIT

STAGED_DIR="$TMP_DIR/staged"
DATA_DIR="$TMP_DIR/data"
RESULTS_ROOT="$TMP_DIR/results"
GENERATOR_DIR="$TMP_DIR/generator"
CAPABILITIES_FILE="$TMP_DIR/capabilities.json"
COMPONENT_MANIFEST="$TMP_DIR/components.json"
MATERIALIZATION_MANIFEST="$TMP_DIR/materialization.json"
CONFIG_FILE="$TMP_DIR/querying-config.xml"
RUNTIME_FILE="$TMP_DIR/runtime-provenance.txt"
FAKE_ENGINE="$TMP_DIR/fake-konclude-adapter"
mkdir -p "$STAGED_DIR" "$RESULTS_ROOT" "$GENERATOR_DIR"
cp "$ROOT_DIR/benchmarks/ontologies/external/owl2bench/query-staged/manifest.tsv" "$STAGED_DIR/manifest.tsv"
find "$ROOT_DIR/benchmarks/ontologies/external/owl2bench/query-staged" -maxdepth 1 -type f -name '*.rq' -exec cp {} "$STAGED_DIR/" \;

for profile in DL EL QL RL; do
  mkdir -p "$DATA_DIR/$profile"
  printf 'Prefix(:=<https://example.test/%s#>)\nDeclaration(Class(:Anchor))\n' "$profile" \
    > "$DATA_DIR/$profile/anchor.ofn"
done

printf '<config>synthetic</config>\n' > "$CONFIG_FILE"
printf 'runtime=fixture\nloader=synthetic\n' > "$RUNTIME_FILE"
printf 'generator\n' > "$GENERATOR_DIR/OWL2Bench.jar"
printf 'generator-config\n' > "$GENERATOR_DIR/config.properties"
printf 'random-names\n' > "$GENERATOR_DIR/RandomNames.xlsx"

cat > "$CAPABILITIES_FILE" <<'JSON'
{
  "supports_query": true,
  "profiles": ["DL", "EL", "QL", "RL"],
  "data_mode": "directory",
  "adapter": "konclude-unified",
  "provenance_relative_path": ".konclude-unified/provenance.json",
  "engine": "Konclude",
  "engine_version": "v0.7.0-1138",
  "release_commit": "0002e80635403960a7df5d93bd0e8f994d4952d0",
  "release_asset_sha256": "37216c18f45a80165aec1e76aeefd62d933d6a3f8f134c5aca0707c676fda76e",
  "executable_sha256": "0000000000000000000000000000000000000000000000000000000000000000",
  "query_config_sha256": "0000000000000000000000000000000000000000000000000000000000000000",
  "converter_sha256": "0000000000000000000000000000000000000000000000000000000000000000"
}
JSON

cat > "$FAKE_ENGINE" <<'ENGINE'
#!/usr/bin/env bash
set -euo pipefail

case "${1:-}" in
  load)
    context_dir="${2:?}"
    profile="$(basename "$context_dir")"
    input="$(find "$context_dir" -maxdepth 1 -type f -name '*.ofn' -print -quit)"
    mkdir -p "$context_dir/.konclude-unified"
    jq -n \
      --arg profile "$profile" \
      --arg context_dir "$context_dir" \
      --arg input "$input" \
      --arg binary "$0" \
      --arg config "${KONCLUDE_UNIFIED_CONFIG:?}" \
      --arg engine_version "v0.7.0-1138" \
      --arg release_commit "0002e80635403960a7df5d93bd0e8f994d4952d0" \
      --arg release_asset_sha256 "37216c18f45a80165aec1e76aeefd62d933d6a3f8f134c5aca0707c676fda76e" \
      --arg binary_sha256 "$(sha256sum "$0" | awk '{print $1}')" \
      --arg config_sha256 "$(sha256sum "$KONCLUDE_UNIFIED_CONFIG" | awk '{print $1}')" \
      --arg dataset_sha256 "$(sha256sum "$input" | awk '{print $1}')" \
      --arg prepared_sha256 "$(sha256sum "$input" | awk '{print $1}')" \
      '{adapter:"konclude-unified",profile:$profile,context_dir:$context_dir,context_identity:("profile=" + $profile + ";directory=" + $context_dir),engine_version:$engine_version,release_commit:$release_commit,release_asset_sha256:$release_asset_sha256,expected_executable_sha256:$binary_sha256,expected_config_sha256:$config_sha256,expected_converter_sha256:"synthetic",query_mode:"sparqlfile",source_kind:"functional-style",source_input:$input,source_sha256:$prepared_sha256,prepared_input:$input,prepared_sha256:$prepared_sha256,config_path:$config,config_sha256:$config_sha256,binary_path:$binary,binary_sha256:$binary_sha256,dataset_identity:{dataset_id:("dataset-" + $profile),dataset_sha256:("sha256:" + $dataset_sha256),source:"fixture"},conversion_status:"native"}' \
      > "$context_dir/.konclude-unified/provenance.json"
    ;;
  query)
    query_file="${2:?}"
    context_dir="${3:?}"
    test -f "$context_dir/.konclude-unified/provenance.json"
    python3 - "$query_file" <<'PY'
import re
import sys

query = open(sys.argv[1], encoding="utf-8").read()
match = re.search(r"(?is)\bSELECT\s+(?:DISTINCT\s+)?(.*?)\s+WHERE\b", query)
variables = re.findall(r"\?([A-Za-z_][A-Za-z0-9_]*)", match.group(1) if match else "x")
print('<?xml version="1.0" encoding="UTF-8"?>')
print('<sparql xmlns="http://www.w3.org/2005/sparql-results#">')
print("  <head>" + "".join(f'<variable name="{name}"/>' for name in variables) + "</head>")
bindings = "".join(
    f'<binding name="{name}"><uri>https://example.test/{name}</uri></binding>'
    for name in variables
)
print(f"  <results><result>{bindings}</result></results>")
print('</sparql>')
PY
    ;;
  *)
    exit 2
    ;;
esac
ENGINE
chmod +x "$FAKE_ENGINE"

python3 - "$COMPONENT_MANIFEST" "$MATERIALIZATION_MANIFEST" "$DATA_DIR" "$CONFIG_FILE" "$FAKE_ENGINE" <<'PY'
import hashlib
import json
import pathlib
import sys

component_path = pathlib.Path(sys.argv[1])
materialization_path = pathlib.Path(sys.argv[2])
data_dir = pathlib.Path(sys.argv[3])
config_path = pathlib.Path(sys.argv[4])
engine_path = pathlib.Path(sys.argv[5])


def digest(path):
    return "sha256:" + hashlib.sha256(path.read_bytes()).hexdigest()


engine_digest = digest(engine_path)
config_digest = digest(config_path)
query_path = component_path.parent / "fixture-query.rq"
query_path.write_text("SELECT ?x WHERE { ?x <urn:query> ?o }\n", encoding="utf-8")
query_digest = digest(query_path)
component_profiles = {}
materialization_profiles = {}
for profile in ("DL", "EL", "QL", "RL"):
    context = data_dir / profile
    source = context / "anchor.ofn"
    source_graph = context / "source.nq"
    candidate_graph = context / "candidate.nq"
    output_graph = context / "graph.nq"
    output = context / "closure.ttl"
    output.write_text(f"closure {profile}\n", encoding="utf-8")
    graph_lines = sorted(
        [
            f"<urn:{profile}:i> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <urn:{profile}:C> .",
            f"<urn:{profile}:i> <urn:{profile}:p> <urn:{profile}:j> .",
            f"<urn:{profile}:i> <urn:{profile}:dp> \"value\" .",
            f"<urn:{profile}:C> <http://www.w3.org/2000/01/rdf-schema#subClassOf> <urn:{profile}:D> .",
            f"<urn:{profile}:p> <http://www.w3.org/2000/01/rdf-schema#subPropertyOf> <urn:{profile}:q> .",
        ]
    )
    graph_text = "\n".join(graph_lines) + "\n"
    for graph_path in (source_graph, candidate_graph, output_graph):
        graph_path.write_text(graph_text, encoding="utf-8")
    source_digest = digest(source)
    source_graph_digest = digest(source_graph)
    output_graph_digest = digest(output_graph)
    component_hash = engine_digest
    exporter = {
        "name": f"{profile.lower()}-exporter",
        "version": "1.0.0",
        "revision": "fixture-exporter-revision",
        "artifact_digest": engine_digest,
        "path": str(engine_path),
    }
    query_backend = {
        "name": f"{profile.lower()}-query",
        "version": "1.0.0",
        "revision": "fixture-query-revision",
        "artifact_digest": engine_digest,
        "path": str(engine_path),
        "read_only": True,
    }
    runtime = {
        "id": "fixture-runtime",
        "dependencies": [
            {"name": "bash", "version": "fixture", "digest": engine_digest}
        ],
    }
    configuration = {
        "id": f"{profile.lower()}-materialization-config",
        "path": str(config_path),
        "sha256": config_digest,
    }
    resource_limits = {
        "timeout_seconds": 600,
        "memory": "24GiB",
        "threads": 1,
        "cpu": "1",
        "disk": "10GiB",
        "network": "local-only",
        "variant": "owl2bench-test-24GiB",
    }
    component_profiles[profile] = {
        "components": {
            "engine": {
                "name": "Konclude",
                "version": "v0.7.0-1138",
                "revision": "fixture-engine-revision",
                "artifact_digest": engine_digest,
            },
            "materializer": {
                "name": f"{profile.lower()}-materializer",
                "version": "1.0.0",
                "revision": "fixture-materializer-revision",
                "artifact_digest": component_hash,
                "path": str(engine_path),
            },
            "query": {
                "name": f"{profile.lower()}-query",
                "version": "1.0.0",
                "revision": "fixture-query-revision",
                "artifact_digest": engine_digest,
            },
        },
        **(
            {
                "ruleset": {
                    "id": f"{profile.lower()}-ruleset",
                    "version": "1.0.0",
                    "revision": "fixture-ruleset-revision",
                    "artifact_digest": config_digest,
                    "conformance_boundary": "fixture",
                }
            }
            if profile in {"QL", "RL"}
            else {}
        ),
        "configuration": {"id": configuration["id"], "digest": config_digest},
        "runtime": runtime,
        "credentials": {"status": "not_required"},
        "license": {"status": "not_required"},
        "capability": {"status": "supported", "supports_query": True},
        "resource_limits": resource_limits,
        "dataset": {
            "id": f"dataset-{profile}",
            "sha256": source_graph_digest,
            "tbox_id": f"tbox-{profile}",
            "tbox_sha256": source_graph_digest,
            "abox_id": f"abox-{profile}",
            "abox_sha256": source_graph_digest,
        },
        "generator": {"id": "fixture-generator", "revision": "fixture-generator-revision", "seed": 1},
        "profile_context": {"id": f"context-{profile}", "directory": str(context)},
    }
    materialization_profiles[profile] = {
        "status": "complete",
        "profile": profile,
        "source_ontology": {"id": f"source-{profile}", "path": str(source_graph), "sha256": source_graph_digest},
        "candidate_graph": {"id": f"candidate-{profile}", "path": str(candidate_graph), "sha256": source_graph_digest, "immutable": True},
        "component": {
            "name": f"{profile.lower()}-materializer",
            "version": "1.0.0",
            "revision": "fixture-materializer-revision",
            "artifact_digest": component_hash,
            "path": str(engine_path),
        },
        "exporter": exporter,
        "query_backend": query_backend,
        "configuration": configuration,
        "runtime": runtime,
        "output_graph": {"id": f"graph-{profile}", "path": str(output_graph), "sha256": output_graph_digest, "immutable": True},
        "derivation": {"version": "materialization-1.0", "revision": "fixture-derivation-revision"},
        "resource_limits": resource_limits,
        "input_sha256": source_graph_digest,
        "output_sha256": output_graph_digest,
        "profile_context": {"id": f"context-{profile}", "directory": str(context)},
        "query_workload": {"id": f"fixture-query-workload-{profile}", "query_count": 1, "queries": [{"path": str(query_path), "sha256": query_digest}]},
        "graph_contract": {"canonicalizer": {"name": "URDNA2015", "version": "2015", "serialization": "N-Quads"}, "closure_scope": "finite_query_complete", "source_included": True, "required_assertion_families": ["class", "object_property", "data_property", "tbox", "rbox"], "anonymous_witness_policy": "not_exported", "independent_obligation_check_required": True, "two_run_determinism_required": True},
        "determinism": {"status": "complete", "runs": 2, "first_graph": {"path": str(output_graph), "sha256": output_graph_digest}, "second_graph": {"path": str(output_graph), "sha256": output_graph_digest}},
    }
    if profile in {"QL", "RL"}:
        materialization_profiles[profile]["ruleset"] = {
            "id": f"{profile.lower()}-ruleset",
            "version": "1.0.0",
            "revision": "fixture-ruleset-revision",
            "artifact_digest": config_digest,
            "conformance_boundary": "fixture",
        }

component_path.write_text(
    json.dumps(
        {
            "schema_version": 1,
            "manifest_id": "fixture-components",
            "matrix": {
                "expansion": "query_profile",
                "expected_query_count": 60,
                "expected_profile_case_counts": {"DL": 22, "EL": 10, "QL": 10, "RL": 18},
            },
            "profiles": component_profiles,
        },
        indent=2,
    ) + "\n",
    encoding="utf-8",
)
materialization_path.write_text(
    json.dumps({"schema_version": 1, "manifest_id": "fixture-materialization", "profiles": materialization_profiles}, indent=2) + "\n",
    encoding="utf-8",
)
PY

BASE_ID="konclude-real-smoke-fixture"
env \
  OWL2BENCH_QUERY_STAGED_DIR="$STAGED_DIR" \
  OWL2BENCH_QUERY_DATA_DIR="$DATA_DIR" \
  OWL2BENCH_QUERY_RESULTS_ROOT="$RESULTS_ROOT" \
  OWL2BENCH_SMOKE_RESULTS_ROOT="$RESULTS_ROOT" \
  OWL2BENCH_QUERY_ENGINE_BIN="$FAKE_ENGINE" \
  OWL2BENCH_QUERY_CAPABILITIES_FILE="$CAPABILITIES_FILE" \
  OWL2BENCH_QUERY_COMPONENT_MANIFEST="$COMPONENT_MANIFEST" \
  OWL2BENCH_QUERY_MATERIALIZATION_MANIFEST="$MATERIALIZATION_MANIFEST" \
  OWL2BENCH_QUERY_SPLIT_STACK=1 \
  KONCLUDE_UNIFIED_CONFIG="$CONFIG_FILE" \
  OWL2BENCH_SMOKE_GENERATOR_DIR="$GENERATOR_DIR" \
  OWL2BENCH_SMOKE_GENERATOR_REVISION=fixture-revision \
  OWL2BENCH_SMOKE_RUNTIME_PROVENANCE_FILE="$RUNTIME_FILE" \
  OWL2BENCH_SMOKE_RUN_ID="$BASE_ID" \
  OWL2BENCH_SMOKE_TEST_MODE=1 \
  "$SMOKE_RUNNER" >/dev/null

SMOKE_DIR="$RESULTS_ROOT/${BASE_ID}_smoke"
RUN_ONE="$RESULTS_ROOT/${BASE_ID}_r1"
RUN_TWO="$RESULTS_ROOT/${BASE_ID}_r2"
test -f "$SMOKE_DIR/summary.json"
test -f "$SMOKE_DIR/report.md"
jq -e '
  .smoke_gate == "passed"
  and .correctness_gate == "blocked"
  and .performance_claims_allowed == false
  and .historical_stardog_reproduction == false
  and .official_oracle_claim == false
  and .expected_profile_case_counts == {DL:22,EL:10,QL:10,RL:18}
  and .runs[0].query_count == 60
  and .runs[1].query_count == 60
  and .stability.status == "passed"
  and .stability.first_materialization_profile_count == 4
  and .stability.second_materialization_profile_count == 4
  and .correctness.completed_without_oracle == 60
  and .correctness.oracle_unavailable == 60
  and .campaign.correctness_gate == "blocked"
  and .campaign.performance_claims_allowed == false
  and .provenance.split_stack.requested == true
  and .provenance.split_stack.component_manifest.status == "available"
  and .provenance.split_stack.materialization.status == "complete"
' "$SMOKE_DIR/summary.json" >/dev/null

for run_dir in "$RUN_ONE" "$RUN_TWO"; do
  test "$(find "$run_dir" -maxdepth 1 -type f -name '*.json' ! -name 'run_metadata.json' ! -name 'summary.json' | wc -l)" -eq 60
  test "$(find "$run_dir/raw" -maxdepth 1 -type f -name '*.output' ! -name 'load_*.output' | wc -l)" -eq 60
  test "$(find "$run_dir/raw" -maxdepth 1 -type f -name '*.diagnostics' | wc -l)" -eq 64
  test "$(find "$run_dir/raw" -maxdepth 1 -type f -name '*.canonical.json' | wc -l)" -eq 60
  jq -e '.query_count == 60 and .profile_case_counts == {DL:22,EL:10,QL:10,RL:18} and .unclassified_count == 0 and .correctness_gate == "blocked"' \
    "$run_dir/summary.json" >/dev/null
  find "$run_dir" -maxdepth 1 -type f -name '*.json' ! -name 'run_metadata.json' ! -name 'summary.json' -print0 \
  | xargs -0 jq -s -e 'all(.[]; .execution_status == "success" and .result_status == "oracle_unavailable" and .oracle.status == "unavailable" and .canonicalization.status == "available" and (.canonicalization.result_hash | startswith("sha256:")) and (.source_query_sha256 | length) == 64 and .query_time_ms >= 0 and .load_time_ms >= 0 and .component_manifest.status == "available" and (.component_manifest.manifest_identity | startswith("sha256:")) and .materialization.status == "complete" and (.materialization.materialization_identity | startswith("sha256:")) and (.materialization.output_graph.sha256 | startswith("sha256:")))' \
    >/dev/null
  for profile in DL EL QL RL; do
    expected_count="$(case "$profile" in DL) printf 22;; EL|QL) printf 10;; RL) printf 18;; esac)"
    find "$run_dir" -maxdepth 1 -type f -name '*.json' ! -name 'run_metadata.json' ! -name 'summary.json' -print0 \
      | xargs -0 jq -s -e --arg profile "$profile" --arg directory "$DATA_DIR/$profile" --argjson expected "$expected_count" \
        '(map(select(.query_profile == $profile)) | length == $expected) and all(.[] | select(.query_profile == $profile); .profile_context.profile == $profile and .profile_context.directory == $directory and .engine_provenance.profile == $profile)' \
      >/dev/null
  done
done

jq -e '(.provenance.engine.declared.adapter == "konclude-unified") and (.provenance.generator.status == "recorded") and (.provenance.query_source.manifest_sha256 | length) == 64 and (.provenance.profile_datasets | keys) == ["DL","EL","QL","RL"] and (.provenance.runtime.status == "recorded")' \
  "$SMOKE_DIR/summary.json" >/dev/null
grep -F 'No historical Stardog or official-oracle correctness claim is made.' "$SMOKE_DIR/report.md" >/dev/null

# Missing split manifests must not fall back to the generic engine path.
MISSING_BASE_ID="konclude-real-smoke-missing-manifests"
if env \
  OWL2BENCH_QUERY_STAGED_DIR="$STAGED_DIR" \
  OWL2BENCH_QUERY_DATA_DIR="$DATA_DIR" \
  OWL2BENCH_QUERY_RESULTS_ROOT="$RESULTS_ROOT" \
  OWL2BENCH_SMOKE_RESULTS_ROOT="$RESULTS_ROOT" \
  OWL2BENCH_QUERY_ENGINE_BIN="$FAKE_ENGINE" \
  OWL2BENCH_QUERY_CAPABILITIES_FILE="$CAPABILITIES_FILE" \
  KONCLUDE_UNIFIED_CONFIG="$CONFIG_FILE" \
  OWL2BENCH_SMOKE_GENERATOR_DIR="$GENERATOR_DIR" \
  OWL2BENCH_SMOKE_GENERATOR_REVISION=fixture-revision \
  OWL2BENCH_SMOKE_RUNTIME_PROVENANCE_FILE="$RUNTIME_FILE" \
  OWL2BENCH_SMOKE_RUN_ID="$MISSING_BASE_ID" \
  "$SMOKE_RUNNER" >/dev/null 2>"$TMP_DIR/missing-smoke.stderr"; then
  echo "assertion failed: smoke passed without split-stack manifests" >&2
  exit 1
fi
jq -e '
  .smoke_gate == "blocked"
  and .correctness_gate == "blocked"
  and .runs[0].query_count == 60
  and .runs[0].result_status_counts.not_available == 60
  and .provenance.split_stack.requested == true
' "$RESULTS_ROOT/${MISSING_BASE_ID}_smoke/summary.json" >/dev/null

echo "owl2bench real smoke tests passed"
