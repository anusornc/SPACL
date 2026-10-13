#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
TMP_DIR="$(mktemp -d)"
trap 'rm -rf "$TMP_DIR"' EXIT

STAGED_DIR="$TMP_DIR/staged"
DATA_DIR="$TMP_DIR/data"
RESULTS_ROOT="$TMP_DIR/results"
COMPONENT_MANIFEST="$TMP_DIR/components.json"
MATERIALIZATION_MANIFEST="$TMP_DIR/materialization.json"
CAPABILITIES_FILE="$TMP_DIR/split-openllet-capabilities.json"
CONFIG_FILE="$TMP_DIR/openllet-direct-semantics.properties"
TRACE_OPENLLET="$TMP_DIR/openllet.trace"
TRACE_GRAPHDB="$TMP_DIR/graphdb.trace"
TRACE_OTHER="$TMP_DIR/other.trace"
mkdir -p "$STAGED_DIR" "$DATA_DIR"/{DL,EL,QL,RL}
printf 'openllet.direct.semantics=true\n' > "$CONFIG_FILE"
: > "$TRACE_OPENLLET"
: > "$TRACE_GRAPHDB"
: > "$TRACE_OTHER"

make_adapter() {
  local adapter="$1"
  local name="$2"
  local state_dir="$3"
  local trace="$4"
  cat > "$adapter" <<'ADAPTER'
#!/usr/bin/env bash
set -euo pipefail
ADAPTER_NAME="__ADAPTER_NAME__"
ADAPTER_STATE_DIR="__ADAPTER_STATE_DIR__"
ADAPTER_TRACE="__ADAPTER_TRACE__"
command="${1:?}"
case "$command" in
  load)
    context="${2:?}"
    mkdir -p "$context/$ADAPTER_STATE_DIR"
    printf '%s %s %s %s\n' "$ADAPTER_NAME" "$command" "$(basename "$context")" "${OWL2BENCH_PROFILE_ADAPTER_NAME:-unset}" >> "$ADAPTER_TRACE"
    jq -n \
      --arg adapter "$ADAPTER_NAME" \
      --arg variant "${OWL2BENCH_PROFILE_ADAPTER_NAME:-$ADAPTER_NAME}" \
      --arg profile "$(basename "$context")" \
      --arg context "$context" \
      '{schema_version:1,adapter:$adapter,variant:$variant,profile:$profile,context_dir:$context,context_identity:("profile=" + $profile + ";directory=" + $context),engine:{name:$adapter,version:"fixture",read_only:true},runtime_probe:"passed"}' \
      > "$context/$ADAPTER_STATE_DIR/provenance.json"
    ;;
  query)
    query="${2:?}"
    context="${3:?}"
    printf '%s %s %s %s\n' "$ADAPTER_NAME" "$command" "$(basename "$context")" "${OWL2BENCH_PROFILE_ADAPTER_NAME:-unset}" >> "$ADAPTER_TRACE"
    printf '%s\n' '<?xml version="1.0" encoding="UTF-8"?>'
    printf '%s\n' '<sparql xmlns="http://www.w3.org/2005/sparql-results#"><head><variable name="x"/></head><results><result><binding name="x"><uri>https://example.test/value</uri></binding></result></results></sparql>'
    ;;
  *) exit 2 ;;
esac
ADAPTER
  chmod +x "$adapter"
  ADAPTER_NAME="$name" ADAPTER_STATE_DIR="$state_dir" ADAPTER_TRACE="$trace" \
    perl -0pi -e 's/__ADAPTER_NAME__/$ENV{ADAPTER_NAME}/g; s/__ADAPTER_STATE_DIR__/$ENV{ADAPTER_STATE_DIR}/g; s#__ADAPTER_TRACE__#$ENV{ADAPTER_TRACE}#g' "$adapter"
}

