#!/usr/bin/env bash
set -euo pipefail

# Query only the immutable graph accepted by materialization_validator.py.
# This adapter has no load/update/reasoning operation at the benchmark seam.
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DOCKER_BIN="${MATERIALIZED_GRAPH_QUERY_DOCKER_BIN:-docker}"
IMAGE_TAG="${MATERIALIZED_GRAPH_QUERY_IMAGE_TAG:-oxigraph/oxigraph:0.5.9}"
IMAGE_DIGEST="${MATERIALIZED_GRAPH_QUERY_IMAGE_DIGEST:-sha256:e68b3625743db4a4b18129a907ae36766f89bb6deeb8ab50d35685dbabe00b0e}"
IMAGE="${MATERIALIZED_GRAPH_QUERY_IMAGE:-oxigraph/oxigraph@${IMAGE_DIGEST}}"
PLATFORM="linux/amd64"
ALLOW_PULL="${MATERIALIZED_GRAPH_QUERY_ALLOW_PULL:-0}"
OWL2BENCH_NAMESPACE_DEFAULT="https://kracr.iiitd.edu.in/OWL2Bench#"
RDF_NAMESPACE="http://www.w3.org/1999/02/22-rdf-syntax-ns#"

# The query terms' namespace follows the materialized graph's dominant
# non-standard predicate vocabulary (the same rule the derivation core's
# ADR 0016 policy uses), with an explicit override for pinned contexts:
# shipped-track graphs are dominated by the generator namespace, patched
# graphs by the TBox namespace, and both resolve without hardcoding.
resolve_query_namespace() {
  if [[ -n "${OWL2BENCH_QUERY_NAMESPACE:-}" ]]; then
    printf '%s\n' "$OWL2BENCH_QUERY_NAMESPACE"
    return 0
  fi
  python3 - "$1" <<'PY' 2>/dev/null || true
import re
import sys
from collections import Counter

counts = Counter()
with open(sys.argv[1], encoding="utf-8") as handle:
    for line in handle:
        match = re.match(r"<([^>]+)> <([^>]+)>", line)
        if not match:
            continue
        predicate = match.group(2)
        if predicate.startswith("http://www.w3.org/"):
            continue
        if "#" in predicate:
            namespace = predicate.rsplit("#", 1)[0] + "#"
        else:
            namespace = predicate.rsplit("/", 1)[0] + "/"
        counts[namespace] += 1
if counts:
    print(max(sorted(counts), key=lambda ns: counts[ns]))
PY
}

not_available() {
  printf '[materialized-graph-query][not_available] %s\n' "$1" >&2
  exit 125
}

failed() {
  printf '[materialized-graph-query][failed] %s\n' "$1" >&2
  exit 1
}

usage() {
  printf 'usage: %s query <query-file>\n' "$0" >&2
  exit 2
}

require_file() {
  local path="$1"
  [[ -f "$path" && -r "$path" ]] || not_available "file is missing or unreadable: $path"
  realpath "$path" || not_available "cannot resolve file: $path"
}

verify_image() {
  command -v "$DOCKER_BIN" >/dev/null 2>&1 || not_available "docker executable is missing: $DOCKER_BIN"
  local inspected
  if ! inspected="$($DOCKER_BIN image inspect "$IMAGE" 2>/dev/null)"; then
    if [[ "$ALLOW_PULL" == "1" ]]; then
      "$DOCKER_BIN" pull --platform "$PLATFORM" "$IMAGE" >/dev/null \
        || not_available "pinned Oxigraph image pull failed: $IMAGE"
      inspected="$($DOCKER_BIN image inspect "$IMAGE" 2>/dev/null)" \
        || not_available "pinned Oxigraph image is unavailable after pull: $IMAGE"
    else
      not_available "pinned Oxigraph image is not present locally: $IMAGE"
    fi
  fi
  [[ -n "$inspected" ]] || not_available "pinned Oxigraph image inspection was empty: $IMAGE"
}

handoff_graph() {
  local handoff="${OWL2BENCH_QUERY_MATERIALIZATION_HANDOFF:-}"
  [[ -n "$handoff" ]] || not_available "materialization handoff is not configured"
  handoff="$(require_file "$handoff")"
  local status graph expected actual
  status="$(jq -r '.status // ""' "$handoff" 2>/dev/null || true)"
  [[ "$status" == "ready" ]] || not_available "materialization handoff is not ready: $status"
  graph="$(jq -r '.graph_path // ""' "$handoff" 2>/dev/null || true)"
  graph="$(require_file "$graph")"
  expected="$(jq -r '.graph_sha256 // ""' "$handoff" 2>/dev/null || true)"
  [[ "$expected" =~ ^sha256:[0-9a-fA-F]{64}$ ]] \
    || failed "materialization handoff graph hash is invalid"
  actual="sha256:$(sha256sum "$graph" | awk '{print $1}')"
  [[ "$expected" == "$actual" ]] || failed "materialization graph hash does not match handoff"
  if [[ -n "${OWL2BENCH_QUERY_MATERIALIZATION_GRAPH_SHA256:-}" \
      && "$OWL2BENCH_QUERY_MATERIALIZATION_GRAPH_SHA256" != "$actual" ]]; then
    failed "materialization graph hash does not match runner identity"
  fi
  printf '%s\n' "$graph"
}

query_graph() {
  [[ $# -eq 1 ]] || usage
  local query_file graph
  query_file="$(require_file "$1")"
  verify_image
  graph="$(handoff_graph)"
  OWL2BENCH_NAMESPACE="$(resolve_query_namespace "$graph")"
  [[ -n "$OWL2BENCH_NAMESPACE" ]] || OWL2BENCH_NAMESPACE="$OWL2BENCH_NAMESPACE_DEFAULT"
  MATERIALIZED_STORE="$(mktemp -d "${TMPDIR:-/tmp}/tableauxx-materialized-graph-query.XXXXXX")" \
    || not_available "cannot create private Oxigraph store directory"
  MATERIALIZED_QUERY_FILE="$(mktemp "${TMPDIR:-/tmp}/tableauxx-materialized-query.XXXXXX")" \
    || not_available "cannot create private query file"
  trap 'rm -rf -- "${MATERIALIZED_STORE:-}" "${MATERIALIZED_QUERY_FILE:-}" >/dev/null 2>&1 || true' EXIT
  {
    printf 'PREFIX : <%s>\n' "$OWL2BENCH_NAMESPACE"
    printf 'PREFIX rdf: <%s>\n' "$RDF_NAMESPACE"
    cat "$query_file"
  } > "$MATERIALIZED_QUERY_FILE"

  "$DOCKER_BIN" run --rm --platform "$PLATFORM" --user "$(id -u):$(id -g)" \
    -v "$graph:/graph.nq:ro" \
    -v "$MATERIALIZED_STORE:/data:rw" \
  "$IMAGE" load --location /data/store --format application/n-quads --file /graph.nq \
    >/dev/null || failed "Oxigraph could not load the validated materialization graph"

  "$DOCKER_BIN" run --rm --platform "$PLATFORM" --user "$(id -u):$(id -g)" \
    -v "$MATERIALIZED_STORE:/data:ro" \
    -v "$MATERIALIZED_QUERY_FILE:/query.rq:ro" \
    "$IMAGE" query --location /data/store --query-file /query.rq --results-format tsv \
    || failed "Oxigraph query execution failed"
}

[[ $# -ge 1 ]] || usage
case "$1" in
  query) query_graph "${@:2}" ;;
  *) usage ;;
esac
