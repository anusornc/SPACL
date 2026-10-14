#!/usr/bin/env python3
"""Merge GraphDB inferred statements with the pinned source graph safely.

GraphDB's statement export may omit explicit source statements and may assign
new blank-node labels to generated witnesses. The materialization contract
requires source inclusion and forbids exporting generated witnesses, so this
boundary keeps every source quad and accepts an inferred quad only when all of
its terms are already in the finite source term domain and its blank nodes
already occur in the source graph. This also avoids exporting GraphDB's
implementation vocabulary outside the benchmark input domain. Named-term
inferred quads within that domain are retained for independent structural
validation.
"""

from __future__ import annotations

import argparse
import sys
from pathlib import Path

from rdf_canonicalization import Quad, parse_nquads, serialize_quad


def merge_graphs(source_path: Path, inferred_text: str) -> bytes:
    source_quads = set(parse_nquads(source_path.read_text(encoding="utf-8")))
    inferred_quads = parse_nquads(inferred_text)
    source_blank_nodes = {
        term.value
        for quad in source_quads
        for term in quad.terms()
        if term.is_blank
    }
    source_terms = {
        term
        for quad in source_quads
        for term in quad.terms()
    }

    def safe_inferred(quad: Quad) -> bool:
        return all(
            (term.is_blank and term.value in source_blank_nodes)
            or (not term.is_blank and term in source_terms)
            for term in quad.terms()
        )

    merged = source_quads | {quad for quad in inferred_quads if safe_inferred(quad)}
    return "".join(sorted(serialize_quad(quad) for quad in merged)).encode("utf-8")


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--source", required=True, type=Path)
    args = parser.parse_args(argv)
    try:
        source = args.source.expanduser().resolve()
        if not source.is_file():
            raise ValueError(f"source graph is missing: {source}")
        inferred = sys.stdin.read()
        if not inferred.strip():
            raise ValueError("inferred GraphDB export is empty")
        sys.stdout.buffer.write(merge_graphs(source, inferred))
        return 0
    except (OSError, UnicodeDecodeError, ValueError) as error:
        print(f"[graphdb-graph-filter][failed] {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
