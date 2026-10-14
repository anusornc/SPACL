#!/usr/bin/env python3
"""Validate the profile-qualified OWL2Bench split-stack manifest.

The manifest is an execution contract, not an answer-key store.  It pins one
component set, dataset, generator identity, and profile context for each OWL 2
profile.  Query-manifest rows are expanded into independent query/profile
cases so that mixed source rows cannot hide cross-profile execution.
"""

from __future__ import annotations

import argparse
import csv
import hashlib
import json
import re
import sys
from pathlib import Path
from typing import Any, Iterable


PROFILES = ("DL", "EL", "QL", "RL")
EXPECTED_PROFILE_COUNTS = {"DL": 22, "EL": 10, "QL": 10, "RL": 18}
EXPECTED_QUERY_COUNT = sum(EXPECTED_PROFILE_COUNTS.values())
QUERY_MANIFEST_COLUMNS = (
    "staged_query",
    "source_query",
    "query_id",
    "profile",
    "workload",
    "source_corpus",
    "source_revision",
)
COMPONENT_NAMES = ("engine", "materializer", "query")
SHA256_PATTERN = re.compile(r"^(?:sha256:)?[0-9a-fA-F]{64}$")
QL_NAMED_VARIANTS = {"graphdb-ql", "openllet-ql", "ql-ruleset"}

# Answer/oracle artifacts are intentionally outside the split-stack
# execution contract.  Keep this policy here because both the component and
# materialization validators import the recursive field scanner from this
# module.
FORBIDDEN_KEYS = {
    "answer_key",
    "answer_keys",
    "answer_file",
    "answer_sha256",
    "oracle",
    "oracles",
    "oracle_key",
}


def normalize_profile(value: Any) -> str:
    return re.sub(r"\s+", "", str(value or "")).upper()


def nonempty(value: Any) -> bool:
    return isinstance(value, str) and bool(value.strip())


def valid_digest(value: Any) -> bool:
    return isinstance(value, str) and SHA256_PATTERN.fullmatch(value.strip()) is not None


def validate_pinned_runtime_image(
    runtime: Any,
) -> tuple[dict[str, Any], list[str], list[str]]:
    """Return public image identity plus unavailable and failed reasons."""

    if not isinstance(runtime, dict):
        return {}, ["runtime_missing"], []
    image = runtime.get("image")
    if not isinstance(image, dict):
        return {}, ["runtime.image_missing"], []
    public_image = {
        key: image.get(key)
        for key in ("reference", "digest")
        if key in image
    }
    unavailable_reasons: list[str] = []
    failed_reasons: list[str] = []
    image_reference = image.get("reference")
    image_digest = image.get("digest")
    if not nonempty(image_reference):
        unavailable_reasons.append("runtime.image.reference_missing")
    elif "@sha256:" not in image_reference:
        failed_reasons.append("runtime.image.reference_mutable")
    if not valid_digest(image_digest):
        failed_reasons.append("runtime.image.digest_invalid")
    elif nonempty(image_reference) and image_digest not in image_reference:
        failed_reasons.append("runtime.image.digest_mismatch")
    return public_image, unavailable_reasons, failed_reasons


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
    return digest.hexdigest()


def identity_digest(value: Any) -> str:
    payload = json.dumps(
        value, ensure_ascii=False, sort_keys=True, separators=(",", ":")
    ).encode("utf-8")
    return "sha256:" + hashlib.sha256(payload).hexdigest()


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


