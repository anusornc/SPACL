#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
TMP_DIR="$(mktemp -d)"
trap 'rm -rf "$TMP_DIR"' EXIT

SOURCE="$TMP_DIR/source.nq"
EXPORTED="$TMP_DIR/exported.nq"
EXPORTER="$TMP_DIR/fake-exporter"
IDENTITY="$TMP_DIR/identity.json"
RUN_DIR="$TMP_DIR/run"
RUN_DIR_2="$TMP_DIR/run-2"
PERMUTED_A="$TMP_DIR/permuted-a.nq"
PERMUTED_B="$TMP_DIR/permuted-b.nq"
CANONICAL_A="$TMP_DIR/permuted-a-canonical.nq"
CANONICAL_B="$TMP_DIR/permuted-b-canonical.nq"
REFERENCE_INPUT="$TMP_DIR/urdna2015-reference.nq"
REFERENCE_EXPECTED="$TMP_DIR/urdna2015-reference-expected.nq"
REFERENCE_CANONICAL="$TMP_DIR/urdna2015-reference-canonical.nq"
QUERY="$TMP_DIR/query.rq"

python3 - "$SOURCE" "$EXPORTED" "$EXPORTER" "$IDENTITY" "$PERMUTED_A" "$PERMUTED_B" <<'PY'
import hashlib
import json
import pathlib
import sys

source, exported, exporter, identity, permuted_a, permuted_b = map(pathlib.Path, sys.argv[1:])
RDF_TYPE = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type"
RDFS_SUBCLASS = "http://www.w3.org/2000/01/rdf-schema#subClassOf"
RDFS_SUBPROPERTY = "http://www.w3.org/2000/01/rdf-schema#subPropertyOf"
XSD_INTEGER = "http://www.w3.org/2001/XMLSchema#integer"

source_lines = [
    '<http://example.test/alice> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <http://example.test/Person> .',
    '<http://example.test/alice> <http://example.test/knows> _:bob .',
    '_:bob <http://example.test/name> "Bob"@en .',
    '_:bob <http://example.test/age> "42"^^<http://www.w3.org/2001/XMLSchema#integer> .',
    '<http://example.test/Person> <http://www.w3.org/2000/01/rdf-schema#subClassOf> <http://example.test/Agent> .',
    '<http://example.test/knows> <http://www.w3.org/2000/01/rdf-schema#subPropertyOf> <http://example.test/related> .',
]
inferred_lines = [
    '<http://example.test/alice> <http://www.w3.org/1999/02/22-rdf-syntax-ns#type> <http://example.test/Agent> .',
    '<http://example.test/alice> <http://example.test/related> _:bob .',
]
source.write_text("\n".join(source_lines) + "\n", encoding="utf-8")
exported.write_text("\n".join(source_lines + inferred_lines) + "\n", encoding="utf-8")

root = source.parent
config = root / "fake-config.json"
engine = root / "fake-engine"
exporter = root / "fake-exporter"
query_backend = root / "fake-query-backend"
context = root / "context" / "DL"
context.mkdir(parents=True)
config.write_text('{"profile":"DL","mode":"fake"}\n', encoding="utf-8")
for executable, text in (
    (engine, "fake materializer 1\\n"),
    (query_backend,
     "#!/usr/bin/env python3\n"
     "import hashlib, json, pathlib, sys\n"
     "if len(sys.argv) != 5 or sys.argv[1] != 'query':\n"
     "    raise SystemExit(2)\n"
     "graph = pathlib.Path(sys.argv[3])\n"
     "handoff = json.loads(pathlib.Path(sys.argv[4]).read_text(encoding='utf-8'))\n"
     "actual = 'sha256:' + hashlib.sha256(graph.read_bytes()).hexdigest()\n"
     "if handoff.get('status') != 'ready' or handoff.get('read_only') is not True or handoff.get('graph_sha256') != actual:\n"
     "    raise SystemExit(3)\n"
     "print('query handoff accepted')\n"),
):
    executable.write_text(text, encoding="utf-8")
    executable.chmod(0o755)
exporter.write_text(
    "#!/usr/bin/env python3\n"
    "import pathlib, sys\n"
    "if sys.argv[1:] != ['export', sys.argv[2]]:\n"
    "    raise SystemExit(2)\n"
    f"sys.stdout.write(pathlib.Path({str(exported)!r}).read_text(encoding='utf-8'))\n",
    encoding="utf-8",
)
exporter.chmod(0o755)
query_backend.chmod(0o755)

def digest(path):
    return "sha256:" + hashlib.sha256(path.read_bytes()).hexdigest()

query = source.parent / "query.rq"
query.write_text(
    "SELECT ?x WHERE { ?x <http://example.test/knows> ?o }\n",
    encoding="utf-8",
)

