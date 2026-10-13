#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
CANONICALIZER="$SCRIPT_DIR/canonicalize_result.py"
TMP_DIR="$(mktemp -d)"
cleanup() {
  rm -rf "$TMP_DIR"
}
trap cleanup EXIT

cat > "$TMP_DIR/query.rq" <<'QUERY'
SELECT ?x ?y WHERE { ?x ?p ?y }
QUERY
cat > "$TMP_DIR/result.xml" <<'XML'
<?xml version="1.0"?>
<sparql xmlns="http://www.w3.org/2005/sparql-results#">
  <head><variable name="y"/><variable name="x"/></head>
  <results>
    <result>
      <binding name="x"><uri>http://example.test/z</uri></binding>
      <binding name="y"><literal xml:lang="EN">hello</literal></binding>
    </result>
    <result>
      <binding name="x"><bnode>alpha</bnode></binding>
      <binding name="y"><literal datatype="http://www.w3.org/2001/XMLSchema#integer">01</literal></binding>
    </result>
  </results>
</sparql>
XML
cat > "$TMP_DIR/result.tsv" <<'TSV'
?y	?x
"01"^^<http://www.w3.org/2001/XMLSchema#integer>	_:renamed
"hello"@en	<http://example.test/z>
TSV
python3 "$CANONICALIZER" --query "$TMP_DIR/query.rq" --input "$TMP_DIR/result.xml" --output "$TMP_DIR/xml.json"
python3 "$CANONICALIZER" --query "$TMP_DIR/query.rq" --input "$TMP_DIR/result.tsv" --output "$TMP_DIR/tsv.json"
xml_hash="$(jq -r '.result_hash' "$TMP_DIR/xml.json")"
tsv_hash="$(jq -r '.result_hash' "$TMP_DIR/tsv.json")"
test "$xml_hash" = "$tsv_hash"
test "$xml_hash" = "sha256:e588c7b5f4d61dc4640dcf7e55087f99e048c26c598cf5fb64a0a954db2fba16"
jq -e '.canonicalizer_version == "owl2bench-query-canonicalizer/2" and .projection == ["x", "y"] and .row_count == 2 and .ordered == false and .rows[0][0].type == "iri" and .rows[0][1].language == "en" and .rows[1][0].type == "blank_node" and .rows[1][1].type == "literal" and .rows[1][1].datatype == "http://www.w3.org/2001/XMLSchema#integer"' \
  "$TMP_DIR/xml.json" >/dev/null

# RDF 1.1 simple-literal normalization: a literal with no language tag whose
# datatype is xsd:string canonicalizes to the simple-literal form, so
# differently-serialized identical answers compare equal.
version_output="$(python3 "$CANONICALIZER" --version)"
test "$version_output" = "owl2bench-query-canonicalizer/2"

cat > "$TMP_DIR/normalization-query.rq" <<'QUERY'
SELECT ?x WHERE { ?x ?p ?y }
QUERY
cat > "$TMP_DIR/typed-string.xml" <<'XML'
<?xml version="1.0"?>
<sparql xmlns="http://www.w3.org/2005/sparql-results#">
  <head><variable name="x"/></head>
  <results>
    <result><binding name="x"><literal datatype="http://www.w3.org/2001/XMLSchema#string">x</literal></binding></result>
  </results>
</sparql>
XML
cat > "$TMP_DIR/plain-literal.xml" <<'XML'
<?xml version="1.0"?>
<sparql xmlns="http://www.w3.org/2005/sparql-results#">
  <head><variable name="x"/></head>
  <results>
    <result><binding name="x"><literal>x</literal></binding></result>
  </results>
</sparql>
XML
printf '?x\n"x"^^<http://www.w3.org/2001/XMLSchema#string>\n' > "$TMP_DIR/typed-string.tsv"
printf '?x\n"x"\n' > "$TMP_DIR/plain-literal.tsv"
python3 "$CANONICALIZER" --query "$TMP_DIR/normalization-query.rq" --input "$TMP_DIR/typed-string.xml" --output "$TMP_DIR/typed-string-xml.json"
python3 "$CANONICALIZER" --query "$TMP_DIR/normalization-query.rq" --input "$TMP_DIR/plain-literal.xml" --output "$TMP_DIR/plain-xml.json"
python3 "$CANONICALIZER" --query "$TMP_DIR/normalization-query.rq" --input "$TMP_DIR/typed-string.tsv" --output "$TMP_DIR/typed-string-tsv.json"
python3 "$CANONICALIZER" --query "$TMP_DIR/normalization-query.rq" --input "$TMP_DIR/plain-literal.tsv" --output "$TMP_DIR/plain-tsv.json"
typed_xml_hash="$(jq -r '.result_hash' "$TMP_DIR/typed-string-xml.json")"
for equivalent in "$(jq -r '.result_hash' "$TMP_DIR/plain-xml.json")" \
                  "$(jq -r '.result_hash' "$TMP_DIR/typed-string-tsv.json")" \
                  "$(jq -r '.result_hash' "$TMP_DIR/plain-tsv.json")"; do
  test "$typed_xml_hash" = "$equivalent"