def validate_profile_component_record(record: Any, profile: str) -> dict[str, Any]:
    """Validate one profile and return a secret-free public record."""

    reasons: list[str] = []
    unsupported_reasons: list[str] = []
    if not isinstance(record, dict):
        return {
            "profile": profile,
            "status": "not_available",
            "reasons": ["profile_record_missing"],
            "manifest_identity": None,
        }

    execution_variant = record.get("execution_variant")
    failed_reasons: list[str] = []
    if execution_variant is not None and not nonempty(execution_variant):
        reasons.append("execution_variant_invalid")
    if profile == "QL" and nonempty(execution_variant) and execution_variant not in QL_NAMED_VARIANTS:
        unsupported_reasons.append("execution_variant.unsupported")

    components = record.get("components")
    if not isinstance(components, dict):
        reasons.append("components_missing")
        components = {}
    public_components: dict[str, Any] = {}
    for component_name in COMPONENT_NAMES:
        component = components.get(component_name)
        if not isinstance(component, dict):
            reasons.append(f"component.{component_name}.missing")
            public_components[component_name] = {}
            continue
        public_components[component_name] = {
            key: component.get(key)
            for key in ("name", "version", "revision", "artifact_digest")
            if key in component
        }
        for field in ("name", "version", "revision"):
            if not nonempty(component.get(field)):
                reasons.append(f"component.{component_name}.{field}_missing")
        if not valid_digest(component.get("artifact_digest")):
            reasons.append(f"component.{component_name}.artifact_digest_invalid")

    ruleset = record.get("ruleset")
    public_ruleset = {
        key: ruleset.get(key)
        for key in ("id", "version", "revision", "artifact_digest", "conformance_boundary")
        if isinstance(ruleset, dict) and key in ruleset
    }
    if profile in {"QL", "RL"}:
        if not isinstance(ruleset, dict):
            reasons.append("ruleset_missing")
        else:
            for field in ("id", "version", "revision", "conformance_boundary"):
                if not nonempty(ruleset.get(field)):
                    reasons.append(f"ruleset.{field}_missing")
            if not valid_digest(ruleset.get("artifact_digest")):
                reasons.append("ruleset.artifact_digest_invalid")

    configuration = record.get("configuration")
    public_configuration = {
        key: configuration.get(key)
        for key in ("id", "path", "digest")
        if isinstance(configuration, dict) and key in configuration
    }
    if not isinstance(configuration, dict):
        reasons.append("configuration_missing")
    else:
        if not nonempty(configuration.get("id")):
            reasons.append("configuration.id_missing")
        if not valid_digest(configuration.get("digest")):
            reasons.append("configuration.digest_invalid")

    runtime = record.get("runtime")
    public_runtime: dict[str, Any] = {}
    if not isinstance(runtime, dict):
        reasons.append("runtime_missing")
    else:
        public_runtime["id"] = runtime.get("id")
        public_image: dict[str, Any] = {}
        dependencies = runtime.get("dependencies")
        public_dependencies: list[dict[str, Any]] = []
        if not nonempty(runtime.get("id")):
            reasons.append("runtime.id_missing")
        if profile == "QL" and execution_variant == "openllet-ql":
            public_image, image_unavailable, image_failed = (
                validate_pinned_runtime_image(runtime)
            )
            reasons.extend(image_unavailable)
            failed_reasons.extend(image_failed)
        if not isinstance(dependencies, list) or not dependencies:
            reasons.append("runtime.dependencies_missing")
            dependencies = []
        for index, dependency in enumerate(dependencies):
            if not isinstance(dependency, dict):
                reasons.append(f"runtime.dependency.{index}.invalid")
                continue
            public_dependency = {
                key: dependency.get(key)
                for key in ("name", "version", "digest")
                if key in dependency
            }
            public_dependencies.append(public_dependency)
            for field in ("name", "version"):
                if not nonempty(dependency.get(field)):
                    reasons.append(f"runtime.dependency.{index}.{field}_missing")
            if not valid_digest(dependency.get("digest")):
                reasons.append(f"runtime.dependency.{index}.digest_invalid")
        if public_image:
            public_runtime["image"] = public_image
        public_runtime["dependencies"] = public_dependencies

    credentials = record.get("credentials")
    credential_status = credentials.get("status") if isinstance(credentials, dict) else None
    if credential_status not in {"available", "not_required", "not_available"}:
        reasons.append("credentials.status_missing_or_invalid")
    elif credential_status == "not_available":
        reasons.append("credentials.not_available")

    license_record = record.get("license")
    public_license = {
        key: license_record.get(key)
        for key in ("status", "id", "digest")
        if isinstance(license_record, dict) and key in license_record
    }
    license_status = license_record.get("status") if isinstance(license_record, dict) else None
    if license_status not in {"available", "not_required", "not_available"}:
        reasons.append("license.status_missing_or_invalid")
    elif license_status == "not_available":
        reasons.append("license.not_available")
    elif license_status == "available":
        if not nonempty(license_record.get("id")):
            reasons.append("license.id_missing")
        if not valid_digest(license_record.get("digest")):
            reasons.append("license.digest_invalid")

    capability = record.get("capability")
    capability_status = capability.get("status") if isinstance(capability, dict) else None
    supports_query = capability.get("supports_query") if isinstance(capability, dict) else None
    if capability_status not in {"supported", "unsupported", "not_available"}:
        reasons.append("capability.status_missing_or_invalid")
    elif capability_status == "unsupported":
        unsupported_reasons.append("capability.unsupported")
    elif capability_status == "not_available":
        reasons.append("capability.not_available")
    if capability_status == "supported" and supports_query is not True:
        unsupported_reasons.append("capability.query_not_supported")

    resource_limits = record.get("resource_limits")
    public_resource_limits = {
        key: resource_limits.get(key)
        for key in ("timeout_seconds", "memory", "threads", "cpu", "disk", "network", "variant")
        if isinstance(resource_limits, dict) and key in resource_limits
    }
    if not isinstance(resource_limits, dict):
        reasons.append("resource_limits_missing")
    else:
        timeout_seconds = resource_limits.get("timeout_seconds")
        threads = resource_limits.get("threads")
        if isinstance(timeout_seconds, bool) or not isinstance(timeout_seconds, int) or timeout_seconds <= 0:
            reasons.append("resource_limits.timeout_seconds_invalid")
        if isinstance(threads, bool) or not isinstance(threads, int) or threads <= 0:
            reasons.append("resource_limits.threads_invalid")
        if not nonempty(resource_limits.get("memory")):
            reasons.append("resource_limits.memory_missing")
        if not nonempty(resource_limits.get("variant")):
            reasons.append("resource_limits.variant_missing")
        for field in ("cpu", "disk", "network"):
            if not nonempty(resource_limits.get(field)):
                reasons.append(f"resource_limits.{field}_missing")

    dataset = record.get("dataset")
    public_dataset = {
        key: dataset.get(key)
        for key in ("id", "sha256", "tbox_id", "tbox_sha256", "abox_id", "abox_sha256")
        if isinstance(dataset, dict) and key in dataset
    }
    if not isinstance(dataset, dict):
        reasons.append("dataset_missing")
    else:
        if not nonempty(dataset.get("id")):
            reasons.append("dataset.id_missing")
        if not valid_digest(dataset.get("sha256")):
            reasons.append("dataset.sha256_invalid")
        for field in ("tbox_id", "abox_id"):
            if not nonempty(dataset.get(field)):
                reasons.append(f"dataset.{field}_missing")
        for field in ("tbox_sha256", "abox_sha256"):
            if not valid_digest(dataset.get(field)):
                reasons.append(f"dataset.{field}_invalid")

    generator = record.get("generator")
    public_generator = {
        key: generator.get(key)
        for key in ("id", "revision", "seed", "configuration_id", "configuration_sha256")
        if isinstance(generator, dict) and key in generator
    }
    if not isinstance(generator, dict):
        reasons.append("generator_missing")
    else:
        if not nonempty(generator.get("id")):
            reasons.append("generator.id_missing")
        if not nonempty(generator.get("revision")):
            reasons.append("generator.revision_missing")
        seed = generator.get("seed")
        if isinstance(seed, bool) or not isinstance(seed, int) or seed < 0:
            reasons.append("generator.seed_invalid")

    profile_context = record.get("profile_context")
    public_profile_context = {
        key: profile_context.get(key)
        for key in ("id", "directory")
        if isinstance(profile_context, dict) and key in profile_context
    }
    if not isinstance(profile_context, dict):
        reasons.append("profile_context_missing")
    else:
        if not nonempty(profile_context.get("id")):
            reasons.append("profile_context.id_missing")
        if not nonempty(profile_context.get("directory")):
            reasons.append("profile_context.directory_missing")

    if failed_reasons:
        status = "failed"
        reasons.extend(failed_reasons)
    elif reasons:
        status = "not_available"
    elif unsupported_reasons:
        status = "unsupported"
        reasons.extend(unsupported_reasons)
    else:
        status = "available"

    public_record = {
        "profile": profile,
        "status": status,
        "reasons": reasons,
        "execution_variant": execution_variant if nonempty(execution_variant) else None,
        "components": public_components,
        "ruleset": public_ruleset,
        "configuration": public_configuration,
        "runtime": public_runtime,
        "credentials": {"status": credential_status},
        "license": public_license,
        "capability": {
            "status": capability_status,
            "supports_query": supports_query,
        },
        "resource_limits": public_resource_limits,
        "dataset": public_dataset,
        "generator": public_generator,
        "profile_context": public_profile_context,
    }
    public_record["manifest_identity"] = identity_digest(public_record)
    return public_record


