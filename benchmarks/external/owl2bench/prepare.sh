#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
DATA_ROOT="${OWL2BENCH_DATA_ROOT:-$ROOT_DIR/benchmarks/ontologies/external/owl2bench}"
SOURCE_DIR="${OWL2BENCH_SOURCE_DIR:-$DATA_ROOT/source}"
STAGED_DIR="${OWL2BENCH_STAGED_DIR:-$DATA_ROOT/staged}"
MANIFEST_FILE="$STAGED_DIR/manifest.tsv"

CLONE_URL="${OWL2BENCH_CLONE_URL:-https://github.com/kracr/owl2bench.git}"
AUTO_CLONE="${OWL2BENCH_AUTO_CLONE:-0}"
MAX_FILES="${OWL2BENCH_MAX_FILES:-0}"
LINK_MODE="${OWL2BENCH_LINK_MODE:-link}" # link|copy
CLEAN_STAGE="${OWL2BENCH_CLEAN_STAGE:-1}"

log() { printf '[owl2bench-prepare] %s\n' "$*"; }
err() { printf '[owl2bench-prepare][error] %s\n' "$*" >&2; }

is_truthy() {
  case "${1,,}" in
    1|true|yes|on) return 0 ;;
    *) return 1 ;;
  esac
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

if [[ "$MAX_FILES" =~ ^[0-9]+$ ]] && [[ "$MAX_FILES" -gt 0 ]] && [[ "${#owl_files[@]}" -gt "$MAX_FILES" ]]; then
  owl_files=("${owl_files[@]:0:$MAX_FILES}")
fi

mkdir -p "$STAGED_DIR"
if is_truthy "$CLEAN_STAGE"; then
  find "$STAGED_DIR" -maxdepth 1 -type f -name '*.owl' -delete
fi
rm -f "$MANIFEST_FILE"

printf 'staged_ontology\tsource_ontology\n' > "$MANIFEST_FILE"

idx=0
for src in "${owl_files[@]}"; do
  rel="${src#$SOURCE_DIR/}"
  safe_rel="$(printf '%s' "$rel" | tr '/ ' '__' | tr -c 'A-Za-z0-9._-' '_')"
  idx=$((idx + 1))
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

  printf '%s\t%s\n' "$dst" "$src" >> "$MANIFEST_FILE"
done

log "Prepared ${#owl_files[@]} ontologies"
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
