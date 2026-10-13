#!/usr/bin/env python3
"""Validate the profile-qualified OWL2Bench result contract.

This validator checks the boundary between execution evidence, canonical query
results, and semantic correctness.  It never creates or repairs an oracle.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import re
import sys
from collections import Counter
from pathlib import Path
from typing import Any

from materialization_manifest import validate_profile_materialization
from split_stack_manifest import validate_profile_component_record


PROFILES = ("DL", "EL", "QL", "RL")
SHA256_RE = re.compile(r"^(?:sha256:)?[0-9a-fA-F]{64}$")
EXECUTION_STATUSES = {"success", "not_available", "unsupported", "failed", "timeout"}
RESULT_STATUSES = {
    "passed",
    "wrong",
    "unsupported",
    "not_available",
    "failed",
    "timeout",
    "oracle_unavailable",
}
CORRECTNESS_STATUSES = {"passed", "wrong", "unsupported", "not_evaluated", "oracle_unavailable"}
ORACLE_STATUSES = {"available", "unavailable", "not_configured", "not_applicable", "malformed"}
ACCEPTED_ORACLE_CLASSES = {"official", "derived"}


def text(value: Any, default: str = "") -> str:
    if value is None:
        return default
    return str(value)


def nonempty(value: Any) -> bool:
    return isinstance(value, str) and bool(value.strip())


def is_hash(value: Any) -> bool:
    return isinstance(value, str) and SHA256_RE.fullmatch(value.strip()) is not None


def normalized_hash(value: Any) -> str:
    return text(value).strip().removeprefix("sha256:").lower()


def file_hash(path: Path) -> str | None:
    if not path.is_file():
        return None
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def text_hash(value: str) -> str:
    return hashlib.sha256(value.encode("utf-8")).hexdigest()


def resolve_artifact(base_dir: Path, value: Any) -> Path | None:
    if not nonempty(value):
        return None
    path = Path(str(value)).expanduser()
    if not path.is_absolute():
        path = base_dir / path
    return path.resolve()


def canonical_payload(document: dict[str, Any]) -> dict[str, Any] | None:
    fields = (
        "canonicalizer_version",
        "projection",
        "select_modifier",
        "distinct",
        "ordered",
        "row_order",
        "rows",
    )
    if any(field not in document for field in fields):
        return None
    payload = {field: document[field] for field in fields}
    if payload["canonicalizer_version"] != "owl2bench-query-canonicalizer/2":
        return None
    if not isinstance(payload["projection"], list) or not all(
        isinstance(value, str) and bool(value) for value in payload["projection"]
    ):
        return None
    if payload["select_modifier"] not in {"none", "distinct", "reduced"}:
        return None
    if not isinstance(payload["distinct"], bool) or not isinstance(payload["ordered"], bool):
        return None
    if payload["row_order"] not in {"query", "typed_tuple"}:
        return None
    rows = payload["rows"]
    if not isinstance(rows, list):
        return None
    for row in rows:
        if not isinstance(row, list) or len(row) != len(payload["projection"]):
            return None
        for term in row:
            if not isinstance(term, dict) or term.get("type") not in {
                "iri",
                "blank_node",
                "literal",
                "unbound",
            }:
                return None
            term_type = term["type"]
            if term_type == "unbound":
                if set(term) != {"type"}:
                    return None
                continue
            if not isinstance(term.get("value"), str):
                return None
            if term_type == "blank_node" and not re.fullmatch(r"b[0-9]+", term["value"]):
                return None
            if term_type == "literal":
                datatype = term.get("datatype")
                language = term.get("language")
                if datatype is not None and not isinstance(datatype, str):
                    return None
                if language is not None and not isinstance(language, str):
                    return None
                if datatype and language:
                    return None
    return payload


def validate_canonical_artifact(
    canonicalization: dict[str, Any], base_dir: Path, path: str
) -> list[str]:
    errors: list[str] = []
    canonical_path = resolve_artifact(base_dir, canonicalization.get("canonical_result_file"))
    if canonical_path is None or file_hash(canonical_path) is None:
        return [f"{path}:canonical_result_artifact_missing"]
    try:
        document = json.loads(canonical_path.read_text(encoding="utf-8"))
    except (OSError, UnicodeDecodeError, json.JSONDecodeError):
        return [f"{path}:canonical_result_artifact_invalid"]
    if not isinstance(document, dict):
        return [f"{path}:canonical_result_artifact_invalid"]
    payload = canonical_payload(document)
    if payload is None:
        errors.append(f"{path}:canonical_payload_invalid")
        return errors
    canonical_hash = document.get("result_hash")
    if not is_hash(canonical_hash) or canonical_hash != canonicalization.get("result_hash"):
        errors.append(f"{path}:canonical_result_artifact_mismatch")
    compact = json.dumps(payload, ensure_ascii=False, sort_keys=True, separators=(",", ":"))
    recomputed = "sha256:" + hashlib.sha256(compact.encode("utf-8")).hexdigest()
    if canonical_hash != recomputed:
        errors.append(f"{path}:canonical_result_hash_mismatch")
    if document.get("row_count") != len(payload["rows"]):
        errors.append(f"{path}:canonical_row_count_mismatch")
    return errors


def nested_object(record: dict[str, Any], field: str) -> dict[str, Any]:
    value = record.get(field)
    return value if isinstance(value, dict) else {}


def forbidden_paths(value: Any, path: str = "") -> list[str]:
    forbidden = {"answer_key", "answer_keys", "oracle_file"}
    found: list[str] = []
    if isinstance(value, dict):
        for key, child in value.items():
            child_path = f"{path}.{key}" if path else str(key)
            if key in forbidden:
                found.append(child_path)
            found.extend(forbidden_paths(child, child_path))
    elif isinstance(value, list):
        for index, child in enumerate(value):
            found.extend(forbidden_paths(child, f"{path}[{index}]"))
    return found


def expected_result_status(record: dict[str, Any]) -> str:
    """Derive the only status allowed by execution and oracle evidence."""

    execution_status = text(record.get("execution_status"), "missing")
    if execution_status != "success":
        return execution_status

    canonicalization = nested_object(record, "canonicalization")
    if text(canonicalization.get("status"), "missing") != "available":
        return "failed"
    semantic_result = text(record.get("semantic_result") or record.get("reasoning_result"))
    canonical_hash = text(canonicalization.get("result_hash"))
    if not is_hash(canonical_hash) or semantic_result != canonical_hash:
        return "failed"

    oracle = nested_object(record, "oracle")
    if text(oracle.get("status"), "unavailable") != "available":
        return "oracle_unavailable"
    expected_hash = text(oracle.get("canonical_result_hash") or oracle.get("expected"))
    if not is_hash(expected_hash):
        return "oracle_unavailable"
    return "passed" if semantic_result == expected_hash else "wrong"


def validate_record(
    record: Any,
    *,
    path: str = "result",
    require_component: bool = False,
    require_materialization: bool = False,
    require_profile_context: bool = False,
    require_engine_provenance: bool = False,
    base_dir: Path | None = None,
) -> list[str]:
    errors: list[str] = []
    if not isinstance(record, dict):
        return [f"{path}:record_not_object"]
    for forbidden in forbidden_paths(record):
        errors.append(f"{path}:forbidden_field:{forbidden}")
    if record.get("schema_version") != 1:
        errors.append(f"{path}:schema_version_invalid")
    if not nonempty(record.get("case_id")):
        errors.append(f"{path}:case_id_missing")
    profile = text(record.get("query_profile"), "missing")
    if profile not in PROFILES:
        errors.append(f"{path}:query_profile_invalid:{profile}")

    execution_status = text(record.get("execution_status"), "missing")
    result_status = text(record.get("result_status"), "missing")
    correctness_status = text(record.get("correctness_status"), "missing")

    if execution_status not in EXECUTION_STATUSES:
        errors.append(f"{path}:execution_status_invalid:{execution_status}")
    if result_status not in RESULT_STATUSES:
        errors.append(f"{path}:result_status_invalid:{result_status}")
    if correctness_status not in CORRECTNESS_STATUSES:
        errors.append(f"{path}:correctness_status_invalid:{correctness_status}")
    if record.get("status") != execution_status:
        errors.append(f"{path}:status_not_execution_status")

    execution_only = record.get("execution_only") is True
    handoff = nested_object(record, "materialization_handoff")
    if execution_only:
        if execution_status == "success" and (
            handoff.get("status") != "ready" or handoff.get("read_only") is not True
        ):
            errors.append(f"{path}:execution_only_handoff_not_ready")
        if "oracle" in record:
            errors.append(f"{path}:execution_only_oracle_present")
        oracle = {}
        oracle_status = "not_applicable"
    else:
        oracle = nested_object(record, "oracle")
        oracle_status = text(oracle.get("status"), "missing")
        if oracle_status not in ORACLE_STATUSES:
            errors.append(f"{path}:oracle_status_invalid:{oracle_status}")
    if oracle_status == "available":
        oracle_class = text(oracle.get("class") or oracle.get("oracle_class"))
        if oracle_class not in ACCEPTED_ORACLE_CLASSES:
            errors.append(f"{path}:oracle_class_not_accepted")
        if text(oracle.get("match_reason")) != "accepted":
            errors.append(f"{path}:oracle_match_not_accepted")
        if not is_hash(oracle.get("canonical_result_hash")):
            errors.append(f"{path}:oracle_canonical_hash_invalid")
        if not nonempty(oracle.get("oracle_id")):
            errors.append(f"{path}:oracle_id_missing")
        if not nonempty(oracle.get("provenance")):
            errors.append(f"{path}:oracle_provenance_missing")
        if not is_hash(oracle.get("provenance_sha256")):
            errors.append(f"{path}:oracle_provenance_hash_invalid")
        if not nonempty(oracle.get("provenance_artifact")):
            errors.append(f"{path}:oracle_provenance_artifact_missing")
        if not is_hash(oracle.get("provenance_artifact_sha256")):
            errors.append(f"{path}:oracle_provenance_artifact_hash_invalid")
        if not is_hash(oracle.get("artifact_sha256") or oracle.get("answer_sha256")):
            errors.append(f"{path}:oracle_artifact_hash_invalid")
        if base_dir is not None:
            answer_path = resolve_artifact(base_dir, oracle.get("source"))
            if answer_path is None or file_hash(answer_path) is None:
                errors.append(f"{path}:oracle_artifact_missing")
            elif normalized_hash(file_hash(answer_path)) != normalized_hash(
                oracle.get("artifact_sha256") or oracle.get("answer_sha256")
            ):
                errors.append(f"{path}:oracle_artifact_hash_mismatch")
            provenance_path = resolve_artifact(base_dir, oracle.get("provenance_artifact"))
            if provenance_path is None or file_hash(provenance_path) is None:
                errors.append(f"{path}:oracle_provenance_artifact_missing")
            elif normalized_hash(file_hash(provenance_path)) != normalized_hash(
                oracle.get("provenance_artifact_sha256")
            ):
                errors.append(f"{path}:oracle_provenance_artifact_hash_mismatch")
            provenance_text = oracle.get("provenance")
            if isinstance(provenance_text, str) and is_hash(oracle.get("provenance_sha256")):
                if text_hash(provenance_text) != normalized_hash(oracle.get("provenance_sha256")):
                    errors.append(f"{path}:oracle_provenance_hash_mismatch")
        dataset = nested_object(record, "dataset_identity")
        oracle_identity = {
            "query_id": record.get("query_id"),
            "profile": profile,
            "source_query_sha256": record.get("source_query_sha256"),
            "source_corpus": record.get("source_corpus"),
            "source_revision": record.get("source_revision"),
            "dataset_id": dataset.get("dataset_id"),
            "dataset_sha256": dataset.get("dataset_sha256"),
            "canonicalizer_version": nested_object(record, "canonicalization").get(
                "canonicalizer_version"
            ),
        }
        for field, expected in oracle_identity.items():
            if not nonempty(expected) or oracle.get(field) != expected:
                errors.append(f"{path}:oracle_identity_mismatch:{field}")
        # Result records carry the lane's "<query_id>__<profile>" case id while
        # derived-oracle rows use the canonical "<query_id>:<profile>" form;
        # both bind the same case (query id and profile are exact above).
        if not nonempty(record.get("case_id")) or oracle.get("case_id") not in (
            record.get("case_id"),
            f"{record.get('query_id')}:{profile}",
        ):
            errors.append(f"{path}:oracle_identity_mismatch:case_id")
        expected_context_identity = nested_object(record, "profile_context").get("identity")
        if expected_context_identity and oracle.get("profile_context_identity") != expected_context_identity:
            errors.append(f"{path}:oracle_identity_mismatch:profile_context_identity")
        if oracle_class == "derived":
            if oracle.get("derivation_deterministic") is not True:
                errors.append(f"{path}:derived_oracle_not_deterministic")
            if oracle.get("sound_complete") is not True:
                errors.append(f"{path}:derived_oracle_not_sound_complete")
            if text(oracle.get("audit_status")) != "independently_audited":
                errors.append(f"{path}:derived_oracle_not_independently_audited")
            if not nonempty(oracle.get("oracle_pipeline_version")):
                errors.append(f"{path}:derived_oracle_pipeline_missing")
            if not nonempty(oracle.get("proof_artifact")):
                errors.append(f"{path}:derived_oracle_proof_missing")
            if not is_hash(oracle.get("proof_artifact_sha256")):
                errors.append(f"{path}:derived_oracle_proof_hash_invalid")
            if not nonempty(oracle.get("audit_artifact")):
                errors.append(f"{path}:derived_oracle_audit_missing")
            if not is_hash(oracle.get("audit_artifact_sha256")):
                errors.append(f"{path}:derived_oracle_audit_hash_invalid")
            if base_dir is not None:
                for field, hash_field, label in (
                    ("proof_artifact", "proof_artifact_sha256", "proof"),
                    ("audit_artifact", "audit_artifact_sha256", "audit"),
                ):
                    artifact_path = resolve_artifact(base_dir, oracle.get(field))
                    artifact_digest = file_hash(artifact_path) if artifact_path else None
                    if artifact_digest is None:
                        errors.append(f"{path}:derived_oracle_{label}_artifact_missing")
                    elif normalized_hash(artifact_digest) != normalized_hash(oracle.get(hash_field)):
                        errors.append(f"{path}:derived_oracle_{label}_hash_mismatch")
    elif result_status in {"passed", "wrong"}:
        errors.append(f"{path}:non_available_oracle_with_comparison_status")

    canonicalization = nested_object(record, "canonicalization")
    canonical_status = text(canonicalization.get("status"), "missing")
    if canonical_status == "available":
        if not nonempty(canonicalization.get("canonicalizer_version")):
            errors.append(f"{path}:canonicalizer_version_missing")
        if not is_hash(canonicalization.get("result_hash")):
            errors.append(f"{path}:canonical_result_hash_invalid")
        if canonicalization.get("canonical_format") != "typed-rdf-results-v1":
            errors.append(f"{path}:canonical_format_invalid")
        if base_dir is not None:
            errors.extend(validate_canonical_artifact(canonicalization, base_dir, path))
    elif execution_status == "success":
        errors.append(f"{path}:successful_execution_without_canonical_result")

    if not isinstance(record.get("output"), str):
        errors.append(f"{path}:raw_output_missing_or_not_string")
    diagnostics_file = record.get("diagnostics_file")
    if diagnostics_file is not None and not isinstance(diagnostics_file, str):
        errors.append(f"{path}:diagnostics_field_invalid")
    for timing_field in ("load_time_ms", "query_time_ms"):
        value = record.get(timing_field)
        if isinstance(value, bool) or not isinstance(value, (int, float)):
            errors.append(f"{path}:{timing_field}_invalid")

    component = nested_object(record, "component_manifest")
    if require_component and execution_status == "success" and not component:
        errors.append(f"{path}:component_manifest_missing")
    if component:
        component_profile = text(component.get("profile"))
        if component_profile and component_profile != profile:
            errors.append(f"{path}:component_profile_mismatch")
        if require_component and execution_status == "success" and text(component.get("status")) != "available":
            errors.append(f"{path}:component_manifest_not_available")
        if require_component and execution_status == "success":
            validated_component = validate_profile_component_record(component, profile)
            if validated_component.get("status") != "available":
                errors.append(f"{path}:component_manifest_identity_incomplete")
            if component.get("manifest_identity") != validated_component.get("manifest_identity"):
                errors.append(f"{path}:component_manifest_identity_mismatch")
    materialization = nested_object(record, "materialization")
    if require_materialization and execution_status == "success" and not materialization:
        errors.append(f"{path}:materialization_missing")
    if materialization:
        materialization_profile = text(materialization.get("profile"))
        if materialization_profile and materialization_profile != profile:
            errors.append(f"{path}:materialization_profile_mismatch")
        if require_materialization and execution_status == "success" and text(materialization.get("status")) != "complete":
            errors.append(f"{path}:materialization_not_complete")
        if require_materialization and execution_status == "success":
            validated_materialization = validate_profile_materialization(materialization, profile)
            if validated_materialization.get("status") != "complete":
                errors.append(f"{path}:materialization_identity_incomplete")
            if materialization.get("materialization_identity") != validated_materialization.get("materialization_identity"):
                errors.append(f"{path}:materialization_identity_mismatch")
    profile_context = nested_object(record, "profile_context")
    if require_profile_context and execution_status == "success" and not profile_context:
        errors.append(f"{path}:profile_context_missing")
    if profile_context and text(profile_context.get("profile")) not in {"", profile}:
        errors.append(f"{path}:profile_context_mismatch")
    if require_engine_provenance and execution_status == "success" and not nested_object(record, "engine_provenance"):
        errors.append(f"{path}:engine_provenance_missing")
    if require_profile_context and execution_status == "success":
        if text(profile_context.get("profile")) != profile:
            errors.append(f"{path}:profile_context_profile_missing_or_mismatched")
        if not nonempty(profile_context.get("identity")):
            errors.append(f"{path}:profile_context_identity_missing")
    if require_engine_provenance and execution_status == "success":
        if not is_hash(record.get("source_query_sha256")):
            errors.append(f"{path}:source_query_hash_missing_or_invalid")
        if not nonempty(record.get("source_query")) or not nonempty(record.get("source_corpus")) or not nonempty(record.get("source_revision")):
            errors.append(f"{path}:source_identity_incomplete")
        dataset = nested_object(record, "dataset_identity")
        if not nonempty(dataset.get("dataset_id")) or not is_hash(dataset.get("dataset_sha256")):
            errors.append(f"{path}:dataset_identity_missing_or_invalid")
    differential = record.get("differential_evidence")
    if differential is not None and not isinstance(differential, list):
        errors.append(f"{path}:differential_evidence_invalid")

    expected_status = expected_result_status(record)
    if result_status != expected_status:
        errors.append(f"{path}:status_precedence:{result_status}:expected:{expected_status}")
    if result_status == "passed" and oracle_status != "available":
        errors.append(f"{path}:passed_without_accepted_oracle")
    if result_status == "wrong" and oracle_status != "available":
        errors.append(f"{path}:wrong_without_accepted_oracle")
    if correctness_status in {"passed", "wrong"}:
        if oracle_status != "available":
            errors.append(f"{path}:correctness_without_accepted_oracle")
        if result_status != correctness_status:
            errors.append(f"{path}:correctness_not_result_status")
    if result_status not in {"passed", "wrong"} and correctness_status in {"passed", "wrong"}:
        errors.append(f"{path}:correctness_overclaims_result")
    if result_status in {"passed", "wrong"} and correctness_status != result_status:
        errors.append(f"{path}:result_status_without_matching_correctness")
    return errors


def validate_run(run_dir: Path) -> dict[str, Any]:
    metadata: dict[str, Any] = {}
    metadata_errors: list[str] = []
    metadata_is_object = False
    metadata_path = run_dir / "run_metadata.json"
    if not metadata_path.is_file():
        metadata_errors.append("run_metadata_missing")
    else:
        try:
            value = json.loads(metadata_path.read_text(encoding="utf-8"))
            if isinstance(value, dict):
                metadata = value
                metadata_is_object = True
            else:
                metadata_errors.append("run_metadata_not_object")
        except (OSError, UnicodeDecodeError, json.JSONDecodeError):
            metadata_errors.append("run_metadata_invalid")
    if metadata_is_object:
        if metadata.get("schema_version") != 1:
            metadata_errors.append("run_metadata_schema_version_invalid")
        if metadata.get("suite") != "owl2bench" or metadata.get("lane") != "query":
            metadata_errors.append("run_metadata_suite_or_lane_invalid")
        result_contract = nested_object(metadata, "result_contract")
        if not result_contract:
            metadata_errors.append("run_metadata_result_contract_missing")
        else:
            if result_contract.get("status") not in {"pending", "valid"}:
                metadata_errors.append("run_metadata_result_contract_not_valid")
            if result_contract.get("answer_key_policy") != "not_permitted":
                metadata_errors.append("run_metadata_answer_key_policy_invalid")
            if not nonempty(result_contract.get("validator")):
                metadata_errors.append("run_metadata_result_contract_validator_missing")
            if not nonempty(result_contract.get("validation_file")):
                metadata_errors.append("run_metadata_result_contract_file_missing")
        for name in ("component_manifest", "materialization"):
            section = nested_object(metadata, name)
            if not section:
                metadata_errors.append(f"run_metadata_{name}_missing")
                continue
            if not isinstance(section.get("requested"), bool):
                metadata_errors.append(f"run_metadata_{name}_requested_invalid")
            if section.get("requested") is True and not nonempty(section.get("validation_file")):
                metadata_errors.append(f"run_metadata_{name}_validation_file_missing")
            if section.get("requested") is True and not isinstance(section.get("profiles"), dict):
                metadata_errors.append(f"run_metadata_{name}_profiles_missing")
    component_metadata = nested_object(metadata, "component_manifest")
    materialization_metadata = nested_object(metadata, "materialization")
    require_component = component_metadata.get("requested") is True
    require_materialization = materialization_metadata.get("requested") is True
    require_profile_context = require_materialization
    require_engine_provenance = require_component and require_materialization
    rows: list[dict[str, Any]] = []
    errors: list[str] = list(metadata_errors)
    excluded_invalid_rows: list[str] = []
    excluded = {"run_metadata.json", "summary.json"}
    for path in sorted(run_dir.glob("*.json")):
        if path.name in excluded:
            continue
        try:
            value = json.loads(path.read_text(encoding="utf-8"))
        except (OSError, UnicodeDecodeError, json.JSONDecodeError) as exc:
            errors.append(f"{path.name}:json_invalid:{exc.__class__.__name__}")
            continue
        for forbidden in forbidden_paths(value):
            errors.append(f"{path.name}:forbidden_field:{forbidden}")
        if not isinstance(value, dict):
            errors.append(f"{path.name}:record_not_object")
            continue
        if value.get("query_contract_scope") == "invalid_manifest_row":
            if value.get("execution_status") != "failed" or value.get("result_status") != "failed":
                errors.append(f"{path.name}:invalid_manifest_row_not_failed")
            else:
                excluded_invalid_rows.append(path.name)
        else:
            errors.extend(
                validate_record(
                    value,
                    path=path.name,
                    base_dir=run_dir,
                    require_component=require_component or bool(nested_object(value, "component_manifest")),
                    require_materialization=require_materialization or bool(nested_object(value, "materialization")),
                    require_profile_context=require_profile_context
                    or bool(nested_object(value, "materialization")),
                    require_engine_provenance=require_engine_provenance
                    or (
                        bool(nested_object(value, "component_manifest"))
                        and bool(nested_object(value, "materialization"))
                    ),
                )
            )
            if isinstance(value, dict):
                rows.append(value)
    errors = sorted(set(errors))
    return {
        "schema_version": 1,
        "validator": "owl2bench-profile-result-contract/1",
        "status": "valid" if not errors else "invalid",
        "run_dir": str(run_dir.resolve()),
        "result_count": len(rows),
        "result_status_counts": dict(sorted(Counter(text(row.get("result_status"), "missing") for row in rows).items())),
        "excluded_invalid_manifest_rows": excluded_invalid_rows,
        "errors": errors,
        "answer_key_policy": "not_permitted",
    }


def write_output(path: Path | None, value: dict[str, Any]) -> None:
    payload = json.dumps(value, indent=2, sort_keys=True) + "\n"
    if path is None:
        sys.stdout.write(payload)
    else:
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(payload, encoding="utf-8")


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    subparsers = parser.add_subparsers(dest="command", required=True)
    record_parser = subparsers.add_parser("validate", help="validate one result record")
    record_parser.add_argument("--result", required=True, type=Path)
    record_parser.add_argument("--output", type=Path)
    run_parser = subparsers.add_parser("validate-run", help="validate all result records in a run")
    run_parser.add_argument("--run-dir", required=True, type=Path)
    run_parser.add_argument("--output", type=Path)
    args = parser.parse_args(argv)
    if args.command == "validate":
        try:
            record = json.loads(args.result.read_text(encoding="utf-8"))
        except (OSError, UnicodeDecodeError, json.JSONDecodeError) as exc:
            result = {"schema_version": 1, "validator": "owl2bench-profile-result-contract/1", "status": "invalid", "errors": [f"json_invalid:{exc.__class__.__name__}"]}
        else:
            errors = validate_record(record, path=args.result.name, base_dir=args.result.parent)
            result = {"schema_version": 1, "validator": "owl2bench-profile-result-contract/1", "status": "valid" if not errors else "invalid", "errors": errors, "answer_key_policy": "not_permitted"}
        write_output(args.output, result)
        return 0 if result["status"] == "valid" else 1
    if args.command == "validate-run":
        result = validate_run(args.run_dir)
        write_output(args.output, result)
        return 0 if result["status"] == "valid" else 1
    return 2


if __name__ == "__main__":
    raise SystemExit(main())