def make_case(
    *,
    line_number: int,
    row: Iterable[Any],
    query_id: str,
    profile: str,
    case_id: str,
    manifest_row_status: str,
    profile_metadata_status: str,
    reason: str | None,
    profile_record: dict[str, Any],
) -> dict[str, Any]:
    fields = list(row)
    fields += [""] * (len(QUERY_MANIFEST_COLUMNS) - len(fields))
    staged_query, source_query, _row_query_id, _row_profile, workload, source_corpus, source_revision = fields[:7]
    case: dict[str, Any] = {
        "case_id": case_id,
        "query_id": query_id,
        "profile": profile,
        "manifest_row_number": line_number,
        "manifest_row_status": manifest_row_status,
        "profile_metadata_status": profile_metadata_status,
        "official_query_source": source_query,
        "query_path": staged_query,
        "source_query_sha256": file_digest(staged_query),
        "workload": workload,
        "source_corpus": source_corpus,
        "source_revision": source_revision,
        "component": {
            "profile": profile,
            "status": profile_record.get("status", "not_available"),
            "reasons": list(profile_record.get("reasons", [])),
            "manifest_identity": profile_record.get("manifest_identity"),
        },
        "dataset": profile_record.get("dataset", {}),
        "generator": profile_record.get("generator", {}),
        "profile_context": profile_record.get("profile_context", {}),
    }
    if reason:
        case["manifest_reason"] = reason
    return case


