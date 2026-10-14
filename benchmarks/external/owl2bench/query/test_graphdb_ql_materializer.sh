#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
TMP_DIR="$(mktemp -d)"
trap 'rm -rf "$TMP_DIR"' EXIT

DATASET_ROOT="$TMP_DIR/dataset"
QUERY_MANIFEST="$TMP_DIR/manifest.tsv"
IDENTITY="$TMP_DIR/graphdb-ql-identity.json"
COMPONENT_MANIFEST="$TMP_DIR/graphdb-ql-components.json"
RUN_DIR="$TMP_DIR/run"
MATERIALIZATION_CHECK="$TMP_DIR/materialization-check.json"
COMPONENT_CHECK="$TMP_DIR/component-check.json"

python3 - "$DATASET_ROOT" "$QUERY_MANIFEST" <<'PY'
import hashlib
import json
import pathlib
import sys

dataset_root = pathlib.Path(sys.argv[1])
query_manifest = pathlib.Path(sys.argv[2])
ql = dataset_root / "profiles" / "QL"
generator = dataset_root / "generator"
ql.mkdir(parents=True)
generator.mkdir(parents=True)

files = {
    ql / "source.nq": '<http://example.test/a> <http://example.test/p> <http://example.test/b> .\n',
    ql / "dataset.owl": '<rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"/>\n',
    ql / "profile-tbox.owl": '<rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"/>\n',
    ql / "abox.owl": '<rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"/>\n',
    ql / "abox.nq": '<http://example.test/a> <http://example.test/p> <http://example.test/b> .\n',
    generator / "OWL2Bench.jar": 'fixture jar\n',
    generator / "config.properties": 'fixture=true\n',
}
for path, text in files.items():
    path.write_text(text, encoding="utf-8")

def digest(path):
    return "sha256:" + hashlib.sha256(path.read_bytes()).hexdigest()

(ql / "profile-identity.json").write_text(
    json.dumps(
        {
            "profile": "QL",
            "status": "complete",
            "dataset": {
                "id": "fixture-ql",
                "artifact_id": "fixture-ql-dataset",
                "profile": "QL",
                "sha256": digest(ql / "source.nq"),
                "source_ontology_sha256": digest(ql / "dataset.owl"),
                "artifact_path": str(ql / "source.nq"),
                "artifact_revision": "fixture-revision",
                "seed": 1,
                "tbox_id": "fixture-ql-tbox",
                "tbox_sha256": digest(ql / "profile-tbox.owl"),
                "abox_id": "fixture-ql-abox",
                "abox_sha256": digest(ql / "abox.nq"),
            },
            "generator": {
                "id": "OWL2Bench",
                "revision": "fixture-revision",
                "seed": 1,
                "path": str(generator / "OWL2Bench.jar"),
                "sha256": digest(generator / "OWL2Bench.jar"),
            },
            "generator_configuration": {
                "id": "fixture-generator-config",
                "revision": "fixture-revision",
                "path": str(generator / "config.properties"),
                "sha256": digest(generator / "config.properties"),
            },
            "profile_context": {"id": "fixture-context-ql", "directory": str(ql)},
        },
        indent=2,
    )
    + "\n",
    encoding="utf-8",
)

query_dir = query_manifest.parent / "queries"
query_dir.mkdir()
lines = ["staged_query\tsource_query\tquery_id\tprofile\tworkload\tsource_corpus\tsource_revision"]
for index in range(1, 11):
    query = query_dir / f"ql-{index:03d}.rq"
    query.write_text("SELECT * WHERE { ?s ?p ?o }\n", encoding="utf-8")
    lines.append(
        "\t".join(
            [
                str(query),
                f"official#ql-{index:03d}",
                f"ql_query_{index:03d}",
                "QL",
                "official-text",
                "owl2bench-fixture",
                "fixture-revision",
            ]
        )
    )
query_manifest.write_text("\n".join(lines) + "\n", encoding="utf-8")
PY

python3 "$SCRIPT_DIR/graphdb_ql_materializer.py" identity \
  --dataset-root "$DATASET_ROOT" \
  --query-manifest "$QUERY_MANIFEST" \
  --output "$IDENTITY" \
  --component-manifest-output "$COMPONENT_MANIFEST"

jq -e '
  .profile == "QL"
  and .component.name == "QL-ruleset-engine"
  and .component.engine.name == "GraphDB"
  and .component.engine.version == "11.5.0"
  and .runtime.docker.image_digest == "sha256:d688879c4e5751a0c323f996d86bf1b2189191290711b1936e211d4510b53485"
  and .runtime.docker.platform == "linux/amd64"
  and .ruleset.id == "owl2-ql"
  and (.ruleset.conformance_boundary | contains("non-conformance"))
  and .configuration.repository_id == "owl2bench-ql-" + (.source_document.sha256 | sub("^sha256:";"") | .[0:16])
  and .query_workload.query_count == 10
  and .resource_limits.timeout_seconds == 600
  and .resource_limits.memory == "24GiB"
  and .resource_limits.threads == 1
  and .resource_limits.cpu == "1"
  and .resource_limits.variant == "owl2bench-comparable-baseline"
  and .license.status == "not_available"
  and (.license.digest | not)
  and .derivation.oracle_policy == "oracle_unavailable"
  and .graphdb.historical_graphdb_claim == "not_used"
  and (has("oracle") | not)
  and (has("answer_key") | not)
