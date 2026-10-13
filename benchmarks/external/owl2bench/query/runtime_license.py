"""Secret-free runtime license provenance for licensed benchmark engines."""

from __future__ import annotations

import os
import re
from typing import Any, Mapping


SHA256 = re.compile(r"^(?:sha256:)?[0-9a-fA-F]{64}$")


def license_identity(
    environment: Mapping[str, str] | None = None,
) -> dict[str, Any]:
    """Return a validated license identity without reading license contents."""

    values = environment if environment is not None else os.environ
    license_id = values.get("GRAPHDB_LICENSE_ID", "graphdb-runtime-license")
    license_digest = values.get("GRAPHDB_LICENSE_DIGEST", "").strip()
    if not license_digest:
        return {
            "status": "not_available",
            "id": license_id,
            "boundary": "runtime license status is established only when export succeeds",
            "reason": "No GraphDB runtime license digest was supplied",
        }
    if not SHA256.fullmatch(license_digest):
        raise ValueError("GRAPHDB_LICENSE_DIGEST must be a SHA-256 digest")
    if not license_id.strip():
        raise ValueError("GRAPHDB_LICENSE_ID must be non-empty")
    return {
        "status": "available",
        "id": license_id,
        "digest": license_digest,
        "provenance": "secret-free digest supplied via GRAPHDB_LICENSE_DIGEST",
    }
