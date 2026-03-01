#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
RUN_STAGE="$ROOT_DIR/benchmarks/competitors/scripts/run_stage_benchmark.sh"

if [[ ! -x "$RUN_STAGE" ]]; then
  echo "[error] missing stage runner: $RUN_STAGE" >&2
  exit 2
fi

if [[ $# -gt 0 ]]; then
  ONTOLOGIES=("$@")
else
  ONTOLOGIES=(doid.owl go-basic.owl pato.owl uberon.owl chebi.owl)
fi

ONTOLOGY_SUITE="${ONTOLOGY_SUITE:-large}"
TIMEOUT_SECONDS="${TIMEOUT_SECONDS:-900}"
CHEBI_TIMEOUT_SECONDS="${CHEBI_TIMEOUT_SECONDS:-1800}"
SKIP_BUILD="${SKIP_BUILD:-1}"
SUITE_ID="${SUITE_ID:-stage_suite_$(date +%Y%m%d_%H%M%S)}"

RESULT_ROOT="$ROOT_DIR/benchmarks/competitors/results/history/$SUITE_ID"
mkdir -p "$RESULT_ROOT"

SUMMARY_CSV="$RESULT_ROOT/stage_suite_summary.csv"
SUMMARY_MD="$RESULT_ROOT/stage_suite_summary.md"

cat >"$SUMMARY_CSV" <<EOF
ontology,status,e2e_cold_wall_ms,parse_only_ms,reason_only_stage_ms,parse_share_pct,stage_summary_csv
EOF

extract_metric() {
  local csv_path="$1"
  local mode="$2"
  awk -F, -v m="$mode" '$3==m { print $4 }' "$csv_path" | tail -1
}

for ontology in "${ONTOLOGIES[@]}"; do
  echo "[info] stage suite ontology=$ontology"
  local_timeout="$TIMEOUT_SECONDS"
  include_chebi=0
  if [[ "$ontology" == "chebi.owl" ]]; then
    include_chebi=1
    local_timeout="$CHEBI_TIMEOUT_SECONDS"
  fi

  log_file="$RESULT_ROOT/${ontology%.owl}.log"
  set +e
  (
    cd "$ROOT_DIR"
    ONTOLOGY_SUITE="$ONTOLOGY_SUITE" \
    INCLUDE_CHEBI="$include_chebi" \
    TIMEOUT_SECONDS="$local_timeout" \
    SKIP_BUILD="$SKIP_BUILD" \
    "$RUN_STAGE" "$ontology"
  ) | tee "$log_file"
  rc=${PIPESTATUS[0]}
  set -e

  if [[ "$rc" -ne 0 ]]; then
    echo "[warn] stage run failed ontology=$ontology exit=$rc"
    echo "$ontology,failed,-1,-1,-1,-1,$log_file" >>"$SUMMARY_CSV"
    continue
  fi

  stage_summary_csv="$(grep -Eo '\[ok\] Stage summary: .+stage_summary\.csv' "$log_file" | sed 's/^\[ok\] Stage summary: //' | tail -1)"
  if [[ -z "$stage_summary_csv" || ! -f "$stage_summary_csv" ]]; then
    echo "[warn] missing stage summary for ontology=$ontology"
    echo "$ontology,missing_summary,-1,-1,-1,-1,$log_file" >>"$SUMMARY_CSV"
    continue
  fi

  wall_ms="$(extract_metric "$stage_summary_csv" "e2e_cold_wall")"
  parse_ms="$(extract_metric "$stage_summary_csv" "parse_only")"
  reason_ms="$(extract_metric "$stage_summary_csv" "reason_only_stage")"
  parse_share_pct="$(awk -v parse="$parse_ms" -v wall="$wall_ms" 'BEGIN { if (wall > 0) printf "%.2f", (parse * 100.0 / wall); else print "-1" }')"

  echo "$ontology,success,$wall_ms,$parse_ms,$reason_ms,$parse_share_pct,$stage_summary_csv" >>"$SUMMARY_CSV"
done

{
  echo "# Stage Suite Summary"
  echo
  echo "- Suite ID: \`$SUITE_ID\`"
  echo "- Ontology suite: \`$ONTOLOGY_SUITE\`"
  echo
  echo "| Ontology | Status | E2E cold wall (ms) | Parse-only (ms) | Reason-only stage (ms) | Parse share (%) |"
  echo "|---|---|---:|---:|---:|---:|"
  tail -n +2 "$SUMMARY_CSV" | while IFS=, read -r ontology status wall parse reason share _; do
    echo "| $ontology | $status | $wall | $parse | $reason | $share |"
  done
} >"$SUMMARY_MD"

echo "[ok] Stage suite CSV: $SUMMARY_CSV"
echo "[ok] Stage suite Markdown: $SUMMARY_MD"
