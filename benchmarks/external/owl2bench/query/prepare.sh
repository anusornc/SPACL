#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../../.." && pwd)"
DATA_ROOT="${OWL2BENCH_DATA_ROOT:-$ROOT_DIR/benchmarks/ontologies/external/owl2bench}"
SOURCE_DIR="${OWL2BENCH_QUERY_SOURCE_DIR:-$DATA_ROOT/query-source}"
STAGED_DIR="${OWL2BENCH_QUERY_STAGED_DIR:-$DATA_ROOT/query-staged}"
MANIFEST_FILE="$STAGED_DIR/manifest.tsv"
SENTINEL="$STAGED_DIR/.owl2bench-query-owned"
SOURCE_ID_EXPLICIT="${OWL2BENCH_QUERY_SOURCE_ID:-}"
SOURCE_CORPUS="${SOURCE_ID_EXPLICIT:-unverified-input}"
SOURCE_REVISION="${OWL2BENCH_QUERY_SOURCE_REVISION:-}"

if [[ ! -d "$SOURCE_DIR" ]]; then
  echo "[owl2bench-query-prepare][error] query source directory not found: $SOURCE_DIR" >&2
  exit 1
fi
command -v python3 >/dev/null 2>&1 || {
  echo "[owl2bench-query-prepare][error] missing required command: python3" >&2
  exit 1
}

mkdir -p "$STAGED_DIR"
if [[ -e "$SENTINEL" ]]; then
  [[ -f "$SENTINEL" ]] || { echo "[owl2bench-query-prepare][error] invalid ownership sentinel" >&2; exit 1; }
else
  if find "$STAGED_DIR" -maxdepth 1 -type f -print -quit | grep -q .; then
    echo "[owl2bench-query-prepare][error] refusing to clean unstamped staged directory: $STAGED_DIR" >&2
    exit 1
  fi
  touch "$SENTINEL"
fi
if [[ -z "$SOURCE_REVISION" ]] && git -C "$SOURCE_DIR" rev-parse HEAD >/dev/null 2>&1; then
  SOURCE_REVISION="$(git -C "$SOURCE_DIR" rev-parse HEAD)"
fi
SOURCE_REVISION="${SOURCE_REVISION:-unknown}"
find "$STAGED_DIR" -maxdepth 1 -type f ! -name "$(basename "$SENTINEL")" -delete
rm -f "$MANIFEST_FILE"
printf 'staged_query\tsource_query\tquery_id\tprofile\tworkload\tsource_corpus\tsource_revision\n' > "$MANIFEST_FILE"

stage_query_record() {
  local source_file="$1"
  local manifest_source="$2"
  local query_id="$3"
  local profile="$4"
  local workload="$5"
  local extension="$6"
  local staged_name
  local staged_path

  staged_name="$(printf '%04d_%s.%s' "$((selected + 1))" "$query_id" "$extension")"
  staged_path="$STAGED_DIR/$staged_name"
  cp -f "$source_file" "$staged_path"
  printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\n' \
    "$staged_path" "$manifest_source" "$query_id" "$profile" "$workload" "$SOURCE_CORPUS" "$SOURCE_REVISION" \
    >> "$MANIFEST_FILE"
  selected=$((selected + 1))
}

infer_profile() {
  local relative="$1"
  local upper
  upper="$(printf '%s' "$relative" | tr '[:lower:]' '[:upper:]')"
  if [[ "$upper" =~ OWL2(DL|EL|QL|RL) ]]; then
    printf '%s\n' "${BASH_REMATCH[1]}"
  elif [[ "$upper" =~ (^|[^A-Z0-9])(DL|EL|QL|RL)([^A-Z0-9]|$) ]]; then
    printf '%s\n' "${BASH_REMATCH[2]}"
  else
    printf '%s\n' "unknown"
  fi
}

selected=0
while IFS= read -r source_query; do
  relative="${source_query#$SOURCE_DIR/}"
  query_id="${relative%.*}"
  query_id="$(printf '%s' "$query_id" | tr '/ ' '__' | tr -c 'A-Za-z0-9._-' '_')"
  profile="$(infer_profile "$relative")"
  workload="$(dirname "$relative")"
  stage_query_record "$source_query" "$source_query" "$query_id" "$profile" "$workload" "${source_query##*.}"
done < <(find "$SOURCE_DIR" -type f \( -iname '*.rq' -o -iname '*.sparql' -o -iname '*.spql' \) | sort)

OFFICIAL_TEXT_DIR="$(mktemp -d)"
trap 'rm -rf "$OFFICIAL_TEXT_DIR"' EXIT
while IFS= read -r source_text; do
  while IFS=$'\t' read -r generated_query query_id profile; do
    [[ -n "$generated_query" ]] || continue
    stage_query_record \
      "$generated_query" \
      "$source_text#$query_id" \
      "$query_id" \
      "$profile" \
      "official-text" \
      "rq"
  done < <(python3 - "$source_text" "$OFFICIAL_TEXT_DIR" <<'PY'
import pathlib
import re
import sys

source = pathlib.Path(sys.argv[1])
output_dir = pathlib.Path(sys.argv[2])
text = source.read_text(encoding="utf-8", errors="replace")
query_starts = list(re.finditer(r"(?im)^\s*SELECT\b", text))

for index, match in enumerate(query_starts, start=1):
    query_end = re.search(r"(?im)^\s*Description\s*:", text[match.start():])
    if query_end is None:
        continue
    query = text[match.start(): match.start() + query_end.start()].strip()
    if not query:
        continue
    block_end = query_starts[index].start() if index < len(query_starts) else len(text)
    block = text[match.start():block_end]
    profile_match = re.search(r"(?im)^\s*Profile\s*:\s*(.+?)\s*$", block)
    profile = re.sub(r"\s+", "", profile_match.group(1)) if profile_match else "unknown"
    query_id = f"owl2bench_query_{index:03d}"
    query_path = output_dir / f"{query_id}.rq"
    query_path.write_text(query + "\n", encoding="utf-8")
    print(f"{query_path}\t{query_id}\t{profile}")
PY
  )
done < <(find "$SOURCE_DIR" -type f -iname 'OWL2BENCH_SPARQL_Queries.txt' | sort)

if [[ "$selected" -eq 0 ]]; then
  echo "[owl2bench-query-prepare][error] no official query files found under: $SOURCE_DIR" >&2
  exit 1
fi

jq -n \
  --arg source_dir "$SOURCE_DIR" \
  --arg staged_dir "$STAGED_DIR" \
  --arg manifest "$MANIFEST_FILE" \
  --arg source_corpus "$SOURCE_CORPUS" \
  --arg source_revision "$SOURCE_REVISION" \
  --arg source_id_explicit "$SOURCE_ID_EXPLICIT" \
  --argjson query_count "$selected" \
  '{source_dir:$source_dir, staged_dir:$staged_dir, manifest:$manifest, query_count:$query_count, source_corpus:$source_corpus, source_revision:$source_revision, source_metadata_status:(if ($source_id_explicit != "" and $source_revision != "unknown") then "verified" else "unverified" end), oracle_policy:"explicit_answer_key_required"}' \
  > "$STAGED_DIR/prepare_summary.json"

echo "[owl2bench-query-prepare] staged=$selected manifest=$MANIFEST_FILE"
