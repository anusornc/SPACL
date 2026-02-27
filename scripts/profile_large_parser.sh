#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
RUN_ID="${RUN_ID:-parser_profile_$(date +%Y%m%d_%H%M%S)}"
TIMEOUT_SECONDS="${TIMEOUT_SECONDS:-900}"
INCLUDE_CHEBI="${INCLUDE_CHEBI:-1}"
SKIP_BUILD="${SKIP_BUILD:-1}"
REASONERS_OVERRIDE="${REASONERS_OVERRIDE:-tableauxx}"
ONTOLOGY_REGEX="${ONTOLOGY_REGEX:-}"
SKIP_RUN="${SKIP_RUN:-0}"

cd "$ROOT_DIR"

export OWL2_REASONER_STAGE_TIMING="${OWL2_REASONER_STAGE_TIMING:-1}"
export OWL2_REASONER_FORCE_TEXT="${OWL2_REASONER_FORCE_TEXT:-1}"
export OWL2_REASONER_BIN_ONLY="${OWL2_REASONER_BIN_ONLY:-0}"
export OWL2_REASONER_AUTO_CACHE="${OWL2_REASONER_AUTO_CACHE:-0}"
export OWL2_REASONER_DISABLE_PARSE_FALLBACK="${OWL2_REASONER_DISABLE_PARSE_FALLBACK:-1}"

cmd=(benchmarks/competitors/scripts/run_benchmarks.sh run)

env_args=(
  "RUN_ID=$RUN_ID"
  "ONTOLOGY_SUITE=large"
  "INCLUDE_CHEBI=$INCLUDE_CHEBI"
  "TIMEOUT_SECONDS=$TIMEOUT_SECONDS"
  "SKIP_BUILD=$SKIP_BUILD"
  "REASONERS_OVERRIDE=$REASONERS_OVERRIDE"
)
if [[ -n "$ONTOLOGY_REGEX" ]]; then
  env_args+=("ONTOLOGY_REGEX=$ONTOLOGY_REGEX")
fi

if [[ "$SKIP_RUN" != "1" ]]; then
  env "${env_args[@]}" "${cmd[@]}"
fi

run_dir="$ROOT_DIR/benchmarks/competitors/results/history/$RUN_ID"
summary_md="$run_dir/parser_stage_profile.md"
summary_csv="$run_dir/parser_stage_profile.csv"

if [[ ! -d "$run_dir" ]]; then
  echo "Run directory not found: $run_dir" >&2
  exit 1
fi

{
  echo "ontology,status,wall_time_ms,parse_time_ms,reason_time_ms,parse_share_pct"
  for f in "$run_dir"/tableauxx_*.json; do
    [[ -f "$f" ]] || continue
    ontology="$(jq -r '.ontology' "$f")"
    status="$(jq -r '.status' "$f")"
    wall="$(jq -r '.wall_time_ms // -1' "$f")"
    parse="$(jq -r '.parse_time_ms // -1' "$f")"
    reason="$(jq -r '.reason_time_ms // -1' "$f")"
    if [[ "$parse" -lt 0 ]]; then
      parse="$(jq -r '.output // ""' "$f" | grep -Eo '\[phase\][[:space:]]+parse_done[[:space:]]+ms=[0-9]+' | tail -1 | grep -Eo '[0-9]+' || true)"
      if [[ -z "$parse" || ! "$parse" =~ ^[0-9]+$ ]]; then
        parse=-1
      fi
    fi
    if [[ "$reason" -lt 0 ]]; then
      reason="$(jq -r '.output // ""' "$f" | grep -Eo '\[phase\][[:space:]]+reason_done[[:space:]]+ms=[0-9]+' | tail -1 | grep -Eo '[0-9]+' || true)"
      if [[ -z "$reason" || ! "$reason" =~ ^[0-9]+$ ]]; then
        reason=-1
      fi
    fi
    parse_share=""
    if [[ "$wall" =~ ^[0-9]+$ ]] && [[ "$parse" =~ ^[0-9]+$ ]] && [[ "$wall" -gt 0 ]] && [[ "$parse" -ge 0 ]]; then
      parse_share="$(awk -v p="$parse" -v w="$wall" 'BEGIN { printf "%.2f", (p*100.0)/w }')"
    else
      parse_share="NA"
    fi
    echo "$ontology,$status,$wall,$parse,$reason,$parse_share"
  done | sort
} > "$summary_csv"

{
  echo "# Parser Stage Profile"
  echo
  printf -- '- Run ID: `%s`\n' "$RUN_ID"
  printf -- '- Scope: `%s` ontologies\n' "large"
  printf -- '- Reasoners: `%s`\n' "$REASONERS_OVERRIDE"
  printf -- '- Timeout: `%ss`\n' "$TIMEOUT_SECONDS"
  printf -- '- Force text parse: `%s`\n' "$OWL2_REASONER_FORCE_TEXT"
  printf -- '- Stage timing: `%s`\n' "$OWL2_REASONER_STAGE_TIMING"
  echo
  echo "| Ontology | Status | Wall (ms) | Parse (ms) | Reason (ms) | Parse Share |"
  echo "|---|---|---:|---:|---:|---:|"
  tail -n +2 "$summary_csv" | while IFS=, read -r ontology status wall parse reason share; do
    if [[ "$share" == "NA" ]]; then
      share_cell="NA"
    else
      share_cell="${share}%"
    fi
    echo "| $ontology | $status | $wall | $parse | $reason | $share_cell |"
  done
  echo
  printf -- 'Source CSV: `%s`\n' "$summary_csv"
} > "$summary_md"

echo "Profile summary written: $summary_md"
echo "Profile csv written: $summary_csv"