identity.write_text(
    json.dumps(
        {
            "component": {
                "name": "fake-materializer",
                "version": "1.0.0",
                "revision": "fake-materializer-r1",
                "artifact_digest": digest(engine),
                "path": str(engine),
            },
            "exporter": {
                "name": "fake-exporter",
                "version": "1.0.0",
                "revision": "fake-exporter-r1",
                "artifact_digest": digest(exporter),
                "path": str(exporter),
            },
            "query_backend": {
                "name": "fake-query-backend",
                "version": "1.0.0",
                "revision": "fake-query-r1",
                "artifact_digest": digest(query_backend),
                "path": str(query_backend),
                "read_only": True,
            },
            "configuration": {
                "id": "fake-config",
                "path": str(config),
                "sha256": digest(config),
            },
            "derivation": {"version": "fake-pipeline/1", "revision": "fake-r1"},
            "runtime": {
                "id": "fake-runtime/python3",
                "dependencies": [
                    {"name": "python", "version": "3", "digest": digest(engine)}
                ],
            },
            "resource_limits": {"timeout_seconds": 600, "memory": "4GiB", "threads": 1, "cpu": "unspecified", "disk": "unspecified", "network": "test-local", "variant": "owl2bench-test-4GiB"},
            "profile_context": {"id": "fake-context-dl", "directory": str(context)},
            "query_workload": {"id": "fake-query-workload", "query_count": 1, "queries": [{"path": str(query), "sha256": digest(query)}]},
        },
        indent=2,
    )
    + "\n",
    encoding="utf-8",
)

permuted_a.write_text(
    "\n".join(
        [
            "_:a <http://example.test/p> <http://example.test/o1> .",
            "_:b <http://example.test/p> <http://example.test/o2> .",
        ]
    )
    + "\n",
    encoding="utf-8",
)
permuted_b.write_text(
    "\n".join(
        [
            "_:y <http://example.test/p> <http://example.test/o2> .",
            "_:x <http://example.test/p> <http://example.test/o1> .",
        ]
    )
    + "\n",
    encoding="utf-8",
)
PY

python3 "$SCRIPT_DIR/materialization_pipeline.py" run-fake \
  --profile DL \
  --source "$SOURCE" \
  --exporter "$EXPORTER" \
  --identity "$IDENTITY" \
  --output-dir "$RUN_DIR"

test -s "$RUN_DIR/candidate_graph.nq"
test -s "$RUN_DIR/graph.nq"
test -s "$RUN_DIR/materialization.json"
jq -e '(
  (has("oracle") | not)
  and (has("answer_key") | not)
  and ((.profiles.DL | has("oracle")) | not)
  and ((.profiles.DL | has("answer_key")) | not)
)' "$RUN_DIR/materialization.json" >/dev/null

if ! python3 "$SCRIPT_DIR/materialization_validator.py" validate \
  --profile DL \
  --source "$SOURCE" \
  --candidate-graph "$RUN_DIR/candidate_graph.nq" \
  --graph "$RUN_DIR/graph.nq" \
  --manifest "$RUN_DIR/materialization.json" \
  --output "$RUN_DIR/validation.json"; then
  cat "$RUN_DIR/validation.json" >&2
  exit 1
fi

jq -e '
  .status == "complete"
  and .profile == "DL"
  and .assertion_counts.class >= 1
  and .assertion_counts.object_property >= 1
  and .assertion_counts.data_property >= 1
  and .assertion_counts.tbox >= 1
  and .assertion_counts.rbox >= 1
  and .query_handoff.status == "ready"
  and .query_handoff.read_only == true
  and (.oracle == null)
' "$RUN_DIR/validation.json" >/dev/null

python3 "$SCRIPT_DIR/materialization_validator.py" handoff \
  --validation "$RUN_DIR/validation.json" \
  --graph "$RUN_DIR/graph.nq" \
  --output "$RUN_DIR/handoff.json"
EXPECTED_GRAPH_HASH="sha256:$(sha256sum "$RUN_DIR/graph.nq" | awk '{print $1}')"
jq -e --arg graph_hash "$EXPECTED_GRAPH_HASH" \
  '.status == "ready" and .read_only == true and .graph_sha256 == $graph_hash' \
  "$RUN_DIR/handoff.json" >/dev/null
"$TMP_DIR/fake-query-backend" query "$QUERY" "$RUN_DIR/graph.nq" "$RUN_DIR/handoff.json" \
  > "$TMP_DIR/query-output.txt"
grep -Fxq 'query handoff accepted' "$TMP_DIR/query-output.txt"

BYTE_DRIFT_GRAPH="$TMP_DIR/byte-drift-graph.nq"
sed '1i# byte drift that preserves the parsed RDF graph' "$RUN_DIR/graph.nq" > "$BYTE_DRIFT_GRAPH"
if python3 "$SCRIPT_DIR/materialization_validator.py" handoff \
  --validation "$RUN_DIR/validation.json" \
  --graph "$BYTE_DRIFT_GRAPH" \
  --output "$TMP_DIR/byte-drift-handoff.json"; then
  echo "assertion failed: handoff accepted byte-drifted graph" >&2
  exit 1
fi
jq -e '.status == "failed" and .reasons == ["validated_graph_identity_mismatch"]' \
  "$TMP_DIR/byte-drift-handoff.json" >/dev/null

python3 "$SCRIPT_DIR/materialization_pipeline.py" run-fake \
  --profile DL \
  --source "$SOURCE" \
  --exporter "$EXPORTER" \
  --identity "$IDENTITY" \
  --output-dir "$RUN_DIR_2" >/dev/null
