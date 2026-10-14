#!/usr/bin/env bash
set -euo pipefail

# SPACL DL engine adapter: the shared SPACL adapter core bound to the DL
# profile (materialize with inverse-role propagation + pinned Oxigraph
# query). Non-DL profile contexts are refused by the core.

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
export SPACL_PROFILE="DL"
exec "$SCRIPT_DIR/spacl_rl_engine.sh" "$@"