make_adapter "$TMP_DIR/dl-adapter" "fixture-dl" ".fixture-dl" "$TRACE_OTHER"
make_adapter "$TMP_DIR/el-adapter" "fixture-el" ".fixture-el" "$TRACE_OTHER"
make_adapter "$TMP_DIR/ql-ruleset-adapter" "ql-ruleset" ".ql-ruleset" "$TRACE_OTHER"
make_adapter "$TMP_DIR/rl-adapter" "fixture-rl" ".fixture-rl" "$TRACE_OTHER"
make_adapter "$TMP_DIR/openllet-adapter" "openllet-ql" ".openllet-ql" "$TRACE_OPENLLET"
make_adapter "$TMP_DIR/graphdb-adapter" "graphdb-ql" ".graphdb-ql" "$TRACE_GRAPHDB"

python3 - "$STAGED_DIR" "$DATA_DIR" "$COMPONENT_MANIFEST" "$MATERIALIZATION_MANIFEST" "$CONFIG_FILE" "$TMP_DIR/openllet-adapter" <<'PY'
import hashlib
import json
import pathlib
import sys

staged = pathlib.Path(sys.argv[1])
data = pathlib.Path(sys.argv[2])
component_path = pathlib.Path(sys.argv[3])
materialization_path = pathlib.Path(sys.argv[4])
config_path = pathlib.Path(sys.argv[5])
openllet_adapter = pathlib.Path(sys.argv[6])


def digest(path: pathlib.Path) -> str:
    return "sha256:" + hashlib.sha256(path.read_bytes()).hexdigest()


query = staged / "query.rq"
query.write_text("SELECT DISTINCT ?x WHERE { ?x <urn:QL:p> ?o }\n", encoding="utf-8")
profiles = (
    ["DL"] * 22
    + ["EL"] * 10
    + ["QL"] * 10
    + ["RL"] * 18
)
lines = ["staged_query\tsource_query\tquery_id\tprofile\tworkload\tsource_corpus\tsource_revision"]
for index, profile in enumerate(profiles, start=1):
    lines.append("\t".join([str(query), f"official#{index:03d}", f"case_{index:03d}", profile, "fixture", "owl2bench-fixture", "r1"]))
(staged / "manifest.tsv").write_text("\n".join(lines) + "\n", encoding="utf-8")

