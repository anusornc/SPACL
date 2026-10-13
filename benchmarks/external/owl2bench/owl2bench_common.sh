#!/usr/bin/env bash

owl2bench_upper() {
  printf '%s' "$1" | tr '[:lower:]' '[:upper:]'
}

owl2bench_infer_profile() {
  local value
  value="$(owl2bench_upper "$1")"
  if [[ "$value" =~ OWL2(DL|EL|QL|RL)([^A-Z0-9]|$) ]]; then
    printf '%s\n' "${BASH_REMATCH[1]}"
    return
  fi
  if [[ "$value" =~ (^|[^A-Z0-9])(DL|EL|QL|RL)([^A-Z0-9]|$) ]]; then
    printf '%s\n' "${BASH_REMATCH[2]}"
    return
  fi
  printf 'unknown\n'
}

owl2bench_infer_seed() {
  if [[ "$1" =~ (^|[^A-Za-z0-9])(seed|randomseed)[_=-]?([0-9]+)([^0-9]|$) ]]; then
    printf '%s\n' "${BASH_REMATCH[3]}"
    return
  fi
  printf 'unknown\n'
}

owl2bench_infer_abox_size() {
  if [[ "$1" =~ (^|[^A-Za-z0-9])(abox|aboxsize|individuals?)[_=-]?([0-9]+)([^0-9]|$) ]]; then
    printf '%s\n' "${BASH_REMATCH[3]}"
    return
  fi
  printf 'unknown\n'
}

owl2bench_infer_university_count() {
  local value="$1"
  if [[ "$value" =~ (^|[^A-Za-z0-9])(universit(y|ies)|univ|universitycount)[_=-]?([0-9]+)([^0-9]|$) ]]; then
    printf '%s\n' "${BASH_REMATCH[4]}"
    return
  fi
  if [[ "$value" =~ University[_-]?([0-9]+)([^0-9]|$) ]]; then
    printf '%s\n' "${BASH_REMATCH[1]}"
    return
  fi
  printf 'unknown\n'
}

owl2bench_infer_stage() {
  local value
  value="$(owl2bench_upper "$1")"
  if [[ "$value" =~ (^|[^A-Z0-9])CORRECTNESS([^A-Z0-9]|$) ]]; then
    printf 'correctness\n'
    return
  fi
  if [[ "$value" =~ (^|[^A-Z0-9])SCALABILITY([^A-Z0-9]|$) ]]; then
    printf 'scalability\n'
    return
  fi
  if [[ "$2" != "unknown" || "$3" != "unknown" ]]; then
    printf 'scalability\n'
    return
  fi
  printf 'correctness\n'
}

owl2bench_write_manifest_header() {
  printf 'staged_ontology\tsource_ontology\tprofile\tseed\tabox_size\tuniversity_count\tstage\n'
}
