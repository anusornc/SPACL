#!/usr/bin/env bash
set -euo pipefail

# SPACL EL engine adapter: the shared SPACL adapter core bound to the EL
# profile (materialize with the profile's fragment + pinned Oxigraph query).
# Non-EL profile contexts are refused by the core.

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
export SPACL_PROFILE="EL"
exec "$SCRIPT_DIR/spacl_rl_engine.sh" "$@"
