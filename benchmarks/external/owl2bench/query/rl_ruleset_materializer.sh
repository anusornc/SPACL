#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
export PROFILE_MATERIALIZER_PROFILE="RL"
export PROFILE_MATERIALIZER_BIN="${RL_RULESET_MATERIALIZER_BIN:-}"
exec "$SCRIPT_DIR/profile_materializer.sh" "$@"
