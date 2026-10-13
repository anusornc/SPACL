#!/usr/bin/env bash
set -euo pipefail

# SPACL QL engine adapter: the shared SPACL adapter core bound to the QL
# profile (materialize with inverse-role propagation + pinned Oxigraph
# query). Non-QL profile contexts are refused by the core.

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
export SPACL_PROFILE="QL"
exec "$SCRIPT_DIR/spacl_rl_engine.sh" "$@"