done
jq -e '.rows[0][0].datatype == null and .rows[0][0].language == null' "$TMP_DIR/typed-string-xml.json" >/dev/null

# Language tags and non-string datatypes keep their v1 identity: normalization
# cannot make different terms compare equal.
printf '?x\n"hello"@en\n' > "$TMP_DIR/lang-tagged.tsv"
printf '?x\n"hello"\n' > "$TMP_DIR/hello-plain.tsv"
python3 "$CANONICALIZER" --query "$TMP_DIR/normalization-query.rq" --input "$TMP_DIR/lang-tagged.tsv" --output "$TMP_DIR/lang-tagged.json"
python3 "$CANONICALIZER" --query "$TMP_DIR/normalization-query.rq" --input "$TMP_DIR/hello-plain.tsv" --output "$TMP_DIR/hello-plain.json"
test "$(jq -r '.result_hash' "$TMP_DIR/lang-tagged.json")" != "$(jq -r '.result_hash' "$TMP_DIR/hello-plain.json")"
jq -e '.rows[0][0].language == "en"' "$TMP_DIR/lang-tagged.json" >/dev/null
printf '?x\n"1"^^<http://www.w3.org/2001/XMLSchema#integer>\n' > "$TMP_DIR/integer-typed.tsv"
python3 "$CANONICALIZER" --query "$TMP_DIR/normalization-query.rq" --input "$TMP_DIR/integer-typed.tsv" --output "$TMP_DIR/integer-typed.json"
jq -e '.rows[0][0].datatype == "http://www.w3.org/2001/XMLSchema#integer"' "$TMP_DIR/integer-typed.json" >/dev/null
test "$(jq -r '.result_hash' "$TMP_DIR/integer-typed.json")" != "$typed_xml_hash"

# Answer preservation against the captured v1 artifact (ADR 0023): the main
# fixture's v1 canonical hash d0ae91f7… was pinned by the v1 test suite. The
# v1 view of the current payload must rehash to that captured value, and the
# same rows rehashed under the v2 version must reproduce the current v2
# hash exactly — v2 is the normalization of the v1 hash inputs.
read -r v1_view_hash normalized_v1_view_hash < <(python3 - "$TMP_DIR/xml.json" <<'PY'
import hashlib, json, sys
document = json.load(open(sys.argv[1], encoding="utf-8"))
def versioned_hash(version: str) -> str:
    payload = {
        "canonicalizer_version": version,
        "projection": document["projection"],
        "select_modifier": document["select_modifier"],
        "distinct": document["distinct"],
        "ordered": document["ordered"],
        "row_order": document["row_order"],
        "rows": document["rows"],
    }
    compact = json.dumps(payload, ensure_ascii=False, sort_keys=True, separators=(",", ":"))
    return "sha256:" + hashlib.sha256(compact.encode("utf-8")).hexdigest()
print(versioned_hash("owl2bench-query-canonicalizer/1"), versioned_hash("owl2bench-query-canonicalizer/2"))
PY
)
test "$v1_view_hash" = "sha256:d0ae91f7acc435f0316d68fb504567c8e4ed267b94e654958d92d3ebae843c91"
test "$normalized_v1_view_hash" = "$xml_hash"

# v1 behavior preserved: unbound variables stay explicit unbound terms.
cat > "$TMP_DIR/unbound.xml" <<'XML'
<?xml version="1.0"?>
<sparql xmlns="http://www.w3.org/2005/sparql-results#">
  <head><variable name="x"/><variable name="y"/></head>
  <results>
    <result><binding name="x"><uri>http://example.test/a</uri></binding></result>
  </results>
</sparql>
XML
python3 "$CANONICALIZER" --query "$TMP_DIR/query.rq" --input "$TMP_DIR/unbound.xml" --output "$TMP_DIR/unbound.json"
jq -e '.rows[0][0].type == "iri" and .rows[0][1].type == "unbound" and .rows[0][1].value == null' "$TMP_DIR/unbound.json" >/dev/null
printf '?x\t?y\n<http://example.test/a>\t\n' > "$TMP_DIR/unbound.tsv"
python3 "$CANONICALIZER" --query "$TMP_DIR/query.rq" --input "$TMP_DIR/unbound.tsv" --output "$TMP_DIR/unbound-tsv.json"
jq -e '.rows[0][1].type == "unbound"' "$TMP_DIR/unbound-tsv.json" >/dev/null

# v1 behavior preserved: without DISTINCT, duplicate rows keep multiplicity.
printf '?x\n<http://example.test/z>\n<http://example.test/z>\n' > "$TMP_DIR/duplicates.tsv"
python3 "$CANONICALIZER" --query "$TMP_DIR/normalization-query.rq" --input "$TMP_DIR/duplicates.tsv" --output "$TMP_DIR/duplicates.json"
jq -e '.select_modifier == "none" and .row_count == 2 and (.rows | length) == 2 and .rows[0] == .rows[1]' "$TMP_DIR/duplicates.json" >/dev/null

