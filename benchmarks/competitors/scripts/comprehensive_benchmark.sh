#!/usr/bin/env bash
#
# Backward-compatible entrypoint for the head-to-head harness.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
exec "$SCRIPT_DIR/run_benchmarks.sh" all
