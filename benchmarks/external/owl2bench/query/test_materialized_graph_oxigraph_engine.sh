#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
TMP_DIR="$(mktemp -d "${TMPDIR:-/tmp}/tableauxx-materialized-query-test.XXXXXX")"
trap 'rm -rf "$TMP_DIR"' EXIT

GRAPH="$TMP_DIR/graph.nq"
QUERY="$TMP_DIR/query.rq"
HANDOFF="$TMP_DIR/handoff.json"
DOCKER="$TMP_DIR/docker"
printf '<urn:a> <urn:p> <urn:b> .\n' > "$GRAPH"
printf 'SELECT ?s WHERE { ?s <urn:p> <urn:b> }\n' > "$QUERY"
GRAPH_HASH="sha256:$(sha256sum "$GRAPH" | awk '{print $1}')"
cat > "$HANDOFF" <<EOF
{"status":"ready","read_only":true,"graph_path":"$GRAPH","graph_sha256":"$GRAPH_HASH"}
EOF
cat > "$DOCKER" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
if [[ "${1:-}" == image && "${2:-}" == inspect ]]; then
  printf '%s\n' 'pinned-image'
  exit 0
fi
if [[ "${1:-}" == run ]]; then
  if [[ " $* " == *" load "* ]]; then
    exit 0
  fi
  printf '?s\n<urn:a>\n'
  exit 0
fi
exit 1
EOF
chmod +x "$DOCKER"

MATERIALIZED_GRAPH_QUERY_DOCKER_BIN="$DOCKER" \
OWL2BENCH_QUERY_MATERIALIZATION_HANDOFF="$HANDOFF" \
OWL2BENCH_QUERY_MATERIALIZATION_GRAPH_SHA256="$GRAPH_HASH" \
  "$SCRIPT_DIR/materialized_graph_oxigraph_engine.sh" query "$QUERY" > "$TMP_DIR/result.tsv"
grep -Fxq '?s' "$TMP_DIR/result.tsv"
grep -Fxq '<urn:a>' "$TMP_DIR/result.tsv"

LOAD_FAIL_DOCKER="$TMP_DIR/load-fail-docker"
cat > "$LOAD_FAIL_DOCKER" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
if [[ "${1:-}" == image && "${2:-}" == inspect ]]; then
  printf '%s\n' 'pinned-image'
  exit 0
fi
if [[ "${1:-}" == run && " $* " == *" load "* ]]; then
  exit 1
fi
exit 1
EOF
chmod +x "$LOAD_FAIL_DOCKER"
set +e
MATERIALIZED_GRAPH_QUERY_DOCKER_BIN="$LOAD_FAIL_DOCKER" \
OWL2BENCH_QUERY_MATERIALIZATION_HANDOFF="$HANDOFF" \
OWL2BENCH_QUERY_MATERIALIZATION_GRAPH_SHA256="$GRAPH_HASH" \
  "$SCRIPT_DIR/materialized_graph_oxigraph_engine.sh" query "$QUERY" > /dev/null 2> "$TMP_DIR/load-fail.stderr"
load_fail_status="$?"
set -e
test "$load_fail_status" = 1
grep -Fq '[materialized-graph-query][failed]' "$TMP_DIR/load-fail.stderr"

set +e
MATERIALIZED_GRAPH_QUERY_DOCKER_BIN="$DOCKER" \
  "$SCRIPT_DIR/materialized_graph_oxigraph_engine.sh" query "$QUERY" > /dev/null 2> "$TMP_DIR/missing.stderr"
status="$?"
set -e
if [[ "$status" -eq 0 ]]; then
  echo "missing materialization handoff unexpectedly passed" >&2
  exit 1
fi
test "$status" = 125
grep -Fq 'materialization handoff is not configured' "$TMP_DIR/missing.stderr"

echo "[test-materialized-graph-oxigraph-engine] ok"