python3 "$SCRIPT_DIR/materialization_validator.py" validate \
  --profile DL \
  --source "$SOURCE" \
  --candidate-graph "$RUN_DIR_2/candidate_graph.nq" \
  --graph "$RUN_DIR_2/graph.nq" \
  --manifest "$RUN_DIR_2/materialization.json" \
  --output "$RUN_DIR_2/validation.json" || {
  cat "$RUN_DIR_2/validation.json" >&2 || true
  exit 1
}
python3 "$SCRIPT_DIR/materialization_validator.py" handoff \
  --validation "$RUN_DIR_2/validation.json" \
  --graph "$RUN_DIR_2/graph.nq" \
  --output "$RUN_DIR_2/handoff.json"
test "$(sha256sum "$RUN_DIR/graph.nq" | awk '{print $1}')" = \
  "$(sha256sum "$RUN_DIR_2/graph.nq" | awk '{print $1}')"
jq -e '.status == "complete" and .query_handoff.status == "ready"' \
  "$RUN_DIR_2/validation.json" >/dev/null

RUNNER_ENGINE="$TMP_DIR/runner-engine"
RUNNER_CAPABILITIES="$TMP_DIR/runner-capabilities.json"
RUNNER_QUERY_MANIFEST="$TMP_DIR/runner-query-manifest.tsv"
RUNNER_RESULTS="$TMP_DIR/runner-results"
python3 - "$RUNNER_ENGINE" <<'PY'
import pathlib
import sys

path = pathlib.Path(sys.argv[1])
path.write_text(
    "#!/usr/bin/env python3\n"
    "import hashlib, json, os, pathlib, sys\n"
    "if sys.argv[1] != 'query' or len(sys.argv) != 3:\n"
    "    raise SystemExit(2)\n"
    "handoff_path = os.environ.get('OWL2BENCH_QUERY_MATERIALIZATION_HANDOFF')\n"
    "expected_hash = os.environ.get('OWL2BENCH_QUERY_MATERIALIZATION_GRAPH_SHA256')\n"
    "if not handoff_path or not expected_hash:\n"
    "    raise SystemExit(4)\n"
    "handoff = json.loads(pathlib.Path(handoff_path).read_text(encoding='utf-8'))\n"
    "graph = pathlib.Path(handoff['graph_path'])\n"
    "actual_hash = 'sha256:' + hashlib.sha256(graph.read_bytes()).hexdigest()\n"
    "if handoff.get('status') != 'ready' or handoff.get('read_only') is not True or expected_hash != actual_hash or handoff.get('graph_sha256') != actual_hash:\n"
    "    raise SystemExit(5)\n"
    "print('?x')\n"
    "print('<http://example.test/value>')\n",
    encoding="utf-8",
)
path.chmod(0o755)
PY
printf '%s\n' '{"supports_query":true,"profiles":["DL"],"data_mode":"query-file-only"}' > "$RUNNER_CAPABILITIES"
printf '%b\n%b\n' \
  'staged_query\tsource_query\tquery_id\tprofile\tworkload\tsource_corpus\tsource_revision' \
  "$QUERY\tofficial-query\trunner-query-001\tDL\tfake-workload\tfake-corpus\tfake-revision" \
  > "$RUNNER_QUERY_MANIFEST"
OWL2BENCH_QUERY_STAGED_DIR="$TMP_DIR" \
OWL2BENCH_QUERY_MANIFEST="$RUNNER_QUERY_MANIFEST" \
OWL2BENCH_QUERY_RESULTS_ROOT="$RUNNER_RESULTS" \
OWL2BENCH_QUERY_ENGINE_BIN="$RUNNER_ENGINE" \
OWL2BENCH_QUERY_CAPABILITIES_FILE="$RUNNER_CAPABILITIES" \
OWL2BENCH_QUERY_MATERIALIZATION_MANIFEST="$RUN_DIR/materialization.json" \
OWL2BENCH_QUERY_EXECUTION_ONLY=1 \
RUN_ID="fake-materialization-handoff" \
  "$SCRIPT_DIR/run.sh" >"$TMP_DIR/runner-stdout.txt" 2>"$TMP_DIR/runner-stderr.txt" || {
    cat "$TMP_DIR/runner-stdout.txt" >&2
    cat "$TMP_DIR/runner-stderr.txt" >&2
    exit 1
  }
RUNNER_RESULT="$RUNNER_RESULTS/fake-materialization-handoff/runner-query-001__DL.json"
RUNNER_GRAPH_HASH="sha256:$(sha256sum "$RUN_DIR/graph.nq" | awk '{print $1}')"
jq -e --arg graph_hash "$RUNNER_GRAPH_HASH" '
  .result_status == "oracle_unavailable"
  and .execution_only == true
  and ((has("oracle")) | not)
  and .materialization_handoff.status == "ready"
  and .materialization_handoff.read_only == true
  and .materialization_handoff.graph_sha256 == $graph_hash
' "$RUNNER_RESULT" >/dev/null

