#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT_DIR"

require_cmd() {
  if ! command -v "$1" >/dev/null 2>&1; then
    echo "Error: required command not found: $1" >&2
    exit 1
  fi
}

require_cmd jq
require_cmd awk
require_cmd sha256sum
require_cmd date

TS="$(date +%Y%m%d_%H%M%S)"
OUT_DIR="results/publish_safe/${TS}"
mkdir -p "$OUT_DIR"

CLAIMS_CSV="$OUT_DIR/claims.csv"
SOURCES_SHA="$OUT_DIR/sources.sha256"
REPORT_MD="$OUT_DIR/report.md"

: > "$CLAIMS_CSV"
: > "$SOURCES_SHA"

echo "claim_id,category,subject,metric,value,unit,source" >> "$CLAIMS_CSV"

claim() {
  local id="$1"
  local category="$2"
  local subject="$3"
  local metric="$4"
  local value="$5"
  local unit="$6"
  local source="$7"
  echo "${id},${category},${subject},${metric},${value},${unit},${source}" >> "$CLAIMS_CSV"
}

hash_source() {
  local file="$1"
  sha256sum "$file" >> "$SOURCES_SHA"
}

ms_from_slope() {
  local file="$1"
  jq -r '.slope.point_estimate / 1000000' "$file"
}

fmt2() {
  awk -v v="$1" 'BEGIN { printf "%.2f", v }'
}

fmt3() {
  awk -v v="$1" 'BEGIN { printf "%.3f", v }'
}

# Real-world criterion claims (slope estimates)
ONTOLOGIES=("LUBM" "PATO" "DOID" "UBERON" "GO_Basic")

for ont in "${ONTOLOGIES[@]}"; do
  seq_file="target/criterion/sequential_baseline/sequential/${ont}/new/estimates.json"
  if [[ "$ont" == "LUBM" ]]; then
    ada_file="target/criterion/adaptive_classification/hierarchical/LUBM/new/estimates.json"
  else
    ada_file="target/criterion/adaptive_classification/hierarchical_grail/${ont}/new/estimates.json"
  fi

  if [[ ! -f "$seq_file" || ! -f "$ada_file" ]]; then
    echo "Error: missing criterion files for ${ont}" >&2
    echo "  expected: $seq_file" >&2
    echo "  expected: $ada_file" >&2
    exit 1
  fi

  seq_ms_raw="$(ms_from_slope "$seq_file")"
  ada_ms_raw="$(ms_from_slope "$ada_file")"
  speed_raw="$(awk -v s="$seq_ms_raw" -v a="$ada_ms_raw" 'BEGIN { if (a==0) print 0; else print s/a }')"

  seq_ms="$(fmt3 "$seq_ms_raw")"
  ada_ms="$(fmt3 "$ada_ms_raw")"
  speed="$(fmt3 "$speed_raw")"

  claim "realworld_${ont}_seq_ms" "realworld" "$ont" "sequential_time" "$seq_ms" "ms" "$seq_file"
  claim "realworld_${ont}_adaptive_ms" "realworld" "$ont" "adaptive_time" "$ada_ms" "ms" "$ada_file"
  claim "realworld_${ont}_speedup" "realworld" "$ont" "speedup_seq_over_adaptive" "$speed" "x" "$seq_file|$ada_file"

  hash_source "$seq_file"
  hash_source "$ada_file"
done

# Disjunctive wall-clock claims from latest verification log
VER_FILE="$(ls -t results/verification_*.txt 2>/dev/null | head -n1 || true)"
if [[ -z "$VER_FILE" ]]; then
  echo "Error: no results/verification_*.txt file found for disjunctive HermiT claims" >&2
  exit 1
fi

if rg -qi "estimated" "$VER_FILE"; then
  echo "Error: latest verification file contains 'estimated' and is not publish-safe: $VER_FILE" >&2
  exit 1
fi

extract_speedup() {
  local test_name="$1"
  awk -v t="$test_name" '
    $0 ~ "Testing: " t { in_block=1; next }
    in_block && /^Testing:/ { in_block=0 }
    in_block && /Speedup:/ { gsub(/x/, "", $2); print $2; exit }
  ' "$VER_FILE"
}

