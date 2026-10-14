#!/usr/bin/env bash
set -euo pipefail

# Adapter contract used by query/run.sh:
#   oxigraph_engine.sh load <data-directory>
#   oxigraph_engine.sh query <query-file> <data-directory>

OXIGRAPH_IMAGE="${OXIGRAPH_IMAGE:-oxigraph/oxigraph:0.5.9}"
OWL2BENCH_NAMESPACE="https://kracr.iiitd.edu.in/OWL2Bench#"
RDF_NAMESPACE="http://www.w3.org/1999/02/22-rdf-syntax-ns#"

usage() {
  echo "usage: $0 load <data-directory> | query <query-file> <data-directory>" >&2
  exit 2
}

require_data_directory() {
  local data_dir="$1"
  [[ -d "$data_dir" ]] || {
    echo "data directory not found: $data_dir" >&2
    exit 1
  }
  realpath "$data_dir"
}

store_directory() {
  local data_dir="$1"
  printf '%s/.oxigraph-store\n' "${data_dir%/}"
}

load_data() {
  local data_dir
  data_dir="$(require_data_directory "$1")"
  local store_dir
  store_dir="$(store_directory "$data_dir")"

  case "$store_dir" in
    */.oxigraph-store) ;;
    *) echo "refusing unsafe Oxigraph store path: $store_dir" >&2; exit 1 ;;
  esac
  rm -rf "$store_dir"
  mkdir -p "$store_dir"

  mapfile -t files < <(
    find "$data_dir" -type f \
      ! -path "$store_dir/*" \
      \( -iname '*.owl' -o -iname '*.rdf' -o -iname '*.xml' -o -iname '*.ttl' -o -iname '*.nt' \) \
      -print | sort
  )
  ((${#files[@]} > 0)) || {
    echo "no RDF data files found under: $data_dir" >&2
    exit 1
  }

  relative_files=()
  for file in "${files[@]}"; do
    relative_files+=("/data/${file#"$data_dir"/}")
  done

  docker run --rm \
    -v "$data_dir:/data" \
    "$OXIGRAPH_IMAGE" \
    load --location /data/.oxigraph-store --format application/rdf+xml \
    --file "${relative_files[@]}"
}

run_query() {
  [[ $# -eq 2 ]] || usage
  local query_file="$1"
  local data_dir
  data_dir="$(require_data_directory "$2")"
  [[ -f "$query_file" ]] || {
    echo "query file not found: $query_file" >&2
    exit 1
  }
  local store_dir
  store_dir="$(store_directory "$data_dir")"
  [[ -d "$store_dir" ]] || {
    echo "Oxigraph store is not loaded: $store_dir" >&2
    exit 1
  }

  local query_with_prefixes
  query_with_prefixes="$(mktemp)"
  trap 'rm -f "${query_with_prefixes:-}"' EXIT
  {
    printf 'PREFIX : <%s>\n' "$OWL2BENCH_NAMESPACE"
    printf 'PREFIX rdf: <%s>\n' "$RDF_NAMESPACE"
    cat "$query_file"
  } > "$query_with_prefixes"

  docker run --rm \
    -v "$data_dir:/data:ro" \
    -v "$query_with_prefixes:/query.rq:ro" \
    "$OXIGRAPH_IMAGE" \
    query --location /data/.oxigraph-store --query-file /query.rq --results-format tsv
}

[[ $# -ge 2 ]] || usage
case "$1" in
  load) [[ $# -eq 2 ]] || usage; load_data "$2" ;;
  query) run_query "${@:2}" ;;
  *) usage ;;
esac
