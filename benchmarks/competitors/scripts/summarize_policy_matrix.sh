#!/usr/bin/env bash
set -euo pipefail

if [[ $# -lt 5 ]]; then
  echo "Usage: $0 <output.csv> <baseline.csv> <heuristic.csv> <hybrid_model.csv> <hybrid_fallback.csv>" >&2
  exit 2
fi

OUTPUT_CSV="$1"
BASELINE_CSV="$2"
HEURISTIC_CSV="$3"
HYBRID_MODEL_CSV="$4"
HYBRID_FALLBACK_CSV="$5"

WORKLOADS="${WORKLOADS:-mixed_operands_16,mixed_operands_32,reused_conflict_12}"
IFS=',' read -r -a WORKLOAD_LIST <<< "$WORKLOADS"

echo "view,policy,workload,n,mean_ms,median_ms,p95_ms,q1_ms,q3_ms,iqr_ms,min_ms,max_ms,mean_reorders,mean_branches_pruned,mean_nogood_hits,mean_hybrid_model_calls,mean_policy_fallbacks" > "$OUTPUT_CSV"

quantile_index() {
  local p="$1"
  local n="$2"
  local idx=$(( (p * n + 99) / 100 - 1 ))
  if (( idx < 0 )); then
    idx=0
  fi
  if (( idx >= n )); then
    idx=$((n - 1))
  fi
  echo "$idx"
}

emit_stats() {
  local view="$1"
  local policy="$2"
  local csv_file="$3"
  local workload="$4"
  local reordered_only="$5"

  mapfile -t vals < <(
    awk -F, -v w="$workload" -v ro="$reordered_only" '
      NR > 1 && $1 == w && (ro == 0 || $19 > 0) { print $6 }
    ' "$csv_file" | sort -n
  )

  local n=${#vals[@]}
  if (( n == 0 )); then
    return
  fi

  local min="${vals[0]}"
  local max="${vals[$((n - 1))]}"
  local q1_idx q3_idx p95_idx
  q1_idx="$(quantile_index 25 "$n")"
  q3_idx="$(quantile_index 75 "$n")"
  p95_idx="$(quantile_index 95 "$n")"

  local q1="${vals[$q1_idx]}"
  local q3="${vals[$q3_idx]}"
  local p95="${vals[$p95_idx]}"
  local median
  if (( n % 2 == 1 )); then
    median="${vals[$((n / 2))]}"
  else
    local a="${vals[$((n / 2 - 1))]}"
    local b="${vals[$((n / 2))]}"
    median="$(awk -v a="$a" -v b="$b" 'BEGIN { printf "%.3f", (a + b) / 2.0 }')"
  fi

  local iqr
  iqr="$(awk -v a="$q3" -v b="$q1" 'BEGIN { printf "%.3f", a - b }')"

  local means
  means="$(
    awk -F, -v w="$workload" -v ro="$reordered_only" '
      NR > 1 && $1 == w && (ro == 0 || $19 > 0) {
        c++;
        wall += $6;
        reorders += $19;
        pruned += $13;
        nogood += $14;
        hm += $22;
        fallback += $20;
      }
      END {
        if (c > 0) {
          printf "%.3f,%.3f,%.3f,%.3f,%.3f,%.3f",
            wall / c, reorders / c, pruned / c, nogood / c, hm / c, fallback / c;
        }
      }
    ' "$csv_file"
  )"

  local mean_ms mean_reorders mean_branches_pruned mean_nogood_hits mean_hybrid_model_calls mean_policy_fallbacks
  IFS=',' read -r mean_ms mean_reorders mean_branches_pruned mean_nogood_hits mean_hybrid_model_calls mean_policy_fallbacks <<< "$means"

  echo "$view,$policy,$workload,$n,$mean_ms,$median,$p95,$q1,$q3,$iqr,$min,$max,$mean_reorders,$mean_branches_pruned,$mean_nogood_hits,$mean_hybrid_model_calls,$mean_policy_fallbacks" >> "$OUTPUT_CSV"
}

for workload in "${WORKLOAD_LIST[@]}"; do
  emit_stats "all" "baseline" "$BASELINE_CSV" "$workload" 0
  emit_stats "all" "heuristic" "$HEURISTIC_CSV" "$workload" 0
  emit_stats "all" "hybrid_model" "$HYBRID_MODEL_CSV" "$workload" 0
  emit_stats "all" "hybrid_fallback" "$HYBRID_FALLBACK_CSV" "$workload" 0

  emit_stats "reordered_only" "baseline" "$BASELINE_CSV" "$workload" 1
  emit_stats "reordered_only" "heuristic" "$HEURISTIC_CSV" "$workload" 1
  emit_stats "reordered_only" "hybrid_model" "$HYBRID_MODEL_CSV" "$workload" 1
  emit_stats "reordered_only" "hybrid_fallback" "$HYBRID_FALLBACK_CSV" "$workload" 1
done

echo "$OUTPUT_CSV"
