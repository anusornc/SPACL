#!/usr/bin/env python3
"""Run the Openllet QL acceptance inventory and emit a structured receipt."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import subprocess
from datetime import datetime, timezone
from pathlib import Path

from openllet_ql_publish_acceptance import EXPECTED_TEST_PATHS, digest, repo_root


def sha256_bytes(value: bytes) -> str:
    return "sha256:" + hashlib.sha256(value).hexdigest()


def parse_generated_at(value: str | None) -> str:
    if value is None:
        source_date_epoch = os.environ.get("SOURCE_DATE_EPOCH")
        if source_date_epoch is not None:
            return datetime.fromtimestamp(int(source_date_epoch), timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")
        raise SystemExit("--generated-at or SOURCE_DATE_EPOCH is required")
    try:
        parsed = datetime.strptime(value, "%Y-%m-%dT%H:%M:%SZ").replace(tzinfo=timezone.utc)
    except ValueError as exc:
        raise SystemExit("--generated-at must be a UTC timestamp like 2026-09-01T12:00:00Z") from exc
    return parsed.strftime("%Y-%m-%dT%H:%M:%SZ")


def write_json(path: Path, value: dict) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, indent=2, sort_keys=True) + "\n", encoding="utf-8")


def parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", type=Path, required=True)
    parser.add_argument("--generated-at")
    return parser


def main() -> int:
    args = parser().parse_args()
    root = repo_root()
    generated_at = parse_generated_at(args.generated_at)
    tests = []
    for script in EXPECTED_TEST_PATHS:
        script_path = root / script
        completed = subprocess.run(
            ["bash", str(script_path)],
            cwd=root,
            check=False,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
        )
        tests.append(
            {
                "script": script,
                "script_sha256": digest(script_path),
                "exit_code": completed.returncode,
                "status": "complete" if completed.returncode == 0 else "failed",
                "stdout_sha256": sha256_bytes(completed.stdout),
                "stderr_sha256": sha256_bytes(completed.stderr),
            }
        )
    receipt = {
        "schema_version": 1,
        "generated_at": generated_at,
        "generated_by": "openllet_ql_inventory_receipt.py",
        "runner_sha256": digest(Path(__file__).resolve()),
        "tests": tests,
    }
    write_json(args.output, receipt)
    return 0 if all(test["status"] == "complete" for test in tests) else 1


if __name__ == "__main__":
    raise SystemExit(main())
