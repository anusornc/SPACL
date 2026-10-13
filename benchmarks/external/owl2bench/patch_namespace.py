#!/usr/bin/env python3
"""Namespace patcher for the patched-namespace track (ADR 0019, #95).

Re-bases the OWL2Bench generator's ABox vocabulary onto the TBox
vocabulary: every occurrence of the generator namespace is rewritten to
the document/TBox namespace. The transformation is a pure byte-level
rewrite of the full namespace IRI, so it is deterministic by
construction; the patched track stages its anchors by running it over
the pinned seed-1 anchors (ADR 0015/0017) and recording the output
sha256. The patcher refuses inputs without the generator namespace (a
no-op patch is never staged) and refuses outputs where any occurrence
survived.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import sys
from pathlib import Path

GENERATOR_NS = "https://kracr.iiitd.edu.in/OWL2Bench#"
TBOX_NS = "http://benchmark/OWL2Bench#"


def patch_bytes(data: bytes, from_ns: str, to_ns: str) -> tuple[bytes, int]:
    count = data.count(from_ns.encode("utf-8"))
    if count == 0:
        raise ValueError("input contains no generator-namespace occurrence; refusing no-op patch")
    patched = data.replace(from_ns.encode("utf-8"), to_ns.encode("utf-8"))
    if patched.count(from_ns.encode("utf-8")):
        raise AssertionError("generator namespace survived the rewrite")
    return patched, count


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("input", type=Path)
    parser.add_argument("output", type=Path)
    parser.add_argument("--from-ns", default=GENERATOR_NS)
    parser.add_argument("--to-ns", default=TBOX_NS)
    arguments = parser.parse_args()

    data = arguments.input.read_bytes()
    try:
        patched, count = patch_bytes(data, arguments.from_ns, arguments.to_ns)
    except ValueError as error:
        print(json.dumps({"status": "refused", "reason": str(error)}))
        return 2

    arguments.output.parent.mkdir(parents=True, exist_ok=True)
    arguments.output.write_bytes(patched)
    digest = hashlib.sha256(patched).hexdigest()
    print(
        json.dumps(
            {
                "status": "patched",
                "input": str(arguments.input.resolve()),
                "output": str(arguments.output.resolve()),
                "replaced_occurrences": count,
                "output_sha256": "sha256:" + digest,
            }
        )
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