extract_hermit_ms() {
  local test_name="$1"
  awk -v t="$test_name" '
    $0 ~ "Testing: " t { in_block=1; next }
    in_block && /^Testing:/ { in_block=0 }
    in_block && /^HermiT:/ { gsub(/ms/, "", $2); print $2; exit }
  ' "$VER_FILE"
}

extract_spacl_ms() {
  local test_name="$1"
  awk -v t="$test_name" '
    $0 ~ "Testing: " t { in_block=1; next }
    in_block && /^Testing:/ { in_block=0 }
    in_block && /^SPACL:/ { gsub(/ms/, "", $2); print $2; exit }
  ' "$VER_FILE"
}

for test in "disjunctive_simple.owl" "disjunctive_test.owl"; do
  hermit_ms="$(extract_hermit_ms "$test")"
  spacl_ms="$(extract_spacl_ms "$test")"
  speedup="$(extract_speedup "$test")"

  if [[ -z "$hermit_ms" || -z "$spacl_ms" || -z "$speedup" ]]; then
    echo "Error: failed to parse $test from $VER_FILE" >&2
    exit 1
  fi

  claim "disjunctive_${test}_hermit_ms" "head_to_head" "$test" "hermit_wall_clock" "$hermit_ms" "ms" "$VER_FILE"
  claim "disjunctive_${test}_spacl_ms" "head_to_head" "$test" "spacl_wall_clock" "$spacl_ms" "ms" "$VER_FILE"
  claim "disjunctive_${test}_speedup" "head_to_head" "$test" "speedup_hermit_over_spacl" "$speedup" "x" "$VER_FILE"
done

hash_source "$VER_FILE"

GIT_SHA="$(git rev-parse --short HEAD 2>/dev/null || echo "unknown")"
RUN_DATE="$(date -Iseconds)"

cat > "$REPORT_MD" <<REPORT
# Publish-Safe Claims Report

Generated: ${RUN_DATE}
Git commit: ${GIT_SHA}

This report contains only claims extracted from raw benchmark artifacts.

## Real-World (Criterion slope estimates)

| Ontology | Seq (ms) | Adaptive (ms) | Speedup (seq/adaptive) |
|---|---:|---:|---:|
REPORT

for ont in "${ONTOLOGIES[@]}"; do
  seq_val="$(awk -F, -v id="realworld_${ont}_seq_ms" '$1==id {print $5}' "$CLAIMS_CSV")"
  ada_val="$(awk -F, -v id="realworld_${ont}_adaptive_ms" '$1==id {print $5}' "$CLAIMS_CSV")"
  spd_val="$(awk -F, -v id="realworld_${ont}_speedup" '$1==id {print $5}' "$CLAIMS_CSV")"
  printf "| %s | %s | %s | %sx |\n" "$ont" "$(fmt3 "$seq_val")" "$(fmt3 "$ada_val")" "$(fmt3 "$spd_val")" >> "$REPORT_MD"
done

cat >> "$REPORT_MD" <<REPORT

## Disjunctive Head-to-Head (wall-clock)

Source file: \
\`${VER_FILE}\`

| Test | HermiT (ms) | SPACL (ms) | Speedup (HermiT/SPACL) |
|---|---:|---:|---:|
REPORT

for test in "disjunctive_simple.owl" "disjunctive_test.owl"; do
  h="$(awk -F, -v id="disjunctive_${test}_hermit_ms" '$1==id {print $5}' "$CLAIMS_CSV")"
  s="$(awk -F, -v id="disjunctive_${test}_spacl_ms" '$1==id {print $5}' "$CLAIMS_CSV")"
  x="$(awk -F, -v id="disjunctive_${test}_speedup" '$1==id {print $5}' "$CLAIMS_CSV")"
  printf "| %s | %s | %s | %sx |\n" "$test" "$h" "$s" "$x" >> "$REPORT_MD"
done

cat >> "$REPORT_MD" <<REPORT

## Provenance Files

- Claims CSV: \
\`${CLAIMS_CSV}\`
- Source hashes: \
\`${SOURCES_SHA}\`

## Policy

- No values from files marked as estimated/simulated are accepted.
- Every claim must map to a concrete source file listed in \
\`${SOURCES_SHA}\`.
REPORT

echo "OK: publish-safe report generated"
echo "  $REPORT_MD"
echo "  $CLAIMS_CSV"
echo "  $SOURCES_SHA"