def forbidden_field_paths(value: Any, path: str = "") -> list[str]:
    """Find answer/oracle fields at any nesting level without reading secrets."""

    paths: list[str] = []
    if isinstance(value, dict):
        for key, child in value.items():
            child_path = f"{path}.{key}" if path else str(key)
            if key.lower().replace("-", "_") in FORBIDDEN_KEYS:
                paths.append(child_path)
            paths.extend(forbidden_field_paths(child, child_path))
    elif isinstance(value, list):
        for index, child in enumerate(value):
            paths.extend(forbidden_field_paths(child, f"{path}[{index}]"))
    return paths


def shared_profile_identity_errors(profiles: dict[str, dict[str, Any]]) -> list[str]:
    """Reject reused dataset or context identities across profile lanes."""

    identity_paths = (
        ("dataset", "id"),
        ("dataset", "sha256"),
        ("dataset", "tbox_id"),
        ("dataset", "tbox_sha256"),
        ("dataset", "abox_id"),
        ("dataset", "abox_sha256"),
        ("profile_context", "id"),
        ("profile_context", "directory"),
    )
    errors: list[str] = []
    for section, field in identity_paths:
        owners: dict[str, list[str]] = {}
        for profile, record in profiles.items():
            value = record.get(section, {}).get(field)
            if isinstance(value, str) and value:
                owners.setdefault(value, []).append(profile)
        for value, owner_profiles in owners.items():
            if len(owner_profiles) > 1:
                joined_profiles = ",".join(sorted(owner_profiles))
                errors.append(f"shared_profile_identity:{section}.{field}:{joined_profiles}")
    return errors