' "$IDENTITY" >/dev/null

GRAPHDB_LICENSE_DIGEST="sha256:0000000000000000000000000000000000000000000000000000000000000007" \
  python3 "$SCRIPT_DIR/graphdb_ql_materializer.py" identity \
  --dataset-root "$DATASET_ROOT" \
  --query-manifest "$QUERY_MANIFEST" \
  --output "$TMP_DIR/licensed-identity.json"
jq -e '
  .license.status == "available"
  and .license.id == "graphdb-runtime-license"
  and .license.digest == "sha256:0000000000000000000000000000000000000000000000000000000000000007"
' "$TMP_DIR/licensed-identity.json" >/dev/null

BAD_DATASET_ROOT="$TMP_DIR/bad-dataset"
cp -a "$DATASET_ROOT" "$BAD_DATASET_ROOT"
rm "$BAD_DATASET_ROOT/profiles/QL/profile-identity.json"
set +e
python3 "$SCRIPT_DIR/graphdb_ql_materializer.py" identity \
  --dataset-root "$BAD_DATASET_ROOT" \
  --query-manifest "$QUERY_MANIFEST" \
  --output "$TMP_DIR/bad-identity.json" > "$TMP_DIR/bad-identity.out" 2> "$TMP_DIR/bad-identity.err"
bad_identity_status="$?"
set -e
test "$bad_identity_status" = 125
grep -Fq '[graphdb-ql][not_available]' "$TMP_DIR/bad-identity.err"

jq -e '
  .profiles.QL.components.materializer.name == "QL-ruleset-engine"
  and .profiles.QL.components.engine.name == "graphdb-ql-rdf4j-exporter"
  and .profiles.QL.ruleset.id == "owl2-ql"
  and .profiles.QL.configuration.digest == .profiles.QL.configuration.digest
  and .profiles.QL.capability.supports_query == true
' "$COMPONENT_MANIFEST" >/dev/null

if GRAPHDB_QL_DOCKER_BIN="$TMP_DIR/missing-docker" python3 "$SCRIPT_DIR/profile_materialization.py" run \
  --profile QL \
  --source "$DATASET_ROOT/profiles/QL/source.nq" \
  --exporter "$SCRIPT_DIR/graphdb_ql_materializer.sh" \
  --identity "$IDENTITY" \
  --output-dir "$RUN_DIR" > "$TMP_DIR/profile-run.out" 2> "$TMP_DIR/profile-run.err"; then
  echo "assertion failed: missing local GraphDB runtime should not produce complete materialization" >&2
  exit 1
fi

jq -e '
  .execution_status == "not_available"
  and .profile == "QL"
  and (.reason | contains("profile exporter declared the profile materialization unavailable"))
' "$TMP_DIR/profile-run.out" >/dev/null

jq -e '
  .profiles.QL.status == "not_available"
  and .profiles.QL.component.name == "QL-ruleset-engine"
  and .profiles.QL.component.engine.name == "GraphDB"
  and .profiles.QL.ruleset.id == "owl2-ql"
  and .profiles.QL.graph_contract.source_included == true
  and .profiles.QL.graph_contract.anonymous_witness_policy == "not_exported"
  and .profiles.QL.graph_contract.ruleset_required == true
  and (.profiles.QL | has("oracle") | not)
  and (.profiles.QL | has("answer_key") | not)
' "$RUN_DIR/materialization.json" >/dev/null

python3 "$SCRIPT_DIR/materialization_manifest.py" validate \
  --materialization-manifest "$RUN_DIR/materialization.json" \
  --output "$MATERIALIZATION_CHECK"
jq -e '.profiles.QL.status == "not_available" and .answer_key_policy == "not_permitted"' \
  "$MATERIALIZATION_CHECK" >/dev/null

TIMEOUT_DOCKER="$TMP_DIR/timeout-docker"
cat > "$TIMEOUT_DOCKER" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
if [[ "${1:-}" == image && "${2:-}" == inspect ]]; then
  exit 0
fi
if [[ "${1:-}" == run ]]; then
  sleep 3
fi
exit 0
EOF
chmod +x "$TIMEOUT_DOCKER"
set +e
GRAPHDB_QL_DOCKER_BIN="$TIMEOUT_DOCKER" GRAPHDB_QL_TIMEOUT_SECONDS=1 \
  python3 "$SCRIPT_DIR/graphdb_ql_materializer.py" export \
  "$DATASET_ROOT/profiles/QL/source.nq" > "$TMP_DIR/timeout.out" 2> "$TMP_DIR/timeout.err"
timeout_status="$?"
set -e
test "$timeout_status" = 124
grep -Fq '[graphdb-ql][timeout]' "$TMP_DIR/timeout.err"

python3 "$SCRIPT_DIR/split_stack_manifest.py" validate \
  --component-manifest "$COMPONENT_MANIFEST" \
  --query-manifest "$QUERY_MANIFEST" \
  --output "$COMPONENT_CHECK"
jq -e '.profiles.QL.status == "not_available" and (.profiles.QL.reasons | index("license.not_available")) and .profiles.DL.status == "not_available"' \
  "$COMPONENT_CHECK" >/dev/null

! grep -R "GraphDB 9\\.0\\.0" "$SCRIPT_DIR"/graphdb_ql_* >/dev/null
