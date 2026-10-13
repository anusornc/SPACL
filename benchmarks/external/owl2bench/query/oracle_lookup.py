#!/usr/bin/env python3
"""Validate one OWL2Bench oracle manifest entry without creating an oracle."""

from __future__ import annotations

import argparse
import hashlib
import json
import re
import sys
from pathlib import Path
from typing import Any, Dict, List, Optional


CANONICALIZER_HASH_PATTERN = re.compile(r"^sha256:[0-9a-f]{64}$")
HEX_HASH_PATTERN = re.compile(r"^[0-9a-f]{64}$")
VALID_ORACLE_CLASSES = {"official", "derived", "differential"}


def unavailable(reason: str, evidence: Optional[List[Dict[str, Any]]] = None) -> Dict[str, Any]:
    return {
        "status": "unavailable",
        "reason": reason,
        "differential_evidence": evidence or [],
    }


def artifact_hash(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def text_hash(value: str) -> str:
    return hashlib.sha256(value.encode("utf-8")).hexdigest()


def safe_oracle_path(oracle_dir: Path, relative_path: Any) -> Optional[Path]:
    if not isinstance(relative_path, str) or not relative_path or Path(relative_path).is_absolute():
        return None
    candidate = (oracle_dir / relative_path).resolve()
    try:
        candidate.relative_to(oracle_dir.resolve())
    except ValueError:
        return None
    return candidate


def exact_identity_matches(
    row: Dict[str, Any],
    expected: Dict[str, str],
    canonicalizer_version: str,
) -> Optional[str]:
    required_fields = {
        "query_id": expected["query_id"],
        "profile": expected["profile"],
        "source_query_sha256": expected["source_query_sha256"],
        "source_corpus": expected["source_corpus"],
        "source_revision": expected["source_revision"],
        "dataset_id": expected["dataset_id"],
        "dataset_sha256": expected["dataset_sha256"],
        "canonicalizer_version": canonicalizer_version,
    }
    for field, value in required_fields.items():
        if not value or row.get(field) != value:
            return f"identity_mismatch:{field}"
    # Accept the derived-manifest canonical "<query_id>:<profile>" form of the
    # lane's "<query_id>__<profile>" case id; every other field above is exact.
    if row.get("case_id") not in (expected["case_id"], f"{expected['query_id']}:{expected['profile']}"):
        return "identity_mismatch:case_id"
    if expected.get("profile_context_identity") and row.get("profile_context_identity") != expected["profile_context_identity"]:
        return "identity_mismatch:profile_context_identity"
    if row.get("acceptance_status") != "accepted":
        return "oracle_not_accepted"
    if row.get("oracle_class") not in {"official", "derived"}:
        return "oracle_class_not_accepted"
    if not isinstance(row.get("oracle_id"), str) or not row["oracle_id"]:
        return "oracle_id_missing"
    if not isinstance(row.get("oracle_provenance"), str) or not row["oracle_provenance"]:
        return "oracle_provenance_missing"
    if row.get("oracle_provenance_sha256") != text_hash(row["oracle_provenance"]):
        return "oracle_provenance_hash_mismatch"
    if not isinstance(row.get("answer_sha256"), str) or not re.fullmatch(r"[0-9a-f]{64}", row["answer_sha256"]):
        return "answer_hash_missing"
    if not isinstance(row.get("canonical_result_hash"), str) or not CANONICALIZER_HASH_PATTERN.fullmatch(row["canonical_result_hash"]):
        return "canonical_result_hash_missing"
    if row["oracle_class"] == "derived":
        if row.get("audit_status") != "independently_audited":
            return "derived_oracle_not_independently_audited"
        if row.get("derivation_deterministic") is not True:
            return "derived_oracle_not_deterministic"
        if row.get("sound_complete") is not True:
            return "derived_oracle_not_sound_complete"
        if not isinstance(row.get("proof_artifact"), str) or not row["proof_artifact"]:
            return "derived_oracle_proof_missing"
        if not isinstance(row.get("oracle_pipeline_version"), str) or not row["oracle_pipeline_version"]:
            return "derived_oracle_pipeline_missing"
    return None


def lookup(arguments: argparse.Namespace) -> Dict[str, Any]:
    oracle_dir = arguments.oracle_dir.resolve()
    if not arguments.manifest:
        return unavailable("oracle_manifest_unconfigured")
    manifest_path = Path(arguments.manifest)
    if not manifest_path.is_file():
        return unavailable("oracle_manifest_missing")
    try:
        document = json.loads(manifest_path.read_text(encoding="utf-8"))
    except (OSError, UnicodeDecodeError, json.JSONDecodeError):
        return unavailable("oracle_manifest_malformed")
    if not isinstance(document, dict) or document.get("schema_version") != 1 or not isinstance(document.get("oracles"), list):
        return unavailable("oracle_manifest_schema_invalid")

    expected = {
        "case_id": arguments.case_id,
        "query_id": arguments.query_id,
        "profile": arguments.profile,
        "source_query_sha256": arguments.source_query_sha256,
        "source_corpus": arguments.source_corpus,
        "source_revision": arguments.source_revision,
        "dataset_id": arguments.dataset_id,
        "dataset_sha256": arguments.dataset_sha256,
        "profile_context_identity": arguments.profile_context_identity,
    }
    # The query lane composes case ids as "<query_id>__<profile>" while the
    # derived-oracle manifest uses the canonical "<query_id>:<profile>"; both
    # forms select the same row, and every other identity field is still
    # verified before a row can be accepted.
    case_id_forms = {arguments.case_id, f"{arguments.query_id}:{arguments.profile}"}
    rows = [
        row
        for row in document["oracles"]
        if isinstance(row, dict) and row.get("case_id") in case_id_forms
    ]
    evidence: List[Dict[str, Any]] = []
    accepted: Dict[str, List[Dict[str, Any]]] = {"official": [], "derived": []}
    rejection_reasons: List[str] = []
    for row in rows:
        oracle_class = row.get("oracle_class")
        answer_path = safe_oracle_path(oracle_dir, row.get("answer_file"))
        if oracle_class == "differential":
            evidence_entry: Dict[str, Any] = {
                "class": "differential",
                "provenance": row.get("oracle_provenance", ""),
                "answer_file": str(answer_path) if answer_path else None,
            }
            if answer_path and answer_path.is_file():
                evidence_entry["artifact_sha256"] = artifact_hash(answer_path)
            evidence.append(evidence_entry)
            continue
        if oracle_class not in VALID_ORACLE_CLASSES - {"differential"}:
            rejection_reasons.append("oracle_class_not_accepted")
            continue
        reason = exact_identity_matches(row, expected, arguments.canonicalizer_version)
        if reason:
            rejection_reasons.append(reason)
            continue
        if answer_path is None or not answer_path.is_file():
            rejection_reasons.append("answer_artifact_missing_or_unsafe")
            continue
        if artifact_hash(answer_path) != row["answer_sha256"]:
            rejection_reasons.append("answer_artifact_hash_mismatch")
            continue
        provenance_path = safe_oracle_path(oracle_dir, row.get("provenance_artifact"))
        if provenance_path is None or not provenance_path.is_file():
            rejection_reasons.append("oracle_provenance_artifact_missing_or_unsafe")
            continue
        if not HEX_HASH_PATTERN.fullmatch(str(row.get("provenance_artifact_sha256", ""))):
            rejection_reasons.append("oracle_provenance_artifact_hash_missing")
            continue
        if artifact_hash(provenance_path) != row["provenance_artifact_sha256"]:
            rejection_reasons.append("oracle_provenance_artifact_hash_mismatch")
            continue
        proof_path = None
        audit_path = None
        if row["oracle_class"] == "derived":
            proof_path = safe_oracle_path(oracle_dir, row.get("proof_artifact"))
            audit_path = safe_oracle_path(oracle_dir, row.get("audit_artifact"))
            if proof_path is None or not proof_path.is_file():
                rejection_reasons.append("derived_oracle_proof_missing")
                continue
            if audit_path is None or not audit_path.is_file():
                rejection_reasons.append("derived_oracle_audit_missing")
                continue
            if not HEX_HASH_PATTERN.fullmatch(str(row.get("proof_artifact_sha256", ""))):
                rejection_reasons.append("derived_oracle_proof_hash_missing")
                continue
            if not HEX_HASH_PATTERN.fullmatch(str(row.get("audit_artifact_sha256", ""))):
                rejection_reasons.append("derived_oracle_audit_hash_missing")
                continue
            if artifact_hash(proof_path) != row["proof_artifact_sha256"]:
                rejection_reasons.append("derived_oracle_proof_hash_mismatch")
                continue
            if artifact_hash(audit_path) != row["audit_artifact_sha256"]:
                rejection_reasons.append("derived_oracle_audit_hash_mismatch")
                continue
        row = dict(row)
        row["answer_file_absolute"] = str(answer_path)
        row["provenance_artifact_absolute"] = str(provenance_path)
        if proof_path:
            row["proof_artifact_absolute"] = str(proof_path)
        if audit_path:
            row["audit_artifact_absolute"] = str(audit_path)
        accepted[oracle_class].append(row)

    selected: Optional[Dict[str, Any]] = None
    for oracle_class in ("official", "derived"):
        if accepted[oracle_class]:
            if len(accepted[oracle_class]) != 1:
                return unavailable("multiple_accepted_oracles", evidence)
            selected = accepted[oracle_class][0]
            break
    if selected is None:
        if rejection_reasons:
            return unavailable(sorted(set(rejection_reasons))[0], evidence)
        return unavailable("oracle_entry_missing", evidence)

    return {
        "status": "available",
        "reason": "accepted",
        "case_id": selected["case_id"],
        "query_id": selected["query_id"],
        "profile": selected["profile"],
        "source_query_sha256": selected["source_query_sha256"],
        "source_corpus": selected["source_corpus"],
        "source_revision": selected["source_revision"],
        "dataset_id": selected["dataset_id"],
        "dataset_sha256": selected["dataset_sha256"],
        "profile_context_identity": selected.get("profile_context_identity", ""),
        "canonicalizer_version": selected["canonicalizer_version"],
        "oracle_class": selected["oracle_class"],
        "oracle_id": selected["oracle_id"],
        "oracle_file": selected["answer_file_absolute"],
        "oracle_provenance": selected["oracle_provenance"],
        "oracle_provenance_sha256": selected["oracle_provenance_sha256"],
        "provenance_artifact": selected["provenance_artifact_absolute"],
        "provenance_artifact_sha256": selected["provenance_artifact_sha256"],
        "oracle_artifact_sha256": selected["answer_sha256"],
        "canonical_result_hash": selected["canonical_result_hash"],
        "proof_artifact": selected.get("proof_artifact_absolute"),
        "proof_artifact_sha256": selected.get("proof_artifact_sha256"),
        "audit_artifact": selected.get("audit_artifact_absolute"),
        "audit_artifact_sha256": selected.get("audit_artifact_sha256"),
        "audit_status": selected.get("audit_status"),
        "derivation_deterministic": selected.get("derivation_deterministic"),
        "sound_complete": selected.get("sound_complete"),
        "oracle_pipeline_version": selected.get("oracle_pipeline_version"),
        "match_reason": "accepted",
        "differential_evidence": evidence,
    }


def parse_arguments() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--manifest", type=Path)
    parser.add_argument("--oracle-dir", type=Path, required=True)
    parser.add_argument("--case-id", required=True)
    parser.add_argument("--query-id", required=True)
    parser.add_argument("--profile", required=True)
    parser.add_argument("--source-query-sha256", required=True)
    parser.add_argument("--source-corpus", required=True)
    parser.add_argument("--source-revision", required=True)
    parser.add_argument("--dataset-id", required=True)
    parser.add_argument("--dataset-sha256", required=True)
    parser.add_argument("--profile-context-identity", default="")
    parser.add_argument("--canonicalizer-version", required=True)
    return parser.parse_args()


def main() -> int:
    arguments = parse_arguments()
    print(json.dumps(lookup(arguments), ensure_ascii=False, sort_keys=True))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
