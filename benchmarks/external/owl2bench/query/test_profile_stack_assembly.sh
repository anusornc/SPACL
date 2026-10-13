#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ASSEMBLER="$SCRIPT_DIR/profile_stack_assembly.py"
TMP_DIR="$(mktemp -d "${TMPDIR:-/tmp}/tableauxx-profile-stack-test.XXXXXX")"
trap 'rm -rf "$TMP_DIR"' EXIT

python3 - "$TMP_DIR" <<'PY'
import hashlib
import json
import pathlib
import sys

root = pathlib.Path(sys.argv[1])
query = root / "query.rq"
query.write_text("SELECT ?x WHERE { ?x <urn:query> ?o }\n", encoding="utf-8")

def digest(path):
    return "sha256:" + hashlib.sha256(path.read_bytes()).hexdigest()

query_file_hash = digest(query)
query_manifest = root / "query-manifest.tsv"
rows = ["staged_query\tsource_query\tquery_id\tprofile\tworkload\tsource_corpus\tsource_revision"]
for profile, count in (("DL", 22), ("EL", 10), ("QL", 10), ("RL", 18)):
    for index in range(1, count + 1):
        rows.append(
            f"{query}\tofficial-{profile}-{index}\t{profile.lower()}-{index:03d}\t{profile}\tofficial-text\ttest-corpus\ttest-revision"
        )
query_manifest.write_text("\n".join(rows) + "\n", encoding="utf-8")

generator_artifact = root / "OWL2Bench.jar"
generator_artifact.write_text("fixture generator\n", encoding="utf-8")

