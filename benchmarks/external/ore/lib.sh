#!/usr/bin/env bash

# Shared helpers for the ORE 2015 external consistency lane.
# This file is sourced by sibling scripts and intentionally leaves shell
# options to the caller.

ORE2015_CORPUS_TITLE="ORE 2015 Reasoner Competition Corpus"
ORE2015_CORPUS_EDITION="v1"
ORE2015_CORPUS_DOI="10.5281/zenodo.18578"
ORE2015_CORPUS_URL="https://zenodo.org/records/18578/files/ore2015_sample.zip"
ORE2015_CORPUS_FILE="ore2015_sample.zip"
ORE2015_CORPUS_MD5="109f04cf8f124eb551d33c100e549730"

ore_log() { printf '[ore2015] %s\n' "$*"; }
ore_err() { printf '[ore2015][error] %s\n' "$*" >&2; }

ore_is_truthy() {
  case "${1,,}" in
    1|true|yes|on) return 0 ;;
    *) return 1 ;;
  esac
}

ore_official_group_for() {
  case "$1" in
    pure_dl) printf '%s\n' "OWL DL" ;;
    el) printf '%s\n' "OWL 2 EL" ;;
    *) return 1 ;;
  esac
}

ore_track_for() {
  case "$1" in
    pure_dl) printf '%s\n' "OWL DL Consistency" ;;
    el) printf '%s\n' "OWL EL Consistency" ;;
    *) return 1 ;;
  esac
}

ore_group_regex_for() {
  case "$1" in
    pure_dl) printf '%s\n' '^pure_dl_[0-9][0-9][0-9][0-9]_.*\.owl$' ;;
    el) printf '%s\n' '^el_[0-9][0-9][0-9][0-9]_.*\.owl$' ;;
    all) printf '%s\n' '^(pure_dl|el)_[0-9][0-9][0-9][0-9]_.*\.owl$' ;;
    *) return 1 ;;
  esac
}

ore_validate_group() {
  case "$1" in
    pure_dl|el|all) return 0 ;;
    *)
      ore_err "Unsupported ORE group: $1 (use pure_dl|el|all)"
      return 1
      ;;
  esac
}

ore_group_dir_candidates() {
  case "$1" in
    pure_dl)
      printf '%s\n' pure_dl pure-dl owl_dl owl-dl owldl proper_owl_dl proper-owl-dl
      ;;
    el)
      printf '%s\n' el owl_el owl-el owl2_el owl2-el owlel
      ;;
    *)
      return 1
      ;;
  esac
}

ore_safe_name() {
  printf '%s' "$1" | tr '/ ' '__' | tr -c 'A-Za-z0-9._-' '_'
}