cat > "$TMP_DIR/ordered-query.rq" <<'QUERY'
SELECT ?x WHERE { ?x ?p ?y } ORDER BY ?x
QUERY
cat > "$TMP_DIR/ordered.tsv" <<'TSV'
?x
<http://example.test/z>
<http://example.test/a>
TSV
python3 "$CANONICALIZER" --query "$TMP_DIR/ordered-query.rq" --input "$TMP_DIR/ordered.tsv" --output "$TMP_DIR/ordered.json"
jq -e '.ordered == true and .row_order == "query" and .rows[0][0].value == "http://example.test/z" and .rows[1][0].value == "http://example.test/a"' \
  "$TMP_DIR/ordered.json" >/dev/null

cat > "$TMP_DIR/distinct-query.rq" <<'QUERY'
SELECT DISTINCT ?x WHERE { ?x ?p ?y }
QUERY
cat > "$TMP_DIR/distinct.tsv" <<'TSV'
?x
<http://example.test/z>
<http://example.test/z>
TSV
python3 "$CANONICALIZER" --query "$TMP_DIR/distinct-query.rq" --input "$TMP_DIR/distinct.tsv" --output "$TMP_DIR/distinct.json"
jq -e '.distinct == true and .row_count == 2 and (.rows | length) == 2' "$TMP_DIR/distinct.json" >/dev/null

cat > "$TMP_DIR/mixed-query.rq" <<'QUERY'
SELECT (?x AS ?alias) ?z WHERE { ?x ?p ?z }
QUERY
cat > "$TMP_DIR/mixed.tsv" <<'TSV'
?z	?alias
"tail"	"head"
TSV
python3 "$CANONICALIZER" --query "$TMP_DIR/mixed-query.rq" --input "$TMP_DIR/mixed.tsv" --output "$TMP_DIR/mixed.json"
jq -e '.projection == ["alias", "z"] and .rows[0][0].value == "head" and .rows[0][1].value == "tail"' \
  "$TMP_DIR/mixed.json" >/dev/null

cat > "$TMP_DIR/plain.tsv" <<'TSV'
?x
"plain"
TSV
python3 "$CANONICALIZER" --query "$TMP_DIR/ordered-query.rq" --input "$TMP_DIR/plain.tsv" --output "$TMP_DIR/plain.json"
jq -e '.rows[0][0].type == "literal" and .rows[0][0].value == "plain" and .rows[0][0].datatype == null and .rows[0][0].language == null' \
  "$TMP_DIR/plain.json" >/dev/null

printf '?x\nnot-a-sparql-tsv-term\n' > "$TMP_DIR/untyped.tsv"
if python3 "$CANONICALIZER" --query "$TMP_DIR/ordered-query.rq" --input "$TMP_DIR/untyped.tsv" --output "$TMP_DIR/untyped.json"; then
  echo "assertion failed: untyped TSV binding was accepted" >&2
  exit 1
fi

printf '<sparql>' > "$TMP_DIR/malformed.xml"
if python3 "$CANONICALIZER" --query "$TMP_DIR/query.rq" --input "$TMP_DIR/malformed.xml" --output "$TMP_DIR/malformed.json"; then
  echo "assertion failed: malformed XML was accepted" >&2
  exit 1
fi
cat > "$TMP_DIR/wrong-namespace.xml" <<'XML'
<sparql xmlns="urn:not-sparql-results"><head><variable name="x"/></head><results/></sparql>
XML
if python3 "$CANONICALIZER" --query "$TMP_DIR/query.rq" --input "$TMP_DIR/wrong-namespace.xml" --output "$TMP_DIR/wrong-namespace.json"; then
  echo "assertion failed: wrong-namespace XML was accepted" >&2
  exit 1
fi
cat > "$TMP_DIR/unknown-child.xml" <<'XML'
<sparql xmlns="http://www.w3.org/2005/sparql-results#"><head><variable name="x"/><unexpected/></head><results/></sparql>
XML
if python3 "$CANONICALIZER" --query "$TMP_DIR/query.rq" --input "$TMP_DIR/unknown-child.xml" --output "$TMP_DIR/unknown-child.json"; then
  echo "assertion failed: XML with an unknown child was accepted" >&2
  exit 1
fi
printf '{"results":[]}' > "$TMP_DIR/unsupported.json"
if python3 "$CANONICALIZER" --query "$TMP_DIR/query.rq" --input "$TMP_DIR/unsupported.json" --output "$TMP_DIR/unsupported-result.json"; then
  echo "assertion failed: unsupported result format was accepted" >&2
  exit 1
fi

echo "[test-owl2bench-canonicalizer] ok"
