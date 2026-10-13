#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
TMP_DIR="$(mktemp -d)"
trap 'rm -rf "$TMP_DIR"' EXIT

DATASET_ROOT="$TMP_DIR/datasets"
PROFILE_DIR="$DATASET_ROOT/profiles/EL"
ELK_ROOT="$TMP_DIR/elk"
QUERY_DIR="$TMP_DIR/queries"
mkdir -p "$PROFILE_DIR" "$DATASET_ROOT/generator" "$ELK_ROOT" "$QUERY_DIR"

SOURCE="$PROFILE_DIR/source.nq"
PROFILE_IDENTITY="$PROFILE_DIR/profile-identity.json"
QUERY_MANIFEST="$TMP_DIR/query-manifest.tsv"
QUERY_BACKEND="$TMP_DIR/query-backend"
IDENTITY="$TMP_DIR/elk-identity.json"
RUN_DIR="$TMP_DIR/run"
FAKE_ELK_JAR="$ELK_ROOT/elk.jar"
FAKE_OWLAPI_JAR="$ELK_ROOT/elk-owlapi.jar"
FAKE_JAVA="$TMP_DIR/java"

python3 - "$TMP_DIR" "$SOURCE" "$PROFILE_IDENTITY" "$QUERY_MANIFEST" "$QUERY_BACKEND" "$FAKE_ELK_JAR" "$FAKE_OWLAPI_JAR" "$FAKE_JAVA" <<'PY'
import hashlib
import json
import pathlib
import sys
import zipfile

root, source, profile_identity, query_manifest, query_backend, elk_jar, owlapi_jar, fake_java = map(pathlib.Path, sys.argv[1:])
profile_dir = source.parent
dataset_root = root / "datasets"
generator = dataset_root / "generator" / "OWL2Bench.jar"
generator_config = dataset_root / "generator" / "config.properties"
tbox = profile_dir / "profile-tbox.owl"
abox = profile_dir / "abox.nq"

source.write_text(
    "<http://example.test/a> <http://example.test/p> <http://example.test/b> .\n",
    encoding="utf-8",
)
generator.write_text("generator\n", encoding="utf-8")
generator_config.write_text("seed=1\n", encoding="utf-8")
tbox.write_text("<rdf>profile tbox</rdf>\n", encoding="utf-8")
abox.write_text("<http://example.test/a> <http://example.test/p> <http://example.test/b> .\n", encoding="utf-8")
for jar, artifact_id in ((elk_jar, "elk-cli"), (owlapi_jar, "elk-owlapi")):
    with zipfile.ZipFile(jar, "w") as archive:
        archive.writestr(
            f"META-INF/maven/io.github.liveontologies/{artifact_id}/pom.properties",
            f"artifactId={artifact_id}\ngroupId=io.github.liveontologies\nversion=0.6.0\n",
        )

def digest(path):
    return "sha256:" + hashlib.sha256(path.read_bytes()).hexdigest()

profile_identity.write_text(
    json.dumps(
        {
            "schema_version": 1,
            "profile": "EL",
            "status": "complete",
            "source_revision": "dataset-freezer-r1",
            "dataset": {
                "id": "owl2bench:EL:universities=1:seed=1",
                "profile": "EL",
                "seed": 1,
                "sha256": digest(source),
                "source_ontology_sha256": digest(source),
                "artifact_id": "profile-source-nquads",
                "artifact_revision": "dataset-freezer-r1",
                "artifact_path": str(source),
                "artifact_revision": "dataset-freezer-r1",
                "tbox_id": "owl2bench-tbox:EL",
                "tbox_sha256": digest(tbox),
                "abox_id": "owl2bench-abox:EL:universities=1:seed=1",
                "abox_sha256": digest(abox),
            },
            "generator": {
                "id": "OWL2Bench-generator",
                "revision": "dataset-freezer-r1",
                "seed": 1,
                "path": str(generator),
                "sha256": digest(generator),
            },
            "generator_configuration": {
                "id": "OWL2Bench-generator-config",
                "revision": "dataset-freezer-r1",
                "path": str(generator_config),
                "sha256": digest(generator_config),
            },
            "profile_context": {"id": "owl2bench-context:EL", "directory": str(profile_dir)},
            "profile_tbox": {
                "id": "owl2bench-tbox:EL",
                "path": str(tbox),
                "revision": "dataset-freezer-r1",
                "sha256": digest(tbox),
            },
            "profile_abox": {
                "id": "owl2bench-abox:EL",
                "path": str(abox),
                "revision": "dataset-freezer-r1",
                "sha256": digest(abox),
            },
            "resource_limits": {"timeout_seconds": 30, "memory": "1GiB", "threads": 1},
        },
        indent=2,
    )
    + "\n",
    encoding="utf-8",
)

