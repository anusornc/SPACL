#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ADAPTER="$SCRIPT_DIR/konclude_unified_engine.sh"
CAPABILITIES="$SCRIPT_DIR/konclude-unified-capabilities.json"
TMP_DIR="$(mktemp -d)"
cleanup() {
  rm -rf "$TMP_DIR"
}
trap cleanup EXIT

CONFIG="$TMP_DIR/querying-config.xml"
printf '<config/>\n' > "$CONFIG"
FAKE_BIN="$TMP_DIR/Konclude"
cat > "$FAKE_BIN" <<'ENGINE'
#!/usr/bin/env bash
set -euo pipefail

trace_file="${KONCLUDE_TRACE:?}"
if [[ "${1:-}" == "--version" ]]; then
  printf 'Konclude v0.7.0-1138\n'
  exit 0
fi
[[ "${1:-}" == "sparqlfile" ]]
shift
query_file=""
input_file=""
output_file=""
config_file=""
while [[ $# -gt 0 ]]; do
  case "$1" in
    -s) query_file="$2"; shift 2 ;;
    -i) input_file="$2"; shift 2 ;;
    -o) output_file="$2"; shift 2 ;;
    -c) config_file="$2"; shift 2 ;;
    -w) printf '%s\n' "$2" > "${trace_file}.threads"; shift 2 ;;
    -v) shift ;;
    *) exit 2 ;;
  esac
done
printf '%s\n' "$query_file" > "${trace_file}.query-path"
printf '%s\n' "$input_file" > "${trace_file}.input-path"
printf '%s\n' "$config_file" > "${trace_file}.config-path"
cat "$query_file" > "${trace_file}.query-body"
printf '%s\n' 'fake Konclude diagnostic' >&2
cat > "$output_file" <<'XML'
<?xml version="1.0"?>
<sparql xmlns="http://www.w3.org/2005/sparql-results#"><head><variable name="x"/></head><results><result><binding name="x"><uri>http://example.test/value</uri></binding></result></results></sparql>
XML
ENGINE
chmod +x "$FAKE_BIN"
export KONCLUDE_UNIFIED_VARIANT="synthetic-test"
export KONCLUDE_UNIFIED_EXPECTED_BINARY_SHA256="$(sha256sum "$FAKE_BIN" | awk '{print $1}')"
export KONCLUDE_UNIFIED_EXPECTED_CONFIG_SHA256="$(sha256sum "$CONFIG" | awk '{print $1}')"

EL_CONTEXT="$TMP_DIR/data/EL"
mkdir -p "$EL_CONTEXT"
printf 'Ontology(<urn:test:el>)\n' > "$EL_CONTEXT/ontology.ofn"
QUERY="$TMP_DIR/query.rq"
printf 'SELECT ?x WHERE { ?x ?p ?o }\n' > "$QUERY"
TRACE="$TMP_DIR/konclude-trace"
export KONCLUDE_TRACE="$TRACE"
export KONCLUDE_UNIFIED_BIN="$FAKE_BIN"
export KONCLUDE_UNIFIED_CONFIG="$CONFIG"

if "$ADAPTER" load "$EL_CONTEXT" > "$TMP_DIR/load.output" 2> "$TMP_DIR/load.stderr"; then
  load_status=0
else
  load_status=$?
fi
test "$load_status" = "0"
test -f "$EL_CONTEXT/.konclude-unified/provenance.json"
jq -e '.adapter == "konclude-unified" and .profile == "EL" and .source_kind == "functional-style" and .prepared_input == "'"$EL_CONTEXT"'/ontology.ofn" and .config_path == "'"$CONFIG"'"' \
  "$EL_CONTEXT/.konclude-unified/provenance.json" >/dev/null
if KONCLUDE_UNIFIED_VARIANT="konclude-unified" "$ADAPTER" load "$EL_CONTEXT" >/dev/null 2>/dev/null; then
  echo "assertion failed: synthetic binary passed the pinned variant" >&2
  exit 1
else
  test "$?" = "125"
fi

"$ADAPTER" query "$QUERY" "$EL_CONTEXT" > "$TMP_DIR/query.output" 2> "$TMP_DIR/query.stderr"
grep -q '^<?xml' "$TMP_DIR/query.output"
grep -q 'fake Konclude diagnostic' "$TMP_DIR/query.stderr"
if grep -q 'fake Konclude diagnostic' "$TMP_DIR/query.output"; then
  echo "assertion failed: Konclude diagnostics leaked into result data" >&2
  exit 1
fi
test "$(cat "${TRACE}.input-path")" = "$EL_CONTEXT/ontology.ofn"
test "$(cat "${TRACE}.config-path")" = "$CONFIG"
test "$(cat "${TRACE}.threads")" = "1"
expected_query="$(printf 'PREFIX rdf: <http://www.w3.org/1999/02/22-rdf-syntax-ns#>\nPREFIX : <https://kracr.iiitd.edu.in/OWL2Bench#>\nSELECT ?x WHERE { ?x ?p ?o }\n')"
test "$(cat "${TRACE}.query-body")" = "$expected_query"

