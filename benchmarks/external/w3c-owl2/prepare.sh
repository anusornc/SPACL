#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
LANE_DIR="$ROOT_DIR/benchmarks/external/w3c-owl2"
DATA_ROOT="${W3C_OWL2_DATA_ROOT:-$ROOT_DIR/benchmarks/ontologies/external/w3c-owl2}"
SOURCE_DIR="${W3C_OWL2_SOURCE_DIR:-$DATA_ROOT/source}"
STAGED_DIR="${W3C_OWL2_STAGED_DIR:-$DATA_ROOT/staged}"
MANIFEST_FILE="${W3C_OWL2_MANIFEST:-$STAGED_DIR/manifest.tsv}"
CLEAN_STAGE="${W3C_OWL2_CLEAN_STAGE:-1}"

args=(
  "$LANE_DIR/w3c_owl2_harness.py"
  prepare
  --source-dir "$SOURCE_DIR"
  --staged-dir "$STAGED_DIR"
  --manifest "$MANIFEST_FILE"
)

case "${CLEAN_STAGE,,}" in
  1|true|yes|on) args+=(--clean) ;;
esac

python3 "${args[@]}"

cat <<EOF

Next step:
  W3C_OWL2_MANIFEST="$MANIFEST_FILE" \\
  benchmarks/external/w3c-owl2/run.sh
EOF