query_manifest.write_text(
    "staged_query\tsource_query\tquery_id\tprofile\tworkload\tsource_corpus\tsource_revision\n",
    encoding="utf-8",
)
with query_manifest.open("a", encoding="utf-8") as handle:
    for index in range(1, 11):
        query = root / "queries" / f"query-{index:03d}.rq"
        query.write_text("SELECT * WHERE { ?s ?p ?o }\n", encoding="utf-8")
        handle.write(
            "\t".join(
                [
                    str(query),
                    f"official-query-{index:03d}.rq",
                    f"el_query_{index:03d}",
                    "EL",
                    "official-text",
                    "owl2bench-test",
                    "query-revision-r1",
                ]
            )
            + "\n"
        )

query_backend.write_text(
    "#!/usr/bin/env bash\n"
    "set -euo pipefail\n"
    "[[ \"${1:-}\" == query && $# -eq 3 ]]\n"
    "printf '%s\\n' '?s'\n",
    encoding="utf-8",
)
query_backend.chmod(0o755)

fake_java.write_text(
    "#!/usr/bin/env bash\n"
    "set -euo pipefail\n"
    "if [[ \"$*\" == *'--version'* || \"$*\" == *'-version'* ]]; then\n"
    "  printf '%s\\n' 'ELK Distribution Package for Command Line 0.6.0'\n"
    "  exit 0\n"
    "fi\n"
    "exit 2\n",
    encoding="utf-8",
)
fake_java.chmod(0o755)
PY

OWL2BENCH_PROFILE_DATASETS_ROOT="$DATASET_ROOT" \
ELK_EL_JAR="$FAKE_ELK_JAR" \
python3 "$SCRIPT_DIR/elk_el_identity.py" \
  --java "$FAKE_JAVA" \
  --query-manifest "$QUERY_MANIFEST" \
  --query-backend "$QUERY_BACKEND" \
  --output "$IDENTITY"

jq -e '
  .profile == "EL"
  and .component.name == "ELK"
  and .component.version == "0.6.0"
  and .exporter.boundary == "export <source.nq>"
  and .configuration.path
  and .query_backend.read_only == true
  and .query_workload.query_count == 10
  and .dataset.profile == "EL"
  and (.profile_capability_boundary | contains("insufficient"))
  and (.runtime_classpath.requested | length) > 0
' "$IDENTITY" >/dev/null

WRONG_SOURCE="$TMP_DIR/wrong-source.nq"
printf '%s\n' '<http://example.test/wrong> <http://example.test/p> <http://example.test/value> .' > "$WRONG_SOURCE"
set +e
OWL2BENCH_PROFILE_DATASETS_ROOT="$DATASET_ROOT" ELK_EL_JAR="$FAKE_ELK_JAR" \
  python3 "$SCRIPT_DIR/elk_el_identity.py" \
  --java "$FAKE_JAVA" \
  --source "$WRONG_SOURCE" \
  --query-manifest "$QUERY_MANIFEST" \
  --query-backend "$QUERY_BACKEND" \
  --output "$TMP_DIR/wrong-identity.json" > "$TMP_DIR/wrong-identity.out" 2> "$TMP_DIR/wrong-identity.err"
