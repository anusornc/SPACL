#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
TMP_DIR="$(mktemp -d "${TMPDIR:-/tmp}/tableauxx-graphdb-filter-test.XXXXXX")"
trap 'rm -rf "$TMP_DIR"' EXIT

SOURCE="$TMP_DIR/source.nq"
cat > "$SOURCE" <<'EOF'
<urn:source> <urn:link> _:input .
_:input <urn:type> <urn:Known> .
<urn:source> <urn:link> <urn:inferred> .
EOF

cat <<'EOF' | python3 "$SCRIPT_DIR/graphdb_graph_filter.py" --source "$SOURCE" > "$TMP_DIR/merged.nq"
<urn:source> <urn:link> _:renamed .
<urn:source> <urn:inferred> <urn:Known> .
<urn:generated> <urn:link> _:witness .
<urn:source> <urn:outside> <urn:not-in-source> .
EOF

grep -Fxq '<urn:source> <urn:link> _:input .' "$TMP_DIR/merged.nq"
grep -Fxq '_:input <urn:type> <urn:Known> .' "$TMP_DIR/merged.nq"
grep -Fxq '<urn:source> <urn:inferred> <urn:Known> .' "$TMP_DIR/merged.nq"
if grep -Fq '_:renamed' "$TMP_DIR/merged.nq" || grep -Fq '_:witness' "$TMP_DIR/merged.nq" || grep -Fq 'urn:not-in-source' "$TMP_DIR/merged.nq"; then
  echo "generated or relabeled blank node escaped the GraphDB filter" >&2
  exit 1
fi

if printf '\n' | python3 "$SCRIPT_DIR/graphdb_graph_filter.py" --source "$SOURCE" > /dev/null 2> "$TMP_DIR/empty.stderr"; then
  echo "empty GraphDB export unexpectedly passed" >&2
  exit 1
fi

echo "[test-graphdb-graph-filter] ok"
