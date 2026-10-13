#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
TMP_DIR="$(mktemp -d)"
trap 'rm -rf "$TMP_DIR"' EXIT

SOURCE="$TMP_DIR/source.nq"
GENERATOR="$TMP_DIR/OWL2Bench-generator.bin"
GENERATOR_CONFIG="$TMP_DIR/generator.properties"
mkdir -p "$TMP_DIR/data"

python3 - "$TMP_DIR" <<'PY'
import hashlib
import json
import pathlib
import sys

root = pathlib.Path(sys.argv[1])
source = root / "source.nq"
source.write_text(
    "\n".join(
        [
            "<https://example.test/alice> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <https://example.test/Person> .",
            "<https://example.test/alice> <https://example.test/knows> <https://example.test/bob> .",
            "<https://example.test/alice> <https://example.test/name> \"Alice\" .",
            "<https://example.test/Person> <http://www.w3.org/2000/01/rdf-schema#subClassOf> <https://example.test/Agent> .",
            "<https://example.test/knows> <http://www.w3.org/2000/01/rdf-schema#subPropertyOf> <https://example.test/related> .",
        ]
    )
    + "\n",
    encoding="utf-8",
)
(root / "OWL2Bench-generator.bin").write_text("generator\n", encoding="utf-8")
(root / "generator.properties").write_text("seed=1\n", encoding="utf-8")

profiles = {"EL": 10, "QL": 10, "RL": 18}
engine_names = {"EL": "ELK", "QL": "QL-ruleset-engine", "RL": "RL-ruleset-engine"}


def digest(path):
    return "sha256:" + hashlib.sha256(path.read_bytes()).hexdigest()


def write_executable(path, body):
    path.write_text(body, encoding="utf-8")
    path.chmod(0o755)


for profile, query_count in profiles.items():
    context = root / "data" / profile
    context.mkdir(parents=True)
    config = root / f"{profile.lower()}-config.properties"
    tbox = root / f"{profile.lower()}-tbox.owl"
    abox = root / f"{profile.lower()}-abox.owl"
    exporter = root / f"{profile.lower()}-materializer"
    query_backend = root / f"{profile.lower()}-query-backend"
    queries = root / f"{profile.lower()}-queries"
    queries.mkdir()
    config.write_text(f"profile={profile}\n", encoding="utf-8")
    tbox.write_text(f"tbox={profile}\n", encoding="utf-8")
    abox.write_text(f"abox={profile}\n", encoding="utf-8")
    inferred = (
        "<https://example.test/alice> "
        "<http://www.w3.org/1999/02/22-rdf-syntax-ns#type> "
        "<https://example.test/Agent> .\n"
        "<https://example.test/alice> <https://example.test/related> "
        "<https://example.test/bob> .\n"
    )
    write_executable(
        exporter,
        "#!/usr/bin/env python3\n"
        "import pathlib, sys\n"
        "if sys.argv[1:] != ['export', sys.argv[2]]:\n"
        "    raise SystemExit(2)\n"
        f"sys.stdout.write(pathlib.Path({str(source)!r}).read_text(encoding='utf-8') + {inferred!r})\n",
    )
    write_executable(
        query_backend,
        "#!/usr/bin/env bash\n"
        "set -euo pipefail\n"
        "[[ \"${1:-}\" == query && $# -eq 3 ]]\n"
        "printf '%s\\n' '?x' '<https://example.test/alice>'\n",
    )
    query_records = []
    for index in range(1, query_count + 1):
        query = queries / f"query-{index:03d}.rq"
        query.write_text(
            "PREFIX ex: <https://example.test/>\n"
            "SELECT ?x WHERE { ?x ex:knows ex:bob }\n",
            encoding="utf-8",
        )
        query_records.append({"path": str(query), "sha256": digest(query)})

    source_hash = digest(source)
    tbox_hash = digest(tbox)
    abox_hash = digest(abox)
    generator = root / "OWL2Bench-generator.bin"
    generator_config = root / "generator.properties"
    identity = {
        "profile": profile,
        "component": {
            "name": engine_names[profile],
            "version": "fixture-1.0.0",
            "revision": f"{profile.lower()}-engine-r1",
            "artifact_digest": digest(exporter),
            "path": str(exporter),
        },
        "exporter": {
            "name": f"tableauxx-{profile.lower()}-exporter",
            "version": "1.0.0",
            "revision": f"{profile.lower()}-exporter-r1",
            "artifact_digest": digest(exporter),
            "path": str(exporter),
        },
        "query_backend": {
            "name": f"{profile.lower()}-read-only-query",
            "version": "1.0.0",
            "revision": f"{profile.lower()}-query-r1",
            "artifact_digest": digest(query_backend),
            "path": str(query_backend),
            "read_only": True,
        },
        "configuration": {
            "id": f"{profile.lower()}-materialization-config",
            "path": str(config),
            "sha256": digest(config),
        },
        "derivation": {
            "version": f"tableauxx-{profile.lower()}-materialization/1",
            "revision": f"{profile.lower()}-derivation-r1",
        },
        "runtime": {
            "id": f"fixture-{profile.lower()}-runtime",
            "dependencies": [
                {"name": "python", "version": "3.12", "digest": digest(generator)}
            ],
        },
        "resource_limits": {"timeout_seconds": 60, "memory": "1GiB", "threads": 1, "cpu": "unspecified", "disk": "unspecified", "network": "test-local", "variant": "owl2bench-test-1GiB"},
        "profile_context": {"id": f"context-{profile}", "directory": str(context)},
        "source_document": {
            "id": f"source-{profile}",
            "path": str(source),
            "sha256": source_hash,
        },
        "query_workload": {
            "id": f"fixture-query-workload-{profile}",
            "query_count": query_count,
            "queries": query_records,
        },
        "dataset": {
            "id": f"anchor-{profile}",
            "profile": profile,
            "sha256": source_hash,
            "source_ontology_sha256": source_hash,
            "artifact_id": f"dataset-artifact-{profile}",
            "artifact_path": str(source),
            "artifact_revision": f"{profile.lower()}-dataset-r1",
            "tbox_id": f"tbox-{profile}",
            "tbox_sha256": tbox_hash,
            "abox_id": f"abox-{profile}",
            "abox_sha256": abox_hash,
            "seed": 1,
        },
        "generator": {
            "id": "fixture-owl2bench-generator",
            "revision": "fixture-generator-r1",
            "seed": 1,
            "artifact_path": str(generator),
            "artifact_digest": digest(generator),
            "configuration_id": f"generator-config-{profile}",
            "configuration_sha256": digest(generator_config),
        },
        "generator_configuration": {
            "id": f"generator-config-{profile}",
            "path": str(generator_config),
            "revision": "fixture-generator-config-r1",
            "sha256": digest(generator_config),
        },
        "profile_tbox": {
            "id": f"tbox-{profile}",
            "path": str(tbox),
            "revision": f"{profile.lower()}-tbox-r1",
            "sha256": tbox_hash,
        },
        "profile_abox": {
            "id": f"abox-{profile}",
            "path": str(abox),
            "revision": f"{profile.lower()}-abox-r1",
            "sha256": abox_hash,
        },
        "dataset_artifact": {
            "id": f"dataset-artifact-{profile}",
            "path": str(source),
            "revision": f"{profile.lower()}-dataset-r1",
            "sha256": source_hash,
        },
        "implementation": {"path": str(exporter), "sha256": digest(exporter)},
    }
    if profile in {"QL", "RL"}:
        identity["ruleset"] = {
            "id": f"{profile.lower()}-ruleset",
            "version": "fixture-1.0.0",
            "revision": f"{profile.lower()}-ruleset-r1",
            "artifact_digest": digest(config),
            "conformance_boundary": f"declared-{profile.lower()}-ruleset",
        }
    (root / f"identity-{profile}.json").write_text(
        json.dumps(identity, indent=2, sort_keys=True) + "\n", encoding="utf-8"
    )