identities = {}
materializations = {}
for index, profile in enumerate(("DL", "EL", "QL", "RL"), start=1):
    profile_dir = root / "profiles" / profile
    profile_dir.mkdir(parents=True)
    source = profile_dir / "source.nq"
    graph = profile_dir / "graph.nq"
    generated_dataset = profile_dir / "dataset.owl"
    tbox = profile_dir / "tbox.owl"
    abox = profile_dir / "abox.nq"
    engine_artifact = profile_dir / "engine.bin"
    exporter_artifact = profile_dir / "exporter.bin"
    query_artifact = profile_dir / "query.bin"
    config_artifact = profile_dir / "config.properties"
    graph_content = "\n".join(sorted([
        f"<urn:{profile}:i> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <urn:{profile}:C> .",
        f"<urn:{profile}:i> <urn:{profile}:p> <urn:{profile}:j> .",
        f"<urn:{profile}:i> <urn:{profile}:dp> \"value\" .",
        f"<urn:{profile}:C> <http://www.w3.org/2000/01/rdf-schema#subClassOf> <urn:{profile}:D> .",
        f"<urn:{profile}:p> <http://www.w3.org/2000/01/rdf-schema#subPropertyOf> <urn:{profile}:q> .",
    ])) + "\n"
    source.write_text(graph_content, encoding="utf-8")
    graph.write_text(graph_content, encoding="utf-8")
    generated_dataset.write_text(f"generated dataset {profile}\n", encoding="utf-8")
    tbox.write_text(f"tbox {profile}\n", encoding="utf-8")
    abox.write_text(f"abox {profile}\n", encoding="utf-8")
    engine_artifact.write_text(f"engine {profile}\n", encoding="utf-8")
    exporter_artifact.write_text(f"exporter {profile}\n", encoding="utf-8")
    query_artifact.write_text(f"query {profile}\n", encoding="utf-8")
    config_artifact.write_text(f"config {profile}\n", encoding="utf-8")
    source_hash = digest(source)
    graph_hash = digest(graph)
    component_hash = digest(engine_artifact)
    exporter_hash = digest(exporter_artifact)
    query_hash = digest(query_artifact)
    config_hash = digest(config_artifact)
    identities[profile] = {
        "profile": profile,
        "component": {"name": f"{profile}-materializer", "version": "1.0", "revision": f"{profile}-r1", "artifact_digest": component_hash, "path": str(engine_artifact)},
        "exporter": {"name": f"{profile}-exporter", "version": "1.0", "revision": f"{profile}-exporter-r1", "artifact_digest": exporter_hash, "path": str(exporter_artifact)},
        "query_backend": {"name": f"{profile}-query", "version": "1.0", "revision": f"{profile}-query-r1", "artifact_digest": query_hash, "path": str(query_artifact), "read_only": True},
        "configuration": {"id": f"config-{profile}", "path": str(config_artifact), "digest": config_hash},
        "runtime": {"id": f"runtime-{profile}", "dependencies": [{"name": "runtime", "version": "1.0", "digest": component_hash}]},
        "credentials": {"status": "not_required"},
        "license": {"status": "not_required"},
        "capability": {"status": "supported", "supports_query": True},
        "resource_limits": {"timeout_seconds": 600, "memory": "4GiB", "threads": 1, "cpu": "unspecified", "disk": "unspecified", "network": "disabled-after-source-acquisition", "variant": "owl2bench-custom-resource-envelope"},
        "profile_context": {"id": f"context-{profile}", "directory": str(profile_dir)},
        "source_document": {"path": str(source), "sha256": source_hash},
        "query_workload": {"id": f"fixture-query-workload-{profile}", "query_count": 1, "queries": [{"path": str(query), "sha256": query_file_hash}]},
        "dataset": {"id": f"dataset-{profile}", "artifact_id": f"profile-source-nquads:{source_hash}", "artifact_path": str(source), "artifact_revision": "generator-r1", "sha256": source_hash, "source_ontology_sha256": digest(generated_dataset), "tbox_id": f"tbox-{profile}", "tbox_sha256": digest(tbox), "abox_id": f"abox-{profile}", "abox_sha256": digest(abox)},
        "generator": {"id": "OWL2Bench", "revision": "generator-r1", "seed": 1, "artifact_digest": digest(generator_artifact), "configuration_id": "generator-config", "configuration_sha256": config_hash},
        "derivation": {"version": "test-materialization/1", "revision": f"{profile}-derivation-r1"},
    }
    if profile in {"QL", "RL"}:
        identities[profile]["ruleset"] = {"id": f"ruleset-{profile}", "version": "1.0", "revision": f"{profile}-ruleset-r1", "artifact_digest": component_hash, "conformance_boundary": "test boundary"}
    materializations[profile] = {
        "profile": profile,
        "status": "complete",
        "source_ontology": {"id": f"source-{profile}", "path": str(source), "sha256": source_hash},
        "component": identities[profile]["component"],
        "configuration": {"id": f"config-{profile}", "sha256": config_hash},
        "candidate_graph": {"id": f"candidate-{profile}", "path": str(graph), "sha256": graph_hash, "immutable": True},
        "output_graph": {"id": f"graph-{profile}", "path": str(graph), "sha256": graph_hash, "immutable": True},
        "derivation": identities[profile]["derivation"],
        "resource_limits": identities[profile]["resource_limits"],
        "input_sha256": source_hash,
        "output_sha256": graph_hash,
        "profile_context": identities[profile]["profile_context"],
        "query_workload": identities[profile]["query_workload"],
        "graph_contract": {"canonicalizer": {"name": "URDNA2015", "version": "2015", "serialization": "N-Quads"}, "closure_scope": "finite_query_complete", "source_included": True, "required_assertion_families": ["class", "object_property", "data_property", "tbox", "rbox"], "anonymous_witness_policy": "not_exported", "independent_obligation_check_required": True, "two_run_determinism_required": True},
        "determinism": {"status": "complete", "runs": 2, "first_graph": {"path": str(graph), "sha256": graph_hash}, "second_graph": {"path": str(graph), "sha256": graph_hash}},
    }
    if profile in {"QL", "RL"}:
        materializations[profile]["ruleset"] = identities[profile]["ruleset"]
    (root / f"identity-{profile}.json").write_text(json.dumps(identities[profile], indent=2) + "\n", encoding="utf-8")
    (root / f"materialization-{profile}.json").write_text(json.dumps({"schema_version": 1, "profiles": {profile: materializations[profile]}}, indent=2) + "\n", encoding="utf-8")

dataset_manifest = root / "dataset-manifest.json"
dataset_manifest.write_text(json.dumps({
    "schema_version": 1,
    "manifest_id": "test-dataset",
    "status": "complete",
    "source": {"revision": "test-source-revision"},
    "generator": {"revision": "test-generator-revision", "seed": 1, "artifact_digest": digest(generator_artifact)},
    "resource_limits": {"timeout_seconds": 600, "memory": "4GiB", "threads": 1, "cpu": "unspecified", "disk": "unspecified", "network": "disabled-after-source-acquisition", "variant": "owl2bench-custom-resource-envelope"},
    "profiles": {
        profile: {"status": "complete", "identity_sha256": f"sha256:{index:064x}", "dataset": identities[profile]["dataset"], "profile_context": identities[profile]["profile_context"]}
        for index, profile in enumerate(("DL", "EL", "QL", "RL"), start=1)
    },
    "validation": {"status": "complete", "errors": []},
    "no_answer_keys": True,
}) + "\n", encoding="utf-8")
PY

