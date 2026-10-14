#!/usr/bin/env bash
set -euo pipefail

PROFILE="${PROFILE_MATERIALIZER_PROFILE:-}"
BACKEND="${PROFILE_MATERIALIZER_BIN:-}"
COMMAND="${1:-}"

if [[ "$COMMAND" != "version" && "$COMMAND" != "export" ]]; then
  echo "usage: $0 version | export <source-nquads>" >&2
  exit 2
fi
if [[ "$COMMAND" == "export" && $# -ne 2 ]]; then
  echo "usage: $0 export <source-nquads>" >&2
  exit 2
fi
[[ -n "$PROFILE" ]] || { echo "profile materializer profile is not configured" >&2; exit 125; }
[[ -n "$BACKEND" && -x "$BACKEND" ]] || {
  echo "[$PROFILE-materializer][not_available] external materializer is missing" >&2
  exit 125
}
if [[ "$COMMAND" == "export" ]]; then
  [[ -r "$2" ]] || { echo "[$PROFILE-materializer][not_available] source graph is missing" >&2; exit 125; }
fi
exec "$BACKEND" "$@"