MISSING_VALIDATOR_RESULTS="$TMP_DIR/missing-validator-results"
MISSING_VALIDATOR_RUN_ID="missing-graph-validator"
OWL2BENCH_QUERY_STAGED_DIR="$TMP_DIR" \
OWL2BENCH_QUERY_MANIFEST="$RUNNER_QUERY_MANIFEST" \
OWL2BENCH_QUERY_RESULTS_ROOT="$MISSING_VALIDATOR_RESULTS" \
OWL2BENCH_QUERY_ENGINE_BIN="$RUNNER_ENGINE" \
OWL2BENCH_QUERY_CAPABILITIES_FILE="$RUNNER_CAPABILITIES" \
OWL2BENCH_QUERY_MATERIALIZATION_MANIFEST="$RUN_DIR/materialization.json" \
OWL2BENCH_QUERY_GRAPH_MATERIALIZATION_VALIDATOR="$TMP_DIR/missing-graph-validator.py" \
OWL2BENCH_QUERY_EXECUTION_ONLY=1 \
RUN_ID="$MISSING_VALIDATOR_RUN_ID" \
  "$SCRIPT_DIR/run.sh" >/dev/null
MISSING_VALIDATOR_RESULT="$MISSING_VALIDATOR_RESULTS/$MISSING_VALIDATOR_RUN_ID/runner-query-001__DL.json"
jq -e '
  .result_status == "not_available"
  and .execution_only == true
  and ((has("oracle")) | not)
  and .materialization_handoff == null
  and (.output | contains("graph_materialization_validator_missing"))
' "$MISSING_VALIDATOR_RESULT" >/dev/null

python3 "$SCRIPT_DIR/rdf_canonicalization.py" canonicalize \
  --input "$PERMUTED_A" --output "$CANONICAL_A"
python3 "$SCRIPT_DIR/rdf_canonicalization.py" canonicalize \
  --input "$PERMUTED_B" --output "$CANONICAL_B"
test "$(sha256sum "$CANONICAL_A" | awk '{print $1}')" = "$(sha256sum "$CANONICAL_B" | awk '{print $1}')"

MULTI_A="$TMP_DIR/multi-related-a.nq"
MULTI_B="$TMP_DIR/multi-related-b.nq"
MULTI_CANONICAL_A="$TMP_DIR/multi-related-a-canonical.nq"
MULTI_CANONICAL_B="$TMP_DIR/multi-related-b-canonical.nq"
printf '%s\n' \
  '_:root <http://example.test/p> _:left _:graph .' \
  '_:left <http://example.test/q> <http://example.test/o1> .' \
  '_:graph <http://example.test/r> <http://example.test/o2> .' > "$MULTI_A"
printf '%s\n' \
  '_:z <http://example.test/p> _:y _:x .' \
  '_:y <http://example.test/q> <http://example.test/o1> .' \
  '_:x <http://example.test/r> <http://example.test/o2> .' > "$MULTI_B"
python3 "$SCRIPT_DIR/rdf_canonicalization.py" canonicalize \
  --input "$MULTI_A" --output "$MULTI_CANONICAL_A"
python3 "$SCRIPT_DIR/rdf_canonicalization.py" canonicalize \
  --input "$MULTI_B" --output "$MULTI_CANONICAL_B"
test "$(sha256sum "$MULTI_CANONICAL_A" | awk '{print $1}')" = "$(sha256sum "$MULTI_CANONICAL_B" | awk '{print $1}')"

printf '%s\n' \
  '<http://example.org/vocab#test> <http://example.org/vocab#A> _:e0 .' \
  '<http://example.org/vocab#test> <http://example.org/vocab#B> _:e1 .' \
  '_:e0 <http://example.org/vocab#next> _:e2 .' \
  '_:e1 <http://example.org/vocab#next> _:e2 .' > "$REFERENCE_INPUT"
printf '%s\n' \
  '<http://example.org/vocab#test> <http://example.org/vocab#A> _:c14n2 .' \
  '<http://example.org/vocab#test> <http://example.org/vocab#B> _:c14n0 .' \
  '_:c14n0 <http://example.org/vocab#next> _:c14n1 .' \
  '_:c14n2 <http://example.org/vocab#next> _:c14n1 .' > "$REFERENCE_EXPECTED"
python3 "$SCRIPT_DIR/rdf_canonicalization.py" canonicalize \
  --input "$REFERENCE_INPUT" --output "$REFERENCE_CANONICAL"
cmp -s "$REFERENCE_EXPECTED" "$REFERENCE_CANONICAL"

ESCAPED_TERMS="$TMP_DIR/escaped-terms.nq"
UTF8_TERMS="$TMP_DIR/utf8-terms.nq"
ESCAPED_TERMS_CANONICAL="$TMP_DIR/escaped-terms-canonical.nq"
UTF8_TERMS_CANONICAL="$TMP_DIR/utf8-terms-canonical.nq"
printf '%s\n' \
  '<http://example.org/caf\u00E9> <http://example.org/p> "\u00E9" .' > "$ESCAPED_TERMS"
printf '%s\n' \
  '<http://example.org/café> <http://example.org/p> "é" .' > "$UTF8_TERMS"
python3 "$SCRIPT_DIR/rdf_canonicalization.py" canonicalize \
  --input "$ESCAPED_TERMS" --output "$ESCAPED_TERMS_CANONICAL"
python3 "$SCRIPT_DIR/rdf_canonicalization.py" canonicalize \
  --input "$UTF8_TERMS" --output "$UTF8_TERMS_CANONICAL"
cmp -s "$ESCAPED_TERMS_CANONICAL" "$UTF8_TERMS_CANONICAL"
grep -Fq '<http://example.org/café> <http://example.org/p> "é" .' \
  "$ESCAPED_TERMS_CANONICAL"

