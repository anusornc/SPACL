#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
SCRIPT_DIR="$ROOT_DIR/benchmarks/external/ore"
source "$SCRIPT_DIR/lib.sh"

DATA_ROOT="${ORE2015_DATA_ROOT:-$ROOT_DIR/benchmarks/ontologies/external/ore}"
SOURCE_DIR="${ORE2015_SOURCE_DIR:-$DATA_ROOT/source/ore2015_sample}"
ARCHIVE_FILE="${ORE2015_ARCHIVE:-$DATA_ROOT/source/$ORE2015_CORPUS_FILE}"
STAGED_DIR="${ORE2015_STAGED_DIR:-$DATA_ROOT/staged}"
MANIFEST_FILE="$STAGED_DIR/manifest.tsv"
METADATA_FILE="$STAGED_DIR/corpus_metadata.json"
ORACLE_FILE="$STAGED_DIR/oracle_provenance.tsv"

AUTO_DOWNLOAD="${ORE2015_AUTO_DOWNLOAD:-0}"
MAX_FILES_PER_GROUP="${ORE2015_MAX_FILES_PER_GROUP:-0}"
LINK_MODE="${ORE2015_LINK_MODE:-link}" # link|copy
CLEAN_STAGE="${ORE2015_CLEAN_STAGE:-1}"
REQUESTED_GROUPS="${ORE2015_GROUPS:-pure_dl,el}"
ORACLE_TSV="${ORE2015_ORACLE_TSV:-}"

require_cmd() {
  if ! command -v "$1" >/dev/null 2>&1; then
    ore_err "Missing required command: $1"
    exit 1
  fi
}

require_cmd python3

download_corpus() {
  require_cmd curl
  require_cmd unzip
  mkdir -p "$(dirname "$ARCHIVE_FILE")"
  ore_log "Downloading $ORE2015_CORPUS_TITLE $ORE2015_CORPUS_EDITION from $ORE2015_CORPUS_URL"
  curl -fsSL "$ORE2015_CORPUS_URL" -o "$ARCHIVE_FILE"
  if command -v md5sum >/dev/null 2>&1; then
    actual_md5="$(md5sum "$ARCHIVE_FILE" | awk '{print $1}')"
    if [[ "$actual_md5" != "$ORE2015_CORPUS_MD5" ]]; then
      ore_err "Archive md5 mismatch: expected $ORE2015_CORPUS_MD5 got $actual_md5"
      exit 1
    fi
  fi
  mkdir -p "$SOURCE_DIR"
  unzip -q "$ARCHIVE_FILE" -d "$SOURCE_DIR"
}

find_group_roots() {
  local group="$1"
  local candidate
  while IFS= read -r candidate; do
    find "$SOURCE_DIR" -type d -iname "$candidate" -print
  done < <(ore_group_dir_candidates "$group")
}

collect_group_files() {
  local group="$1"
  local roots=()
  mapfile -t roots < <(find_group_roots "$group" | sort -u)
  if [[ "${#roots[@]}" -eq 0 ]]; then
    return 0
  fi

  local direct_files=()
  mapfile -t direct_files < <(
    find "${roots[@]}" -type f \( -name '*.owl' -o -name '*.ofn' -o -name '*.omn' \) | sort -u
  )
  if [[ "${#direct_files[@]}" -gt 0 ]]; then
    printf '%s\n' "${direct_files[@]}"
    return 0
  fi

  local metadata_files=()
  mapfile -t metadata_files < <(
    find "${roots[@]}" -type f -path '*/consistency/metadata.csv' | sort -u
  )
  for metadata_file in "${metadata_files[@]}"; do
    python3 - "$metadata_file" "$SOURCE_DIR" <<'PY'
import csv
import pathlib
import sys

metadata_path = pathlib.Path(sys.argv[1])
source_dir = pathlib.Path(sys.argv[2])
missing = []

with metadata_path.open(newline="", encoding="utf-8", errors="replace") as handle:
    for row in csv.DictReader(handle):
        filename = (row.get("ore2015_filename") or "").strip()
        if not filename:
            continue
        matches = sorted(source_dir.rglob(filename))
        if not matches:
            missing.append(filename)
            continue
        print(matches[0])

if missing:
    for filename in missing:
        print(
            f"[ore2015][error] metadata ontology not found under {source_dir}: {filename}",
            file=sys.stderr,
        )
    raise SystemExit(1)
PY
  done
}

oracle_lookup() {
  local key="$1"
  local base="$2"
  local row=""
  if [[ -n "$ORACLE_TSV" && -f "$ORACLE_TSV" ]]; then
    row="$(awk -F '\t' -v key="$key" -v base="$base" '
      NR == 1 { next }
      $1 == key || $1 == base { print; exit }
    ' "$ORACLE_TSV")"
  fi

  if [[ -n "$row" ]]; then
    printf '%s\n' "$row"
  else
    printf '%s\t%s\t%s\t%s\n' "$key" "unavailable" "" ""
  fi
}

if [[ ! -d "$SOURCE_DIR" ]]; then
  if [[ -f "$ARCHIVE_FILE" ]]; then
    require_cmd unzip
    mkdir -p "$SOURCE_DIR"
    unzip -q "$ARCHIVE_FILE" -d "$SOURCE_DIR"
  elif ore_is_truthy "$AUTO_DOWNLOAD"; then
    download_corpus
  else
    ore_err "Source directory not found: $SOURCE_DIR"
    ore_err "Set ORE2015_SOURCE_DIR, place $ORE2015_CORPUS_FILE at $ARCHIVE_FILE, or run with ORE2015_AUTO_DOWNLOAD=1."
    exit 1
  fi
fi

