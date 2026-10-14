"""Derive stable names for OWL2Bench execution resource envelopes."""

from __future__ import annotations

import hashlib
import json
from typing import Any


DEFAULT_BASELINE = {
    "timeout_seconds": 600,
    "memory": "24GiB",
    "threads": 1,
    "cpu": "1",
    "disk": "10GiB",
    "network": "local-only",
}


def resource_variant(
    resource_limits: dict[str, Any],
    baseline: dict[str, Any] | None = None,
) -> str:
    """Return a reproducible baseline/custom identifier for an envelope."""

    baseline_values = baseline or DEFAULT_BASELINE
    if all(resource_limits.get(key) == value for key, value in baseline_values.items()):
        return "owl2bench-comparable-baseline"
    payload = json.dumps(
        {key: value for key, value in resource_limits.items() if key != "variant"},
        ensure_ascii=False,
        sort_keys=True,
        separators=(",", ":"),
    ).encode("utf-8")
    return "owl2bench-custom-" + hashlib.sha256(payload).hexdigest()[:16]