def expand_query_manifest(
    query_manifest_path: Path,
    profile_records: dict[str, dict[str, Any]],
) -> dict[str, Any]:
    cases: list[dict[str, Any]] = []
    errors: list[str] = []
    seen_case_ids: set[str] = set()
    seen_query_profiles: set[tuple[str, str]] = set()
    profile_case_counts = {profile: 0 for profile in PROFILES}
    unclassified_case_count = 0

    if not query_manifest_path.is_file():
        return {
            "status": "failed",
            "errors": [f"query_manifest_missing:{query_manifest_path}"],
            "cases": [],
            "case_count": 0,
            "profile_case_counts": profile_case_counts,
            "unclassified_case_count": 0,
        }

    try:
        handle = query_manifest_path.open("r", encoding="utf-8", newline="")
    except OSError as exc:
        return {
            "status": "failed",
            "errors": [f"query_manifest_unreadable:{exc.__class__.__name__}"],
            "cases": [],
            "case_count": 0,
            "profile_case_counts": profile_case_counts,
            "unclassified_case_count": 0,
        }

    with handle:
        reader = csv.reader(handle, delimiter="\t")
        try:
            header = next(reader)
        except StopIteration:
            return {
                "status": "failed",
                "errors": ["query_manifest_header_missing"],
                "cases": [],
                "case_count": 0,
                "profile_case_counts": profile_case_counts,
                "unclassified_case_count": 0,
            }
        if tuple(header) != QUERY_MANIFEST_COLUMNS:
            errors.append("query_manifest_header_invalid")

        for line_number, row in enumerate(reader, start=2):
            row_valid = len(row) == len(QUERY_MANIFEST_COLUMNS) and all(
                nonempty(row[index]) for index in (0, 1, 2, 3, 4, 5, 6)
            )
            query_id = row[2].strip() if len(row) > 2 else ""
            raw_profiles = row[3] if len(row) > 3 else ""
            normalized_profiles = [normalize_profile(value) for value in raw_profiles.split(",") if normalize_profile(value)]
            if not normalized_profiles:
                normalized_profiles = ["unknown"]
            if not row_valid:
                errors.append(f"invalid_query_manifest_row:{line_number}")
            row_profiles_seen: set[str] = set()
            for profile in normalized_profiles:
                profile_record = profile_records.get(
                    profile,
                    {
                        "profile": profile,
                        "status": "not_available",
                        "reasons": ["unknown_profile"],
                        "manifest_identity": None,
                    },
                )
                profile_valid = profile in PROFILES and profile not in row_profiles_seen
                row_profiles_seen.add(profile)
                key = (query_id, profile)
                duplicate = key in seen_query_profiles
                seen_query_profiles.add(key)
                if duplicate:
                    errors.append(f"duplicate_query_profile:{query_id}:{profile}")
                if profile not in PROFILES:
                    errors.append(f"unknown_profile:{profile}")
                if not profile_valid:
                    errors.append(f"duplicate_profile_in_row:{line_number}:{profile}")
                if profile in PROFILES:
                    profile_case_counts[profile] += 1
                else:
                    unclassified_case_count += 1

                base_case_id = f"{query_id or f'invalid_manifest_row_{line_number}'}__{profile}"
                case_id = base_case_id
                if duplicate or case_id in seen_case_ids:
                    duplicate_index = 2
                    while f"{base_case_id}__duplicate{duplicate_index}" in seen_case_ids:
                        duplicate_index += 1
                    case_id = f"{base_case_id}__duplicate{duplicate_index}"
                seen_case_ids.add(case_id)
                case_status = "valid" if row_valid and profile_valid and not duplicate else "invalid"
                cases.append(
                    make_case(
                        line_number=line_number,
                        row=row,
                        query_id=query_id or f"invalid_manifest_row_{line_number}",
                        profile=profile,
                        case_id=case_id,
                        manifest_row_status=case_status,
                        profile_metadata_status="valid" if profile in PROFILES and not duplicate else "invalid",
                        reason=None if case_status == "valid" else "invalid_query_profile_identity",
                        profile_record=profile_record,
                    )
                )

    expected_counts_match = profile_case_counts == EXPECTED_PROFILE_COUNTS
    if len(cases) != EXPECTED_QUERY_COUNT:
        errors.append(f"query_case_count:{len(cases)}:expected:{EXPECTED_QUERY_COUNT}")
    if not expected_counts_match:
        errors.append(f"profile_case_counts:{profile_case_counts}:expected:{EXPECTED_PROFILE_COUNTS}")
    if unclassified_case_count:
        errors.append(f"unclassified_case_count:{unclassified_case_count}")
    if any(case["manifest_row_status"] != "valid" for case in cases):
        errors.append("invalid_case_rows_present")
    return {
        "status": "available" if not errors else "failed",
        "errors": errors,
        "cases": cases,
        "case_count": len(cases),
        "profile_case_counts": profile_case_counts,
        "unclassified_case_count": unclassified_case_count,
    }


