#!/usr/bin/env bash
set -euo pipefail

usage() {
  echo "usage: $0 load <profile-context> | query <query-file> <profile-context>" >&2
  exit 2
}

not_available() {
  echo "[split-profile][not_available] $1" >&2
  exit 125
}

unsupported() {
  echo "[split-profile][unsupported] $1" >&2
  exit 123
}

profile_for_context() {
  local context
  context="$(realpath "$1")" || not_available "cannot resolve profile context: $1"
  [[ -d "$context" ]] || not_available "profile context directory is missing: $context"
  case "$(basename "$context")" in
    DL|EL|QL|RL) printf '%s\n' "$(basename "$context")" ;;
    *) not_available "profile context must be named DL, EL, QL, or RL: $context" ;;
  esac
}

adapter_for_profile() {
  local adapter_variable adapter selected_variant
  case "$1" in
    DL) adapter_variable="OWL2BENCH_SPLIT_DL_ENGINE" ;;
    EL) adapter_variable="OWL2BENCH_SPLIT_EL_ENGINE" ;;
    QL)
      selected_variant="${OWL2BENCH_SPLIT_QL_VARIANT:-ql-ruleset}"
      case "$selected_variant" in
        ql-ruleset) adapter_variable="OWL2BENCH_SPLIT_QL_ENGINE" ;;
        openllet-ql) adapter_variable="OWL2BENCH_OPENLLET_QL_ENGINE" ;;
        graphdb-ql) adapter_variable="OWL2BENCH_GRAPHDB_QL_ENGINE" ;;
        *) unsupported "unsupported QL adapter variant: $selected_variant" ;;
      esac
      ;;
    RL) adapter_variable="OWL2BENCH_SPLIT_RL_ENGINE" ;;
    *) not_available "no adapter is registered for profile $1" ;;
  esac
  adapter="${!adapter_variable:-}"
  [[ -n "$adapter" ]] || not_available "$adapter_variable must be explicitly configured"
  printf '%s\n' "$adapter"
}

[[ $# -ge 2 ]] || usage
case "$1" in
  load)
    [[ $# -eq 2 ]] || usage
    PROFILE="$(profile_for_context "$2")"
    ADAPTER="$(adapter_for_profile "$PROFILE")"
    [[ -x "$ADAPTER" ]] || not_available "$PROFILE adapter is missing or not executable: $ADAPTER"
    if [[ "$PROFILE" == "QL" ]]; then
      OWL2BENCH_PROFILE_ADAPTER_NAME="${OWL2BENCH_SPLIT_QL_VARIANT:-ql-ruleset}" exec "$ADAPTER" load "$2"
    fi
    exec "$ADAPTER" load "$2"
    ;;
  query)
    [[ $# -eq 3 ]] || usage
    [[ -r "$2" ]] || not_available "query file is missing or unreadable: $2"
    PROFILE="$(profile_for_context "$3")"
    ADAPTER="$(adapter_for_profile "$PROFILE")"
    [[ -x "$ADAPTER" ]] || not_available "$PROFILE adapter is missing or not executable: $ADAPTER"
    if [[ "$PROFILE" == "QL" ]]; then
      OWL2BENCH_PROFILE_ADAPTER_NAME="${OWL2BENCH_SPLIT_QL_VARIANT:-ql-ruleset}" exec "$ADAPTER" query "$2" "$3"
    fi
    exec "$ADAPTER" query "$2" "$3"
    ;;
  *)
    usage
    ;;
esac