config_digest = digest(config_path)
adapter_digest = digest(openllet_adapter)
component_profiles = {}
materialization_profiles = {}
variant_by_profile = {"DL": "hermit-dl", "EL": "elk-el", "QL": "openllet-ql", "RL": "rl-ruleset"}
for profile in ("DL", "EL", "QL", "RL"):
    context = data / profile
    source_graph = context / "source.nq"
    candidate_graph = context / "candidate.nq"
    output_graph = context / "graph.nq"
    graph_text = "\n".join(
        sorted(
            [
                f"<urn:{profile}:a> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <urn:{profile}:C> .",
                f"<urn:{profile}:a> <urn:{profile}:p> <urn:{profile}:b> .",
                f"<urn:{profile}:a> <urn:{profile}:dp> \"value\" .",
                f"<urn:{profile}:C> <http://www.w3.org/2000/01/rdf-schema#subClassOf> <urn:{profile}:D> .",
                f"<urn:{profile}:p> <http://www.w3.org/2000/01/rdf-schema#subPropertyOf> <urn:{profile}:q> .",
            ]
        )
    ) + "\n"
    for path in (source_graph, candidate_graph, output_graph):
        path.write_text(graph_text, encoding="utf-8")
    graph_digest = digest(output_graph)
    source_digest = digest(source_graph)
    variant = variant_by_profile[profile]
    component_profiles[profile] = {
        "execution_variant": variant,
        "components": {
            "engine": {"name": "Openllet" if profile == "QL" else f"{profile}-engine", "version": "fixture", "revision": f"{variant}-engine", "artifact_digest": adapter_digest},
            "materializer": {"name": f"{variant}-materializer", "version": "fixture", "revision": f"{variant}-materializer", "artifact_digest": adapter_digest},
            "query": {"name": f"{variant}-query", "version": "fixture", "revision": f"{variant}-query", "artifact_digest": adapter_digest},
        },
        "configuration": {"id": f"{variant}-config", "digest": config_digest},
        "runtime": {"id": f"{variant}-runtime", "image": {"reference": f"{variant}@{adapter_digest}", "digest": adapter_digest}, "dependencies": [{"name": "bash", "version": "fixture", "digest": adapter_digest}]},
        "credentials": {"status": "not_required"},
        "license": {"status": "not_required"},
        "capability": {"status": "supported", "supports_query": True},
        "resource_limits": {"timeout_seconds": 600, "memory": "4GiB", "threads": 1, "cpu": "1", "disk": "1GiB", "network": "local-only", "variant": variant},
        "dataset": {"id": f"dataset-{profile}", "sha256": source_digest, "tbox_id": f"tbox-{profile}", "tbox_sha256": source_digest, "abox_id": f"abox-{profile}", "abox_sha256": source_digest},
        "generator": {"id": "fixture-generator", "revision": "fixture", "seed": 1},
        "profile_context": {"id": f"context-{profile}", "directory": str(context)},
    }
    materialization_profiles[profile] = {
        "profile": profile,
        "status": "complete",
        "execution_variant": variant,
        "source_ontology": {"id": f"source-{profile}", "path": str(source_graph), "sha256": source_digest},
        "candidate_graph": {"id": f"candidate-{profile}", "path": str(candidate_graph), "sha256": graph_digest, "immutable": True},
        "component": {"name": f"{variant}-materializer", "version": "fixture", "revision": f"{variant}-materializer", "artifact_digest": adapter_digest, "path": str(openllet_adapter)},
        "exporter": {"name": f"{variant}-exporter", "version": "fixture", "revision": f"{variant}-exporter", "artifact_digest": adapter_digest, "path": str(openllet_adapter)},
        "query_backend": {"name": f"{variant}-query", "version": "fixture", "revision": f"{variant}-query", "artifact_digest": adapter_digest, "path": str(openllet_adapter), "read_only": True},
        "configuration": {"id": f"{variant}-config", "path": str(config_path), "sha256": config_digest},
        "runtime": {"id": f"{variant}-runtime", "image": {"reference": f"{variant}@{adapter_digest}", "digest": adapter_digest}, "dependencies": [{"name": "bash", "version": "fixture", "digest": adapter_digest}]},
        "output_graph": {"id": f"graph-{profile}", "path": str(output_graph), "sha256": graph_digest, "immutable": True},
        "derivation": {
            "version": "openllet-direct-semantics-over-owl2-ql-input" if profile == "QL" else "fixture-materialization",
            "revision": f"{variant}-derivation",
            "semantics": "Openllet Direct Semantics over OWL 2 QL input" if profile == "QL" else "fixture profile materialization",
        },
        "resource_limits": {"timeout_seconds": 600, "memory": "4GiB", "threads": 1, "cpu": "1", "disk": "1GiB", "network": "local-only", "variant": variant},
        "input_sha256": source_digest,
        "output_sha256": graph_digest,
        "profile_context": {"id": f"context-{profile}", "directory": str(context)},
        "query_workload": {"id": f"query-workload-{profile}", "query_count": 1, "queries": [{"path": str(query), "sha256": digest(query)}]},
        "graph_contract": {"canonicalizer": {"name": "URDNA2015", "version": "2015", "serialization": "N-Quads"}, "closure_scope": "finite_query_complete", "source_included": True, "required_assertion_families": ["class", "object_property", "data_property", "tbox", "rbox"], "anonymous_witness_policy": "not_exported", "independent_obligation_check_required": True, "two_run_determinism_required": True},
        "determinism": {"status": "complete", "runs": 2, "first_graph": {"path": str(output_graph), "sha256": graph_digest}, "second_graph": {"path": str(output_graph), "sha256": graph_digest}},
    }
    if profile in {"QL", "RL"}:
        ruleset = (
            {
                "id": "openllet-direct-semantics-on-ql-input",
                "version": "2.6.5",
                "revision": "openllet-ql-runtime-v1",
                "artifact_digest": config_digest,
                "conformance_boundary": "Openllet Direct Semantics over OWL 2 QL input",
            }
            if profile == "QL"
            else {
                "id": "rl-ruleset",
                "version": "fixture",
                "revision": "rl-ruleset",
                "artifact_digest": config_digest,
                "conformance_boundary": "OWL 2 RL ruleset fixture",
            }
        )
        component_profiles[profile]["ruleset"] = ruleset
        materialization_profiles[profile]["ruleset"] = ruleset

