#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

RUN_DIR="${1:-}"
MANIFEST_FILE="${2:-$RUN_DIR/owl2bench_manifest.tsv}"

if [[ -z "$RUN_DIR" ]]; then
  echo "Usage: $0 <run-dir> [manifest.tsv]" >&2
  exit 1
fi
if [[ ! -d "$RUN_DIR" ]]; then
  echo "[owl2bench-stage-report][error] run directory not found: $RUN_DIR" >&2
  exit 1
fi
if [[ ! -f "$RUN_DIR/results.csv" ]]; then
  echo "[owl2bench-stage-report][error] missing harness CSV: $RUN_DIR/results.csv" >&2
  exit 1
fi
if [[ ! -f "$MANIFEST_FILE" ]]; then
  echo "[owl2bench-stage-report][error] missing OWL2Bench manifest: $MANIFEST_FILE" >&2
  exit 1
fi

OUT_CSV="$RUN_DIR/owl2bench_stage_results.csv"
OUT_MD="$RUN_DIR/owl2bench_stage_report.md"

awk -F '\t' '
  NR == 1 { next }
  {
    base=$1
    sub(/^.*\//, "", base)
    meta[base]=$3 "\t" $4 "\t" $5 "\t" $6 "\t" $7 "\t" $2
  }
  END {
    for (name in meta) {
      print name "\t" meta[name]
    }
  }
' "$MANIFEST_FILE" | sort > "$RUN_DIR/.owl2bench_manifest_by_name.tsv"

python3 - "$RUN_DIR/results.csv" "$RUN_DIR/.owl2bench_manifest_by_name.tsv" "$OUT_CSV" "$OUT_MD" <<'PY'
import csv
import sys
from collections import defaultdict
from pathlib import Path

results_csv = Path(sys.argv[1])
manifest_tsv = Path(sys.argv[2])
out_csv = Path(sys.argv[3])
out_md = Path(sys.argv[4])

metadata = {}
with manifest_tsv.open(newline="") as f:
    for row in csv.reader(f, delimiter="\t"):
        if len(row) != 7:
            continue
        ontology, profile, seed, abox_size, university_count, stage, source = row
        metadata[ontology] = {
            "profile": profile,
            "seed": seed,
            "abox_size": abox_size,
            "university_count": university_count,
            "stage": stage,
            "source_ontology": source,
        }

with results_csv.open(newline="") as f:
    rows = list(csv.DictReader(f))

fieldnames = [
    "run_id",
    "stage",
    "profile",
    "seed",
    "abox_size",
    "university_count",
    "reasoner",
    "ontology",
    "operation",
    "execution_status",
    "result_status",
    "correctness_status",
    "semantic_result",
    "wall_time_ms",
    "reported_time_ms",
    "parse_time_ms",
    "reason_time_ms",
    "source_ontology",
]

enriched = []
for row in rows:
    meta = metadata.get(row.get("ontology", ""), {})
    enriched.append({
        "run_id": row.get("run_id", ""),
        "stage": meta.get("stage", "unknown"),
        "profile": meta.get("profile", "unknown"),
        "seed": meta.get("seed", "unknown"),
        "abox_size": meta.get("abox_size", "unknown"),
        "university_count": meta.get("university_count", "unknown"),
        "reasoner": row.get("reasoner", ""),
        "ontology": row.get("ontology", ""),
        "operation": row.get("operation", ""),
        "execution_status": row.get("execution_status", row.get("status", "")),
        "result_status": row.get("result_status", ""),
        "correctness_status": row.get("correctness_status", ""),
        "semantic_result": row.get("semantic_result", row.get("reasoning_result", "")),
        "wall_time_ms": row.get("wall_time_ms", ""),
        "reported_time_ms": row.get("reported_time_ms", ""),
        "parse_time_ms": row.get("parse_time_ms", ""),
        "reason_time_ms": row.get("reason_time_ms", ""),
        "source_ontology": meta.get("source_ontology", ""),
    })

with out_csv.open("w", newline="") as f:
    writer = csv.DictWriter(f, fieldnames=fieldnames)
    writer.writeheader()
    writer.writerows(enriched)

summary = defaultdict(lambda: defaultdict(int))
performance = defaultdict(lambda: {"total": 0, "eligible": 0, "wall": 0})

for row in enriched:
    key = (row["stage"], row["profile"])
    execution = row["execution_status"]
    result = row["result_status"]
    summary[key]["total"] += 1
    summary[key][f"execution:{execution}"] += 1
    summary[key][f"result:{result}"] += 1
    if row["stage"] == "scalability":
        perf = performance[key]
        perf["total"] += 1
        try:
            wall = int(row["wall_time_ms"])
        except ValueError:
            wall = -1
        # ADR 0027: this lane adjudicates no correctness, so scalability
        # timing averages count completed executions with recorded wall
        # time; correctness statuses never gate them.
        if execution == "success" and wall >= 0:
            perf["eligible"] += 1
            perf["wall"] += wall

with out_md.open("w") as f:
    f.write("# OWL2Bench Stage Report\n\n")
    f.write(f"- Source CSV: `{results_csv.name}`\n")
    f.write(f"- Enriched CSV: `{out_csv.name}`\n")
    f.write("- Lane kind: performance (ADR 0027).\n")
    f.write(
        "- Performance-lane contract: this lane adjudicates no correctness and no "
        "oracle exists for OWL2Bench consistency; `oracle_unavailable` is the "
        "permanent correctness status of every row, and this report claims "
        "execution performance and completability only.\n"
    )
    f.write("- Query workloads are not evaluated here; issue #7 owns query execution and query oracles.\n\n")
    f.write("## Stage Summary\n\n")
    f.write("| Stage | Profile | Total | Success | Failed | Timeout | Not Available | Wrong | Performance Eligible | Avg Wall (ms) |\n")
    f.write("|---|---|---:|---:|---:|---:|---:|---:|---:|---:|\n")
    for stage, profile in sorted(summary):
        counts = summary[(stage, profile)]
        perf = performance[(stage, profile)]
        eligible = perf["eligible"]
        avg = "N/A"
        if perf["eligible"]:
            avg = str(perf["wall"] // perf["eligible"])
        f.write(
            f"| {stage} | {profile} | {counts['total']} | {counts['execution:success']} | "
            f"{counts['execution:failed']} | {counts['execution:timeout']} | "
            f"{counts['execution:not_available']} | {counts['result:wrong']} | "
            f"{eligible} | {avg} |\n"
        )
    f.write(
        "\nNo oracle exists for OWL2Bench consistency (ADR 0027), so this lane "
        "adjudicates no correctness: `passed` and `wrong` are unreachable by "
        "design and `oracle_unavailable` is the standing correctness status of "
        "every row. The result columns above are the honest record; they carry "
        "no gate authority. Scalability averages summarize completed executions "
        "with recorded wall time only.\n"
    )
PY

rm -f "$RUN_DIR/.owl2bench_manifest_by_name.tsv"
echo "[owl2bench-stage-report] csv   : $OUT_CSV"
echo "[owl2bench-stage-report] report: $OUT_MD"
