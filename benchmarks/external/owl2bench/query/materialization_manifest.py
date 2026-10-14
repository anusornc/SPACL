#!/usr/bin/env python3
"""Validate profile-specific OWL2Bench materialization artifacts.

This contract describes the inferred RDF graph handed to the query layer.  It
does not create answer keys and never treats a partial or unverifiable graph as
complete.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import sys
from pathlib import Path
from typing import Any

from split_stack_manifest import (
    EXPECTED_PROFILE_COUNTS,
    EXPECTED_QUERY_COUNT,
    PROFILES,
    forbidden_field_paths,
    identity_digest,
    valid_digest,
    validate_pinned_runtime_image,
    validate_manifest as validate_component_manifest,
)


MATERIALIZATION_STATUSES = {
    "complete",
    "partial",
    "failed",
    "not_available",
    "unsupported",
    "timeout",
}
IDENTITY_SECTIONS = {
    "source_ontology": ("id", "path", "sha256"),
    "component": ("name", "version", "revision", "artifact_digest"),
    "configuration": ("id", "path", "sha256"),
    "output_graph": ("id", "path", "sha256"),
    "derivation": ("version", "revision"),
    "resource_limits": ("timeout_seconds", "memory", "threads", "cpu", "disk", "network", "variant"),
    "profile_context": ("id", "directory"),
}
RULESET_FIELDS = ("id", "version", "revision", "artifact_digest", "conformance_boundary")
TIMING_FIELDS = (
    "export_materialization_time_ms",
    "determinism_time_ms",
    "validation_time_ms",
    "total_time_ms",
)


def nonempty(value: Any) -> bool:
    return isinstance(value, str) and bool(value.strip())


def file_digest(path_value: Any) -> str | None:
    if not isinstance(path_value, str) or not path_value:
        return None
    path = Path(path_value).expanduser()
    if not path.is_file():
        return None
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return "sha256:" + digest.hexdigest()


def digest_value(value: Any) -> str:
    return str(value).strip().removeprefix("sha256:").lower()


def load_json(path: Path) -> tuple[dict[str, Any] | None, list[str]]:
    if not path.is_file():
        return None, [f"manifest_missing:{path}"]
    try:
        value = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, UnicodeDecodeError, json.JSONDecodeError) as exc:
        return None, [f"manifest_malformed:{exc.__class__.__name__}"]
    if not isinstance(value, dict):
        return None, ["manifest_root_not_object"]
    return value, []


def public_section(record: Any, section: str) -> dict[str, Any]:
    value = record.get(section) if isinstance(record, dict) else None
    if not isinstance(value, dict):
        return {}
    public = {
        key: value[key]
        for key in IDENTITY_SECTIONS[section]
        if key in value
    }
    if section == "derivation":
        for optional_field in ("semantics", "input_profile"):
            if nonempty(value.get(optional_field)):
                public[optional_field] = value[optional_field]
    if section == "resource_limits" and "variant" in value:
        public["variant"] = value["variant"]
    return public


def public_ruleset(record: Any) -> dict[str, Any]:
    value = record.get("ruleset") if isinstance(record, dict) else None
    if not isinstance(value, dict):
        return {}
    return {key: value[key] for key in RULESET_FIELDS if key in value}


def public_runtime(record: Any) -> dict[str, Any]:
    value = record.get("runtime") if isinstance(record, dict) else None
    if not isinstance(value, dict):
        return {}
    public: dict[str, Any] = {}
    if "id" in value:
        public["id"] = value["id"]
    image = value.get("image")
    if isinstance(image, dict):
        public["image"] = {
            key: image[key]
            for key in ("reference", "digest")
            if key in image
        }
    dependencies = value.get("dependencies")
    if isinstance(dependencies, list):
        public["dependencies"] = [
            {
                key: dependency[key]
                for key in ("name", "version", "digest")
                if isinstance(dependency, dict) and key in dependency
            }
            for dependency in dependencies
            if isinstance(dependency, dict)
        ]
    return public


def public_timing(record: Any) -> dict[str, Any] | None:
    value = record.get("timing") if isinstance(record, dict) else None
    if value is None:
        return None
    if not isinstance(value, dict):
        return {}
    return {key: value[key] for key in TIMING_FIELDS if key in value}


def public_diagnostics(record: Any) -> dict[str, Any] | None:
    value = record.get("diagnostics") if isinstance(record, dict) else None
    if value is None:
        return None
    if not isinstance(value, dict):
        return {}
    return {key: value[key] for key in ("path", "sha256") if key in value}


def validate_profile_materialization(record: Any, profile: str) -> dict[str, Any]:
    """Return one secret-free, status-bearing materialization record."""

    if not isinstance(record, dict):
        return {
            "profile": profile,
            "declared_status": None,
            "status": "not_available",
            "reasons": ["profile_record_missing"],
            "materialization_identity": None,
        }

    declared_status = record.get("status")
    execution_variant = record.get("execution_variant")
    reasons: list[str] = []
    unavailable_reasons: list[str] = []
    failed_reasons: list[str] = []
    if declared_status not in MATERIALIZATION_STATUSES:
        failed_reasons.append("status_missing_or_invalid")
        declared_status = None
    if record.get("profile") != profile:
        failed_reasons.append("profile_identity_mismatch")

    public_record: dict[str, Any] = {
        "profile": profile,
        "execution_variant": execution_variant if nonempty(execution_variant) else None,
        "declared_status": declared_status,
        "status": declared_status or "failed",
        "reasons": [],
        "ruleset": public_ruleset(record),
        "runtime": public_runtime(record),
    }
    for section in IDENTITY_SECTIONS:
        public_record[section] = public_section(record, section)
    for field in ("input_sha256", "output_sha256", "reason"):
        if field in record and (field != "reason" or nonempty(record[field])):
            public_record[field] = record[field]

    if declared_status == "complete":
        for section, fields in IDENTITY_SECTIONS.items():
            value = record.get(section)
            if not isinstance(value, dict):
                failed_reasons.append(f"{section}_missing")
                continue
            for field in fields:
                if field in {"path", "cpu", "disk", "network"}:
                    continue
                if field in {"timeout_seconds", "threads"}:
                    numeric = value.get(field)
                    if isinstance(numeric, bool) or not isinstance(numeric, int) or numeric <= 0:
                        failed_reasons.append(f"{section}.{field}_invalid")
                elif field in {"sha256", "artifact_digest"}:
                    if not valid_digest(value.get(field)):
                        failed_reasons.append(f"{section}.{field}_invalid")
                elif not nonempty(value.get(field)):
                    failed_reasons.append(f"{section}.{field}_missing")
        resource_limits = record.get("resource_limits")
        if isinstance(resource_limits, dict):
            for field in ("cpu", "disk", "network"):
                if not nonempty(resource_limits.get(field)):
                    failed_reasons.append(f"resource_limits.{field}_missing")

        source = record.get("source_ontology", {})
        output = record.get("output_graph", {})
        source_path = source.get("path") if isinstance(source, dict) else None
        output_path = output.get("path") if isinstance(output, dict) else None
        if not nonempty(source_path) or file_digest(source_path) is None:
            unavailable_reasons.append("source_ontology.path_missing")
        if not nonempty(output_path) or file_digest(output_path) is None:
            unavailable_reasons.append("output_graph.path_missing")

        input_hash = record.get("input_sha256")
        output_hash = record.get("output_sha256")
        source_hash = source.get("sha256") if isinstance(source, dict) else None
        graph_hash = output.get("sha256") if isinstance(output, dict) else None
        if not valid_digest(input_hash):
            failed_reasons.append("input_sha256_invalid")
        elif not valid_digest(source_hash) or digest_value(input_hash) != digest_value(source_hash):
            failed_reasons.append("input_hash_mismatch")
        if not valid_digest(output_hash):
            failed_reasons.append("output_sha256_invalid")
        elif not valid_digest(graph_hash) or digest_value(output_hash) != digest_value(graph_hash):
            failed_reasons.append("output_hash_mismatch")
        if file_digest(source_path) is not None and valid_digest(source_hash):
            if digest_value(file_digest(source_path)) != digest_value(source_hash):
                failed_reasons.append("source_artifact_hash_mismatch")
        if file_digest(output_path) is not None and valid_digest(graph_hash):
            if digest_value(file_digest(output_path)) != digest_value(graph_hash):
                failed_reasons.append("output_artifact_hash_mismatch")
        runtime = record.get("runtime")
        if profile == "QL" and execution_variant == "openllet-ql":
            _public_image, image_unavailable, image_failed = (
                validate_pinned_runtime_image(runtime)
            )
            unavailable_reasons.extend(image_unavailable)
            failed_reasons.extend(image_failed)

    if profile in {"QL", "RL"} and declared_status in {
        "complete",
        "partial",
        "failed",
        "unsupported",
    }:
        ruleset = record.get("ruleset")
        if not isinstance(ruleset, dict):
            failed_reasons.append("ruleset_missing")
        else:
            for field in RULESET_FIELDS:
                if field == "artifact_digest":
                    if not valid_digest(ruleset.get(field)):
                        failed_reasons.append("ruleset.artifact_digest_invalid")
                elif not nonempty(ruleset.get(field)):
                    failed_reasons.append(f"ruleset.{field}_missing")
    elif declared_status in {
        "partial",
        "failed",
        "not_available",
        "unsupported",
        "timeout",
    }:
        if not nonempty(record.get("reason")):
            reasons.append("reason_missing")

    timing = public_timing(record)
    if timing is not None:
        if set(timing) != set(TIMING_FIELDS):
            failed_reasons.append("timing_fields_missing")
        for field in TIMING_FIELDS:
            value = timing.get(field)
            if isinstance(value, bool) or not isinstance(value, (int, float)) or value < 0:
                failed_reasons.append(f"timing.{field}_invalid")

    diagnostics = public_diagnostics(record)
    if diagnostics is not None:
        if not nonempty(diagnostics.get("path")) or not valid_digest(diagnostics.get("sha256")):
            failed_reasons.append("diagnostics_identity_invalid")
        else:
            diagnostics_path = Path(str(diagnostics["path"])).expanduser()
            actual_diagnostics_hash = file_digest(str(diagnostics_path))
            if actual_diagnostics_hash is None:
                unavailable_reasons.append("diagnostics.path_missing")
            elif digest_value(diagnostics["sha256"]) != digest_value(actual_diagnostics_hash):
                failed_reasons.append("diagnostics_hash_mismatch")

    if failed_reasons:
        public_record["status"] = "failed"
        reasons.extend(failed_reasons)
    elif unavailable_reasons:
        public_record["status"] = "not_available"
        reasons.extend(unavailable_reasons)
    else:
        public_record["status"] = declared_status or "failed"
    public_record["reasons"] = reasons
    public_record["materialization_identity"] = identity_digest(public_record)
    if timing is not None:
        public_record["timing"] = timing
    if diagnostics is not None:
        public_record["diagnostics"] = diagnostics
    return public_record


def shared_output_identity_errors(profiles: dict[str, dict[str, Any]]) -> list[str]:
    """Mark profiles that reuse one materialized graph or context."""

    identity_paths = (
        ("output_graph", "id"),
        ("output_graph", "sha256"),
        ("output_graph", "path"),
        ("profile_context", "id"),
        ("profile_context", "directory"),
    )
    errors: list[str] = []
    for section, field in identity_paths:
        owners: dict[str, list[str]] = {}
        for profile, record in profiles.items():
            if record.get("status") not in {"complete", "partial"}:
                continue
            value = record.get(section, {}).get(field)
            if isinstance(value, str) and value:
                owners.setdefault(value, []).append(profile)
        for value, owner_profiles in owners.items():
            if len(owner_profiles) < 2:
                continue
            joined_profiles = ",".join(sorted(owner_profiles))
            error = f"cross_profile_materialization:{section}.{field}:{joined_profiles}"
            errors.append(error)
            for profile in owner_profiles:
                record = profiles[profile]
                record["status"] = "failed"
                if error not in record["reasons"]:
                    record["reasons"].append(error)
                record["materialization_identity"] = identity_digest(record)
    return errors


def validate_manifest(manifest_path: Path) -> dict[str, Any]:
    document, load_errors = load_json(manifest_path)
    document_errors = list(load_errors)
    manifest_missing = any(error.startswith("manifest_missing:") for error in load_errors)
    if document is None:
        document = {}
    document_errors.extend(
        f"answer_key_field_not_allowed:{path}"
        for path in forbidden_field_paths(document)
    )
    if document.get("schema_version") != 1:
        document_errors.append("schema_version_invalid")
    if not nonempty(document.get("manifest_id")):
        document_errors.append("manifest_id_missing")
    profile_document = document.get("profiles")
    if not isinstance(profile_document, dict):
        document_errors.append("profiles_missing")
        profile_document = {}
    for profile in sorted(set(profile_document) - set(PROFILES)):
        document_errors.append(f"unknown_manifest_profile:{profile}")

    profiles = {
        profile: validate_profile_materialization(profile_document.get(profile), profile)
        for profile in PROFILES
    }
    isolation_errors = shared_output_identity_errors(profiles)
    if not document_errors and not isolation_errors:
        status = status_precedence([record["status"] for record in profiles.values()])
    elif manifest_missing and not isolation_errors:
        status = "not_available"
    else:
        status = "failed"

    return {
        "schema_version": 1,
        "validator": "owl2bench-materialization-manifest/1",
        "status": status,
        "fatal": bool(document_errors and not manifest_missing),
        "manifest": str(manifest_path.resolve()),
        "manifest_errors": document_errors + isolation_errors,
        "profiles": profiles,
        "answer_key_policy": "not_permitted",
    }


def artifact_identity(path_value: Path, role: str) -> dict[str, Any]:
    digest = file_digest(str(path_value))
    if digest is None:
        return {"role": role, "status": "not_available", "path": str(path_value.resolve())}
    return {
        "role": role,
        "status": "recorded",
        "path": str(path_value.resolve()),
        "sha256": digest,
        "bytes": path_value.stat().st_size,
    }


def status_precedence(statuses: list[str]) -> str:
    for status in ("failed", "timeout", "not_available", "unsupported", "partial"):
        if status in statuses:
            return status
    return "complete"


def is_missing_input_error(error: str) -> bool:
    return error.startswith(
        (
            "component:manifest_missing:",
            "component_query:manifest_missing:",
            "component_query:query_manifest_missing:",
            "materialization:manifest_missing:",
        )
    )


def validator_input_unavailable(result: dict[str, Any]) -> bool:
    return result.get("status") == "not_available" and any(
        error.startswith("manifest_missing:")
        for error in result.get("manifest_errors", [])
    )


def compare_value(
    errors: list[str],
    left: Any,
    right: Any,
    error: str,
) -> None:
    if left != right:
        errors.append(error)


def cross_contract_errors(
    profile: str, component: dict[str, Any], materialization: dict[str, Any]
) -> list[str]:
    errors: list[str] = []
    compare_value(
        errors,
        component.get("execution_variant"),
        materialization.get("execution_variant"),
        "execution_variant_mismatch",
    )
    component_engine = component.get("components", {}).get("materializer", {})
    materializer = materialization.get("component", {})
    for field in ("name", "version", "revision", "artifact_digest"):
        compare_value(
            errors,
            component_engine.get(field),
            materializer.get(field),
            f"materializer_identity_mismatch:{field}",
        )

    if profile in {"QL", "RL"}:
        component_ruleset = component.get("ruleset", {})
        materialization_ruleset = materialization.get("ruleset", {})
        for field in RULESET_FIELDS:
            compare_value(
                errors,
                component_ruleset.get(field),
                materialization_ruleset.get(field),
                f"ruleset_identity_mismatch:{field}",
            )

    compare_value(
        errors,
        component.get("configuration", {}).get("digest"),
        materialization.get("configuration", {}).get("sha256"),
        "configuration_identity_mismatch",
    )
    component_dataset_hash = component.get("dataset", {}).get("sha256")
    source_hash = materialization.get("source_ontology", {}).get("sha256")
    input_hash = materialization.get("input_sha256")
    if valid_digest(component_dataset_hash) and valid_digest(source_hash):
        if digest_value(component_dataset_hash) != digest_value(source_hash):
            errors.append("dataset_source_identity_mismatch")
    if valid_digest(component_dataset_hash) and valid_digest(input_hash):
        if digest_value(component_dataset_hash) != digest_value(input_hash):
            errors.append("dataset_input_identity_mismatch")
    for field in ("id", "directory"):
        compare_value(
            errors,
            component.get("profile_context", {}).get(field),
            materialization.get("profile_context", {}).get(field),
            f"profile_context_identity_mismatch:{field}",
        )
    for field in ("timeout_seconds", "memory", "threads", "cpu", "disk", "network", "variant"):
        compare_value(
            errors,
            component.get("resource_limits", {}).get(field),
            materialization.get("resource_limits", {}).get(field),
            f"resource_limit_mismatch:{field}",
        )
    if profile == "QL" and component.get("execution_variant") == "openllet-ql":
        for field in ("id",):
            compare_value(
                errors,
                component.get("runtime", {}).get(field),
                materialization.get("runtime", {}).get(field),
                f"runtime_identity_mismatch:{field}",
            )
        for field in ("reference", "digest"):
            compare_value(
                errors,
                component.get("runtime", {}).get("image", {}).get(field),
                materialization.get("runtime", {}).get("image", {}).get(field),
                f"runtime_image_identity_mismatch:{field}",
            )
    return errors


def verify_stack(
    component_manifest_path: Path,
    materialization_manifest_path: Path,
    query_manifest_path: Path,
) -> dict[str, Any]:
    """Verify supplied profile artifacts; never create a graph or oracle."""

    component_result = validate_component_manifest(
        component_manifest_path, query_manifest_path
    )
    materialization_result = validate_manifest(materialization_manifest_path)
    profiles: dict[str, dict[str, Any]] = {}
    errors = [
        f"component:{error}"
        for error in component_result.get("manifest_errors", [])
    ]
    errors.extend(
        f"component_query:{error}"
        for error in component_result.get("query_manifest_errors", [])
    )
    errors.extend(
        f"materialization:{error}"
        for error in materialization_result.get("manifest_errors", [])
    )

    for profile in PROFILES:
        component = component_result.get("profiles", {}).get(profile, {})
        materialization = materialization_result.get("profiles", {}).get(profile, {})
        profile_errors: list[str] = []
        component_status = component.get("status", "not_available")
        materialization_status = materialization.get("status", "not_available")
        if component_status != "available":
            profile_errors.append(f"component_status:{component_status}")
        if materialization_status != "complete":
            profile_errors.append(f"materialization_status:{materialization_status}")
        if component_status == "available" and materialization_status == "complete":
            profile_errors.extend(cross_contract_errors(profile, component, materialization))
        profile_status = status_precedence(
            [
                "failed" if error.endswith("_mismatch") else component_status
                for error in profile_errors
            ]
            + [materialization_status]
        )
        if profile_errors:
            profile_status = "failed" if any(
                "mismatch" in error for error in profile_errors
            ) else profile_status
        profiles[profile] = {
            "profile": profile,
            "status": profile_status,
            "component": component,
            "materialization": materialization,
            "cross_contract_errors": profile_errors,
        }

    profile_statuses = [record["status"] for record in profiles.values()]
    component_unavailable = validator_input_unavailable(component_result)
    materialization_unavailable = validator_input_unavailable(materialization_result)
    query_unavailable = any(
        error.startswith(("manifest_missing:", "query_manifest_missing:"))
        for error in component_result.get("query_manifest_errors", [])
    )
    fatal_errors = [
        error
        for error in errors
        if not is_missing_input_error(error)
        and not (component_unavailable and error.startswith("component:"))
        and not (materialization_unavailable and error.startswith("materialization:"))
        and not (query_unavailable and error.startswith("component_query:"))
    ]
    if fatal_errors:
        overall_status = "failed"
    elif query_unavailable:
        overall_status = "not_available"
    else:
        overall_status = status_precedence(profile_statuses)
    if all(status == "complete" for status in profile_statuses) and not errors:
        overall_status = "complete"
    return {
        "schema_version": 1,
        "validator": "owl2bench-split-stack-materialization/1",
        "status": overall_status,
        "component_manifest": artifact_identity(
            component_manifest_path, "component-manifest"
        ),
        "materialization_manifest": artifact_identity(
            materialization_manifest_path, "materialization-manifest"
        ),
        "query_manifest": artifact_identity(query_manifest_path, "query-manifest"),
        "expected_query_count": EXPECTED_QUERY_COUNT,
        "expected_profile_case_counts": EXPECTED_PROFILE_COUNTS,
        "profiles": profiles,
        "errors": sorted(set(errors)),
        "answer_key_policy": "not_permitted",
    }


def write_output(path: Path | None, result: dict[str, Any]) -> None:
    payload = json.dumps(result, ensure_ascii=False, indent=2, sort_keys=True) + "\n"
    if path is None:
        sys.stdout.write(payload)
    else:
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(payload, encoding="utf-8")


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    subparsers = parser.add_subparsers(dest="command", required=True)
    validate_parser = subparsers.add_parser("validate", help="validate a materialization manifest")
    validate_parser.add_argument("--materialization-manifest", required=True, type=Path)
    validate_parser.add_argument("--output", type=Path)
    stack_parser = subparsers.add_parser(
        "verify-stack",
        help="verify component and materialization manifests as one profile stack",
    )
    stack_parser.add_argument("--component-manifest", required=True, type=Path)
    stack_parser.add_argument("--materialization-manifest", required=True, type=Path)
    stack_parser.add_argument("--query-manifest", required=True, type=Path)
    stack_parser.add_argument("--output", type=Path)
    args = parser.parse_args(argv)
    if args.command == "validate":
        write_output(args.output, validate_manifest(args.materialization_manifest))
        return 0
    if args.command == "verify-stack":
        result = verify_stack(
            args.component_manifest,
            args.materialization_manifest,
            args.query_manifest,
        )
        write_output(args.output, result)
        return 0 if result["status"] == "complete" else 1
    return 2


if __name__ == "__main__":
    raise SystemExit(main())
