#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
if [[ -n "${ELK_EL_MATERIALIZER_BIN:-}" ]]; then
  export PROFILE_MATERIALIZER_PROFILE="EL"
  export PROFILE_MATERIALIZER_BIN="$ELK_EL_MATERIALIZER_BIN"
  exec "$SCRIPT_DIR/profile_materializer.sh" "$@"
fi

exec python3 "$SCRIPT_DIR/elk_el_export_boundary.py" "$@"
