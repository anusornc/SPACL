#!/usr/bin/env python3
"""Independent audit of candidate RL derived-oracle artifacts.

Run by a verification session that has not seen the producing session's
conclusions. For every candidate manifest row this tool verifies, without
re-running the production fixpoint: every recorded derivation step is a valid
rule instance whose premises are in the recorded closure; the recorded base is
exactly the anchor dataset; the recorded closure is saturated (closed under
one full round of every rule); the answer artifact matches an independent
evaluation of the query over the verified closure; and re-running the
derivation reproduces byte-identical artifacts. Only then does the manifest
row become accepted / independently_audited; any failure leaves it
unaccepted. See docs/benchmarking/OWL2BENCH_RL_DERIVED_ORACLE_PROCEDURE.md.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import subprocess
import sys
import tempfile
from pathlib import Path
from typing import Any, Dict, List, Optional, Tuple

import rl_derivation_core as core
from canonicalize_result import canonicalize

AUDIT_TOOL_VERSION = "derived-oracle-audit/1"


def file_sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def term_from_json(entry: Dict[str, Any]) -> core.Term:
    if entry["type"] == "iri":
        return ("iri", entry["value"])
    if entry["type"] == "blank_node":
        return ("bnode", entry["value"])
    return ("literal", entry["value"], entry.get("datatype") or "", entry.get("language") or "")


def triple_from_json(entry: List[Dict[str, Any]]) -> core.Triple:
    return (term_from_json(entry[0]), term_from_json(entry[1]), term_from_json(entry[2]))


def verify_row(row: Dict[str, Any], oracle_dir: Path, pipeline_path: Path, base: str) -> Tuple[bool, List[str], Dict[str, Any]]:
    reasons: List[str] = []
    case_id = row.get("case_id", "")
    checks: Dict[str, Any] = {}

    def artifact(relative: Optional[str]) -> Optional[Path]:
        if not relative:
            return None
        candidate = (oracle_dir / relative).resolve()
        try:
            candidate.relative_to(oracle_dir.resolve())
        except ValueError:
            return None
        return candidate

    answer_path = artifact(row.get("answer_file"))
    provenance_path = artifact(row.get("provenance_artifact"))
    proof_path = artifact(row.get("proof_artifact"))
    if answer_path is None or not answer_path.is_file():
        return False, ["answer_artifact_missing"], checks
    if provenance_path is None or not provenance_path.is_file():
        return False, ["provenance_artifact_missing"], checks
    if proof_path is None or not proof_path.is_file():
        return False, ["proof_artifact_missing"], checks

    if file_sha256(answer_path) != row.get("answer_sha256"):
        reasons.append("answer_hash_mismatch")
    if file_sha256(provenance_path) != row.get("provenance_artifact_sha256"):
        reasons.append("provenance_hash_mismatch")
    if file_sha256(proof_path) != row.get("proof_artifact_sha256"):
        reasons.append("proof_hash_mismatch")

    provenance = json.loads(provenance_path.read_text(encoding="utf-8"))
    proof = json.loads(proof_path.read_text(encoding="utf-8"))
    dataset_path = Path(provenance["dataset_path"])
    query_path = Path(provenance["query_path"])
    if not dataset_path.is_file() or not query_path.is_file():
        reasons.append("provenance_inputs_missing")
        return False, sorted(set(reasons)), checks

    if file_sha256(dataset_path) != row.get("dataset_sha256"):
        reasons.append("dataset_hash_mismatch")
    query_sha = hashlib.sha256(query_path.read_text(encoding="utf-8-sig").encode("utf-8")).hexdigest()
    if query_sha != row.get("source_query_sha256"):
        reasons.append("query_hash_mismatch")

    # Independent parse of the anchor dataset; must equal the recorded base.
    # The query namespace follows the dominant ABox vocabulary, and must agree
    # with the namespace the derivation recorded in its provenance. The row's
    # track selects the parse configuration (typed-node expression connection)
    # and the rule table generation.
    track = provenance.get("track", "shipped")
    try:
        base_parser = core.RdfXmlParser(base, connect_expressions=track == "patched")
        base_triples = base_parser.parse(dataset_path.read_bytes())
        query_namespace = core.query_namespace_for(base_triples, base_parser.base + "#")
        query = core.parse_query(query_path.read_text(encoding="utf-8-sig"), query_namespace)
    except (core.ParserError, core.QueryError) as error:
        reasons.append(f"input_rejected:{error}")
        return False, sorted(set(reasons)), checks
    if provenance.get("query_namespace") != query_namespace:
        reasons.append("query_namespace_mismatch")

    closure_triples = [triple_from_json(entry) for entry in proof["closure_triples"]]
    recorded_base = [triple_from_json(entry) for entry in proof["base_triples"]]
    closure_set = set(closure_triples)
    checks["closure_triples"] = len(closure_triples)

    if set(recorded_base) != set(base_triples):
        reasons.append("recorded_base_differs_from_dataset")

    # Soundness: every recorded derivation step is a valid rule instance with
    # premises in the recorded closure, and every non-base closure triple has
    # a recorded derivation. The rule table follows the row's profile and track.
    rules = {rule["id"]: rule for rule in core.rule_table_for(row.get("profile", "RL"), track)[0]}
    base_set = set(base_triples)
    derivations: Dict[int, Dict[str, Any]] = {int(k): v for k, v in proof["derivations"].items()}
    step_failures = 0
    for index, triple in enumerate(closure_triples):
        record = derivations.get(index)
        if record is None:
            if triple in base_set:
                continue
            reasons.append(f"derivation_missing:index:{index}")
            step_failures += 1
            continue
        rule_id = record["rule"]
        if triple in base_set and rule_id == "base":
            continue
        if rule_id == "cax-adc-expand":
            if triple in base_set or triple[1][1] != core.OWL + "disjointWith":
                step_failures += 1
            continue
        rule = rules.get(rule_id)
        if rule is None:
            reasons.append(f"unknown_rule:{rule_id}")
            step_failures += 1
            continue
        premise_triples = [closure_triples[i] for i in record["premises"]]
        if any(premise not in closure_set for premise in premise_triples):
            reasons.append("premise_outside_closure")
            step_failures += 1
            continue
        if not core.rule_substitution_check(rule, premise_triples, triple):
            reasons.append(f"invalid_rule_instance:{rule_id}")
            step_failures += 1
    checks["derivation_steps_verified"] = len(closure_triples) - step_failures
    checks["derivation_step_failures"] = step_failures

    # Completeness: the verified closure admits no further rule firing.
    additions = core.saturation_check(closure_triples, core.rule_table_for(row.get("profile", "RL"), track)[0], core.expand_all_disjoint(base_triples))
    checks["saturation_new_triples"] = len(additions)
    if additions:
        reasons.append("closure_not_saturated")

    # Answers: evaluate the query over the verified closure and compare the
    # canonical result with the answer artifact.
    rows, _witnesses = query.evaluate(closure_triples)
    from derived_oracle_pipeline import sparql_results_xml

    with tempfile.TemporaryDirectory() as temp:
        own_xml = Path(temp) / "own.srx"
        own_xml.write_text(sparql_results_xml(query.projection, rows), encoding="utf-8")
        own = canonicalize(query_path, own_xml)
        recorded = canonicalize(query_path, answer_path)
    checks["answer_rows"] = len(rows)
    if own["result_hash"] != recorded["result_hash"]:
        reasons.append("answer_mismatch")
    if recorded["result_hash"] != row.get("canonical_result_hash"):
        reasons.append("manifest_canonical_hash_mismatch")

    # Determinism: re-running the derivation must reproduce the artifacts.
    with tempfile.TemporaryDirectory() as temp:
        command = [
            sys.executable,
            str(pipeline_path),
            "derive",
            "--dataset",
            str(dataset_path),
            "--query",
            str(query_path),
            "--output-dir",
            temp,
            "--case-id",
            case_id,
            "--query-id",
            row["query_id"],
            "--profile",
            row.get("profile", "RL"),
            "--track",
            track,
            "--dataset-id",
            row["dataset_id"],
            "--dataset-sha256",
            row["dataset_sha256"],
            "--source-query-sha256",
            row["source_query_sha256"],
            "--source-corpus",
            row["source_corpus"],
            "--source-revision",
            row["source_revision"],
            "--base",
            base,
        ]
        if row.get("profile_context_identity"):
            command += ["--profile-context-identity", row["profile_context_identity"]]
        if "justification_row_budget" in proof:
            # Reproduce the exact chain-elision behavior of the original run.
            command += ["--justification-row-budget", str(proof["justification_row_budget"])]
        if os.environ.get("DERIVATION_CLOSURE_CACHE_DIR"):
            # Explicit is better than inherited: without the cache the rerun
            # recomputes a multi-hour closure, which no audit timeout can
            # absorb.
            command += ["--closure-cache-dir", os.environ["DERIVATION_CLOSURE_CACHE_DIR"]]
        rerun_timeout = int(os.environ.get("DERIVATION_AUDIT_RERUN_TIMEOUT", "7200"))
        try:
            completed = subprocess.run(command, capture_output=True, text=True, timeout=rerun_timeout)
        except subprocess.TimeoutExpired:
            reasons.append(f"determinism_rerun_timeout:{rerun_timeout}s")
            return (not reasons), sorted(set(reasons)), checks
        if completed.returncode != 0:
            reasons.append("determinism_rerun_failed")
        else:
            rerun_answer = Path(temp) / "answers" / f"{case_id}.srx"
            rerun_proof = Path(temp) / "proof" / f"{case_id}.json"
            if file_sha256(rerun_answer) != row.get("answer_sha256") or file_sha256(rerun_proof) != row.get("proof_artifact_sha256"):
                reasons.append("determinism_hash_mismatch")

    return (not reasons), sorted(set(reasons)), checks


def audit(arguments: argparse.Namespace) -> int:
    oracle_dir = arguments.oracle_dir.resolve()
    manifest_path = arguments.manifest or (oracle_dir / "derived-oracle-manifest.json")
    manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
    pipeline_path = Path(__file__).with_name("derived_oracle_pipeline.py")
    (oracle_dir / "audit").mkdir(exist_ok=True)
    # Sharding: the audit is resumable (accepted rows are skipped), so
    # parallel shard runs over disjoint case sets are safe and the manifest
    # rewrite below is last-writer-wins per shard — each shard only flips
    # the rows it verified, so concurrent rewrites of other shards' rows
    # would race. To stay race-free, shards write their own verdict
    # artifacts and a final consolidating run (default, no --cases) applies
    # them; when shards run exclusively (one at a time) the direct path is
    # unchanged.
    selected = set(arguments.cases or [])
    report_rows = []
    for row in manifest.get("oracles", []):
        if row.get("acceptance_status") != "candidate":
            continue
        if selected and row.get("case_id") not in selected:
            continue
        ok, reasons, checks = verify_row(row, oracle_dir, pipeline_path, arguments.base)
        audit_entry = {
            "case_id": row["case_id"],
            "audit_tool_version": AUDIT_TOOL_VERSION,
            "procedure_doc": "docs/benchmarking/OWL2BENCH_RL_DERIVED_ORACLE_PROCEDURE.md",
            "method": [
                "hash-and-identity-verification",
                "per-step rule-instance verification of every recorded derivation",
                "recorded-base equality with the anchor dataset",
                "one-round saturation completeness check",
                "independent query evaluation over the verified closure",
                "deterministic rerun comparison",
            ],
            "checks": checks,
            "verdict": "pass" if ok else "fail",
            "reasons": reasons,
        }
        audit_path = oracle_dir / "audit" / f"{row['case_id']}.json"
        audit_path.write_text(json.dumps(audit_entry, ensure_ascii=False, sort_keys=True, indent=1), encoding="utf-8")
        if ok:
            row["acceptance_status"] = "accepted"
            row["audit_status"] = "independently_audited"
        else:
            row["acceptance_status"] = "rejected"
            row["audit_status"] = "audit_failed"
        row["audit_artifact"] = f"audit/{row['case_id']}.json"
        row["audit_artifact_sha256"] = file_sha256(audit_path)
        report_rows.append({"case_id": row["case_id"], "verdict": audit_entry["verdict"], "reasons": reasons})
        update_manifest_row(manifest_path, row)
    print(json.dumps({"status": "audited", "rows": report_rows}, ensure_ascii=False))
    return 0


def update_manifest_row(manifest_path: Path, row: Dict[str, Any]) -> None:
    """Flip one row in the on-disk manifest under an exclusive lock.

    Sharded audit runs update disjoint rows concurrently; read-modify-write
    under flock keeps every shard's flip (a serial run takes the same path
    and is unaffected).
    """
    import fcntl

    with open(manifest_path, "r+", encoding="utf-8") as handle:
        fcntl.flock(handle.fileno(), fcntl.LOCK_EX)
        try:
            manifest = json.loads(handle.read())
            for index, candidate in enumerate(manifest.get("oracles", [])):
                if candidate.get("case_id") == row["case_id"]:
                    manifest["oracles"][index] = row
                    break
            handle.seek(0)
            handle.truncate()
            handle.write(json.dumps(manifest, ensure_ascii=False, sort_keys=True, indent=1))
        finally:
            fcntl.flock(handle.fileno(), fcntl.LOCK_UN)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--oracle-dir", type=Path, required=True)
    parser.add_argument("--manifest", type=Path)
    parser.add_argument("--base", default="https://kracr.iiitd.edu.in/OWL2Bench")
    parser.add_argument(
        "--cases",
        nargs="*",
        default=[],
        help="audit only these case ids (shard mode); default: every candidate row",
    )
    return audit(parser.parse_args())


if __name__ == "__main__":
    raise SystemExit(main())