IFS=',' read -r -a groups <<< "$REQUESTED_GROUPS"
if [[ "${#groups[@]}" -eq 0 ]]; then
  ore_err "ORE2015_GROUPS produced no groups"
  exit 1
fi

mkdir -p "$STAGED_DIR"
if ore_is_truthy "$CLEAN_STAGE"; then
  find "$STAGED_DIR" -maxdepth 1 -type f \( -name '*.owl' -o -name '*.ofn' -o -name '*.omn' \) -delete
fi
rm -f "$MANIFEST_FILE" "$METADATA_FILE" "$ORACLE_FILE"

printf 'local_group\tofficial_group\tore_track\tstaged_ontology\tsource_ontology\toracle_status\toracle_source\toracle_case_id\tconsensus_expected\n' > "$MANIFEST_FILE"
printf 'local_group\tstaged_ontology\tsource_ontology\toracle_status\toracle_source\toracle_case_id\tconsensus_expected\n' > "$ORACLE_FILE"

total=0
for raw_group in "${groups[@]}"; do
  group="$(printf '%s' "$raw_group" | tr -d '[:space:]')"
  [[ -n "$group" ]] || continue
  ore_validate_group "$group"
  [[ "$group" != "all" ]] || { ore_err "prepare groups must be concrete: use pure_dl,el"; exit 1; }

  official_group="$(ore_official_group_for "$group")"
  ore_track="$(ore_track_for "$group")"
  mapfile -t owl_files < <(collect_group_files "$group")
  if [[ "$MAX_FILES_PER_GROUP" =~ ^[0-9]+$ ]] && [[ "$MAX_FILES_PER_GROUP" -gt 0 ]] && [[ "${#owl_files[@]}" -gt "$MAX_FILES_PER_GROUP" ]]; then
    owl_files=("${owl_files[@]:0:$MAX_FILES_PER_GROUP}")
  fi
  if [[ "${#owl_files[@]}" -eq 0 ]]; then
    ore_log "No source ontologies found for local group $group under $SOURCE_DIR"
    continue
  fi

  idx=0
  for src in "${owl_files[@]}"; do
    rel="${src#$SOURCE_DIR/}"
    idx=$((idx + 1))
    printf -v staged_name '%s_%04d_%s' "$group" "$idx" "$(ore_safe_name "$rel")"
    [[ "$staged_name" == *.owl ]] || staged_name="${staged_name%.*}.owl"
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

    oracle_row="$(oracle_lookup "$rel" "$(basename "$src")")"
    oracle_status="$(printf '%s\n' "$oracle_row" | awk -F '\t' '{print $2}')"
    oracle_source="$(printf '%s\n' "$oracle_row" | awk -F '\t' '{print $3}')"
    consensus_expected="$(printf '%s\n' "$oracle_row" | awk -F '\t' '{print $4}')"
    oracle_case_id="$group:$rel:consistency"
    if [[ "$oracle_status" != "available" ]]; then
      oracle_status="unavailable"
      oracle_source=""
      consensus_expected=""
    fi

    printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n' \
      "$group" "$official_group" "$ore_track" "$dst" "$src" "$oracle_status" "$oracle_source" "$oracle_case_id" "$consensus_expected" >> "$MANIFEST_FILE"
    printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\n' \
      "$group" "$dst" "$src" "$oracle_status" "$oracle_source" "$oracle_case_id" "$consensus_expected" >> "$ORACLE_FILE"
    total=$((total + 1))
  done
done

if [[ "$total" -eq 0 ]]; then
  ore_err "No ORE ontologies staged. Expected group directories such as pure_dl/ and el/ under $SOURCE_DIR."
  exit 1
fi

jq -n \
  --arg corpus_title "$ORE2015_CORPUS_TITLE" \
  --arg corpus_edition "$ORE2015_CORPUS_EDITION" \
  --arg corpus_doi "$ORE2015_CORPUS_DOI" \
  --arg corpus_url "$ORE2015_CORPUS_URL" \
  --arg corpus_file "$ORE2015_CORPUS_FILE" \
  --arg corpus_md5 "$ORE2015_CORPUS_MD5" \
  --arg source_dir "$SOURCE_DIR" \
  --arg staged_dir "$STAGED_DIR" \
  --arg manifest_file "$MANIFEST_FILE" \
  --arg oracle_file "$ORACLE_FILE" \
  --argjson staged_count "$total" \
  '{
    corpus: {
      title: $corpus_title,
      edition: $corpus_edition,
      doi: $corpus_doi,
      source_url: $corpus_url,
      archive_file: $corpus_file,
      archive_md5: $corpus_md5
    },
    group_mapping: {
      pure_dl: {official_group: "OWL DL", profile: "proper OWL DL", track: "OWL DL Consistency"},
      el: {official_group: "OWL 2 EL", profile: "OWL 2 EL", track: "OWL EL Consistency"}
    },
    oracle_policy: {
      consensus_inputs_are_provenance: true,
      external_gold_labels_present: false,
      missing_or_ambiguous_oracle_result: "oracle_unavailable"
    },
    source_dir: $source_dir,
    staged_dir: $staged_dir,
    manifest_file: $manifest_file,
    oracle_file: $oracle_file,
    staged_count: $staged_count
  }' > "$METADATA_FILE"

ore_log "Prepared $total ORE 2015 ontologies"
ore_log "Staged dir : $STAGED_DIR"
ore_log "Manifest   : $MANIFEST_FILE"
ore_log "Metadata   : $METADATA_FILE"
cat <<EOF

Next step:
  RUN_ID=ore2015_consistency_smoke \\
  REASONERS_OVERRIDE=tableauxx \\
  benchmarks/external/ore/run.sh all run
EOF
