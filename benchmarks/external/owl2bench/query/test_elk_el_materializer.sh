#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
TMP_DIR="$(mktemp -d)"
trap 'rm -rf "$TMP_DIR"' EXIT

SOURCE="$TMP_DIR/source.nq"
OWLAPI_JAR="$TMP_DIR/elk-owlapi-0.6.0.jar"
CLI_JAR="$TMP_DIR/elk.jar"
CONFIG="$TMP_DIR/elk-el.properties"
EXPORTER="$TMP_DIR/elk-finite-exporter"
PROVENANCE="$TMP_DIR/provenance.json"

python3 - "$TMP_DIR" <<'PY'
import pathlib
import stat
import sys
import zipfile

root = pathlib.Path(sys.argv[1])
(root / "source.nq").write_text(
    "<https://example.test/a> <https://example.test/p> <https://example.test/b> .\n",
    encoding="utf-8",
)
with zipfile.ZipFile(root / "elk-owlapi-0.6.0.jar", "w") as archive:
    archive.writestr(
        "META-INF/maven/io.github.liveontologies/elk-owlapi/pom.properties",
        "artifactId=elk-owlapi\ngroupId=io.github.liveontologies\nversion=0.6.0\n",
    )
with zipfile.ZipFile(root / "elk.jar", "w") as archive:
    archive.writestr(
        "META-INF/maven/io.github.liveontologies/elk-cli/pom.properties",
        "artifactId=elk-cli\ngroupId=io.github.liveontologies\nversion=0.6.0\n",
    )
exporter = root / "elk-finite-exporter"
exporter.write_text(
    "#!/usr/bin/env bash\n"
    "set -euo pipefail\n"
    "[[ \"${1:-}\" == export && $# -eq 2 ]]\n"
    "[[ \"${ELK_EL_ENGINE_VERSION:-}\" == 0.6.0 ]]\n"
    "cat \"$2\"\n"
    "printf '%s\\n' '<https://example.test/a> <https://example.test/inferred> <https://example.test/c> .'\n",
    encoding="utf-8",
)
exporter.chmod(exporter.stat().st_mode | stat.S_IXUSR)
PY

owlapi_hash="sha256:$(sha256sum "$OWLAPI_JAR" | awk '{print $1}')"
cli_hash="sha256:$(sha256sum "$CLI_JAR" | awk '{print $1}')"
exporter_hash="sha256:$(sha256sum "$EXPORTER" | awk '{print $1}')"

cat > "$CONFIG" <<EOF
elk.version=0.6.0
elk.revision=fixture-official-0.6.0
elk.owlapi_jar=$OWLAPI_JAR
elk.owlapi_sha256=$owlapi_hash
elk.cli_jar=$CLI_JAR
elk.cli_sha256=$cli_hash
resource.timeout_seconds=60
resource.memory=1GiB
resource.threads=1
EOF

if ELK_EL_EXPORT_BOUNDARY_CONFIG="$CONFIG" \
  "$SCRIPT_DIR/elk_el_materializer.sh" export "$SOURCE" \
  > "$TMP_DIR/missing.out" 2> "$TMP_DIR/missing.stderr"; then
  echo "assertion failed: taxonomy-only ELK boundary was accepted" >&2
  exit 1
else
  test "$?" = 125
fi
jq -e '
  .status == "not_available"
  and .answer_key_policy == "not_permitted"
  and (.reason | test("classification/taxonomy output is not a complete"))
' "$TMP_DIR/missing.stderr" >/dev/null
test ! -s "$TMP_DIR/missing.out"

cat >> "$CONFIG" <<EOF
exporter.path=$EXPORTER
exporter.sha256=$exporter_hash
exporter.version=fixture-1.0.0
exporter.revision=fixture-r1
exporter.boundary=finite_query_complete_rdf_graph
EOF

ELK_EL_EXPORT_BOUNDARY_CONFIG="$CONFIG" \
ELK_EL_PROVENANCE_OUT="$PROVENANCE" \
  "$SCRIPT_DIR/elk_el_materializer.sh" export "$SOURCE" \
  > "$TMP_DIR/export.nq" 2> "$TMP_DIR/export.stderr"

grep -Fxq '<https://example.test/a> <https://example.test/p> <https://example.test/b> .' "$TMP_DIR/export.nq"
grep -Fxq '<https://example.test/a> <https://example.test/inferred> <https://example.test/c> .' "$TMP_DIR/export.nq"
jq -e --arg owlapi "$owlapi_hash" --arg cli "$cli_hash" --arg exporter "$exporter_hash" '
  .status == "complete"
  and .answer_key_policy == "not_permitted"
  and .engine.version == "0.6.0"
  and .engine.owlapi_jar.sha256 == $owlapi
  and .engine.cli_jar.sha256 == $cli
  and .exporter.sha256 == $exporter
  and .exporter.boundary == "finite_query_complete_rdf_graph"
  and (.source_graph.sha256 | startswith("sha256:"))
' "$PROVENANCE" >/dev/null

ELK_EL_EXPORT_BOUNDARY_CONFIG="$CONFIG" "$SCRIPT_DIR/elk_el_materializer.sh" version \
  > "$TMP_DIR/version.json"
jq -e '.engine.name == "ELK" and .engine.version == "0.6.0" and .profile == "EL"' \
  "$TMP_DIR/version.json" >/dev/null

cat > "$EXPORTER" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
printf '%s\n' '<https://example.test/partial> <https://example.test/p> <https://example.test/o> .'
exit 125
EOF
chmod +x "$EXPORTER"
exporter_hash="sha256:$(sha256sum "$EXPORTER" | awk '{print $1}')"
python3 - "$CONFIG" "$exporter_hash" <<'PY'
import pathlib
import sys

path = pathlib.Path(sys.argv[1])
lines = path.read_text(encoding="utf-8").splitlines()
path.write_text(
    "\n".join(
        f"exporter.sha256={sys.argv[2]}" if line.startswith("exporter.sha256=") else line
        for line in lines
    )
    + "\n",
    encoding="utf-8",
)
PY
if ELK_EL_EXPORT_BOUNDARY_CONFIG="$CONFIG" \
  "$SCRIPT_DIR/elk_el_materializer.sh" export "$SOURCE" \
  > "$TMP_DIR/unavailable.out" 2> "$TMP_DIR/unavailable.stderr"; then
  echo "assertion failed: unavailable finite exporter was accepted" >&2
  exit 1
else
  test "$?" = 125
fi
test ! -s "$TMP_DIR/unavailable.out"
jq -s -e '
  map(select(type == "object" and .status == "not_available")) | length >= 1
' "$TMP_DIR/unavailable.stderr" >/dev/null

echo "owl2bench ELK materializer boundary tests passed"
