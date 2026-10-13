#!/usr/bin/env bash
set -euo pipefail

# SPACL RL engine adapter (#84).
#
# load: materializes the RL profile-context ontology with the project's own
#       reasoner (`owl2-reasoner materialize`) and writes a materialization
#       handoff (graph + sha256) plus engine-stack provenance.
# query: re-validates the handoff graph hash and delegates to the pinned
#       materialized-graph Oxigraph backend, which executes the staged query
#       file over the immutable materialized graph.
#
# The adapter is bound to the RL profile; non-RL profile contexts are refused.

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/../../../.." && pwd)"
PROFILE="${SPACL_PROFILE:-RL}"
PROFILE_KEY="$(printf '%s' "$PROFILE" | tr '[:upper:]' '[:lower:]')"
ADAPTER_NAME="spacl-${PROFILE_KEY}"
STATE_DIR_NAME=".spacl-${PROFILE_KEY}"
QUERY_BACKEND="${SPACL_RL_QUERY_BACKEND:-$SCRIPT_DIR/materialized_graph_oxigraph_engine.sh}"
DEFAULT_REASONER="$ROOT_DIR/target/release/owl2-reasoner"
REASONER_BIN="${SPACL_RL_REASONER_BIN:-$DEFAULT_REASONER}"
OXIGRAPH_IMAGE="${MATERIALIZED_GRAPH_QUERY_IMAGE:-oxigraph/oxigraph:0.5.9}"
OXIGRAPH_IMAGE_DIGEST="${MATERIALIZED_GRAPH_QUERY_IMAGE_DIGEST:-sha256:e68b3625743db4a4b18129a907ae36766f89bb6deeb8ab50d35685dbabe00b0e}"

usage() {
  printf 'usage: %s load <RL-profile-context> | query <query-file> <RL-profile-context>\n' "$0" >&2
  exit 2
}

not_available() {
  printf '[%s][not_available] %s\n' "$ADAPTER_NAME" "$1" >&2
  exit 125
}

adapter_failed() {
  printf '[%s][failed] %s\n' "$ADAPTER_NAME" "$1" >&2
  exit 1
}

sha256_file() {
  sha256sum "$1" | awk '{print $1}'
}

resolve_context() {
  local context="$1"
  [[ -n "$context" && -d "$context" ]] || not_available "profile context directory is missing: $context"
  [[ "$(basename "$context")" == "$PROFILE" ]] \
    || adapter_failed "$ADAPTER_NAME refuses non-$PROFILE profile context: $context"
  realpath "$context"
}

reasoner_executable() {
  [[ -n "$REASONER_BIN" && -x "$REASONER_BIN" ]] \
    || not_available "SPACL reasoner binary is missing or not executable: $REASONER_BIN (set SPACL_RL_REASONER_BIN)"
  realpath "$REASONER_BIN"
}

load_context() {
  local context="$1"
  local state graph ontology graph_hash stats_file
  state="$context/$STATE_DIR_NAME"
  mkdir -p "$state" || not_available "cannot create adapter state directory: $state"
  graph="$state/materialized.nt"
  stats_file="$state/materialize-stats.json"

  local ontology_files=()
  mapfile -t ontology_files < <(find "$context" -maxdepth 1 -type f -name '*.owl' | sort)
  [[ "${#ontology_files[@]}" -eq 1 ]] \
    || not_available "RL profile context must contain exactly one .owl ontology (found ${#ontology_files[@]}): $context"
  ontology="${ontology_files[0]}"

  local reasoner
  reasoner="$(reasoner_executable)"

  if ! "$reasoner" materialize "$ontology" "$graph" --profile "$PROFILE" > "$stats_file" 2> "$state/materialize-stderr.txt"; then
    adapter_failed "SPACL materialization failed: $(tail -n 3 "$state/materialize-stderr.txt" 2>/dev/null || true)"
  fi
  [[ -s "$graph" ]] || adapter_failed "SPACL materialization produced an empty graph"
  [[ -s "$stats_file" ]] || adapter_failed "SPACL materialization produced no stats line"

  # The reasoner CLI mixes progress lines into stdout; the stats payload is
  # the last JSON-object line.
  local stats_json
  stats_json="$(grep -E '^\{.*\}[[:space:]]*$' "$stats_file" | tail -n 1 || true)"
  [[ -n "$stats_json" ]] || adapter_failed "SPACL materialization produced no JSON stats line"
  jq -e '.command == "materialize"' <<< "$stats_json" >/dev/null \
    || adapter_failed "SPACL materialization stats line is not the expected payload"
  printf '%s\n' "$stats_json" > "$stats_file"

  graph_hash="sha256:$(sha256_file "$graph")"
  jq -n \
    --arg adapter "$ADAPTER_NAME" \
    --arg profile "$PROFILE" \
    --arg graph_path "$graph" \
    --arg graph_sha256 "$graph_hash" \
    '{status: "ready", adapter: $adapter, profile: $profile, graph_path: $graph_path, graph_sha256: $graph_sha256}' \
    > "$state/handoff.json"

  jq -n \
    --arg adapter "$ADAPTER_NAME" \
    --arg profile "$PROFILE" \
    --arg context_dir "$context" \
    --arg engine "SPACL-$PROFILE" \
    --arg reasoner_bin "$reasoner" \
    --arg reasoner_bin_sha256 "sha256:$(sha256_file "$reasoner")" \
    --arg ontology "$ontology" \
    --arg ontology_sha256 "sha256:$(sha256_file "$ontology")" \
    --arg image "$OXIGRAPH_IMAGE" \
    --arg image_digest "$OXIGRAPH_IMAGE_DIGEST" \
    --argjson materialize "$(jq -c '{base_triples, derived_triples, closure_triples, out_of_fragment_axioms, parse_time_ms, materialize_time_ms}' <<< "$stats_json")" \
    '{
      adapter: $adapter,
      profile: $profile,
      context_dir: $context_dir,
      engine: $engine,
      reasoner_bin: $reasoner_bin,
      reasoner_bin_sha256: $reasoner_bin_sha256,
      ontology: {path: $ontology, sha256: $ontology_sha256},
      oxigraph: {image: $image, image_digest: $image_digest},
      materialize: $materialize
    }' \
    > "$state/provenance.json"

  cat "$stats_file"
}

query_context() {
  local query_file="$1"
  local context="$2"
  local state handoff graph expected actual
  context="$(resolve_context "$context")"
  state="$context/$STATE_DIR_NAME"
  handoff="$state/handoff.json"
  [[ -f "$handoff" ]] || not_available "no materialization handoff under $state; run load first"
  graph="$(jq -r '.graph_path // ""' "$handoff")"
  [[ -n "$graph" && -f "$graph" ]] || not_available "handoff graph is missing: $graph"
  expected="$(jq -r '.graph_sha256 // ""' "$handoff")"
  actual="sha256:$(sha256_file "$graph")"
  [[ "$expected" == "$actual" ]] || adapter_failed "materialized graph hash does not match handoff: $actual"
  [[ -x "$QUERY_BACKEND" ]] || not_available "query backend is missing or not executable: $QUERY_BACKEND"
  export OWL2BENCH_QUERY_MATERIALIZATION_HANDOFF="$handoff"
  exec "$QUERY_BACKEND" query "$query_file"
}

[[ $# -ge 2 ]] || usage
case "$1" in
  load)
    [[ $# -eq 2 ]] || usage
    load_context "$(resolve_context "$2")"
    ;;
  query)
    [[ $# -eq 3 ]] || usage
    query_context "$2" "$3"
    ;;
  *)
    usage
    ;;
esac