wrong_identity_status="$?"
set -e
test "$wrong_identity_status" = 125
grep -Fq 'source path does not match the frozen conversion artifact' "$TMP_DIR/wrong-identity.err"
jq -e '
  [
    paths
    | map(tostring)
    | join(".")
    | select(test("(^|\\.)(oracle|oracles|answer_key|answer_keys)$"))
  ]
  | length == 0
' "$IDENTITY" >/dev/null

set +e
ELK_EL_OWLAPI_JAR="$FAKE_OWLAPI_JAR" ELK_EL_CLI_JAR="$FAKE_ELK_JAR" python3 "$SCRIPT_DIR/profile_materialization.py" run \
  --profile EL \
  --source "$SOURCE" \
  --exporter "$SCRIPT_DIR/elk_el_materializer.sh" \
  --identity "$IDENTITY" \
  --output-dir "$RUN_DIR" > "$TMP_DIR/profile.out"
status="$?"
set -e
test "$status" = 1
test -s "$RUN_DIR/pipeline_summary.json" || {
  cat "$TMP_DIR/profile.out" >&2
  find "$RUN_DIR" -maxdepth 2 -type f -print >&2 2>/dev/null || true
  exit 1
}
jq -e '
  .profile == "EL"
  and .status == "not_available"
  and (.reason | contains("unavailable"))
' "$RUN_DIR/pipeline_summary.json" >/dev/null
jq -e '
  .profiles.EL.status == "not_available"
  and .profiles.EL.component.name == "ELK"
  and .profiles.EL.graph_contract.required_assertion_families == ["class","object_property","data_property","tbox","rbox"]
  and ((has("oracle")) | not)
  and ((has("answer_key")) | not)
' "$RUN_DIR/materialization.json" >/dev/null
jq -e '
  .status == "not_available"
  and .adapter == "elk-el-export-boundary"
  and (.reason | contains("not a complete OWL2Bench query graph"))
  and .answer_key_policy == "not_permitted"
' "$RUN_DIR/export.diagnostics" >/dev/null

set +e
ELK_EL_OWLAPI_JAR="$FAKE_OWLAPI_JAR" ELK_EL_CLI_JAR="$FAKE_ELK_JAR" "$SCRIPT_DIR/elk_el_materializer.sh" export "$SOURCE" \
  > "$TMP_DIR/export.out" 2> "$TMP_DIR/export.err"
direct_status="$?"
set -e
test "$direct_status" = 125
test ! -s "$TMP_DIR/export.out"
jq -e '.status == "not_available" and (.reason | contains("classification/taxonomy"))' \
  "$TMP_DIR/export.err" >/dev/null

TIMEOUT_EXPORTER="$TMP_DIR/timeout-exporter"
cat > "$TIMEOUT_EXPORTER" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
sleep 3
EOF
chmod +x "$TIMEOUT_EXPORTER"
TIMEOUT_CONFIG="$TMP_DIR/timeout.properties"
cat > "$TIMEOUT_CONFIG" <<EOF
elk.version=0.6.0
elk.owlapi_jar=$FAKE_OWLAPI_JAR
elk.cli_jar=$FAKE_ELK_JAR
exporter.path=$TIMEOUT_EXPORTER
exporter.boundary=finite_query_complete_rdf_graph
exporter.version=timeout-fixture
exporter.revision=timeout-fixture-r1
resource.timeout_seconds=1
EOF
set +e
python3 "$SCRIPT_DIR/elk_el_export_boundary.py" --config "$TIMEOUT_CONFIG" export "$SOURCE" \
  > "$TMP_DIR/timeout.out" 2> "$TMP_DIR/timeout.err"
timeout_status="$?"
set -e
test "$timeout_status" = 124
jq -e '.status == "timeout" and (.reason | contains("exceeded timeout"))' \
  "$TMP_DIR/timeout.err" >/dev/null

jq -e '
  .engine_version == "0.6.0"
  and .materialization_adapter == "elk-el-export-boundary"
  and .materialization_export_status == "not_available"
  and .taxonomy_output_is_query_complete_graph == false
' "$SCRIPT_DIR/elk-el-capabilities.json" >/dev/null

echo "[test-owl2bench-elk-el-export-boundary] ok"