BAD_GRAPH="$TMP_DIR/bad-graph.nq"
cp "$RUN_DIR/graph.nq" "$BAD_GRAPH"
printf '%s\n' '<http://example.test/unvalidated> <http://example.test/p> <http://example.test/o> .' >> "$BAD_GRAPH"
if python3 "$SCRIPT_DIR/materialization_validator.py" handoff \
  --validation "$RUN_DIR/validation.json" \
  --graph "$BAD_GRAPH" \
  --output "$TMP_DIR/bad-handoff.json"; then
  echo "assertion failed: handoff accepted a graph with a different digest" >&2
  exit 1
fi
jq -e '.status == "failed" and .reasons == ["validated_graph_identity_mismatch"]' \
  "$TMP_DIR/bad-handoff.json" >/dev/null

MALFORMED_GRAPH="$TMP_DIR/malformed-graph.nq"
MALFORMED_MANIFEST="$TMP_DIR/malformed-manifest.json"
printf '%s\n' 'this is not N-Quads' > "$MALFORMED_GRAPH"
jq --arg path "$MALFORMED_GRAPH" \
  '.profiles.DL.output_graph.path = $path' \
  "$RUN_DIR/materialization.json" > "$MALFORMED_MANIFEST"
if python3 "$SCRIPT_DIR/materialization_validator.py" validate \
  --profile DL \
  --source "$SOURCE" \
  --candidate-graph "$RUN_DIR/candidate_graph.nq" \
  --graph "$MALFORMED_GRAPH" \
  --manifest "$MALFORMED_MANIFEST" \
  --output "$TMP_DIR/malformed-validation.json"; then
  echo "assertion failed: malformed graph passed validation" >&2
  exit 1
fi
jq -e '.status == "failed" and ([.reasons[] | select(startswith("graph_malformed:"))] | length) == 1' \
  "$TMP_DIR/malformed-validation.json" >/dev/null

HASH_MANIFEST="$TMP_DIR/hash-mismatch-manifest.json"
jq '.profiles.DL.output_sha256 = "sha256:0000000000000000000000000000000000000000000000000000000000000000"' \
  "$RUN_DIR/materialization.json" > "$HASH_MANIFEST"
if python3 "$SCRIPT_DIR/materialization_validator.py" validate \
  --profile DL \
  --source "$SOURCE" \
  --candidate-graph "$RUN_DIR/candidate_graph.nq" \
  --graph "$RUN_DIR/graph.nq" \
  --manifest "$HASH_MANIFEST" \
  --output "$TMP_DIR/hash-validation.json"; then
  echo "assertion failed: graph hash mismatch passed validation" >&2
  exit 1
fi
jq -e '.status == "failed" and ([.reasons[] | select(. == "output_hash_mismatch")] | length) == 1' \
  "$TMP_DIR/hash-validation.json" >/dev/null

MISSING_CONFIG_WITH_HASH_MANIFEST="$TMP_DIR/missing-config-with-hash-manifest.json"
jq --arg config "$TMP_DIR/missing-config.json" \
  '.profiles.DL.configuration.path = $config
   | .profiles.DL.output_sha256 = "sha256:0000000000000000000000000000000000000000000000000000000000000000"' \
  "$RUN_DIR/materialization.json" > "$MISSING_CONFIG_WITH_HASH_MANIFEST"
if python3 "$SCRIPT_DIR/materialization_validator.py" validate \
  --profile DL \
  --source "$SOURCE" \
  --candidate-graph "$RUN_DIR/candidate_graph.nq" \
  --graph "$RUN_DIR/graph.nq" \
  --manifest "$MISSING_CONFIG_WITH_HASH_MANIFEST" \
  --output "$TMP_DIR/missing-config-with-hash-validation.json"; then
  echo "assertion failed: missing config with hash mismatch passed validation" >&2
  exit 1
fi
jq -e '
  .status == "failed"
  and ([.reasons[] | select(. == "output_hash_mismatch")] | length) == 1
  and ([.reasons[] | select(startswith("configuration_missing:"))] | length) == 1
' "$TMP_DIR/missing-config-with-hash-validation.json" >/dev/null

FORBIDDEN_MANIFEST="$TMP_DIR/forbidden-manifest.json"
jq '.profiles.DL.oracle = {}' "$RUN_DIR/materialization.json" > "$FORBIDDEN_MANIFEST"
if python3 "$SCRIPT_DIR/materialization_validator.py" validate \
  --profile DL \
  --source "$SOURCE" \
  --candidate-graph "$RUN_DIR/candidate_graph.nq" \
  --graph "$RUN_DIR/graph.nq" \
  --manifest "$FORBIDDEN_MANIFEST" \
  --output "$TMP_DIR/forbidden-validation.json"; then
  echo "assertion failed: forbidden oracle field passed validation" >&2
  exit 1
fi
jq -e '.status == "failed" and .reasons == ["forbidden_field:profiles.DL.oracle"]' \
  "$TMP_DIR/forbidden-validation.json" >/dev/null