ASSEMBLY_DIR="$TMP_DIR/assembly"
IDENTITY_ARGS=()
MATERIALIZATION_ARGS=()
for profile in DL EL QL RL; do
  IDENTITY_ARGS+=(--profile-identity "$profile=$TMP_DIR/identity-$profile.json")
  MATERIALIZATION_ARGS+=(--profile-materialization "$profile=$TMP_DIR/materialization-$profile.json")
done

if ! python3 "$ASSEMBLER" assemble \
  --dataset-manifest "$TMP_DIR/dataset-manifest.json" \
  --query-manifest "$TMP_DIR/query-manifest.tsv" \
  --output-dir "$ASSEMBLY_DIR" \
  "${IDENTITY_ARGS[@]}" \
  "${MATERIALIZATION_ARGS[@]}"; then
  cat "$ASSEMBLY_DIR/stack-validation.json" >&2
  exit 1
fi

jq -e '
  .status == "complete"
  and ([.profiles[].status] | sort) == ["complete","complete","complete","complete"]
  and .answer_key_policy == "not_permitted"
' "$ASSEMBLY_DIR/stack.json" >/dev/null

if jq -e '.. | objects | has("oracle") or has("answer_key") or has("answer_keys")' \
  "$ASSEMBLY_DIR/component.json" "$ASSEMBLY_DIR/materialization.json" >/dev/null; then
  echo "answer/oracle field unexpectedly copied into assembled manifests" >&2
  exit 1
fi

MISMATCH_MATERIALIZATION="$TMP_DIR/materialization-DL-mismatch.json"
jq '.profiles.DL.source_ontology.sha256 = "sha256:0000000000000000000000000000000000000000000000000000000000000001"' \
  "$TMP_DIR/materialization-DL.json" > "$MISMATCH_MATERIALIZATION"
MISMATCH_MATERIALIZATION_ARGS=()
for profile in DL EL QL RL; do
  if [ "$profile" = DL ]; then
    MISMATCH_MATERIALIZATION_ARGS+=(--profile-materialization "$profile=$MISMATCH_MATERIALIZATION")
  else
    MISMATCH_MATERIALIZATION_ARGS+=(--profile-materialization "$profile=$TMP_DIR/materialization-$profile.json")
  fi
done
MISMATCH_DIR="$TMP_DIR/mismatch"
if python3 "$ASSEMBLER" assemble \
  --dataset-manifest "$TMP_DIR/dataset-manifest.json" \
  --query-manifest "$TMP_DIR/query-manifest.tsv" \
  --output-dir "$MISMATCH_DIR" \
  "${IDENTITY_ARGS[@]}" \
  "${MISMATCH_MATERIALIZATION_ARGS[@]}" >/dev/null; then
  echo "materialization source identity mismatch unexpectedly passed" >&2
  exit 1
fi
jq -e '
  .status == "failed"
  and .profiles.DL.status == "failed"
  and (.profiles.DL.materialization.reason | contains("frozen profile identity"))
' "$MISMATCH_DIR/stack-validation.json" >/dev/null

# A rerun must discard the previous graph-validation artifact.  Mutating the
# graph makes the old materialization hash invalid; a stale complete result
# must not survive the failed independent validation.
printf '%s\n' '# changed after first assembly' >> "$TMP_DIR/profiles/DL/graph.nq"
if python3 "$ASSEMBLER" assemble \
  --dataset-manifest "$TMP_DIR/dataset-manifest.json" \
  --query-manifest "$TMP_DIR/query-manifest.tsv" \
  --output-dir "$ASSEMBLY_DIR" \
  "${IDENTITY_ARGS[@]}" \
  "${MATERIALIZATION_ARGS[@]}" >/dev/null; then
  echo "mutated graph unexpectedly passed assembly" >&2
  exit 1
fi
jq -e '
  .status != "complete"
  and .graph_validations.DL.status != "complete"
  and .profiles.DL.status != "complete"
' "$ASSEMBLY_DIR/stack-validation.json" >/dev/null

MISSING_DIR="$TMP_DIR/missing"
if python3 "$ASSEMBLER" assemble \
  --dataset-manifest "$TMP_DIR/dataset-manifest.json" \
  --query-manifest "$TMP_DIR/query-manifest.tsv" \
  --output-dir "$MISSING_DIR" \
  "${IDENTITY_ARGS[@]}" \
  --profile-materialization "DL=$TMP_DIR/materialization-DL.json" \
  --profile-materialization "EL=$TMP_DIR/materialization-EL.json" \
  --profile-materialization "QL=$TMP_DIR/materialization-QL.json" >/dev/null; then
  echo "missing RL materialization unexpectedly passed" >&2
  exit 1
fi
jq -e '.status != "complete" and .profiles.RL.status != "complete"' "$MISSING_DIR/stack.json" >/dev/null

echo "[test-owl2bench-profile-stack-assembly] ok"