component_path.write_text(json.dumps({"schema_version": 1, "manifest_id": "openllet-ql-components", "matrix": {"expansion": "query_profile", "expected_query_count": 60, "expected_profile_case_counts": {"DL": 22, "EL": 10, "QL": 10, "RL": 18}}, "profiles": component_profiles}, indent=2) + "\n", encoding="utf-8")
materialization_path.write_text(json.dumps({"schema_version": 1, "manifest_id": "openllet-ql-materialization", "profiles": materialization_profiles}, indent=2) + "\n", encoding="utf-8")
PY

MISSING_RULESET_COMPONENT="$TMP_DIR/missing-ruleset-components.json"
MISSING_RULESET_MATERIALIZATION="$TMP_DIR/missing-ruleset-materialization.json"
jq 'del(.profiles.QL.ruleset)' "$COMPONENT_MANIFEST" > "$MISSING_RULESET_COMPONENT"
jq 'del(.profiles.QL.ruleset)' "$MATERIALIZATION_MANIFEST" > "$MISSING_RULESET_MATERIALIZATION"
python3 "$SCRIPT_DIR/split_stack_manifest.py" validate \
  --component-manifest "$MISSING_RULESET_COMPONENT" \
  --query-manifest "$STAGED_DIR/manifest.tsv" \
  --output "$TMP_DIR/missing-ruleset-component-result.json"
python3 "$SCRIPT_DIR/materialization_manifest.py" validate \
  --materialization-manifest "$MISSING_RULESET_MATERIALIZATION" \
  --output "$TMP_DIR/missing-ruleset-materialization-result.json"
jq -e '.profiles.QL.status == "not_available" and (.profiles.QL.reasons | index("ruleset_missing")) != null' \
  "$TMP_DIR/missing-ruleset-component-result.json" >/dev/null
jq -e '.profiles.QL.status == "failed" and (.profiles.QL.reasons | index("ruleset_missing")) != null' \
  "$TMP_DIR/missing-ruleset-materialization-result.json" >/dev/null

cat > "$CAPABILITIES_FILE" <<'JSON'
{
  "supports_query": true,
  "profiles": ["DL", "EL", "QL", "RL"],
  "data_mode": "directory",
  "adapter": "split-profile",
  "provenance_adapters": {
    "DL": "fixture-dl",
    "EL": "fixture-el",
    "QL": "openllet-ql",
    "RL": "fixture-rl"
  },
  "provenance_relative_paths": {
    "DL": ".fixture-dl/provenance.json",
    "EL": ".fixture-el/provenance.json",
    "QL": ".openllet-ql/provenance.json",
    "RL": ".fixture-rl/provenance.json"
  },
  "query_result_format": "sparql-results-xml",
  "component_manifest_required": true,
  "materialization_manifest_required": true,
  "no_profile_fallback": true,
  "not_available_exit_code": 125,
  "unsupported_exit_code": 123
}
JSON

