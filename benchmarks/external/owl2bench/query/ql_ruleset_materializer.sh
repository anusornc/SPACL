#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
export PROFILE_MATERIALIZER_PROFILE="QL"
export PROFILE_MATERIALIZER_BIN="${QL_RULESET_MATERIALIZER_BIN:-}"
exec "$SCRIPT_DIR/profile_materializer.sh" "$@"
