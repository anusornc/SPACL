#!/usr/bin/env python3
"""External-key cross-check recorder (ADR 0019, #100).

Compares every accepted (or candidate) case of a patched-track oracle
manifest against the pinned external answer key and records per-case
agree/disagree/not-covered results plus a summary. The key is differential
evidence only: nothing here feeds the derivation, the audit, or oracle
acceptance, and a disagreement is a finding to diagnose (a rule-table gap
on our side or a reasoner defect on theirs), never a gate veto.

Key TSV format (per profile directory): comment lines starting with '#',
one header row of variable names, then one answer row per line of
namespace-stripped local names, tab-separated, sorted.
"""

from __future__ import annotations

import argparse
import json
import re
import xml.etree.ElementTree as ET
from pathlib import Path
from typing import Any, Dict, List, Optional, Tuple

SPARQL_NS = "{http://www.w3.org/2005/sparql-results#}"

TRACKING = "crosscheck/1"


def oracle_rows(srx_path: Path) -> Tuple[List[str], List[Tuple[str, ...]]]:
    """Variables and bound-value tuples of an oracle answer artifact."""
    root = ET.parse(srx_path).getroot()
    variables = [
        variable.attrib["name"] for variable in root.findall(f".//{SPARQL_NS}variable")
    ]
    rows: List[Tuple[str, ...]] = []
    for result in root.findall(f".//{SPARQL_NS}result"):
        bound: List[str] = []
        for binding in result.findall(f"./{SPARQL_NS}binding"):
            uri = binding.find(f"./{SPARQL_NS}uri")
            literal = binding.find(f"./{SPARQL_NS}literal")
            bnode = binding.find(f"./{SPARQL_NS}bnode")
            if uri is not None:
                bound.append(uri.text or "")
            elif bnode is not None:
                bound.append("_:" + (bnode.text or ""))
            elif literal is not None:
                # The key stores lexical forms; compare on those.
                bound.append(literal.text or "")
        rows.append(tuple(bound))
    return variables, rows


def local_name(value: str) -> str:
    return value.rsplit("#", 1)[-1].rsplit("/", 1)[-1]


def key_rows(tsv_path: Path) -> Tuple[List[Tuple[str, ...]], str]:
    """Answer tuples of one key TSV plus its '# source:' verdict line."""
    rows: List[Tuple[str, ...]] = []
    source = ""
    for line in tsv_path.read_text(encoding="utf-8").splitlines():
        line = line.rstrip("\n")
        if line.startswith("#"):
            if line.startswith("# source:"):
                source = line[len("# source:"):].strip()
            continue
        if not line.strip():
            continue
        cells = [cell.strip() for cell in line.split("\t")]
        if all(re.fullmatch(r"\??\w+", cell) for cell in cells) and not rows and cells[0] in ("x", "y", "s", "c", "?x", "?y"):
            # header row of variable names
            continue
        rows.append(tuple(cells))
    return rows, source


def crosscheck(arguments: argparse.Namespace) -> int:
    oracle_dir = arguments.oracle_dir.resolve()
    key_root = arguments.key_dir.resolve()
    manifest = json.loads(
        (arguments.manifest or (oracle_dir / "derived-oracle-manifest.json")).read_text(
            encoding="utf-8"
        )
    )

    results = []
    for row in manifest.get("oracles", []):
        if row.get("acceptance_status") not in ("candidate", "accepted"):
            continue
        profile = row.get("profile", "")
        query_number = re.search(r"query_(\d+)", row.get("query_id", ""))
        if not query_number:
            continue
        query_tag = f"Q{int(query_number.group(1)):02d}"
        key_file = key_root / "answer_key" / f"{profile}-1-s1" / f"{query_tag}.tsv"
        case = {
            "case_id": row["case_id"],
            "profile": profile,
            "query": query_tag,
        }
        if not key_file.is_file():
            case.update({"verdict": "not_covered", "reason": "no key file"})
            results.append(case)
            continue
        _, oracle_answer_rows = oracle_rows(oracle_dir / row["answer_file"])
        key_tuples, key_source = key_rows(key_file)
        oracle_normalized = sorted(
            tuple(local_name(value) for value in answer) for answer in oracle_answer_rows
        )
        # The two sides may project differently (the key's tool returns full
        # witness pairs where the staged query projects one variable). When
        # the shared-width prefixes are unique on both sides, no answer is
        # lost by trimming and the prefixes carry the same comparison.
        width = min(
            len(oracle_normalized[0]) if oracle_normalized else 0,
            max((len(answer) for answer in key_tuples), default=0),
        )

        def prefix_unique(rows: List[Tuple[str, ...]]) -> bool:
            trimmed = [answer[:width] for answer in rows]
            return len(set(trimmed)) == len(trimmed)

        if prefix_unique(oracle_normalized) and prefix_unique(key_tuples):
            oracle_compare = sorted(answer[:width] for answer in oracle_normalized)
            key_compare = sorted(answer[:width] for answer in key_tuples)
        else:
            oracle_compare = oracle_normalized
            key_compare = sorted(key_tuples)
        case.update(
            {
                "key_source": key_source,
                "oracle_rows": len(oracle_compare),
                "key_rows": len(key_compare),
                "verdict": (
                    "agree"
                    if oracle_compare == key_compare
                    else "disagree"
                ),
            }
        )
        results.append(case)

    summary = {
        "tool": TRACKING,
        "oracle_dir": str(oracle_dir),
        "key_dir": str(key_root),
        "cases": len(results),
        "agree": sum(1 for case in results if case["verdict"] == "agree"),
        "disagree": sum(1 for case in results if case["verdict"] == "disagree"),
        "not_covered": sum(1 for case in results if case["verdict"] == "not_covered"),
        "results": results,
    }
    output = arguments.output or (oracle_dir / "crosscheck.json")
    output = Path(output)
    output.parent.mkdir(parents=True, exist_ok=True)
    output.write_text(json.dumps(summary, ensure_ascii=False, indent=1, sort_keys=True) + "\n", encoding="utf-8")
    print(
        json.dumps(
            {
                "status": "crosschecked",
                "cases": summary["cases"],
                "agree": summary["agree"],
                "disagree": summary["disagree"],
                "not_covered": summary["not_covered"],
                "output": str(output),
            }
        )
    )
    return 0


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--oracle-dir", type=Path, required=True)
    parser.add_argument("--key-dir", type=Path, required=True)
    parser.add_argument("--manifest", type=Path)
    parser.add_argument("--output", type=Path)
    return crosscheck(parser.parse_args())


if __name__ == "__main__":
    raise SystemExit(main())
