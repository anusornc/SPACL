#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
if [[ "${1:-}" == "--version" ]]; then
  shift
  exec python3 "$SCRIPT_DIR/openllet_ql_materialization.py" version "$@"
fi
exec python3 "$SCRIPT_DIR/openllet_ql_materialization.py" "$@"
