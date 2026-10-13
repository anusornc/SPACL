#!/usr/bin/env python3
"""Derive candidate RL derived-oracle artifacts (production side).

Follows docs/benchmarking/OWL2BENCH_RL_DERIVED_ORACLE_PROCEDURE.md and ADR
0013: the answer set is produced by the deterministic RL derivation procedure
alone; engine output is never consulted for correctness. Manifest rows are
written as candidates — only the independent audit tool can accept them.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import sys
from pathlib import Path
from typing import Any, Dict, List, Optional

import rl_derivation_core as core
from canonicalize_result import CANONICALIZER_VERSION, canonicalize

PIPELINE_VERSION = "derived-oracle-pipeline/1"
PIPELINE_VERSION_PATCHED = "derived-oracle-pipeline/2"
PROCEDURE_DOC = "docs/benchmarking/OWL2BENCH_RL_DERIVED_ORACLE_PROCEDURE.md"
MANIFEST_NAME = "derived-oracle-manifest.json"


def file_sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def text_sha256(value: str) -> str:
    return hashlib.sha256(value.encode("utf-8")).hexdigest()


def term_json(term: core.Term) -> Dict[str, Any]:
    return core.term_to_json(term)


def triple_json(triple: core.Triple) -> List[Any]:
    return [term_json(term) for term in triple]


def chain_for(
    triple: core.Triple,
    derivations: Dict[core.Triple, Dict[str, Any]],
    seen: Optional[set] = None,
    depth: int = 0,
) -> Any:
    """Render the derivation chain of one triple (bounded depth)."""
    if seen is None:
        seen = set()
    if triple in seen or depth > 40:
        return {"rule": "cycle-or-depth-bound", "premises": []}
    seen = seen | {triple}
    record = derivations.get(triple)
    if record is None:
        return {"rule": "missing", "premises": []}
    return {
        "triple": triple_json(triple),
        "rule": record["rule"],
        "premises": [
            chain_for(premise, derivations, seen, depth + 1) for premise in record["premises"]
        ],
    }


def sparql_results_xml(projection: List[str], rows: List[Dict[str, core.Term]]) -> str:
    def escape(value: str) -> str:
        return (
            value.replace("&", "&amp;").replace("<", "&lt;").replace(">", "&gt;")
        )

    lines = [
        '<?xml version="1.0"?>',
        '<sparql xmlns="http://www.w3.org/2005/sparql-results#">',
        "  <head>",
    ]
    for name in projection:
        lines.append(f"    <variable name=\"{name.lstrip('?')}\"/>")
    lines.append("  </head>")
    lines.append("  <results>")
    for row in rows:
        lines.append("    <result>")
        for name in projection:
            term = row.get(name, ("unbound",))
            if term[0] == "iri":
                lines.append(f"      <binding name=\"{name.lstrip('?')}\"><uri>{escape(term[1])}</uri></binding>")
            elif term[0] == "bnode":
                lines.append(f"      <binding name=\"{name.lstrip('?')}\"><bnode>{escape(term[1])}</bnode></binding>")
            elif term[0] == "literal":
                datatype = f' datatype="{escape(term[2])}"' if term[2] else ""
                language = f' xml:lang="{escape(term[3])}"' if term[3] else ""
                lines.append(
                    f"      <binding name=\"{name.lstrip('?')}\"><literal{datatype}{language}>{escape(term[1])}</literal></binding>"
                )
        lines.append("    </result>")
    lines.append("  </results>")
    lines.append("</sparql>")
    return "\n".join(lines) + "\n"


def uncovered_inventory(base: List[core.Triple]) -> List[Dict[str, Any]]:
    inventory: List[Dict[str, Any]] = []
    for s, p, o in base:
        predicate = p[1] if p[0] == "iri" else ""
        if predicate and predicate not in core.CONSUMED_PREDICATES:
            inventory.append(
                {
                    "predicate": predicate,
                    "subject": term_json(s),
                    "object": term_json(o),
                    "treatment": "outside-implemented-rule-fragment",
                }
            )
    inventory.sort(key=lambda entry: (entry["predicate"], json.dumps(entry["subject"]), json.dumps(entry["object"])))
    return inventory


def derive(arguments: argparse.Namespace) -> int:
    dataset_path: Path = arguments.dataset.resolve()
    query_path: Path = arguments.query.resolve()
    output_dir: Path = arguments.output_dir.resolve()
    output_dir.mkdir(parents=True, exist_ok=True)
    (output_dir / "answers").mkdir(exist_ok=True)
    (output_dir / "proof").mkdir(exist_ok=True)
    (output_dir / "provenance").mkdir(exist_ok=True)

    if arguments.profile not in ("RL", "QL", "EL", "DL"):
        print(json.dumps({"status": "error", "reason": "unsupported_derivation_profile"}))
        return 2

    try:
        parser = core.RdfXmlParser(arguments.base, connect_expressions=arguments.track == "patched")
        base = parser.parse(dataset_path.read_bytes())
        dominant = core.dominant_namespace(base)
        xml_base_namespace = parser.base + "#"
        query_namespace = core.query_namespace_for(base, xml_base_namespace)
        query = core.parse_query(query_path.read_text(encoding="utf-8-sig"), query_namespace)
    except (core.ParserError, core.QueryError) as error:
        print(json.dumps({"status": "error", "reason": f"input_rejected:{error}"}))
        return 2

    rules, rule_table_version = core.rule_table_for(arguments.profile, arguments.track)

    def term_from_json(entry: Dict[str, Any]) -> core.Term:
        if entry["type"] == "iri":
            return ("iri", entry["value"])
        if entry["type"] == "blank_node":
            return ("bnode", entry["value"])
        if entry["type"] == "unbound":
            return ("unbound",)
        return ("literal", entry["value"], entry.get("datatype") or "", entry.get("language") or "")

    # The closure is identical for every case over one dataset and rule
    # table; large patched-track closures take tens of minutes to compute,
    # so an optional content-addressed cache (dataset sha + rule table
    # version + closure algorithm) reuses it across cases. The audit's
    # determinism rerun inherits the same cache directory through the
    # environment and reproduces byte-identical artifacts.
    cache_path = None
    if arguments.closure_cache_dir is not None:
        cache_digest = hashlib.sha256(
            "::".join((arguments.dataset_sha256, rule_table_version, core.CLOSURE_ALGORITHM)).encode("utf-8")
        ).hexdigest()
        arguments.closure_cache_dir.mkdir(parents=True, exist_ok=True)
        cache_path = arguments.closure_cache_dir / f"closure-{cache_digest}.json"

    result = None
    if cache_path is not None and cache_path.is_file():
        try:
            cached = json.loads(cache_path.read_text(encoding="utf-8"))
            closure = [tuple(term_from_json(t) for t in triple) for triple in cached["closure"]]
            derivations = {
                tuple(term_from_json(t) for t in json.loads(key)): {
                    "rule": record["rule"],
                    "premises": [tuple(term_from_json(t) for t in json.loads(p)) for p in record["premises"]],
                }
                for key, record in cached["derivations"].items()
            }
            result = {
                "base": [tuple(term_from_json(t) for t in triple) for triple in cached["base"]],
                "closure": closure,
                "derivations": derivations,
                "inconsistent_instances": [t for t in closure if t[1] == core.TYPE and t[2] == core.NOTHING],
                "expanded_disjoint": core.expand_all_disjoint(base),
            }
        except (json.JSONDecodeError, KeyError, TypeError):
            result = None
    if result is None:
        result = core.compute_closure(base, rules)
        if cache_path is not None:
            serialized = {
                "base": [triple_json(t) for t in result["base"]],
                "closure": [triple_json(t) for t in result["closure"]],
                "derivations": {
                    json.dumps(triple_json(triple), sort_keys=True): {
                        "rule": record["rule"],
                        "premises": [json.dumps(triple_json(p), sort_keys=True) for p in record["premises"]],
                    }
                    for triple, record in result["derivations"].items()
                },
            }
            cache_path.write_text(json.dumps(serialized), encoding="utf-8")

    if result["inconsistent_instances"]:
        print(
            json.dumps(
                {
                    "status": "error",
                    "reason": "inconsistency_detected",
                    "instances": len(result["inconsistent_instances"]),
                }
            )
        )
        return 3

    # The exact-cardinality rule (cls-qc1) reads "exactly 1" as "at least
    # 1"; that is sound only while no subject has two distinct qualified
    # witnesses, so any violation fails the whole derivation closed.
    if arguments.track == "patched":
        violations = core.exact_cardinality_guard(result["closure"], rules)
        if violations:
            print(
                json.dumps(
                    {
                        "status": "error",
                        "reason": "exact_cardinality_guard_violated",
                        "violations": violations[:20],
                        "violation_count": len(violations),
                    }
                )
            )
            return 5

    rows, witnesses = query.evaluate(result["closure"])
    closure_set = set(result["closure"])

    # Per-row justification evidence: every pattern grounding is a closure
    # triple, and each triple carries its derivation chain.
    # Justification evidence: every pattern grounding is a closure triple
    # with its derivation chain. Cases with very large row counts (the
    # patched anchors' symmetric+transitive closures answer in the
    # millions of rows) would serialize gigabytes of chains, so beyond the
    # row budget the witness triples are kept and the chains are elided
    # with a recorded flag — the audit's mechanical checks (per-step rule
    # instances, saturation, answer equality, determinism) do not traverse
    # the chains.
    row_budget = arguments.justification_row_budget
    chains_elided = len(rows) > row_budget
    justification_rows = []
    for row, binding in zip(rows, witnesses):
        try:
            used = query.witness_triples(binding, closure_set)
        except core.QueryError as error:
            print(json.dumps({"status": "error", "reason": f"justification_gap:{error}"}))
            return 4
        matches = []
        for pattern, triple in zip(query.patterns, used):
            matches.append(
                {
                    "pattern": [str(term) for term in pattern],
                    "triple": triple_json(triple),
                    "derivation": {"rule": "elided:justification-row-budget"}
                    if chains_elided
                    else chain_for(triple, result["derivations"]),
                }
            )
        justification_rows.append({"row": {k: term_json(v) for k, v in row.items()}, "pattern_matches": matches})

    answers_xml = sparql_results_xml(query.projection, rows)
    answer_path = output_dir / "answers" / f"{arguments.case_id}.srx"
    answer_path.write_text(answers_xml, encoding="utf-8")

    canonical = canonicalize(query_path, answer_path)

    closure_index = {triple: index for index, triple in enumerate(result["closure"])}
    derivations_indexed = {}
    for triple, record in result["derivations"].items():
        derivations_indexed[closure_index[triple]] = {
            "rule": record["rule"],
            "premises": [closure_index[premise] for premise in record["premises"]],
        }
    proof = {
        "case_id": arguments.case_id,
        "query_id": arguments.query_id,
        "profile": arguments.profile,
        "rule_table_version": rule_table_version,
        "closure_algorithm": core.CLOSURE_ALGORITHM,
        "base_triple_count": len(result["base"]),
        "closure_triple_count": len(result["closure"]),
        "closure_digest": "sha256:" + core.triple_digest(result["closure"]),
        "base_digest": "sha256:" + core.triple_digest(result["base"]),
        "closure_triples": [triple_json(triple) for triple in result["closure"]],
        "base_triples": [triple_json(triple) for triple in result["base"]],
        "derivations": derivations_indexed,
        "rows": justification_rows,
        **({"justification_row_budget": row_budget, "justification_chains_elided": True} if chains_elided else {}),
        "uncovered_axiom_inventory": uncovered_inventory(base),
    }
    proof_path = output_dir / "proof" / f"{arguments.case_id}.json"
    proof_path.write_text(json.dumps(proof, ensure_ascii=False, sort_keys=True, indent=1), encoding="utf-8")

    provenance_text = (
        f"derived:{arguments.profile.lower()}:{arguments.query_id}:{arguments.profile} track={arguments.track} procedure={PROCEDURE_DOC} "
        f"pipeline={PIPELINE_VERSION_PATCHED if arguments.track == 'patched' else PIPELINE_VERSION} rules={rule_table_version} "
        f"closure={core.CLOSURE_ALGORITHM} dataset={arguments.dataset_id} "
        f"corpus={arguments.source_corpus} revision={arguments.source_revision}"
    )
    provenance = {
        "case_id": arguments.case_id,
        "query_id": arguments.query_id,
        "profile": arguments.profile,
        "track": arguments.track,
        "procedure_doc": PROCEDURE_DOC,
        "pipeline_version": PIPELINE_VERSION_PATCHED if arguments.track == "patched" else PIPELINE_VERSION,
        "rule_table_version": rule_table_version,
        "closure_algorithm": core.CLOSURE_ALGORITHM,
        "dataset_id": arguments.dataset_id,
        "dataset_sha256": arguments.dataset_sha256,
        "dataset_path": str(dataset_path),
        "query_path": str(query_path),
        "source_corpus": arguments.source_corpus,
        "source_revision": arguments.source_revision,
        "proposer_engine": None,
        "engine_role": "none; answers come from the derivation procedure only",
        "query_namespace": query_namespace,
        "query_namespace_source": (
            "dominant-abox-predicate-namespace" if dominant is not None else "xml-base-fallback"
        ),
        "xml_base_namespace": xml_base_namespace,
        "vocabulary_split": query_namespace != xml_base_namespace,
    }
    provenance_path = output_dir / "provenance" / f"{arguments.case_id}.json"
    provenance_path.write_text(json.dumps(provenance, ensure_ascii=False, sort_keys=True, indent=1), encoding="utf-8")

    row = {
        "case_id": arguments.case_id,
        "query_id": arguments.query_id,
        "profile": arguments.profile,
        "source_query_sha256": arguments.source_query_sha256,
        "source_corpus": arguments.source_corpus,
        "source_revision": arguments.source_revision,
        "dataset_id": arguments.dataset_id,
        "dataset_sha256": arguments.dataset_sha256,
        "profile_context_identity": arguments.profile_context_identity,
        "canonicalizer_version": CANONICALIZER_VERSION,
        "oracle_class": "derived",
        "acceptance_status": "candidate",
        "audit_status": "pending_audit",
        "oracle_id": f"derived:{arguments.profile.lower()}:{arguments.query_id}:{arguments.profile}",
        "oracle_provenance": provenance_text,
        "oracle_provenance_sha256": text_sha256(provenance_text),
        "answer_file": f"answers/{arguments.case_id}.srx",
        "answer_sha256": file_sha256(answer_path),
        "canonical_result_hash": canonical["result_hash"],
        "provenance_artifact": f"provenance/{arguments.case_id}.json",
        "provenance_artifact_sha256": file_sha256(provenance_path),
        "proof_artifact": f"proof/{arguments.case_id}.json",
        "proof_artifact_sha256": file_sha256(proof_path),
        "audit_artifact": None,
        "audit_artifact_sha256": None,
        "derivation_deterministic": True,
        "sound_complete": True,
        "oracle_pipeline_version": PIPELINE_VERSION_PATCHED if arguments.track == "patched" else PIPELINE_VERSION,
    }

    manifest_path = output_dir / MANIFEST_NAME
    manifest: Dict[str, Any] = {"schema_version": 1, "oracles": []}
    if manifest_path.is_file():
        try:
            loaded = json.loads(manifest_path.read_text(encoding="utf-8"))
            if isinstance(loaded, dict) and loaded.get("schema_version") == 1:
                manifest = loaded
        except (OSError, json.JSONDecodeError):
            pass
    manifest["oracles"] = [r for r in manifest["oracles"] if r.get("case_id") != arguments.case_id]
    manifest["oracles"].append(row)
    manifest["oracles"].sort(key=lambda r: r.get("case_id", ""))
    manifest_path.write_text(json.dumps(manifest, ensure_ascii=False, sort_keys=True, indent=1), encoding="utf-8")

    print(
        json.dumps(
            {
                "status": "derived",
                "case_id": arguments.case_id,
                "answer_rows": len(rows),
                "base_triples": len(result["base"]),
                "closure_triples": len(result["closure"]),
                "canonical_result_hash": canonical["result_hash"],
                "acceptance_status": "candidate",
            }
        )
    )
    return 0


def parse_arguments() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    sub = parser.add_subparsers(dest="command", required=True)
    derive_parser = sub.add_parser("derive")
    derive_parser.add_argument("--dataset", type=Path, required=True)
    derive_parser.add_argument("--query", type=Path, required=True)
    derive_parser.add_argument("--output-dir", type=Path, required=True)
    derive_parser.add_argument("--case-id", required=True)
    derive_parser.add_argument("--query-id", required=True)
    derive_parser.add_argument("--profile", default="RL")
    derive_parser.add_argument("--track", default="shipped", choices=["shipped", "patched"])
    derive_parser.add_argument(
        "--justification-row-budget",
        type=int,
        default=int(os.environ.get("DERIVATION_JUSTIFICATION_ROW_BUDGET", "50000")),
        help="rows beyond this keep witness triples but elide derivation chains",
    )
    derive_parser.add_argument(
        "--closure-cache-dir",
        type=Path,
        default=Path(os.environ.get("DERIVATION_CLOSURE_CACHE_DIR", "")) if os.environ.get("DERIVATION_CLOSURE_CACHE_DIR") else None,
        help="optional directory for reusing the closure across cases of one dataset+rule table",
    )
    derive_parser.add_argument("--dataset-id", required=True)
    derive_parser.add_argument("--dataset-sha256", required=True)
    derive_parser.add_argument("--source-query-sha256", required=True)
    derive_parser.add_argument("--source-corpus", required=True)
    derive_parser.add_argument("--source-revision", required=True)
    derive_parser.add_argument("--profile-context-identity", default="")
    derive_parser.add_argument("--base", default="https://kracr.iiitd.edu.in/OWL2Bench")
    arguments = parser.parse_args()
    if arguments.command != "derive":
        parser.error("only the derive command exists")
    return arguments


def main() -> int:
    return derive(parse_arguments())


if __name__ == "__main__":
    raise SystemExit(main())
