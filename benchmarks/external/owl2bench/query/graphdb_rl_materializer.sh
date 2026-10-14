#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
COMMAND="${1:-}"
GRAPHDB_RL_IMAGE="ontotext/graphdb@sha256:d688879c4e5751a0c323f996d86bf1b2189191290711b1936e211d4510b53485"
GRAPHDB_RL_TAG="ontotext/graphdb:11.5.0"
GRAPHDB_RL_DIGEST="sha256:d688879c4e5751a0c323f996d86bf1b2189191290711b1936e211d4510b53485"
GRAPHDB_RL_PLATFORM="linux/amd64"
GRAPHDB_RL_REPOSITORY_ID="owl2bench-graphdb-rl"
GRAPHDB_RL_CONFIG="$SCRIPT_DIR/graphdb_rl_repository_config.ttl"
DOCKER_BIN="${GRAPHDB_RL_DOCKER_BIN:-docker}"
TIMEOUT_SECONDS="${GRAPHDB_RL_TIMEOUT_SECONDS:-600}"

not_available() {
  printf '[graphdb-rl][not_available] %s\n' "$1" >&2
  exit 125
}

unsupported() {
  printf '[graphdb-rl][unsupported] %s\n' "$1" >&2
  exit 123
}

usage() {
  printf 'usage: %s version | export <source.nq>\n' "$0" >&2
  exit 2
}

require_runtime() {
  command -v "$DOCKER_BIN" >/dev/null 2>&1 || not_available "docker executable is missing: $DOCKER_BIN"
  [[ -r "$GRAPHDB_RL_CONFIG" ]] || not_available "GraphDB RL repository config is missing: $GRAPHDB_RL_CONFIG"
  local inspected
  if ! inspected="$("$DOCKER_BIN" image inspect "$GRAPHDB_RL_IMAGE" --format '{{.Id}}|{{.Architecture}}|{{.Os}}|{{json .RepoDigests}}' 2>/dev/null)"; then
    not_available "pinned GraphDB image is not present locally: $GRAPHDB_RL_IMAGE"
  fi
  local image_id architecture os_name repo_digests
  IFS='|' read -r image_id architecture os_name repo_digests <<< "$inspected"
  [[ "$image_id" == "$GRAPHDB_RL_DIGEST" || "$repo_digests" == *"ontotext/graphdb@$GRAPHDB_RL_DIGEST"* ]] \
    || not_available "GraphDB image identity does not include the pinned digest"
  [[ "$architecture" == "amd64" && "$os_name" == "linux" ]] || not_available "GraphDB image platform is not linux/amd64"
}

version_probe() {
  require_runtime
  "$DOCKER_BIN" run --rm --platform "$GRAPHDB_RL_PLATFORM" --entrypoint /bin/bash "$GRAPHDB_RL_IMAGE" \
    -lc 'set -euo pipefail; test -x /opt/graphdb/dist/bin/importrdf; test -x /opt/graphdb/dist/bin/graphdb; java -version 2>&1 | head -n 1; printf "GRAPHDB_INSTALL_DIR=%s\n" "${GRAPHDB_INSTALL_DIR:-/opt/graphdb/dist}"' \
    || not_available "GraphDB runtime probe failed"
  printf 'graphdb-rl image=%s tag=%s repository=%s ruleset=owl2-rl\n' \
    "$GRAPHDB_RL_IMAGE" "$GRAPHDB_RL_TAG" "$GRAPHDB_RL_REPOSITORY_ID"
}

contains_license_error() {
  grep -Eiq 'license|licensed|licen[cs]e|not authorized|authorization|forbidden|401|403' "$1"
}

export_graph() {
  [[ $# -eq 2 ]] || usage
  require_runtime
  local source_file="$2"
  [[ -r "$source_file" && -f "$source_file" ]] || not_available "source graph is missing or unreadable: $source_file"

  local run_dir
  run_dir="$(mktemp -d)"
  GRAPHDB_RL_TEMP_DIR="$run_dir"
  trap 'rm -rf -- "$GRAPHDB_RL_TEMP_DIR" >/dev/null 2>&1 || true' EXIT
  cp "$source_file" "$run_dir/source.nq"
  cp "$GRAPHDB_RL_CONFIG" "$run_dir/repository-config.ttl"
  local stderr_file="$run_dir/graphdb.stderr"

  set +e
  timeout "$TIMEOUT_SECONDS" "$DOCKER_BIN" run --rm --platform "$GRAPHDB_RL_PLATFORM" \
      --entrypoint /bin/bash -v "$run_dir:/work" "$GRAPHDB_RL_IMAGE" -lc '
        set -euo pipefail
        export GRAPHDB_HOME_DATA=/work/data
        mkdir -p /work/data /work/tmp
        /opt/graphdb/dist/bin/importrdf \
          -Dgraphdb.home.data=/work/data \
          -Djava.io.tmpdir=/work/tmp \
          load -c /work/repository-config.ttl -m parallel /work/source.nq \
          >/work/importrdf.log 2>&1
        /opt/graphdb/dist/bin/graphdb \
          -Dgraphdb.home.data=/work/data \
          -Dgraphdb.home=/work/home \
          -Dgraphdb.workbench.importDirectory=/work/import \
          >/work/graphdb.log 2>&1 &
        graphdb_pid="$!"
        trap "kill \"$graphdb_pid\" >/dev/null 2>&1 || true" EXIT
        for _ in $(seq 1 120); do
          if curl -fsS "http://127.0.0.1:7200/rest/repositories" >/dev/null 2>&1; then
            break
          fi
          sleep 1
        done
        curl -fsS \
          -H "Accept: application/n-quads" \
          "http://127.0.0.1:7200/repositories/owl2bench-graphdb-rl/statements?includeInferred=true"
      ' 2>"$stderr_file" \
      | python3 "$SCRIPT_DIR/graphdb_graph_filter.py" --source "$source_file" 2>>"$stderr_file"
  pipeline_statuses=("${PIPESTATUS[@]}")
  set -e
  docker_status="${pipeline_statuses[0]}"
  filter_status="${pipeline_statuses[1]}"
  if [[ "$docker_status" -eq 0 && "$filter_status" -eq 0 ]]; then
    return 0
  fi
  cat "$run_dir/importrdf.log" "$run_dir/graphdb.log" >> "$stderr_file" 2>/dev/null || true
  sed -n '1,120p' "$stderr_file" >&2 || true
  if [[ "$docker_status" -eq 124 ]]; then
    printf '[graphdb-rl][timeout] GraphDB RL materialization/export exceeded %ss\n' "$TIMEOUT_SECONDS" >&2
    exit 124
  fi
  if contains_license_error "$stderr_file"; then
    not_available "GraphDB runtime license or authorization is unavailable"
  fi
  printf '[graphdb-rl][failed] GraphDB RL materialization/export failed (docker=%s filter=%s)\n' \
    "$docker_status" "$filter_status" >&2
  exit 1
}

case "$COMMAND" in
  version)
    [[ $# -eq 1 ]] || usage
    version_probe
    ;;
  export)
    export_graph "$@"
    ;;
  *)
    usage
    ;;
esac