RDF_CONTEXT="$TMP_DIR/data/DL"
mkdir -p "$RDF_CONTEXT"
printf '<rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"/>\n' > "$RDF_CONTEXT/source.owl"
CONVERTER="$TMP_DIR/convert.sh"
cat > "$CONVERTER" <<'CONVERTER'
#!/usr/bin/env bash
set -euo pipefail
[[ $# -eq 2 ]]
printf 'Ontology(<urn:test:converted>)\n' > "$2"
CONVERTER
chmod +x "$CONVERTER"
KONCLUDE_UNIFIED_CONVERTER="$CONVERTER" \
KONCLUDE_UNIFIED_EXPECTED_CONVERTER_SHA256="$(sha256sum "$CONVERTER" | awk '{print $1}')" \
"$ADAPTER" load "$RDF_CONTEXT" > "$TMP_DIR/rdf-load.output" 2> "$TMP_DIR/rdf-load.stderr"
jq -e '.source_kind == "rdf-xml" and .converter_path == "'"$CONVERTER"'" and (.prepared_input | endswith("converted.ofn"))' \
  "$RDF_CONTEXT/.konclude-unified/provenance.json" >/dev/null
test -f "$RDF_CONTEXT/.konclude-unified/converted.ofn"

MISSING_CONVERTER_CONTEXT="$TMP_DIR/data/QL"
mkdir -p "$MISSING_CONVERTER_CONTEXT"
printf '<rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"/>\n' > "$MISSING_CONVERTER_CONTEXT/source.owl"
if KONCLUDE_UNIFIED_CONVERTER="" "$ADAPTER" load "$MISSING_CONVERTER_CONTEXT" >/dev/null 2>/dev/null; then
  echo "assertion failed: missing converter was accepted" >&2
  exit 1
else
  test "$?" = "125"
fi

RUNTIME_CONTEXT="$TMP_DIR/data/RL"
mkdir -p "$RUNTIME_CONTEXT"
printf 'Ontology(<urn:test:runtime>)\n' > "$RUNTIME_CONTEXT/ontology.ofn"
RUNTIME_BIN="$TMP_DIR/runtime-missing"
cat > "$RUNTIME_BIN" <<'ENGINE'
#!/usr/bin/env bash
printf 'error while loading shared libraries: libpcre.so.3: cannot open shared object file: No such file or directory\n' >&2
exit 127
ENGINE
chmod +x "$RUNTIME_BIN"
if KONCLUDE_UNIFIED_BIN="$RUNTIME_BIN" \
  KONCLUDE_UNIFIED_EXPECTED_BINARY_SHA256="$(sha256sum "$RUNTIME_BIN" | awk '{print $1}')" \
  "$ADAPTER" load "$RUNTIME_CONTEXT" >/dev/null 2>/dev/null; then
  echo "assertion failed: missing runtime was accepted" >&2
  exit 1
else
  test "$?" = "125"
fi

FAILURE_BIN="$TMP_DIR/failing-konclude"
cat > "$FAILURE_BIN" <<'ENGINE'
#!/usr/bin/env bash
if [[ "${1:-}" == "--version" ]]; then
  printf 'Konclude v0.7.0-1138\n'
  exit 0
fi
printf 'Konclude query failure\n' >&2
exit 7
ENGINE
chmod +x "$FAILURE_BIN"
FAILURE_CONTEXT="$TMP_DIR/failure-data/DL"
mkdir -p "$FAILURE_CONTEXT"
printf 'Ontology(<urn:test:failure>)\n' > "$FAILURE_CONTEXT/ontology.ofn"
KONCLUDE_UNIFIED_BIN="$FAILURE_BIN" \
KONCLUDE_UNIFIED_EXPECTED_BINARY_SHA256="$(sha256sum "$FAILURE_BIN" | awk '{print $1}')" \
"$ADAPTER" load "$FAILURE_CONTEXT" >/dev/null
if KONCLUDE_UNIFIED_BIN="$FAILURE_BIN" "$ADAPTER" query "$QUERY" "$FAILURE_CONTEXT" >/dev/null 2>/dev/null; then
  echo "assertion failed: engine failure was accepted" >&2
  exit 1
else
  test "$?" = "7"
fi

jq -e '.supports_query == true and .data_mode == "directory" and (.profiles | sort) == ["DL", "EL", "QL", "RL"] and .adapter == "konclude-unified" and .engine_version == "v0.7.0-1138" and .release_commit == "0002e80635403960a7df5d93bd0e8f994d4952d0" and .release_asset_sha256 == "37216c18f45a80165aec1e76aeefd62d933d6a3f8f134c5aca0707c676fda76e" and .executable_sha256 == "5484f16dcff71486a5deed9cf9cea8a0f7febf115aaa6915ad2e8c1cf16965e3" and .query_config_sha256 == "03686d52957c07781745b069df1ecd1a9ae17ba4efd11cd82d21a0e22a6a3829" and .resource_limits.threads == 1 and .resource_limits.timeout_seconds == 600' \
  "$CAPABILITIES" >/dev/null

STAGED_CONTEXT="$TMP_DIR/staged"
mkdir -p "$STAGED_CONTEXT"
printf 'staged_query\tsource_query\tquery_id\tprofile\tworkload\tsource_corpus\tsource_revision\n' > "$STAGED_CONTEXT/manifest.tsv"
printf '%s\t%s\tadapter-query\tEL\tofficial-text\ttest-corpus\ttest-revision\n' \
  "$QUERY" "$TMP_DIR/source#adapter-query" >> "$STAGED_CONTEXT/manifest.tsv"
RUN_ROOT="$TMP_DIR/results"
KONCLUDE_UNIFIED_CONVERTER="" \
OWL2BENCH_QUERY_STAGED_DIR="$STAGED_CONTEXT" \
OWL2BENCH_QUERY_RESULTS_ROOT="$RUN_ROOT" \
OWL2BENCH_QUERY_ENGINE_BIN="$ADAPTER" \
OWL2BENCH_QUERY_CAPABILITIES_FILE="$CAPABILITIES" \
OWL2BENCH_QUERY_DATA_DIR="$TMP_DIR/data" \
KONCLUDE_UNIFIED_BIN="$FAKE_BIN" \
KONCLUDE_UNIFIED_CONFIG="$CONFIG" \
KONCLUDE_TRACE="$TRACE" \
RUN_ID="konclude-adapter-smoke" \
"$SCRIPT_DIR/run.sh" >/dev/null
jq -e '.query_count == 1 and .result_status_counts.oracle_unavailable == 1 and .correctness_gate == "blocked"' \
  "$RUN_ROOT/konclude-adapter-smoke/summary.json" >/dev/null
jq -e '.execution_status == "success" and .result_status == "oracle_unavailable" and .canonicalization.status == "available" and (.output | contains("fake Konclude diagnostic") | not) and (.diagnostics_file | endswith(".diagnostics")) and (.profile_context.provenance_file | endswith("provenance.json")) and .engine_provenance.source_kind == "functional-style" and .engine_provenance.prepared_input == "'"$EL_CONTEXT"'/ontology.ofn"' \
  "$RUN_ROOT/konclude-adapter-smoke/adapter-query__EL.json" >/dev/null
test -s "$(jq -r '.diagnostics_file' "$RUN_ROOT/konclude-adapter-smoke/adapter-query__EL.json")"
grep -q 'fake Konclude diagnostic' "$(jq -r '.diagnostics_file' "$RUN_ROOT/konclude-adapter-smoke/adapter-query__EL.json")"

MISSING_RUN_CONTEXT="$TMP_DIR/staged-missing-converter"
mkdir -p "$MISSING_RUN_CONTEXT"
printf 'staged_query\tsource_query\tquery_id\tprofile\tworkload\tsource_corpus\tsource_revision\n' > "$MISSING_RUN_CONTEXT/manifest.tsv"
printf '%s\t%s\tmissing-converter-query\tQL\tofficial-text\ttest-corpus\ttest-revision\n' \
  "$QUERY" "$TMP_DIR/source#missing-converter-query" >> "$MISSING_RUN_CONTEXT/manifest.tsv"
RUN_ROOT_MISSING="$TMP_DIR/results-missing-converter"
KONCLUDE_UNIFIED_CONVERTER="" \
OWL2BENCH_QUERY_STAGED_DIR="$MISSING_RUN_CONTEXT" \
OWL2BENCH_QUERY_RESULTS_ROOT="$RUN_ROOT_MISSING" \
OWL2BENCH_QUERY_ENGINE_BIN="$ADAPTER" \
OWL2BENCH_QUERY_CAPABILITIES_FILE="$CAPABILITIES" \
OWL2BENCH_QUERY_DATA_DIR="$TMP_DIR/data" \
KONCLUDE_UNIFIED_BIN="$FAKE_BIN" \
KONCLUDE_UNIFIED_CONFIG="$CONFIG" \
RUN_ID="konclude-missing-converter" \
"$SCRIPT_DIR/run.sh" >/dev/null
jq -e '.query_count == 1 and .result_status_counts.not_available == 1 and .correctness_gate == "blocked"' \
  "$RUN_ROOT_MISSING/konclude-missing-converter/summary.json" >/dev/null
jq -e '.execution_status == "not_available" and .result_status == "not_available"' \
  "$RUN_ROOT_MISSING/konclude-missing-converter/missing-converter-query__QL.json" >/dev/null

echo "[test-owl2bench-konclude-unified] ok"