if python3 "$SCRIPT_DIR/materialization_validator.py" validate \
  --profile DL \
  --source "$SOURCE" \
  --candidate-graph "$RUN_DIR/candidate_graph.nq" \
  --graph "$RUN_DIR/graph.nq" \
  --manifest "$TMP_DIR/missing-materialization.json" \
  --output "$TMP_DIR/missing-manifest-validation.json"; then
  echo "assertion failed: missing manifest passed validation" >&2
  exit 1
fi
jq -e '.status == "not_available" and ([.reasons[] | select(startswith("manifest missing:"))] | length) == 1' \
  "$TMP_DIR/missing-manifest-validation.json" >/dev/null

MISSING_SOURCE_MANIFEST="$TMP_DIR/missing-source-manifest.json"
jq --arg source "$TMP_DIR/missing-source.nq" \
  '.profiles.DL.source_ontology.path = $source' \
  "$RUN_DIR/materialization.json" > "$MISSING_SOURCE_MANIFEST"
if python3 "$SCRIPT_DIR/materialization_validator.py" validate \
  --profile DL \
  --source "$TMP_DIR/missing-source.nq" \
  --candidate-graph "$RUN_DIR/candidate_graph.nq" \
  --graph "$RUN_DIR/graph.nq" \
  --manifest "$MISSING_SOURCE_MANIFEST" \
  --output "$TMP_DIR/missing-source-validation.json"; then
  echo "assertion failed: missing source passed validation" >&2
  exit 1
fi
jq -e '.status == "not_available" and ([.reasons[] | select(startswith("artifact_missing:"))] | length) == 1' \
  "$TMP_DIR/missing-source-validation.json" >/dev/null

for missing_kind in source exporter identity; do
  MISSING_INPUT_RUN="$TMP_DIR/missing-input-$missing_kind"
  case "$missing_kind" in
    source)
      MISSING_SOURCE="$TMP_DIR/missing-input-source.nq"
      MISSING_EXPORTER="$EXPORTER"
      MISSING_IDENTITY="$IDENTITY"
      ;;
    exporter)
      MISSING_SOURCE="$SOURCE"
      MISSING_EXPORTER="$TMP_DIR/missing-input-exporter"
      MISSING_IDENTITY="$IDENTITY"
      ;;
    identity)
      MISSING_SOURCE="$SOURCE"
      MISSING_EXPORTER="$EXPORTER"
      MISSING_IDENTITY="$TMP_DIR/missing-input-identity.json"
      ;;
  esac
  if python3 "$SCRIPT_DIR/materialization_pipeline.py" run-fake \
    --profile DL \
    --source "$MISSING_SOURCE" \
    --exporter "$MISSING_EXPORTER" \
    --identity "$MISSING_IDENTITY" \
    --output-dir "$MISSING_INPUT_RUN"; then
    echo "assertion failed: missing $missing_kind returned success" >&2
    exit 1
  fi
  test -s "$MISSING_INPUT_RUN/pipeline_summary.json"
  test -s "$MISSING_INPUT_RUN/materialization.json"
  jq -e --arg kind "$missing_kind" \
    '.status == "not_available" and (.reason | length > 0) and (.exporter.path | length > 0)' \
    "$MISSING_INPUT_RUN/pipeline_summary.json" >/dev/null
  jq -e --arg kind "$missing_kind" \
    '.profiles.DL.status == "not_available" and (.profiles.DL.reason | length > 0)' \
    "$MISSING_INPUT_RUN/materialization.json" >/dev/null
done

MALFORMED_IDENTITY="$TMP_DIR/malformed-identity.json"
MALFORMED_RUN="$TMP_DIR/malformed-identity-run"
printf '%s\n' '{not valid json' > "$MALFORMED_IDENTITY"
if python3 "$SCRIPT_DIR/materialization_pipeline.py" run-fake \
  --profile DL \
  --source "$SOURCE" \
  --exporter "$EXPORTER" \
  --identity "$MALFORMED_IDENTITY" \
  --output-dir "$MALFORMED_RUN"; then
  echo "assertion failed: malformed identity returned success" >&2
  exit 1
fi
jq -e '.status == "failed" and (.reason | contains("identity manifest is malformed"))' \
  "$MALFORMED_RUN/pipeline_summary.json" >/dev/null
jq -e '.profiles.DL.status == "failed"' \
  "$MALFORMED_RUN/materialization.json" >/dev/null

MISMATCH_IDENTITY="$TMP_DIR/mismatch-identity.json"
MISMATCH_RUN="$TMP_DIR/mismatch-identity-run"
jq '.component.artifact_digest = "sha256:0000000000000000000000000000000000000000000000000000000000000000"' \
  "$IDENTITY" > "$MISMATCH_IDENTITY"
if python3 "$SCRIPT_DIR/materialization_pipeline.py" run-fake \
  --profile DL \
  --source "$SOURCE" \
  --exporter "$EXPORTER" \
  --identity "$MISMATCH_IDENTITY" \
  --output-dir "$MISMATCH_RUN"; then
  echo "assertion failed: identity hash mismatch returned success" >&2
  exit 1
fi
jq -e '.status == "failed" and (.reason | contains("artifact_digest does not match"))' \
  "$MISMATCH_RUN/pipeline_summary.json" >/dev/null
jq -e '.profiles.DL.status == "failed"' \
  "$MISMATCH_RUN/materialization.json" >/dev/null

