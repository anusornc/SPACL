#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
SCHEMA_DIR="$ROOT_DIR/specs/blockchain/schemas"
EXAMPLE_DIR="$ROOT_DIR/specs/blockchain/examples"

STRICT_SCHEMA_VALIDATION="${STRICT_SCHEMA_VALIDATION:-0}"

if ! command -v jq >/dev/null 2>&1; then
  echo "[error] jq is required but not found in PATH" >&2
  exit 2
fi

schemas=(
  "$SCHEMA_DIR/transaction-envelope.schema.json"
  "$SCHEMA_DIR/ontology-pack-manifest.schema.json"
)

examples=(
  "$EXAMPLE_DIR/transaction-envelope.example.json"
  "$EXAMPLE_DIR/ontology-pack-manifest.example.json"
)

echo "[info] Validating JSON syntax with jq"
for file in "${schemas[@]}" "${examples[@]}"; do
  if [[ ! -f "$file" ]]; then
    echo "[error] Missing file: $file" >&2
    exit 2
  fi
  jq empty "$file"
done

echo "[info] JSON syntax OK"

have_python_jsonschema=0
if command -v python3 >/dev/null 2>&1; then
  if python3 - <<'PY' >/dev/null 2>&1
import importlib.util
import sys
sys.exit(0 if importlib.util.find_spec("jsonschema") else 1)
PY
  then
    have_python_jsonschema=1
  fi
fi

if [[ "$have_python_jsonschema" -eq 1 ]]; then
  echo "[info] Validating examples against schemas using python jsonschema"
  python3 - "$SCHEMA_DIR" "$EXAMPLE_DIR" <<'PY'
import json
import pathlib
import sys
from jsonschema import Draft202012Validator

schema_dir = pathlib.Path(sys.argv[1])
example_dir = pathlib.Path(sys.argv[2])

pairs = [
    (
        schema_dir / "transaction-envelope.schema.json",
        example_dir / "transaction-envelope.example.json",
    ),
    (
        schema_dir / "ontology-pack-manifest.schema.json",
        example_dir / "ontology-pack-manifest.example.json",
    ),
]

for schema_path, example_path in pairs:
    schema = json.loads(schema_path.read_text(encoding="utf-8"))
    instance = json.loads(example_path.read_text(encoding="utf-8"))
    Draft202012Validator.check_schema(schema)
    validator = Draft202012Validator(schema)
    errors = sorted(validator.iter_errors(instance), key=lambda e: e.path)
    if errors:
      print(f"[error] Validation failed: {example_path}")
      for err in errors:
        path = ".".join(str(p) for p in err.absolute_path) or "<root>"
        print(f"  - {path}: {err.message}")
      sys.exit(1)

print("[info] Schema validation OK")
PY
else
  if [[ "$STRICT_SCHEMA_VALIDATION" == "1" ]]; then
    echo "[error] STRICT_SCHEMA_VALIDATION=1 but python jsonschema is unavailable" >&2
    echo "[hint] Install with: pip install jsonschema" >&2
    exit 2
  fi
  echo "[warn] python jsonschema not found; skipped semantic schema validation"
  echo "[warn] set STRICT_SCHEMA_VALIDATION=1 to require schema validation"
fi

echo "[ok] Blockchain spec validation completed"