if ! env \
    OWL2BENCH_QUERY_STAGED_DIR="$STAGED_DIR" \
    OWL2BENCH_QUERY_MANIFEST="$STAGED_DIR/manifest.tsv" \
    OWL2BENCH_QUERY_DATA_DIR="$DATA_DIR" \
    OWL2BENCH_QUERY_RESULTS_ROOT="$RESULTS_ROOT" \
    OWL2BENCH_QUERY_ENGINE_BIN="$SCRIPT_DIR/split_profile_engine.sh" \
    OWL2BENCH_QUERY_CAPABILITIES_FILE="$CAPABILITIES_FILE" \
    OWL2BENCH_QUERY_COMPONENT_MANIFEST="$COMPONENT_MANIFEST" \
    OWL2BENCH_QUERY_MATERIALIZATION_MANIFEST="$MATERIALIZATION_MANIFEST" \
    OWL2BENCH_QUERY_SPLIT_STACK=1 \
    OWL2BENCH_SPLIT_DL_ENGINE="$TMP_DIR/dl-adapter" \
    OWL2BENCH_SPLIT_EL_ENGINE="$TMP_DIR/el-adapter" \
    OWL2BENCH_SPLIT_QL_ENGINE="$TMP_DIR/ql-ruleset-adapter" \
    OWL2BENCH_SPLIT_RL_ENGINE="$TMP_DIR/rl-adapter" \
    OWL2BENCH_SPLIT_QL_VARIANT="openllet-ql" \
    OWL2BENCH_OPENLLET_QL_ENGINE="$TMP_DIR/openllet-adapter" \
    RUN_ID="openllet-ql-split-stack" \
    "$SCRIPT_DIR/run.sh" >/dev/null; then
  jq '.errors // .' "$RESULTS_ROOT/openllet-ql-split-stack/provenance/profile_result_contract.json" >&2 || true
  exit 1
fi

RUN_DIR="$RESULTS_ROOT/openllet-ql-split-stack"
if ! jq -e '
  .query_count == 60
  and .profile_case_counts == {DL:22,EL:10,QL:10,RL:18}
  and .result_status_counts.oracle_unavailable == 60
  and .engine_provenance_status_counts.available == 60
  and .profiles.QL.engine_provenance_status_counts.available == 10
  and .profile_gates.QL == "blocked"
  and .query_gate == "blocked"
  and .correctness_gate == "blocked"
  and .performance_claims_allowed == false
' "$RUN_DIR/summary.json" >/dev/null; then
  jq '.profiles | map_values(.gate_blocker_counts)' "$RUN_DIR/summary.json" >&2 || true
  jq '.profiles | map_values({status, cross_contract_errors, materialization: .materialization})' \
    "$RUN_DIR/provenance/split_stack_materialization.json" >&2 || true
  first_result="$(find "$RUN_DIR" -maxdepth 1 -type f -name '*.json' ! -name 'summary.json' ! -name 'run_metadata.json' | sort | head -1)"
  if [[ -n "$first_result" ]]; then
    jq '{case_id, query_profile, execution_status, result_status, diagnostics_file, output_file, load_diagnostics_file}' "$first_result" >&2 || true
    jq -r '.output // empty' "$first_result" >&2 || true
    diagnostics_file="$(jq -r '.diagnostics_file // empty' "$first_result")"
    output_file="$(jq -r '.output_file // empty' "$first_result")"
    [[ -n "$diagnostics_file" && -f "$diagnostics_file" ]] && cat "$diagnostics_file" >&2
    [[ -n "$output_file" && -f "$output_file" ]] && cat "$output_file" >&2
  fi
  exit 1
fi

test "$(wc -l < "$TRACE_OPENLLET" | tr -d '[:space:]')" = "11"
test "$(wc -l < "$TRACE_GRAPHDB" | tr -d '[:space:]')" = "0"
test "$(grep -c '^ql-ruleset ' "$TRACE_OTHER" || true)" = "0"