for state in unsupported failed not_available timeout; do
  STATE_MANIFEST="$TMP_DIR/$state-manifest.json"
  STATE_VALIDATION="$TMP_DIR/$state-validation.json"
  jq --arg state "$state" \
    '.profiles.DL.status = $state | .profiles.DL.reason = ("synthetic " + $state)' \
    "$RUN_DIR/materialization.json" > "$STATE_MANIFEST"
  if python3 "$SCRIPT_DIR/materialization_validator.py" validate \
    --profile DL \
    --source "$SOURCE" \
    --candidate-graph "$RUN_DIR/candidate_graph.nq" \
    --graph "$RUN_DIR/graph.nq" \
    --manifest "$STATE_MANIFEST" \
    --output "$STATE_VALIDATION"; then
    echo "assertion failed: $state materialization passed validation" >&2
    exit 1
  fi
  jq -e --arg state "$state" '.status == $state and .query_handoff.status == "not_ready"' \
    "$STATE_VALIDATION" >/dev/null
done

TIMEOUT_EXPORTER="$TMP_DIR/timeout-exporter"
TIMEOUT_IDENTITY="$TMP_DIR/timeout-identity.json"
TIMEOUT_RUN="$TMP_DIR/timeout-run"
python3 - "$TIMEOUT_EXPORTER" <<'PY'
import pathlib
import sys

path = pathlib.Path(sys.argv[1])
path.write_text(
    "#!/usr/bin/env python3\n"
    "import time\n"
    "time.sleep(2)\n",
    encoding="utf-8",
)
path.chmod(0o755)
PY
jq '.resource_limits.timeout_seconds = 1' "$IDENTITY" > "$TIMEOUT_IDENTITY"
if python3 "$SCRIPT_DIR/materialization_pipeline.py" run-fake \
  --profile DL \
  --source "$SOURCE" \
  --exporter "$TIMEOUT_EXPORTER" \
  --identity "$TIMEOUT_IDENTITY" \
  --output-dir "$TIMEOUT_RUN"; then
  echo "assertion failed: timed-out exporter returned success" >&2
  exit 1
fi
if python3 "$SCRIPT_DIR/materialization_validator.py" validate \
  --profile DL \
  --source "$SOURCE" \
  --candidate-graph "$TIMEOUT_RUN/candidate_graph.nq" \
  --graph "$TIMEOUT_RUN/graph.nq" \
  --manifest "$TIMEOUT_RUN/materialization.json" \
  --output "$TMP_DIR/timeout-pipeline-validation.json"; then
  echo "assertion failed: timeout materialization passed validation" >&2
  exit 1
fi
jq -e '.status == "timeout" and .query_handoff.status == "not_ready"' \
  "$TMP_DIR/timeout-pipeline-validation.json" >/dev/null

WITNESS_EXPORTER="$TMP_DIR/witness-exporter"
WITNESS_IDENTITY="$TMP_DIR/witness-identity.json"
WITNESS_RUN="$TMP_DIR/witness-run"
python3 - "$WITNESS_EXPORTER" "$EXPORTED" <<'PY'
import pathlib
import sys

path, graph = map(pathlib.Path, sys.argv[1:])
path.write_text(
    "#!/usr/bin/env python3\n"
    "import pathlib, sys\n"
    "if sys.argv[1:] != ['export', sys.argv[2]]:\n"
    "    raise SystemExit(2)\n"
    f"sys.stdout.write(pathlib.Path({str(graph)!r}).read_text(encoding='utf-8'))\n"
    "print('_:generated <http://example.test/related> <http://example.test/o> .')\n",
    encoding="utf-8",
)
path.chmod(0o755)
PY
WITNESS_HASH="sha256:$(sha256sum "$WITNESS_EXPORTER" | awk '{print $1}')"
jq --arg path "$WITNESS_EXPORTER" --arg digest "$WITNESS_HASH" \
  '.exporter.path = $path | .exporter.artifact_digest = $digest' \
  "$IDENTITY" > "$WITNESS_IDENTITY"
python3 "$SCRIPT_DIR/materialization_pipeline.py" run-fake \
  --profile DL \
  --source "$SOURCE" \
  --exporter "$WITNESS_EXPORTER" \
  --identity "$WITNESS_IDENTITY" \
  --output-dir "$WITNESS_RUN" >/dev/null
if python3 "$SCRIPT_DIR/materialization_validator.py" validate \
  --profile DL \
  --source "$SOURCE" \
  --candidate-graph "$WITNESS_RUN/candidate_graph.nq" \
  --graph "$WITNESS_RUN/graph.nq" \
  --manifest "$WITNESS_RUN/materialization.json" \
  --output "$TMP_DIR/witness-validation.json"; then
  echo "assertion failed: generated witness passed validation" >&2
  exit 1
fi
jq -e '.status == "failed" and ([.reasons[] | select(. == "generated_anonymous_witness_not_allowed")] | length) == 1' \
  "$TMP_DIR/witness-validation.json" >/dev/null

