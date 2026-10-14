#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
SCRIPT_DIR="$ROOT_DIR/benchmarks/external/ore"
source "$SCRIPT_DIR/lib.sh"
source "$ROOT_DIR/benchmarks/competitors/scripts/benchmark_result.sh"

RUN_BENCH="$ROOT_DIR/benchmarks/competitors/scripts/run_benchmarks.sh"
RESULTS_HISTORY="${ORE2015_RESULTS_HISTORY:-$ROOT_DIR/benchmarks/competitors/results/history}"
RUN_ID="${1:-${RUN_ID:-}}"

if [[ -z "$RUN_ID" ]]; then
  latest="$(ls -td "$RESULTS_HISTORY"/ore2015_* 2>/dev/null | head -n 1 || true)"
  if [[ -z "$latest" ]]; then
    ore_err "No ore2015_* run directories found in $RESULTS_HISTORY"
    exit 1
  fi
  RUN_ID="$(basename "$latest")"
fi

RUN_DIR="$RESULTS_HISTORY/$RUN_ID"
if [[ ! -d "$RUN_DIR" ]]; then
  ore_err "Run directory not found: $RUN_DIR"
  exit 1
fi

if ! ore_is_truthy "${ORE2015_SKIP_SHARED_REPORT:-0}"; then
  RESULTS_DIR_OVERRIDE="$(dirname "$RESULTS_HISTORY")" RUN_ID="$RUN_ID" "$RUN_BENCH" report
fi

MANIFEST_FILE="${ORE2015_MANIFEST_FILE:-}"
if [[ -z "$MANIFEST_FILE" && -f "$RUN_DIR/ore_metadata.json" ]]; then
  MANIFEST_FILE="$(jq -r '.manifest_file // ""' "$RUN_DIR/ore_metadata.json")"
fi
if [[ -z "$MANIFEST_FILE" || ! -f "$MANIFEST_FILE" ]]; then
  ore_err "ORE manifest unavailable for $RUN_ID"
  exit 1
fi

declare -A manifest_group manifest_official manifest_track manifest_source manifest_oracle_status manifest_oracle_source manifest_oracle_case manifest_expected
while IFS=$'\t' read -r local_group official_group ore_track staged_ontology source_ontology oracle_status oracle_source oracle_case_id consensus_expected; do
  [[ "$local_group" != "local_group" ]] || continue
  key="$(basename "$staged_ontology")"
  manifest_group["$key"]="$local_group"
  manifest_official["$key"]="$official_group"
  manifest_track["$key"]="$ore_track"
  manifest_source["$key"]="$source_ontology"
  manifest_oracle_status["$key"]="$oracle_status"
  manifest_oracle_source["$key"]="$oracle_source"
  manifest_oracle_case["$key"]="$oracle_case_id"
  manifest_expected["$key"]="$consensus_expected"
done < "$MANIFEST_FILE"

ORE_REPORT="$RUN_DIR/ore_consistency_report.md"
ORE_SUMMARY="$RUN_DIR/ore_consistency_summary.tsv"
ORE_DETAILS="$RUN_DIR/ore_consistency_details.tsv"

# ADR 0024: this is a performance lane. The report is performance-first; the
# summary leads with completability counts and recorded timing, and keeps the
# correctness status counts as the standing no-oracle record. Timing fields
# are summarized only where a run records them (-1 or missing means not
# recorded); values are never invented or coerced.
ORE_TIMING_FIELDS=(wall_time_ms reported_time_ms parse_time_ms reason_time_ms)

ore_timing_is_recorded() {
  [[ "$1" =~ ^[0-9]+$ ]]
}