find "$RUN_DIR" -maxdepth 1 -type f -name '*__QL.json' -print0 \
  | xargs -0 jq -s -e '
    length == 10
    and all(.[]; .query_profile == "QL"
      and .result_status == "oracle_unavailable"
      and .oracle.status == "unavailable"
      and .engine_provenance.adapter == "openllet-ql"
      and .engine_provenance.variant == "openllet-ql"
      and .component_manifest.execution_variant == "openllet-ql"
      and .component_manifest.components.engine.name == "Openllet"
      and .component_manifest.ruleset.id == "openllet-direct-semantics-on-ql-input"
      and .component_manifest.ruleset.version == "2.6.5"
      and .component_manifest.ruleset.revision == "openllet-ql-runtime-v1"
      and (.component_manifest.ruleset.conformance_boundary | contains("Openllet Direct Semantics over OWL 2 QL input"))
      and .materialization.execution_variant == "openllet-ql"
      and (.materialization.derivation.semantics | contains("Openllet Direct Semantics over OWL 2 QL input"))
      and ((.materialization.derivation.semantics | test("rules engine"; "i")) | not)
      and .profile_context.profile == "QL")
  ' >/dev/null

for profile in DL EL RL; do
  find "$RUN_DIR" -maxdepth 1 -type f -name "*__${profile}.json" -print0 \
    | xargs -0 jq -s -e --arg profile "$profile" '
      all(.[]; .query_profile == $profile
        and .result_status == "oracle_unavailable"
        and .engine_provenance.variant != "openllet-ql"
        and .component_manifest.execution_variant != "openllet-ql")
    ' >/dev/null
done

MISSING_RESULTS="$TMP_DIR/missing-results"
env \
  OWL2BENCH_QUERY_STAGED_DIR="$STAGED_DIR" \
  OWL2BENCH_QUERY_MANIFEST="$STAGED_DIR/manifest.tsv" \
  OWL2BENCH_QUERY_DATA_DIR="$DATA_DIR" \
  OWL2BENCH_QUERY_RESULTS_ROOT="$MISSING_RESULTS" \
  OWL2BENCH_QUERY_ENGINE_BIN="$SCRIPT_DIR/split_profile_engine.sh" \
  OWL2BENCH_QUERY_CAPABILITIES_FILE="$CAPABILITIES_FILE" \
  OWL2BENCH_QUERY_COMPONENT_MANIFEST="$COMPONENT_MANIFEST" \
  OWL2BENCH_QUERY_MATERIALIZATION_MANIFEST="$MATERIALIZATION_MANIFEST" \
  OWL2BENCH_QUERY_SPLIT_STACK=1 \
  OWL2BENCH_SPLIT_DL_ENGINE="$TMP_DIR/dl-adapter" \
  OWL2BENCH_SPLIT_EL_ENGINE="$TMP_DIR/el-adapter" \
  OWL2BENCH_SPLIT_QL_ENGINE="$TMP_DIR/ql-ruleset-adapter" \
  OWL2BENCH_SPLIT_RL_ENGINE="$TMP_DIR/rl-adapter" \
  OWL2BENCH_SPLIT_QL_VARIANT="openllet-ql" \
  RUN_ID="openllet-ql-missing-adapter" \
  "$SCRIPT_DIR/run.sh" >/dev/null
jq -e '
  .query_count == 60
  and .result_status_counts.not_available == 10
  and .result_status_counts.oracle_unavailable == 50
  and .profile_gates.QL == "blocked"
  and .correctness_gate == "blocked"
' "$MISSING_RESULTS/openllet-ql-missing-adapter/summary.json" >/dev/null
test "$(wc -l < "$TRACE_GRAPHDB" | tr -d '[:space:]')" = "0"