PY

for profile in EL QL RL; do
  expected=10
  [[ "$profile" == RL ]] && expected=18
  python3 "$SCRIPT_DIR/profile_materialization.py" run \
    --profile "$profile" \
    --source "$SOURCE" \
    --exporter "$TMP_DIR/${profile,,}-materializer" \
    --identity "$TMP_DIR/identity-$profile.json" \
    --output-dir "$TMP_DIR/run-$profile" >/dev/null
  jq -e --arg profile "$profile" --argjson expected "$expected" '
    .profile == $profile
    and .status == "oracle_unavailable"
    and .execution_status == "complete"
    and .determinism.status == "complete"
    and .determinism.runs == 2
    and (.timing.export_materialization_time_ms >= 0)
    and (.timing.determinism_time_ms >= 0)
    and (.timing.validation_time_ms >= 0)
    and (.timing.total_time_ms >= 0)
    and .oracle_policy == "no_answer_key_oracle"
  ' "$TMP_DIR/run-$profile/profile_materialization_summary.json" >/dev/null
  jq -e --arg profile "$profile" --argjson expected "$expected" '
    .profiles[$profile].status == "complete"
    and (.profiles[$profile].output_graph.sha256 | startswith("sha256:"))
    and (.profiles[$profile].diagnostics.sha256 | startswith("sha256:"))
    and (.profiles[$profile].timing.total_time_ms >= 0)
    and ((has("oracle")) | not)
    and ((has("answer_key")) | not)
  ' "$TMP_DIR/run-$profile/materialization.json" >/dev/null
  if [[ "$profile" == QL || "$profile" == RL ]]; then
    jq -e --arg profile "$profile" '.profiles[$profile].ruleset.conformance_boundary | startswith("declared-")' \
      "$TMP_DIR/run-$profile/materialization.json" >/dev/null
  fi
  if ! python3 "$SCRIPT_DIR/materialization_validator.py" validate \
    --profile "$profile" \
    --source "$SOURCE" \
    --candidate-graph "$TMP_DIR/run-$profile/candidate_graph.nq" \
    --graph "$TMP_DIR/run-$profile/graph.nq" \
    --manifest "$TMP_DIR/run-$profile/materialization.json" \
    --output "$TMP_DIR/run-$profile/independent-validation.json"; then
    cat "$TMP_DIR/run-$profile/independent-validation.json" >&2
    exit 1
  fi
  jq -e '.status == "complete" and .query_handoff.status == "ready"' \
    "$TMP_DIR/run-$profile/independent-validation.json" >/dev/null
done

ELK_EL_MATERIALIZER_BIN="$TMP_DIR/el-materializer" \
  "$SCRIPT_DIR/elk_el_materializer.sh" export "$SOURCE" > "$TMP_DIR/elk-wrapper.nq"
cmp -s "$SOURCE" "$TMP_DIR/elk-wrapper.nq" && {
  echo "assertion failed: wrapper did not invoke the external materializer" >&2
  exit 1
}

if "$SCRIPT_DIR/ql_ruleset_materializer.sh" version >/dev/null 2>"$TMP_DIR/missing.stderr"; then
  echo "assertion failed: missing QL materializer was accepted" >&2
  exit 1
else
  test "$?" = 125
fi

echo "owl2bench profile materialization tests passed"