def validate_manifest(component_manifest_path: Path, query_manifest_path: Path) -> dict[str, Any]:
    document, load_errors = load_json(component_manifest_path)
    document_errors = list(load_errors)
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

    matrix = document.get("matrix")
    if not isinstance(matrix, dict):
        document_errors.append("matrix_missing")
        matrix = {}
    if matrix.get("expansion") != "query_profile":
        document_errors.append("matrix.expansion_invalid")
    if matrix.get("expected_query_count") != EXPECTED_QUERY_COUNT:
        document_errors.append("matrix.expected_query_count_invalid")
    if matrix.get("expected_profile_case_counts") != EXPECTED_PROFILE_COUNTS:
        document_errors.append("matrix.expected_profile_case_counts_invalid")

    profiles_document = document.get("profiles")
    if not isinstance(profiles_document, dict):
        document_errors.append("profiles_missing")
        profiles_document = {}
    unknown_profiles = sorted(set(profiles_document) - set(PROFILES))
    document_errors.extend(f"unknown_manifest_profile:{profile}" for profile in unknown_profiles)

    profiles: dict[str, dict[str, Any]] = {}
    for profile in PROFILES:
        profiles[profile] = validate_profile_component_record(profiles_document.get(profile), profile)

    document_errors.extend(shared_profile_identity_errors(profiles))

    expanded = expand_query_manifest(query_manifest_path, profiles)
    manifest_missing = any(error.startswith("manifest_missing:") for error in load_errors)
    matrix_status = "available" if expanded["status"] == "available" and not (
        document_errors and not manifest_missing
    ) else "failed"
    if matrix_status == "available" or (manifest_missing and expanded["status"] == "available"):
        profile_statuses = {record["status"] for record in profiles.values()}
        if "not_available" in profile_statuses:
            status = "not_available"
        elif "unsupported" in profile_statuses:
            status = "unsupported"
        elif manifest_missing:
            status = "not_available"
        else:
            status = "available"
    else:
        status = "failed"

    for case in expanded["cases"]:
        profile_record = profiles.get(case["profile"])
        if profile_record is not None:
            case["dataset"] = profile_record.get("dataset", {})
            case["generator"] = profile_record.get("generator", {})
            case["profile_context"] = profile_record.get("profile_context", {})
            case["component"]["status"] = profile_record.get("status", "not_available")
            case["component"]["reasons"] = list(profile_record.get("reasons", []))
            case["component"]["manifest_identity"] = profile_record.get("manifest_identity")

    return {
        "schema_version": 1,
        "validator": "owl2bench-split-stack-manifest/1",
        "status": status,
        "manifest": str(component_manifest_path.resolve()),
        "query_manifest": str(query_manifest_path.resolve()),
        "manifest_errors": document_errors,
        "query_manifest_status": expanded["status"],
        "query_manifest_errors": expanded["errors"],
        "matrix_status": matrix_status,
        "expected_query_count": EXPECTED_QUERY_COUNT,
        "expected_profile_case_counts": EXPECTED_PROFILE_COUNTS,
        "case_count": expanded["case_count"],
        "profile_case_counts": expanded["profile_case_counts"],
        "unclassified_case_count": expanded["unclassified_case_count"],
        "profiles": profiles,
        "cases": expanded["cases"],
        "answer_key_policy": "not_permitted",
    }


def write_output(path: Path | None, result: dict[str, Any]) -> None:
    payload = json.dumps(result, ensure_ascii=False, indent=2, sort_keys=True) + "\n"
    if path is None:
        sys.stdout.write(payload)
        return
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(payload, encoding="utf-8")


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    subparsers = parser.add_subparsers(dest="command", required=True)
    validate_parser = subparsers.add_parser("validate", help="validate and expand a split-stack manifest")
    validate_parser.add_argument("--component-manifest", required=True, type=Path)
    validate_parser.add_argument("--query-manifest", required=True, type=Path)
    validate_parser.add_argument("--output", type=Path)
    args = parser.parse_args(argv)

    if args.command == "validate":
        result = validate_manifest(args.component_manifest, args.query_manifest)
        write_output(args.output, result)
        return 0
    return 2


if __name__ == "__main__":
    raise SystemExit(main())
