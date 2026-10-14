#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
source "$SCRIPT_DIR/owl2bench_common.sh"

DATA_ROOT="${OWL2BENCH_DATA_ROOT:-$ROOT_DIR/benchmarks/ontologies/external/owl2bench}"
SOURCE_DIR="${OWL2BENCH_SOURCE_DIR:-$DATA_ROOT/source}"
STAGED_DIR="${OWL2BENCH_STAGED_DIR:-$DATA_ROOT/staged}"
MANIFEST_FILE="$STAGED_DIR/manifest.tsv"

CLONE_URL="${OWL2BENCH_CLONE_URL:-https://github.com/kracr/owl2bench.git}"
AUTO_CLONE="${OWL2BENCH_AUTO_CLONE:-0}"
MAX_FILES="${OWL2BENCH_MAX_FILES:-0}"
LINK_MODE="${OWL2BENCH_LINK_MODE:-link}" # link|copy
CLEAN_STAGE="${OWL2BENCH_CLEAN_STAGE:-1}"
PROFILES="${OWL2BENCH_PROFILES:-DL,EL,QL,RL}"
STAGE_FILTER="${OWL2BENCH_STAGE:-all}" # correctness|scalability|all
SMOKE="${OWL2BENCH_SMOKE:-0}"

log() { printf '[owl2bench-prepare] %s\n' "$*"; }
err() { printf '[owl2bench-prepare][error] %s\n' "$*" >&2; }

is_truthy() {
  case "${1,,}" in
    1|true|yes|on) return 0 ;;
    *) return 1 ;;
  esac
}

profile_enabled() {
  local profile="$1"
  local normalized=",${PROFILES// /},"
  normalized="$(owl2bench_upper "$normalized")"
  [[ "$profile" != "unknown" && "$normalized" == *",$profile,"* ]]
}

stage_enabled() {
  local stage="$1"
  [[ "$STAGE_FILTER" == "all" || "$STAGE_FILTER" == "$stage" ]]
}

smoke_enabled_for_row() {
  local profile="$1"
  local stage="$2"
  local abox_size="$3"
  local university_count="$4"
  local key

  if ! is_truthy "$SMOKE"; then
    return 0
  fi

  if [[ "$stage" == "correctness" ]]; then
    key="$profile:$stage"
    if [[ -z "${smoke_seen[$key]:-}" ]]; then
      smoke_seen["$key"]=1
      return 0
    fi
    return 1
  fi

  if [[ "$abox_size" != "unknown" ]]; then
    key="abox:$abox_size"
  elif [[ "$university_count" != "unknown" ]]; then
    key="universities:$university_count"
  else
    key="size:unknown"
  fi

  local profile_size_key="$profile:$stage:$key"
  local profile_count_key="$profile:$stage:count"
  if [[ -n "${smoke_seen[$profile_size_key]:-}" ]]; then
    return 1
  fi
  if [[ "${smoke_seen[$profile_count_key]:-0}" -ge 2 ]]; then
    return 1
  fi
  smoke_seen["$profile_size_key"]=1
  smoke_seen["$profile_count_key"]=$(( ${smoke_seen[$profile_count_key]:-0} + 1 ))
  return 0
}

if [[ ! -d "$SOURCE_DIR" ]]; then
  if is_truthy "$AUTO_CLONE"; then
    log "Source not found, cloning OWL2Bench: $CLONE_URL"
    mkdir -p "$(dirname "$SOURCE_DIR")"
    git clone --depth 1 "$CLONE_URL" "$SOURCE_DIR"
  else
    err "Source directory not found: $SOURCE_DIR"
    err "Set OWL2BENCH_SOURCE_DIR, or run with OWL2BENCH_AUTO_CLONE=1."
    exit 1
  fi
fi

mapfile -t owl_files < <(find "$SOURCE_DIR" -type f -name '*.owl' | sort)
if [[ "${#owl_files[@]}" -eq 0 ]]; then
  err "No .owl files found under: $SOURCE_DIR"
  exit 1
fi

mkdir -p "$STAGED_DIR"
if is_truthy "$CLEAN_STAGE"; then
  find "$STAGED_DIR" -maxdepth 1 -type f -name '*.owl' -delete
fi
rm -f "$MANIFEST_FILE"

owl2bench_write_manifest_header > "$MANIFEST_FILE"

idx=0
selected=0
skipped_missing_scalability_metadata=0
declare -A smoke_seen=()
for src in "${owl_files[@]}"; do
  rel="${src#$SOURCE_DIR/}"
  profile="$(owl2bench_infer_profile "$rel")"
  seed="$(owl2bench_infer_seed "$rel")"
  abox_size="$(owl2bench_infer_abox_size "$rel")"
  university_count="$(owl2bench_infer_university_count "$rel")"
  stage="$(owl2bench_infer_stage "$rel" "$abox_size" "$university_count")"

  if [[ "$stage" == "scalability" ]] \
    && { [[ "$seed" == "unknown" ]] || { [[ "$abox_size" == "unknown" ]] && [[ "$university_count" == "unknown" ]]; }; }; then
    skipped_missing_scalability_metadata=$((skipped_missing_scalability_metadata + 1))
    continue
  fi

  if ! profile_enabled "$profile"; then
    continue
  fi
  if ! stage_enabled "$stage"; then
    continue
  fi
  if ! smoke_enabled_for_row "$profile" "$stage" "$abox_size" "$university_count"; then
    continue
  fi
  if [[ "$MAX_FILES" =~ ^[0-9]+$ ]] && [[ "$MAX_FILES" -gt 0 ]] && [[ "$selected" -ge "$MAX_FILES" ]]; then
    break
  fi

  safe_rel="$(printf '%s' "$rel" | tr '/ ' '__' | tr -c 'A-Za-z0-9._-' '_')"
  idx=$((idx + 1))
  selected=$((selected + 1))
  printf -v staged_name '%04d_%s' "$idx" "$safe_rel"
  dst="$STAGED_DIR/$staged_name"

  if [[ -e "$dst" ]]; then
    rm -f "$dst"
  fi

  if [[ "$LINK_MODE" == "copy" ]]; then
    cp -f "$src" "$dst"
  else
    if ! ln -f "$src" "$dst" 2>/dev/null; then
      cp -f "$src" "$dst"
    fi
  fi

  printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\n' \
    "$dst" "$src" "$profile" "$seed" "$abox_size" "$university_count" "$stage" >> "$MANIFEST_FILE"
done

if [[ "$selected" -eq 0 ]]; then
  err "No OWL2Bench ontologies matched profiles=$PROFILES stage=$STAGE_FILTER smoke=$SMOKE"
  exit 1
fi

log "Prepared $selected ontologies"
log "Skipped scalability rows without seed and size metadata: $skipped_missing_scalability_metadata"
log "Staged dir : $STAGED_DIR"
log "Manifest   : $MANIFEST_FILE"
cat <<EOF

Next step:
  ONTOLOGIES_DIR_OVERRIDE="$STAGED_DIR" \\
  ONTOLOGY_SUITE=standard \\
  REASONERS_OVERRIDE=tableauxx,hermit,konclude,openllet,elk,jfact,pellet \\
  TIMEOUT_SECONDS=900 \\
  SKIP_BUILD=1 \\
  benchmarks/external/owl2bench/run.sh all
EOF