NO_DATA_SOURCE="$TMP_DIR/no-data-source.nq"
NO_DATA_EXPORTED="$TMP_DIR/no-data-exported.nq"
NO_DATA_EXPORTER="$TMP_DIR/no-data-exporter"
NO_DATA_IDENTITY="$TMP_DIR/no-data-identity.json"
NO_DATA_RUN="$TMP_DIR/no-data-run"
python3 - "$SOURCE" "$NO_DATA_SOURCE" "$NO_DATA_EXPORTED" "$NO_DATA_EXPORTER" <<'PY'
import pathlib
import sys

source, no_data_source, no_data_exported, exporter = map(pathlib.Path, sys.argv[1:])
lines = [line for line in source.read_text(encoding='utf-8').splitlines() if '<http://example.test/name>' not in line and '<http://example.test/age>' not in line]
payload = '\n'.join(lines) + '\n'
no_data_source.write_text(payload, encoding='utf-8')
no_data_exported.write_text(payload, encoding='utf-8')
exporter.write_text(
    "#!/usr/bin/env python3\n"
    "import pathlib, sys\n"
    "if sys.argv[1:] != ['export', sys.argv[2]]:\n"
    "    raise SystemExit(2)\n"
    f"sys.stdout.write(pathlib.Path({str(no_data_exported)!r}).read_text(encoding='utf-8'))\n",
    encoding='utf-8',
)
exporter.chmod(0o755)
PY
NO_DATA_HASH="sha256:$(sha256sum "$NO_DATA_EXPORTER" | awk '{print $1}')"
jq --arg source "$NO_DATA_SOURCE" --arg path "$NO_DATA_EXPORTER" --arg digest "$NO_DATA_HASH" \
  '.exporter.path = $path | .exporter.artifact_digest = $digest' \
  "$IDENTITY" > "$NO_DATA_IDENTITY"
python3 "$SCRIPT_DIR/materialization_pipeline.py" run-fake \
  --profile DL \
  --source "$NO_DATA_SOURCE" \
  --exporter "$NO_DATA_EXPORTER" \
  --identity "$NO_DATA_IDENTITY" \
  --output-dir "$NO_DATA_RUN" >/dev/null
if python3 "$SCRIPT_DIR/materialization_validator.py" validate \
  --profile DL \
  --source "$NO_DATA_SOURCE" \
  --candidate-graph "$NO_DATA_RUN/candidate_graph.nq" \
  --graph "$NO_DATA_RUN/graph.nq" \
  --manifest "$NO_DATA_RUN/materialization.json" \
  --output "$TMP_DIR/no-data-validation.json"; then
  echo "assertion failed: missing assertion family passed validation" >&2
  exit 1
fi
jq -e '.status == "failed" and ([.reasons[] | select(. == "assertion_family_missing:data_property")] | length) == 1' \
  "$TMP_DIR/no-data-validation.json" >/dev/null

for mode in failed unsupported unavailable empty; do
  STATUS_EXPORTER="$TMP_DIR/$mode-exporter"
  STATUS_IDENTITY="$TMP_DIR/$mode-identity.json"
  STATUS_RUN="$TMP_DIR/$mode-run"
  python3 - "$STATUS_EXPORTER" "$mode" <<'PY'
import pathlib
import sys

path, mode = map(pathlib.Path, sys.argv[1:])
if mode == pathlib.Path('failed'):
    payload = "#!/usr/bin/env python3\nraise SystemExit(7)\n"
elif mode == pathlib.Path('unsupported'):
    payload = "#!/usr/bin/env python3\nraise SystemExit(123)\n"
elif mode == pathlib.Path('unavailable'):
    payload = "#!/usr/bin/env python3\nraise SystemExit(125)\n"
else:
    payload = "#!/usr/bin/env python3\n"
path.write_text(payload, encoding='utf-8')
path.chmod(0o755)
PY
  STATUS_HASH="sha256:$(sha256sum "$STATUS_EXPORTER" | awk '{print $1}')"
  jq --arg path "$STATUS_EXPORTER" --arg digest "$STATUS_HASH" \
    '.exporter.path = $path | .exporter.artifact_digest = $digest' \
    "$IDENTITY" > "$STATUS_IDENTITY"
  if python3 "$SCRIPT_DIR/materialization_pipeline.py" run-fake \
    --profile DL \
    --source "$SOURCE" \
    --exporter "$STATUS_EXPORTER" \
    --identity "$STATUS_IDENTITY" \
    --output-dir "$STATUS_RUN"; then
    echo "assertion failed: $mode exporter returned success" >&2
    exit 1
  fi
  if python3 "$SCRIPT_DIR/materialization_validator.py" validate \
    --profile DL \
    --source "$SOURCE" \
    --candidate-graph "$STATUS_RUN/candidate_graph.nq" \
    --graph "$STATUS_RUN/graph.nq" \
    --manifest "$STATUS_RUN/materialization.json" \
    --output "$TMP_DIR/$mode-validation.json"; then
    echo "assertion failed: $mode materialization passed validation" >&2
    exit 1
  fi
  expected_status="$mode"
  if [[ "$mode" == "empty" ]]; then
    expected_status="failed"
  elif [[ "$mode" == "unavailable" ]]; then
    expected_status="not_available"
  fi
  jq -e --arg expected "$expected_status" '.status == $expected and .query_handoff.status == "not_ready"' \
    "$TMP_DIR/$mode-validation.json" >/dev/null
done

echo "[test-owl2bench-materialization-pipeline] ok"