run_openllet_preflight_case() {
  local run_id="$1"
  local component_manifest="$2"
  local materialization_manifest="$3"
  local results_root="$4"
  env \
    OWL2BENCH_QUERY_STAGED_DIR="$STAGED_DIR" \
    OWL2BENCH_QUERY_MANIFEST="$STAGED_DIR/manifest.tsv" \
    OWL2BENCH_QUERY_DATA_DIR="$DATA_DIR" \
    OWL2BENCH_QUERY_RESULTS_ROOT="$results_root" \
    OWL2BENCH_QUERY_ENGINE_BIN="$SCRIPT_DIR/split_profile_engine.sh" \
    OWL2BENCH_QUERY_CAPABILITIES_FILE="$CAPABILITIES_FILE" \
    OWL2BENCH_QUERY_COMPONENT_MANIFEST="$component_manifest" \
    OWL2BENCH_QUERY_MATERIALIZATION_MANIFEST="$materialization_manifest" \
    OWL2BENCH_QUERY_SPLIT_STACK=1 \
    OWL2BENCH_SPLIT_DL_ENGINE="$TMP_DIR/dl-adapter" \
    OWL2BENCH_SPLIT_EL_ENGINE="$TMP_DIR/el-adapter" \
    OWL2BENCH_OPENLLET_QL_ENGINE="$TMP_DIR/openllet-adapter" \
    OWL2BENCH_SPLIT_RL_ENGINE="$TMP_DIR/rl-adapter" \
    OWL2BENCH_SPLIT_QL_VARIANT="openllet-ql" \
    RUN_ID="$run_id" \
    "$SCRIPT_DIR/run.sh" >/dev/null
}

assert_openllet_preflight_failed() {
  local run_dir="$1"
  jq -e '
    .query_count == 60
    and .result_status_counts.failed == 10
    and .result_status_counts.oracle_unavailable == 50
    and .profile_gates.QL == "blocked"
    and .correctness_gate == "blocked"
  ' "$run_dir/summary.json" >/dev/null
  test "$(wc -l < "$TRACE_OPENLLET" | tr -d '[:space:]')" = "11"
  test "$(wc -l < "$TRACE_GRAPHDB" | tr -d '[:space:]')" = "0"
}

BAD_IMAGE_COMPONENT="$TMP_DIR/bad-image-components.json"
jq '.profiles.QL.runtime.image.digest = "sha256:0000000000000000000000000000000000000000000000000000000000000000"' \
  "$COMPONENT_MANIFEST" > "$BAD_IMAGE_COMPONENT"
BAD_IMAGE_RESULTS="$TMP_DIR/bad-image-results"
run_openllet_preflight_case \
  "openllet-ql-bad-image-digest" "$BAD_IMAGE_COMPONENT" \
  "$MATERIALIZATION_MANIFEST" "$BAD_IMAGE_RESULTS"
assert_openllet_preflight_failed \
  "$BAD_IMAGE_RESULTS/openllet-ql-bad-image-digest"
jq -e '
  .profiles.QL.status == "failed"
  and (.profiles.QL.component.reasons | index("runtime.image.digest_mismatch")) != null
' "$BAD_IMAGE_RESULTS/openllet-ql-bad-image-digest/provenance/split_stack_materialization.json" >/dev/null

BAD_MATERIALIZATION_IMAGE="$TMP_DIR/bad-image-materialization.json"
jq '.profiles.QL.runtime.image.digest = "sha256:0000000000000000000000000000000000000000000000000000000000000000"' \
  "$MATERIALIZATION_MANIFEST" > "$BAD_MATERIALIZATION_IMAGE"
BAD_MATERIALIZATION_IMAGE_RESULTS="$TMP_DIR/bad-materialization-image-results"
run_openllet_preflight_case \
  "openllet-ql-bad-materialization-image" "$COMPONENT_MANIFEST" \
  "$BAD_MATERIALIZATION_IMAGE" "$BAD_MATERIALIZATION_IMAGE_RESULTS"
assert_openllet_preflight_failed \
  "$BAD_MATERIALIZATION_IMAGE_RESULTS/openllet-ql-bad-materialization-image"
jq -e '
  .profiles.QL.status == "failed"
  and (.profiles.QL.materialization.reasons | index("runtime.image.digest_mismatch")) != null
' "$BAD_MATERIALIZATION_IMAGE_RESULTS/openllet-ql-bad-materialization-image/provenance/split_stack_materialization.json" >/dev/null

