#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
CANONICALIZER="$SCRIPT_DIR/canonicalize_result.py"
TMP_DIR="$(mktemp -d)"
trap 'rm -rf "$TMP_DIR"' EXIT

cat > "$TMP_DIR/query.rq" <<'QUERY'
SELECT DISTINCT ?x ?y WHERE { ?x rdf:type :SelfAwarePerson }
QUERY

# Konclude query 006 emits this shape when the solution set is empty: the
# result head omits ?y even though the SELECT projection contains it. This is
# a protocol-failure guard: do not silently repair the external engine output.
cat > "$TMP_DIR/result.xml" <<'XML'
<?xml version="1.0" encoding="UTF-8"?>
<sparql xmlns="http://www.w3.org/2005/sparql-results#">
  <head><variable name="x"/></head>
  <results/>
</sparql>
XML

set +e
python3 "$CANONICALIZER" \
  --query "$TMP_DIR/query.rq" \
  --input "$TMP_DIR/result.xml" \
  --output "$TMP_DIR/canonical.json" \
  > "$TMP_DIR/stdout" 2> "$TMP_DIR/stderr"
status="$?"
set -e
test "$status" -ne 0
grep -F "result projection does not match the SELECT projection" "$TMP_DIR/stderr" >/dev/null

# A conforming empty result remains canonicalizable, proving the guard is
# specific to the malformed head rather than to empty result sets generally.
sed 's/<variable name="x"\/>/<variable name="x"\/><variable name="y"\/>/' \
  "$TMP_DIR/result.xml" > "$TMP_DIR/conforming-empty.xml"
python3 "$CANONICALIZER" \
  --query "$TMP_DIR/query.rq" \
  --input "$TMP_DIR/conforming-empty.xml" \
  --output "$TMP_DIR/conforming-empty.json"
jq -e '.projection == ["x", "y"] and .row_count == 0 and .rows == []' \
  "$TMP_DIR/conforming-empty.json" >/dev/null

echo "query 006 malformed projection remains fail-closed"
