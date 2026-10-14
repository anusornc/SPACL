#!/usr/bin/env bash
set -euo pipefail

if [[ $# -lt 3 ]]; then
  echo "Usage: $0 <run_id> <criterion_group> <summary_generator_args...>" >&2
  echo "Example: $0 paper_synth_refresh_20260416_123000 scalability --metric point_estimate" >&2
  exit 1
fi

RUN_ID="$1"
CRITERION_GROUP="$2"
shift 2

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
RUN_DIR="$ROOT_DIR/benchmarks/competitors/results/history/$RUN_ID"
RAW_DIR="$RUN_DIR/criterion_raw"

mkdir -p "$RAW_DIR"

if [[ ! -d "$ROOT_DIR/target/criterion/$CRITERION_GROUP" ]]; then
  echo "Missing Criterion group: target/criterion/$CRITERION_GROUP" >&2
  exit 1
fi

rm -rf "$RAW_DIR/$CRITERION_GROUP"
cp -r "$ROOT_DIR/target/criterion/$CRITERION_GROUP" "$RAW_DIR/"

python3 "$ROOT_DIR/scripts/generate_synthetic_paper_summaries.py" \
  --criterion-root "$ROOT_DIR/target/criterion" \
  --output-dir "$RUN_DIR" \
  --run-id "$RUN_ID" \
  "$@"

GIT_COMMIT="$(git -C "$ROOT_DIR" rev-parse HEAD)"
RUST_VERSION="$(rustc --version)"
CARGO_VERSION="$(cargo --version)"
HOSTNAME_VALUE="$(hostname)"
CPU_MODEL="$(lscpu | awk -F: '/Model name:/ {gsub(/^[ \t]+/, "", $2); print $2; exit}')"
EXECUTION_DATE="$(date --iso-8601=seconds)"

cat > "$RUN_DIR/run_metadata.json" <<EOF
{
  "run_id": "$RUN_ID",
  "criterion_group": "$CRITERION_GROUP",
  "metric_policy": "point_estimate",
  "benchmark_source": "benches/$CRITERION_GROUP.rs",
  "series": ["sequential", "spacl_adaptive"],
  "workload_set": [100, 500, 1000, 5000, 10000],
  "source_root": "target/criterion/$CRITERION_GROUP",
  "archived_raw_dir": "benchmarks/competitors/results/history/$RUN_ID/criterion_raw/$CRITERION_GROUP",
  "git_commit": "$GIT_COMMIT",
  "execution_date": "$EXECUTION_DATE",
  "rust_version": "$RUST_VERSION",
  "cargo_version": "$CARGO_VERSION",
  "hostname": "$HOSTNAME_VALUE",
  "cpu_model": "$CPU_MODEL"
}
EOF

cat > "$RUN_DIR/README.md" <<EOF
# Synthetic Paper Run: $RUN_ID

- Criterion group: \`$CRITERION_GROUP\`
- Raw Criterion archive: \`criterion_raw/$CRITERION_GROUP\`
- Summary CSVs: generated in this directory
- Metric policy: \`point_estimate\`
- Git commit: \`$GIT_COMMIT\`
- Execution date: \`$EXECUTION_DATE\`
- Host: \`$HOSTNAME_VALUE\`
- CPU: \`$CPU_MODEL\`

This run directory is intended to act as a stable provenance anchor for
paper-facing synthetic artifacts.
EOF

echo "Archived synthetic paper run to: $RUN_DIR"