ore_accumulate_timing() {
  local summary_key="$1"
  local prefix="$2"
  local value="$3"
  ore_timing_is_recorded "$value" || return 0
  local -n rec_ref="${prefix}_rec"
  local -n sum_ref="${prefix}_sum"
  local -n min_ref="${prefix}_min"
  local -n max_ref="${prefix}_max"
  local numeric=$((10#$value))
  local current_min="${min_ref["$summary_key"]:-}"
  local current_max="${max_ref["$summary_key"]:-}"
  rec_ref["$summary_key"]=$(( ${rec_ref["$summary_key"]:-0} + 1 ))
  sum_ref["$summary_key"]=$(( ${sum_ref["$summary_key"]:-0} + numeric ))
  if [[ -z "$current_min" ]] || (( numeric < current_min )); then
    min_ref["$summary_key"]="$numeric"
  fi
  if [[ -z "$current_max" ]] || (( numeric > current_max )); then
    max_ref["$summary_key"]="$numeric"
  fi
}

ore_timing_mean_ms() {
  local sum="$1"
  local count="$2"
  (( count > 0 )) || return 0
  awk -v s="$sum" -v c="$count" 'BEGIN { printf "%.1f", s / c }'
}

# Emits "recorded<TAB>min<TAB>mean<TAB>max" for one timing field; the stats
# stay empty when nothing was recorded so no value is ever invented.
ore_timing_tsv_cells() {
  local summary_key="$1"
  local prefix="$2"
  local -n rec_ref="${prefix}_rec"
  local -n sum_ref="${prefix}_sum"
  local -n min_ref="${prefix}_min"
  local -n max_ref="${prefix}_max"
  local count="${rec_ref["$summary_key"]:-0}"
  if (( count == 0 )); then
    printf '0\t\t\t'
    return 0
  fi
  printf '%s\t%s\t%s\t%s' \
    "$count" \
    "${min_ref["$summary_key"]}" \
    "$(ore_timing_mean_ms "${sum_ref["$summary_key"]}" "$count")" \
    "${max_ref["$summary_key"]}"
}

# Same cells rendered for the markdown table; n/a marks unrecorded timing.
ore_timing_md_cells() {
  local summary_key="$1"
  local prefix="$2"
  local -n rec_ref="${prefix}_rec"
  local -n sum_ref="${prefix}_sum"
  local -n min_ref="${prefix}_min"
  local -n max_ref="${prefix}_max"
  local count="${rec_ref["$summary_key"]:-0}"
  if (( count == 0 )); then
    printf '%s | n/a | n/a | n/a' "$count"
    return 0
  fi
  printf '%s | %s | %s | %s' \
    "$count" \
    "${min_ref["$summary_key"]}" \
    "$(ore_timing_mean_ms "${sum_ref["$summary_key"]}" "$count")" \
    "${max_ref["$summary_key"]}"
}

declare -A total completed timeout failed not_available other_execution \
  passed wrong oracle_unavailable not_evaluated unsupported
declare -A wall_rec wall_sum wall_min wall_max \
  reported_rec reported_sum reported_min reported_max \
  parse_rec parse_sum parse_min parse_max \
  reason_rec reason_sum reason_min reason_max
# Differential appendix inputs (ADR 0024): per-ontology engine outputs kept
# side by side as observational evidence only.
declare -A appendix_reasoner_seen appendix_ontology_seen \
  appendix_semantic appendix_execution
printf 'local_group\tofficial_group\tore_track\tontology\tsource_ontology\treasoner\texecution_status\tresult_status\tcorrectness_status\toracle_status\tsemantic_result\toracle_case_id\tconsensus_expected\toracle_source\n' > "$ORE_DETAILS"

mapfile -t result_files < <(
  find "$RUN_DIR" -maxdepth 1 -type f -name '*.json' \
    ! -name 'run_metadata.json' ! -name 'ore_metadata.json' ! -name 'summary.json' | sort
)
if [[ "${#result_files[@]}" -eq 0 ]]; then
  ore_err "No result JSON files found in $RUN_DIR"
  exit 1
fi

for result_file in "${result_files[@]}"; do
  ontology="$(jq -r '.ontology' "$result_file")"
  reasoner="$(jq -r '.reasoner' "$result_file")"
  execution_status="$(jq -r '.execution_status // .status' "$result_file")"
  result_status="$(jq -r '.result_status // .status' "$result_file")"
  correctness_status="$(jq -r '.correctness_status // "not_evaluated"' "$result_file")"
  result_oracle_status="$(jq -r '.oracle.status // "not_configured"' "$result_file")"
  semantic_result="$(jq -r '.semantic_result // .reasoning_result // "unknown"' "$result_file")"

  local_group="${manifest_group[$ontology]:-unknown}"
  official_group="${manifest_official[$ontology]:-unknown}"
  ore_track="${manifest_track[$ontology]:-unknown}"
  source_ontology="${manifest_source[$ontology]:-}"
  oracle_case_id="${manifest_oracle_case[$ontology]:-}"
  consensus_expected="${manifest_expected[$ontology]:-}"
  oracle_source="${manifest_oracle_source[$ontology]:-}"

  printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n' \
    "$local_group" "$official_group" "$ore_track" "$ontology" "$source_ontology" "$reasoner" \
    "$execution_status" "$result_status" "$correctness_status" "$result_oracle_status" "$semantic_result" \
    "$oracle_case_id" "$consensus_expected" "$oracle_source" >> "$ORE_DETAILS"

  summary_key="$local_group	$official_group	$ore_track	$reasoner"
  total["$summary_key"]=$(( ${total["$summary_key"]:-0} + 1 ))
  case "$result_status" in
    passed) passed["$summary_key"]=$(( ${passed["$summary_key"]:-0} + 1 )) ;;
    wrong) wrong["$summary_key"]=$(( ${wrong["$summary_key"]:-0} + 1 )) ;;
    oracle_unavailable) oracle_unavailable["$summary_key"]=$(( ${oracle_unavailable["$summary_key"]:-0} + 1 )) ;;
    not_evaluated) not_evaluated["$summary_key"]=$(( ${not_evaluated["$summary_key"]:-0} + 1 )) ;;
    unsupported) unsupported["$summary_key"]=$(( ${unsupported["$summary_key"]:-0} + 1 )) ;;
  esac
  # Completability: completed + timeout + failed + not_available + other_execution
  # always sums to total, so no execution outcome is silently dropped.
  case "$execution_status" in
    success) completed["$summary_key"]=$(( ${completed["$summary_key"]:-0} + 1 )) ;;
    timeout) timeout["$summary_key"]=$(( ${timeout["$summary_key"]:-0} + 1 )) ;;
    failed) failed["$summary_key"]=$(( ${failed["$summary_key"]:-0} + 1 )) ;;
    not_available) not_available["$summary_key"]=$(( ${not_available["$summary_key"]:-0} + 1 )) ;;
    *) other_execution["$summary_key"]=$(( ${other_execution["$summary_key"]:-0} + 1 )) ;;
  esac
  ore_accumulate_timing "$summary_key" wall "$(jq -r '.wall_time_ms // ""' "$result_file")"
  ore_accumulate_timing "$summary_key" reported "$(jq -r '.reported_time_ms // ""' "$result_file")"
  ore_accumulate_timing "$summary_key" parse "$(jq -r '.parse_time_ms // ""' "$result_file")"
  ore_accumulate_timing "$summary_key" reason "$(jq -r '.reason_time_ms // ""' "$result_file")"

  appendix_reasoner_seen["$reasoner"]=1
  appendix_ontology_seen["$ontology"]=1
  appendix_semantic["$ontology|$reasoner"]="$semantic_result"
  appendix_execution["$ontology|$reasoner"]="$execution_status"