BAD_VARIANT_COMPONENT="$TMP_DIR/bad-variant-components.json"
jq '.profiles.QL.execution_variant = "graphdb-ql"' "$COMPONENT_MANIFEST" > "$BAD_VARIANT_COMPONENT"
BAD_VARIANT_RESULTS="$TMP_DIR/bad-variant-results"
run_openllet_preflight_case \
  "openllet-ql-bad-component-variant" "$BAD_VARIANT_COMPONENT" \
  "$MATERIALIZATION_MANIFEST" "$BAD_VARIANT_RESULTS"
assert_openllet_preflight_failed \
  "$BAD_VARIANT_RESULTS/openllet-ql-bad-component-variant"
jq -e '
  .profiles.QL.status == "failed"
  and (.profiles.QL.cross_contract_errors | index("execution_variant_mismatch") != null)
' "$BAD_VARIANT_RESULTS/openllet-ql-bad-component-variant/provenance/split_stack_materialization.json" >/dev/null

UNSUPPORTED_COMPONENT="$TMP_DIR/unsupported-components.json"
jq '.profiles.QL.capability.status = "unsupported"' "$COMPONENT_MANIFEST" > "$UNSUPPORTED_COMPONENT"
UNSUPPORTED_RESULTS="$TMP_DIR/unsupported-results"
env \
  OWL2BENCH_QUERY_STAGED_DIR="$STAGED_DIR" \
  OWL2BENCH_QUERY_MANIFEST="$STAGED_DIR/manifest.tsv" \
  OWL2BENCH_QUERY_DATA_DIR="$DATA_DIR" \
  OWL2BENCH_QUERY_RESULTS_ROOT="$UNSUPPORTED_RESULTS" \
  OWL2BENCH_QUERY_ENGINE_BIN="$SCRIPT_DIR/split_profile_engine.sh" \
  OWL2BENCH_QUERY_CAPABILITIES_FILE="$CAPABILITIES_FILE" \
  OWL2BENCH_QUERY_COMPONENT_MANIFEST="$UNSUPPORTED_COMPONENT" \
  OWL2BENCH_QUERY_MATERIALIZATION_MANIFEST="$MATERIALIZATION_MANIFEST" \
  OWL2BENCH_QUERY_SPLIT_STACK=1 \
  OWL2BENCH_SPLIT_DL_ENGINE="$TMP_DIR/dl-adapter" \
  OWL2BENCH_SPLIT_EL_ENGINE="$TMP_DIR/el-adapter" \
  OWL2BENCH_OPENLLET_QL_ENGINE="$TMP_DIR/openllet-adapter" \
  OWL2BENCH_SPLIT_RL_ENGINE="$TMP_DIR/rl-adapter" \
  OWL2BENCH_SPLIT_QL_VARIANT="openllet-ql" \
  RUN_ID="openllet-ql-unsupported" \
  "$SCRIPT_DIR/run.sh" >/dev/null
jq -e '
  .query_count == 60
  and .result_status_counts.unsupported == 10
  and .result_status_counts.oracle_unavailable == 50
  and .profile_gates.QL == "blocked"
  and .correctness_gate == "blocked"
' "$UNSUPPORTED_RESULTS/openllet-ql-unsupported/summary.json" >/dev/null

set +e
OWL2BENCH_SPLIT_QL_VARIANT="unknown-ql-variant" \
  "$SCRIPT_DIR/split_profile_engine.sh" load "$DATA_DIR/QL" \
  >"$TMP_DIR/unknown-variant.out" 2>"$TMP_DIR/unknown-variant.err"
unknown_variant_status="$?"
set -e
test "$unknown_variant_status" = 123
grep -q '\[split-profile\]\[unsupported\] unsupported QL adapter variant' \
  "$TMP_DIR/unknown-variant.err"

echo "[test-openllet-ql-split-stack-runner] ok"
