#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
QUERY_BACKEND="$SCRIPT_DIR/materialized_graph_oxigraph_engine.sh"
EXPECTED_QUERY_BACKEND_SHA256="f28d5ada78f7aa8b71ccedd9323b30662eaa37da1c0ee165308228ccf9bb1308"

not_available() {
  printf '[openllet-ql-query][not_available] %s\n' "$1" >&2
  exit 125
}

failed() {
  printf '[openllet-ql-query][failed] %s\n' "$1" >&2
  exit 1
}

[[ "${1:-}" == "query" && $# -ge 2 && $# -le 3 ]] \
  || { echo "usage: $0 query <query-file> [validated-graph]" >&2; exit 2; }
[[ -x "$QUERY_BACKEND" ]] || not_available "materialized graph backend is missing"
[[ "$(sha256sum "$QUERY_BACKEND" | awk '{print $1}')" == "$EXPECTED_QUERY_BACKEND_SHA256" ]] \
  || not_available "materialized graph backend does not match its pinned digest"
[[ -r "$2" && -f "$2" ]] || not_available "query file is missing: $2"

handoff="${OWL2BENCH_QUERY_MATERIALIZATION_HANDOFF:-}"
[[ -n "$handoff" && -r "$handoff" ]] \
  || not_available "materialization handoff is missing"
handoff_graph="$(jq -r '.graph_path // empty' "$handoff" 2>/dev/null || true)"
handoff_hash="$(jq -r '.graph_sha256 // empty' "$handoff" 2>/dev/null || true)"
[[ "$(jq -r '.status // empty' "$handoff" 2>/dev/null || true)" == "ready" ]] \
  || not_available "materialization handoff is not ready"
[[ "$(jq -r '.read_only // false' "$handoff" 2>/dev/null || true)" == "true" ]] \
  || failed "materialization handoff is not read-only"
[[ -r "$handoff_graph" ]] || not_available "validated graph is missing"
actual_hash="sha256:$(sha256sum "$handoff_graph" | awk '{print $1}')"
[[ "$handoff_hash" == "$actual_hash" ]] || failed "validated graph hash changed"
if [[ $# -eq 3 ]]; then
  requested_graph="$(realpath "$3")" || not_available "cannot resolve validated graph: $3"
  [[ "$requested_graph" == "$(realpath "$handoff_graph")" ]] \
    || failed "adapter graph does not match the read-only handoff"
fi

exec "$QUERY_BACKEND" query "$2"