done

mapfile -t appendix_reasoners < <(printf '%s\n' "${!appendix_reasoner_seen[@]}" | sort)
mapfile -t appendix_ontologies < <(printf '%s\n' "${!appendix_ontology_seen[@]}" | sort)

# Observational appendix cell: the semantic result the engine returned, or
# the standard status code when the engine did not complete.
ore_appendix_cell() {
  local ontology="$1"
  local reasoner="$2"
  local execution="${appendix_execution["$ontology|$reasoner"]:-}"
  if [[ "$execution" == "success" ]]; then
    printf '%s' "${appendix_semantic["$ontology|$reasoner"]:-unknown}"
  else
    benchmark_result_status_code "$execution"
  fi
}

ore_render_differential_appendix() {
  echo "## Differential Appendix (Contract; Observational Only)"
  echo
  if [[ "${#appendix_reasoners[@]}" -ge 2 ]]; then
    echo "Per-ontology engine outputs from this comparative run, placed side by side as observational evidence only (ADR 0024): agreement between engines adjudicates no correctness, attaches to no gate, and is never reported as correctness. Each cell shows the semantic result the engine returned, or its execution status code when the engine did not complete."
    echo
    printf '| Ontology |'
    local reasoner ontology
    for reasoner in "${appendix_reasoners[@]}"; do
      printf ' %s |' "$reasoner"
    done
    echo
    printf '|---|'
    for reasoner in "${appendix_reasoners[@]}"; do
      printf -- '---|'
    done
    echo
    for ontology in "${appendix_ontologies[@]}"; do
      printf '| %s |' "$ontology"
      for reasoner in "${appendix_reasoners[@]}"; do
        printf ' %s |' "$(ore_appendix_cell "$ontology" "$reasoner")"
      done
      echo
    done
    echo
  else
    echo "Reserved for future comparative runs (ADR 0024). Engine-output comparisons presented in this section are observational evidence only: agreement between engines adjudicates no correctness, attaches to no gate, and is never reported as correctness. No comparative runs are included in this report; this section is intentionally empty."
    echo
  fi
}

{
  printf 'local_group\tofficial_group\tore_track\treasoner\ttotal\tcompleted\ttimeout\tfailed\tnot_available\tother_execution'
  for timing_field in "${ORE_TIMING_FIELDS[@]}"; do
    printf '\t%s_recorded\t%s_min\t%s_mean\t%s_max' "$timing_field" "$timing_field" "$timing_field" "$timing_field"
  done
  printf '\tpassed\twrong\toracle_unavailable\tnot_evaluated\tunsupported\n'
} > "$ORE_SUMMARY"
for summary_key in "${!total[@]}"; do
  {
    printf '%s\t%s\t%s\t%s\t%s\t%s\t%s' \
      "$summary_key" \
      "${total["$summary_key"]:-0}" \
      "${completed["$summary_key"]:-0}" \
      "${timeout["$summary_key"]:-0}" \
      "${failed["$summary_key"]:-0}" \
      "${not_available["$summary_key"]:-0}" \
      "${other_execution["$summary_key"]:-0}"
    for timing_field in "${ORE_TIMING_FIELDS[@]}"; do
      printf '\t'
      ore_timing_tsv_cells "$summary_key" "${timing_field%_time_ms}"
    done
    printf '\t%s\t%s\t%s\t%s\t%s\n' \
      "${passed["$summary_key"]:-0}" \
      "${wrong["$summary_key"]:-0}" \
      "${oracle_unavailable["$summary_key"]:-0}" \
      "${not_evaluated["$summary_key"]:-0}" \
      "${unsupported["$summary_key"]:-0}"
  } >> "$ORE_SUMMARY"
done

{
  echo "# ORE 2015 Consistency Report"
  echo
  echo "- Run ID: \`$RUN_ID\`"
  echo "- Corpus: $ORE2015_CORPUS_TITLE $ORE2015_CORPUS_EDITION ($ORE2015_CORPUS_DOI)"
  echo "- Lane kind: performance (ADR 0024)."
  echo "- Performance-lane contract: this lane adjudicates no correctness and no public oracle exists for the ORE 2015 consistency sample; \`oracle_unavailable\` is the permanent correctness status of every row, and this report claims execution performance and completability only."
  echo "- Local group mapping: \`pure_dl\` -> proper OWL DL / OWL DL Consistency; \`el\` -> OWL 2 EL / OWL EL Consistency"
  echo "- Timing policy: only recorded timing is summarized and each \`*_recorded\` column counts the runs that recorded the field; \`parse_time_ms\` and \`reason_time_ms\` may be recorded as \`-1\` (not recorded) and unrecorded timing is never coerced or invented."
  echo "- Manifest: \`$MANIFEST_FILE\`"
  echo
  echo "## Execution Performance and Completability by Group, Track, and Reasoner"
  echo
  echo "Completed-run counts, timeouts, failures, and not-available counts per local_group/official_group/ore_track/reasoner. Timing columns summarize only recorded values; \`n/a\` means no run recorded the field."
  echo
  echo "| Local Group | Official Group | Track | Reasoner | Total | Completed | Timeout | Failed | Not Available | Other Execution | Wall ms Recorded | Wall ms Min | Wall ms Mean | Wall ms Max | Reported ms Recorded | Reported ms Min | Reported ms Mean | Reported ms Max | Parse ms Recorded | Parse ms Min | Parse ms Mean | Parse ms Max | Reason ms Recorded | Reason ms Min | Reason ms Mean | Reason ms Max |"
  echo "|---|---|---|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|"
  while IFS=$'\t' read -r local_group official_group ore_track reasoner; do
    summary_key="$local_group	$official_group	$ore_track	$reasoner"
    echo "| $local_group | $official_group | $ore_track | $reasoner | ${total["$summary_key"]:-0} | ${completed["$summary_key"]:-0} | ${timeout["$summary_key"]:-0} | ${failed["$summary_key"]:-0} | ${not_available["$summary_key"]:-0} | ${other_execution["$summary_key"]:-0} | $(ore_timing_md_cells "$summary_key" wall) | $(ore_timing_md_cells "$summary_key" reported) | $(ore_timing_md_cells "$summary_key" parse) | $(ore_timing_md_cells "$summary_key" reason) |"
  done < <(printf '%s\n' "${!total[@]}" | sort)
  echo
  echo "## Correctness Record by Group, Track, and Reasoner"
  echo
  echo "No public oracle exists for the ORE 2015 consistency sample, so this lane adjudicates no correctness (ADR 0024): \`passed\` and \`wrong\` are unreachable by design and \`oracle_unavailable\` is the standing correctness status of every adjudicated row. These counts are retained as the honest record; they carry no gate authority."
  echo
  echo "| Local Group | Official Group | Track | Reasoner | Passed | Wrong | Oracle Unavailable | Not Evaluated | Unsupported |"
  echo "|---|---|---|---|---:|---:|---:|---:|---:|"
  while IFS=$'\t' read -r local_group official_group ore_track reasoner; do
    summary_key="$local_group	$official_group	$ore_track	$reasoner"
    echo "| $local_group | $official_group | $ore_track | $reasoner | ${passed["$summary_key"]:-0} | ${wrong["$summary_key"]:-0} | ${oracle_unavailable["$summary_key"]:-0} | ${not_evaluated["$summary_key"]:-0} | ${unsupported["$summary_key"]:-0} |"
  done < <(printf '%s\n' "${!total[@]}" | sort)
  echo
  ore_render_differential_appendix
  echo "Details TSV: \`$ORE_DETAILS\`"
} > "$ORE_REPORT"

ore_log "RUN_ID=$RUN_ID"
ore_log "ORE report : $ORE_REPORT"
ore_log "ORE summary: $ORE_SUMMARY"
ore_log "ORE details: $ORE_DETAILS"
