#!/usr/bin/env python3
"""Build claim-safe OWL2Bench query reports and profile gates.

The query lane has four independent OWL 2 profile contexts.  This reporter
keeps those contexts separate and computes the overall query gate as their
conjunction.  Aggregate counts in the overall report are diagnostic only;
they never replace the per-profile correctness decisions.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import re
from collections import Counter
from datetime import datetime, timezone
from pathlib import Path
from typing import Any, Iterable


PROFILES = ("DL", "EL", "QL", "RL")
EXPECTED_PROFILE_COUNTS = {"DL": 22, "EL": 10, "QL": 10, "RL": 18}
EXPECTED_QUERY_COUNT = sum(EXPECTED_PROFILE_COUNTS.values())
ACCEPTED_ORACLE_CLASSES = {"official", "derived"}
SHA256_RE = re.compile(r"^(?:sha256:)?[0-9a-fA-F]{64}$")


def text(value: Any, default: str = "") -> str:
    if value is None:
        return default
    if isinstance(value, bool):
        return "true" if value else "false"
    return str(value)


def nonempty(value: Any) -> bool:
    return text(value).strip() not in {"", "null", "unavailable"}


def is_sha256(value: Any) -> bool:
    return bool(SHA256_RE.fullmatch(text(value).strip()))


def sorted_counter(counter: Counter[str]) -> dict[str, int]:
    return dict(sorted(counter.items()))


def counter_for(rows: Iterable[dict[str, Any]], field: str, default: str = "unknown") -> dict[str, int]:
    values = Counter(text(row.get(field), default) or default for row in rows)
    return sorted_counter(values)


def nested_status_counter(
    rows: Iterable[dict[str, Any]],
    field: str,
    default: str,
) -> dict[str, int]:
    return counter_for(
        (nested_object(row, field) for row in rows),
        "status",
        default,
    )


def engine_provenance_status_counter(
    rows: Iterable[dict[str, Any]],
) -> dict[str, int]:
    return sorted_counter(
        Counter(
            "available" if nested_object(row, "engine_provenance") else "not_available"
            for row in rows
        )
    )


def nested_object(row: dict[str, Any], field: str) -> dict[str, Any]:
    value = row.get(field)
    return value if isinstance(value, dict) else {}


def safe_scalar(value: Any) -> Any:
    if value is None or isinstance(value, (str, int, float, bool)):
        return value
    return None


def project_fields(value: Any, fields: tuple[str, ...]) -> dict[str, Any]:
    if not isinstance(value, dict):
        return {}
    return {
        field: safe_scalar(value[field])
        for field in fields
        if field in value and safe_scalar(value[field]) is not None
    }


def project_dependencies(value: Any) -> list[dict[str, Any]]:
    if not isinstance(value, list):
        return []
    return [
        project_fields(dependency, ("name", "version", "digest"))
        for dependency in value
        if isinstance(dependency, dict)
    ]


def project_component_record(value: Any) -> dict[str, Any]:
    """Project the public split-stack record without copying secret fields."""

    component = value if isinstance(value, dict) else {}
    projected = {
        "profile": safe_scalar(component.get("profile")),
        "status": safe_scalar(component.get("status")),
        "execution_variant": safe_scalar(component.get("execution_variant")),
        "manifest_identity": safe_scalar(component.get("manifest_identity")),
        "components": {
            name: project_fields(
                nested_object(component, "components").get(name),
                ("name", "version", "revision", "artifact_digest"),
            )
            for name in ("engine", "materializer", "query")
        },
        "ruleset": project_fields(
            component.get("ruleset"),
            ("id", "version", "revision", "artifact_digest", "conformance_boundary"),
        ),
        "configuration": project_fields(
            component.get("configuration"), ("id", "path", "digest")
        ),
        "runtime": {
            "id": safe_scalar(nested_object(component, "runtime").get("id")),
            "dependencies": project_dependencies(
                nested_object(component, "runtime").get("dependencies")
            ),
        },
        "credentials": project_fields(component.get("credentials"), ("status",)),
        "license": project_fields(component.get("license"), ("status", "id", "digest")),
        "capability": project_fields(component.get("capability"), ("status", "supports_query")),
        "resource_limits": project_fields(
            component.get("resource_limits"),
            ("timeout_seconds", "memory", "threads", "cpu", "disk", "network", "variant"),
        ),
        "dataset": project_fields(
            component.get("dataset"),
            ("id", "sha256", "tbox_id", "tbox_sha256", "abox_id", "abox_sha256"),
        ),
        "generator": project_fields(
            component.get("generator"),
            ("id", "revision", "seed", "configuration_id", "configuration_sha256"),
        ),
        "profile_context": project_fields(component.get("profile_context"), ("id", "directory")),
    }
    return projected


def project_materialization_record(value: Any) -> dict[str, Any]:
    """Project the public materialization record without copying extensions."""

    materialization = value if isinstance(value, dict) else {}
    return {
        "profile": safe_scalar(materialization.get("profile")),
        "status": safe_scalar(materialization.get("status")),
        "execution_variant": safe_scalar(materialization.get("execution_variant")),
        "materialization_identity": safe_scalar(materialization.get("materialization_identity")),
        "source_ontology": project_fields(
            materialization.get("source_ontology"), ("id", "path", "sha256")
        ),
        "component": project_fields(
            materialization.get("component"),
            ("name", "version", "revision", "artifact_digest"),
        ),
        "ruleset": project_fields(
            materialization.get("ruleset"),
            ("id", "version", "revision", "artifact_digest", "conformance_boundary"),
        ),
        "configuration": project_fields(
            materialization.get("configuration"), ("id", "path", "sha256")
        ),
        "output_graph": project_fields(
            materialization.get("output_graph"), ("id", "path", "sha256")
        ),
        "derivation": project_fields(
            materialization.get("derivation"),
            ("version", "revision", "semantics", "input_profile"),
        ),
        "resource_limits": project_fields(
            materialization.get("resource_limits"),
            ("timeout_seconds", "memory", "threads", "cpu", "disk", "network", "variant"),
        ),
        "input_sha256": safe_scalar(materialization.get("input_sha256")),
        "output_sha256": safe_scalar(materialization.get("output_sha256")),
        "timing": project_fields(
            materialization.get("timing"),
            (
                "export_materialization_time_ms",
                "determinism_time_ms",
                "validation_time_ms",
                "total_time_ms",
            ),
        ),
        "diagnostics": project_fields(materialization.get("diagnostics"), ("path", "sha256")),
        "profile_context": project_fields(materialization.get("profile_context"), ("id", "directory")),
    }


def unique_strings(values: Iterable[Any]) -> list[str]:
    return sorted({text(value).strip() for value in values if nonempty(value)})


def unique_objects(values: Iterable[dict[str, Any]]) -> list[dict[str, Any]]:
    encoded = {
        json.dumps(value, sort_keys=True, separators=(",", ":"))
        for value in values
        if value
    }
    return [json.loads(value) for value in sorted(encoded)]


def source_query_hash(row: dict[str, Any]) -> str:
    # The hash must be persisted by the runner.  Never hash a path supplied by
    # a result record while rendering a report: that would make report input
    # control filesystem reads and would hide an incomplete run contract.
    return text(row.get("source_query_sha256")).strip()


def dataset_identity(row: dict[str, Any]) -> dict[str, str]:
    dataset = nested_object(row, "dataset_identity")
    return {
        "dataset_id": text(dataset.get("dataset_id") or row.get("dataset_id")),
        "dataset_sha256": text(dataset.get("dataset_sha256") or row.get("dataset_sha256")),
        "source": text(dataset.get("source") or row.get("dataset_identity_source")),
    }


def engine_identity(row: dict[str, Any], metadata: dict[str, Any]) -> dict[str, Any]:
    provenance = row.get("engine_provenance")
    if isinstance(provenance, dict) and provenance:
        identity = project_fields(
            provenance,
            (
                "schema_version",
                "adapter",
                "variant",
                "profile",
                "context_dir",
                "context_identity",
                "engine_version",
                "release_commit",
                "release_asset_sha256",
                "expected_executable_sha256",
                "expected_config_sha256",
                "expected_converter_sha256",
                "query_mode",
                "threads",
                "memory_limit",
                "runtime_probe",
                "source_kind",
                "source_input",
                "source_sha256",
                "prepared_input",
                "prepared_sha256",
                "converter_path",
                "converter_sha256",
                "config_path",
                "config_sha256",
                "binary_path",
                "binary_sha256",
                "canonical_result_format",
                "conversion_status",
                "credential_status",
                "license_status",
            ),
        )
        for section, fields in (
            ("dataset_identity", ("dataset_id", "dataset_sha256", "source")),
            ("source_ontology", ("path", "sha256")),
            ("output_graph", ("path", "sha256")),
            ("query_backend", ("path", "sha256", "interface", "backend_result_format")),
            ("resource_limits", ("timeout_seconds", "memory", "threads", "cpu", "disk", "network", "variant")),
            ("prefix_injection", ("rdf", "default")),
            ("ruleset", ("id", "version", "revision", "artifact_digest", "conformance_boundary")),
        ):
            if section in provenance:
                identity[section] = project_fields(provenance[section], fields)
        if isinstance(provenance.get("component_manifest"), dict):
            manifest = project_fields(
                provenance["component_manifest"],
                ("path", "sha256", "validation_file", "validation_sha256"),
            )
            manifest_profile = provenance["component_manifest"].get("profile")
            if isinstance(manifest_profile, dict):
                manifest["profile"] = project_component_record(manifest_profile)
            identity["component_manifest"] = manifest
        if isinstance(provenance.get("materialization_manifest"), dict):
            manifest = project_fields(
                provenance["materialization_manifest"],
                ("path", "sha256", "validation_file", "validation_sha256"),
            )
            manifest_profile = provenance["materialization_manifest"].get("profile")
            if isinstance(manifest_profile, dict):
                manifest["profile"] = project_materialization_record(manifest_profile)
            identity["materialization_manifest"] = manifest
        if isinstance(provenance.get("engine"), dict):
            identity["engine"] = project_fields(
                provenance["engine"],
                (
                    "name",
                    "version",
                    "binary_path",
                    "binary_sha256",
                    "config_path",
                    "config_sha256",
                ),
            )
        if isinstance(provenance.get("hermit"), dict):
            identity["hermit"] = project_fields(
                provenance["hermit"],
                (
                    "name",
                    "version",
                    "binary_path",
                    "binary_sha256",
                    "config_path",
                    "config_sha256",
                ),
            )
        return identity

    identity: dict[str, Any] = {}
    for field in (
        "engine",
        "engine_status",
        "engine_image",
        "engine_image_digest",
        "capabilities_file",
    ):
        value = metadata.get(field)
        if nonempty(value):
            identity[field] = value
    if not identity and nonempty(row.get("engine_detail")):
        identity["engine_detail"] = row["engine_detail"]
    return identity


def canonicalizer_identity(row: dict[str, Any], metadata: dict[str, Any]) -> dict[str, str]:
    canonicalization = nested_object(row, "canonicalization")
    canonicalizer = nested_object(metadata, "canonicalizer")
    return {
        "name": text(canonicalization.get("canonicalizer") or canonicalizer.get("name")),
        "version": text(
            canonicalization.get("canonicalizer_version")
            or canonicalizer.get("version")
            or row.get("canonicalizer_version")
        ),
    }


def split_stack_requested(metadata: dict[str, Any]) -> bool:
    return any(
        nested_object(metadata, section).get("requested") is True
        for section in ("component_manifest", "materialization")
    )


def variant_control_statuses(metadata: dict[str, Any], run_dir: Path) -> dict[str, Any]:
    fields = (
        "implementation_completeness",
        "release_readiness",
        "execution_availability",
        "semantic_correctness",
    )
    unavailable: dict[str, Any] = {field: "not_available" for field in fields}
    unavailable["official_status_counts"] = {}
    unavailable["revision_id"] = "not_available"
    unavailable["category_sha256"] = "not_available"
    control_ref = nested_object(metadata, "variant_control")
    validation_file = text(control_ref.get("validation_file")).strip()
    declared_sha = text(control_ref.get("sha256")).strip()
    if not validation_file or not is_sha256(declared_sha):
        return unavailable | {"status": "not_available", "reason": "variant_control_validation_missing"}
    path = Path(validation_file)
    if not path.is_file():
        return unavailable | {"status": "not_available", "reason": "variant_control_validation_file_missing"}
    actual_sha = "sha256:" + hashlib.sha256(path.read_bytes()).hexdigest()
    if actual_sha != declared_sha:
        return unavailable | {"status": "failed", "reason": "variant_control_validation_hash_mismatch"}
    try:
        document = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, UnicodeDecodeError, json.JSONDecodeError):
        return unavailable | {"status": "failed", "reason": "variant_control_validation_malformed"}
    if not isinstance(document, dict):
        return unavailable | {"status": "failed", "reason": "variant_control_validation_malformed"}
    evidence = nested_object(document, "evidence")
    run_lineage = nested_object(evidence, "run_lineage")
    if (
        text(run_lineage.get("run_dir")) != str(run_dir)
        or text(run_lineage.get("report_path")) != str(run_dir / "report.md")
    ):
        return unavailable | {
            "status": "failed",
            "reason": "variant_control_run_lineage_mismatch",
        }
    status = text(document.get("status"), "not_available")
    projection = nested_object(document, "report_projection")
    projected_statuses = {
        field: text(projection.get(field), "not_available") for field in fields
    }
    allowed_statuses = {"complete", "blocked", "not_available", "failed"}
    if status not in allowed_statuses or any(
        value not in allowed_statuses for value in projected_statuses.values()
    ):
        return unavailable | {"status": "failed", "reason": "variant_control_status_invalid"}
    revision = nested_object(document, "variant_revision")
    revision_id = text(revision.get("revision_id")).strip()
    category_sha256 = text(revision.get("category_sha256")).strip()
    if not revision_id or not is_sha256(category_sha256):
        return unavailable | {"status": "failed", "reason": "variant_control_revision_invalid"}
    official_status_counts = nested_object(evidence, "release_comparison").get(
        "official_status_counts"
    )
    if official_status_counts != {"oracle_unavailable": 10}:
        return unavailable | {
            "status": "failed",
            "reason": "variant_control_official_status_invalid",
        }
    return {
        "status": status,
        "validation_file": str(path),
        "sha256": actual_sha,
        "revision_id": revision_id,
        "category_sha256": category_sha256,
        "official_status_counts": official_status_counts,
        **projected_statuses,
    }


def component_identity(row: dict[str, Any]) -> dict[str, Any]:
    component = nested_object(row, "component_manifest")
    if not component:
        return {}
    return project_component_record(component)


def materialization_identity(row: dict[str, Any]) -> dict[str, Any]:
    materialization = nested_object(row, "materialization")
    if not materialization:
        return {}
    return project_materialization_record(materialization)


def materialization_blockers(row: dict[str, Any], profile: str) -> list[str]:
    materialization = nested_object(row, "materialization")
    blockers: list[str] = []
    if not materialization:
        return ["materialization_missing"]
    if text(materialization.get("profile")) != profile:
        blockers.append("materialization_profile_mismatch")
    if text(materialization.get("status")) != "complete":
        blockers.append(f"materialization_status:{text(materialization.get('status'), 'missing')}")
        return blockers
    if not is_sha256(materialization.get("materialization_identity")):
        blockers.append("materialization_identity_missing")
    source = nested_object(materialization, "source_ontology")
    output = nested_object(materialization, "output_graph")
    derivation = nested_object(materialization, "derivation")
    configuration = nested_object(materialization, "configuration")
    if not nonempty(source.get("id")) or not is_sha256(source.get("sha256")):
        blockers.append("materialization_source_identity_invalid")
    if not nonempty(output.get("id")) or not is_sha256(output.get("sha256")):
        blockers.append("materialization_output_identity_invalid")
    if not nonempty(derivation.get("version")) or not nonempty(derivation.get("revision")):
        blockers.append("materialization_derivation_identity_invalid")
    if not nonempty(configuration.get("id")) or not is_sha256(configuration.get("sha256")):
        blockers.append("materialization_configuration_invalid")
    if not is_sha256(materialization.get("input_sha256")) or not is_sha256(materialization.get("output_sha256")):
        blockers.append("materialization_input_output_hash_invalid")
    profile_context = nested_object(materialization, "profile_context")
    if not nonempty(profile_context.get("id")) or not nonempty(profile_context.get("directory")):
        blockers.append("materialization_profile_context_invalid")
    return blockers


def component_blockers(row: dict[str, Any], profile: str) -> list[str]:
    component = nested_object(row, "component_manifest")
    blockers: list[str] = []
    if not component:
        return ["component_manifest_missing"]
    if text(component.get("profile")) != profile:
        blockers.append("component_profile_mismatch")
    if text(component.get("status")) != "available":
        blockers.append(f"component_status:{text(component.get('status'), 'missing')}")
    if not is_sha256(component.get("manifest_identity")):
        blockers.append("component_manifest_identity_invalid")
    components = component.get("components")
    if not isinstance(components, dict):
        blockers.append("component_definitions_missing")
    else:
        for name in ("engine", "materializer", "query"):
            item = nested_object(components, name)
            if (
                not nonempty(item.get("name"))
                or not nonempty(item.get("version"))
                or not nonempty(item.get("revision"))
                or not is_sha256(item.get("artifact_digest"))
            ):
                blockers.append(f"component_{name}_identity_invalid")
    if profile in {"QL", "RL"}:
        ruleset = nested_object(component, "ruleset")
        if (
            not nonempty(ruleset.get("id"))
            or not nonempty(ruleset.get("version"))
            or not nonempty(ruleset.get("revision"))
            or not is_sha256(ruleset.get("artifact_digest"))
            or not nonempty(ruleset.get("conformance_boundary"))
        ):
            blockers.append("component_ruleset_identity_invalid")
    configuration = nested_object(component, "configuration")
    runtime = nested_object(component, "runtime")
    resources = nested_object(component, "resource_limits")
    if not nonempty(configuration.get("id")) or not is_sha256(configuration.get("digest")):
        blockers.append("component_configuration_invalid")
    dependencies = runtime.get("dependencies")
    if not nonempty(runtime.get("id")) or not isinstance(dependencies, list) or not dependencies:
        blockers.append("component_runtime_invalid")
    else:
        for dependency in dependencies:
            if (
                not isinstance(dependency, dict)
                or not nonempty(dependency.get("name"))
                or not nonempty(dependency.get("version"))
                or not is_sha256(dependency.get("digest"))
            ):
                blockers.append("component_runtime_dependency_invalid")
                break
    if (
        not nonempty(resources.get("memory"))
        or isinstance(resources.get("timeout_seconds"), bool)
        or not isinstance(resources.get("timeout_seconds"), int)
        or resources.get("timeout_seconds") <= 0
        or isinstance(resources.get("threads"), bool)
        or not isinstance(resources.get("threads"), int)
        or resources.get("threads") <= 0
    ):
        blockers.append("component_resource_limits_invalid")
    credentials = nested_object(component, "credentials")
    license_status = nested_object(component, "license").get("status")
    if text(credentials.get("status")) not in {"available", "not_required"}:
        blockers.append("component_credentials_unavailable")
    if text(license_status) not in {"available", "not_required"}:
        blockers.append("component_license_unavailable")
    capability = nested_object(component, "capability")
    if text(capability.get("status")) != "supported" or capability.get("supports_query") is not True:
        blockers.append("component_capability_invalid")
    dataset = nested_object(component, "dataset")
    if (
        not nonempty(dataset.get("id"))
        or not is_sha256(dataset.get("sha256"))
        or not nonempty(dataset.get("tbox_id"))
        or not is_sha256(dataset.get("tbox_sha256"))
        or not nonempty(dataset.get("abox_id"))
        or not is_sha256(dataset.get("abox_sha256"))
    ):
        blockers.append("component_dataset_identity_invalid")
    generator = nested_object(component, "generator")
    if (
        not nonempty(generator.get("id"))
        or not nonempty(generator.get("revision"))
        or isinstance(generator.get("seed"), bool)
        or not isinstance(generator.get("seed"), int)
        or generator.get("seed") < 0
    ):
        blockers.append("component_generator_identity_invalid")
    profile_context = nested_object(component, "profile_context")
    if not nonempty(profile_context.get("id")) or not nonempty(profile_context.get("directory")):
        blockers.append("component_profile_context_invalid")
    return blockers


def oracle_hash(row: dict[str, Any]) -> str:
    oracle = nested_object(row, "oracle")
    return text(oracle.get("artifact_sha256") or oracle.get("answer_sha256"))


def oracle_canonical_hash(row: dict[str, Any]) -> str:
    oracle = nested_object(row, "oracle")
    return text(oracle.get("canonical_result_hash"))


def timing_summary(rows: list[dict[str, Any]]) -> dict[str, Any]:
    def values(field: str) -> list[int]:
        result = []
        for row in rows:
            value = row.get(field)
            if isinstance(value, bool):
                continue
            try:
                number = int(value)
            except (TypeError, ValueError):
                continue
            if number >= 0:
                result.append(number)
        return result

    summary: dict[str, Any] = {"unit": "ms"}
    for field in ("load_time_ms", "query_time_ms"):
        numbers = values(field)
        name = "load" if field == "load_time_ms" else "query"
        recorded_count = sum(
            1
            for row in rows
            if isinstance(row.get(field), (int, float))
            and not isinstance(row.get(field), bool)
        )
        summary[name] = {
            "count": len(numbers),
            "recorded_count": recorded_count,
            "not_applicable_count": recorded_count - len(numbers),
            "total_ms": sum(numbers),
            "min_ms": min(numbers) if numbers else None,
            "max_ms": max(numbers) if numbers else None,
            "average_ms": (sum(numbers) / len(numbers)) if numbers else None,
        }
    return summary


def row_blockers(row: dict[str, Any], metadata: dict[str, Any]) -> list[str]:
    blockers: list[str] = []
    result_status = text(row.get("result_status"), "missing")
    if result_status != "passed":
        blockers.append(f"result_status:{result_status}")
    if text(row.get("execution_status"), "missing") != "success":
        blockers.append(f"execution_status:{text(row.get('execution_status'), 'missing')}")
    if text(row.get("correctness_status"), "missing") != "passed":
        blockers.append(f"correctness_status:{text(row.get('correctness_status'), 'missing')}")
    for field in ("load_time_ms", "query_time_ms"):
        value = row.get(field)
        valid_number = (
            isinstance(value, (int, float))
            and not isinstance(value, bool)
            and value >= 0
        )
        explicitly_not_applicable = (
            field == "load_time_ms"
            and value == -1
            and text(row.get("load_status")) == "not_applicable"
        )
        if not valid_number and not explicitly_not_applicable:
            blockers.append(f"{field}_missing_or_invalid")

    source_hash = source_query_hash(row)
    if not is_sha256(source_hash):
        blockers.append("source_query_hash_missing_or_invalid")
    if not nonempty(row.get("source_query")) or not nonempty(row.get("source_corpus")) or not nonempty(row.get("source_revision")):
        blockers.append("source_identity_incomplete")

    dataset = dataset_identity(row)
    if not nonempty(dataset["dataset_id"]) or not is_sha256(dataset["dataset_sha256"]):
        blockers.append("dataset_identity_missing_or_invalid")

    engine = engine_identity(row, metadata)
    if not engine:
        blockers.append("engine_identity_missing")

    canonicalization = nested_object(row, "canonicalization")
    if text(canonicalization.get("status"), "missing") != "available":
        blockers.append(f"canonicalization_status:{text(canonicalization.get('status'), 'missing')}")
    if not nonempty(canonicalization.get("canonicalizer_version")):
        blockers.append("canonicalizer_version_missing")
    canonical_result_hash = text(canonicalization.get("result_hash"))
    if not is_sha256(canonical_result_hash):
        blockers.append("canonical_result_hash_missing_or_invalid")
    semantic_result = text(row.get("semantic_result") or row.get("reasoning_result"))
    if not nonempty(semantic_result):
        blockers.append("semantic_result_missing")
    elif semantic_result != canonical_result_hash:
        blockers.append("semantic_result_not_canonical")

    oracle = nested_object(row, "oracle")
    oracle_status = text(oracle.get("status"), "missing")
    if oracle_status != "available":
        blockers.append(f"oracle_status:{oracle_status}")
    oracle_class = text(oracle.get("class") or oracle.get("oracle_class"), "missing")
    if oracle_class not in ACCEPTED_ORACLE_CLASSES:
        blockers.append(f"oracle_class:{oracle_class}")
    if not nonempty(oracle.get("oracle_id")):
        blockers.append("oracle_id_missing")
    if not nonempty(oracle.get("provenance")):
        blockers.append("oracle_provenance_missing")
    if text(oracle.get("match_reason"), "missing") != "accepted":
        blockers.append(f"oracle_match:{text(oracle.get('match_reason'), 'missing')}")
    if not is_sha256(oracle.get("provenance_sha256")):
        blockers.append("oracle_provenance_hash_missing_or_invalid")
    if not nonempty(oracle.get("provenance_artifact")):
        blockers.append("oracle_provenance_artifact_missing")
    if not is_sha256(oracle.get("provenance_artifact_sha256")):
        blockers.append("oracle_provenance_artifact_hash_missing_or_invalid")
    if not is_sha256(oracle_hash(row)):
        blockers.append("oracle_artifact_hash_missing_or_invalid")
    expected_hash = oracle_canonical_hash(row)
    if not is_sha256(expected_hash):
        blockers.append("oracle_canonical_hash_missing_or_invalid")
    elif semantic_result != expected_hash:
        blockers.append("semantic_result_not_equal_to_oracle")

    if oracle_class == "derived":
        if text(oracle.get("audit_status"), "missing") != "independently_audited":
            blockers.append("derived_oracle_not_independently_audited")
        if oracle.get("derivation_deterministic") is not True:
            blockers.append("derived_oracle_not_deterministic")
        if oracle.get("sound_complete") is not True:
            blockers.append("derived_oracle_not_sound_complete")
        if not nonempty(oracle.get("oracle_pipeline_version")):
            blockers.append("derived_oracle_pipeline_missing")
        if not nonempty(oracle.get("proof_artifact")):
            blockers.append("derived_oracle_proof_missing")
        if not is_sha256(oracle.get("proof_artifact_sha256")):
            blockers.append("derived_oracle_proof_hash_missing_or_invalid")
        if not nonempty(oracle.get("audit_artifact")):
            blockers.append("derived_oracle_audit_missing")
        if not is_sha256(oracle.get("audit_artifact_sha256")):
            blockers.append("derived_oracle_audit_hash_missing_or_invalid")
    if split_stack_requested(metadata):
        profile = text(row.get("query_profile"), "missing")
        blockers.extend(component_blockers(row, profile))
        blockers.extend(materialization_blockers(row, profile))
    return blockers


def profile_summary(
    profile: str,
    rows: list[dict[str, Any]],
    metadata: dict[str, Any],
    duplicate_case_ids: set[str],
) -> dict[str, Any]:
    split_requested = split_stack_requested(metadata)
    blockers = Counter()
    for row in rows:
        for blocker in row_blockers(row, metadata):
            blockers[blocker] += 1
        if text(row.get("case_id"), "missing") in duplicate_case_ids:
            blockers["duplicate_case_id"] += 1

    expected_count = EXPECTED_PROFILE_COUNTS[profile]
    if len(rows) != expected_count:
        blockers["profile_case_count_mismatch"] += 1

    gate = "passed" if not blockers else "blocked"
    source_hashes = unique_strings(source_query_hash(row) for row in rows)
    oracle_hashes = unique_strings(oracle_hash(row) for row in rows)
    oracle_canonical_hashes = unique_strings(oracle_canonical_hash(row) for row in rows)
    oracle_provenance_hashes = unique_strings(
        nested_object(row, "oracle").get("provenance_sha256") for row in rows
    )
    oracle_provenance_artifact_hashes = unique_strings(
        nested_object(row, "oracle").get("provenance_artifact_sha256") for row in rows
    )
    source_identities = unique_objects(
        {
            "source_query": text(row.get("source_query")),
            "source_corpus": text(row.get("source_corpus")),
            "source_revision": text(row.get("source_revision")),
            "source_query_sha256": source_query_hash(row),
        }
        for row in rows
    )
    dataset_identities = unique_objects(dataset_identity(row) for row in rows)
    engine_identities = unique_objects(engine_identity(row, metadata) for row in rows)
    canonicalizer_identities = unique_objects(canonicalizer_identity(row, metadata) for row in rows)
    component_identities = unique_objects(component_identity(row) for row in rows)
    materialization_identities = unique_objects(materialization_identity(row) for row in rows)
    profile_context_identities = unique_objects(
        {
            "profile": safe_scalar(row.get("query_profile")),
            "directory": safe_scalar(nested_object(row, "profile_context").get("directory")),
            "identity": safe_scalar(nested_object(row, "profile_context").get("identity")),
            "status": safe_scalar(nested_object(row, "profile_context").get("status")),
        }
        for row in rows
    )
    oracle_identities = unique_objects(
        {
            "class": text(
                nested_object(row, "oracle").get("class")
                or nested_object(row, "oracle").get("oracle_class")
            ),
            "oracle_id": text(nested_object(row, "oracle").get("oracle_id")),
            "provenance": text(nested_object(row, "oracle").get("provenance")),
            "provenance_sha256": text(
                nested_object(row, "oracle").get("provenance_sha256")
            ),
            "provenance_artifact": text(
                nested_object(row, "oracle").get("provenance_artifact")
            ),
            "provenance_artifact_sha256": text(
                nested_object(row, "oracle").get("provenance_artifact_sha256")
            ),
            "artifact_sha256": oracle_hash(row),
            "canonical_result_hash": oracle_canonical_hash(row),
            "match_reason": text(nested_object(row, "oracle").get("match_reason")),
            "proof_artifact": text(nested_object(row, "oracle").get("proof_artifact")),
            "proof_artifact_sha256": text(
                nested_object(row, "oracle").get("proof_artifact_sha256")
            ),
            "audit_artifact": text(nested_object(row, "oracle").get("audit_artifact")),
            "audit_artifact_sha256": text(
                nested_object(row, "oracle").get("audit_artifact_sha256")
            ),
            "audit_status": text(nested_object(row, "oracle").get("audit_status")),
            "derivation_deterministic": nested_object(row, "oracle").get(
                "derivation_deterministic"
            ),
            "sound_complete": nested_object(row, "oracle").get("sound_complete"),
            "oracle_pipeline_version": text(
                nested_object(row, "oracle").get("oracle_pipeline_version")
            ),
        }
        for row in rows
    )

    return {
        "report_schema_version": 1,
        "profile": profile,
        "split_stack_requested": split_requested,
        "case_count": len(rows),
        "expected_case_count": expected_count,
        "case_count_matches": len(rows) == expected_count,
        "correctness_gate": gate,
        "status": gate,
        "performance_claims_allowed": gate == "passed",
        "result_status_counts": counter_for(rows, "result_status"),
        "execution_status_counts": counter_for(rows, "execution_status"),
        "correctness_status_counts": counter_for(rows, "correctness_status"),
        "oracle_status_counts": counter_for(
            [nested_object(row, "oracle") for row in rows], "status"
        ),
        "oracle_class_counts": counter_for(
            [nested_object(row, "oracle") for row in rows], "class"
        ),
        "component_status_counts": nested_status_counter(
            rows,
            "component_manifest",
            "not_available" if split_requested else "not_requested",
        ),
        "materialization_status_counts": nested_status_counter(
            rows,
            "materialization",
            "not_available" if split_requested else "not_requested",
        ),
        "engine_provenance_status_counts": engine_provenance_status_counter(rows),
        "gate_blocker_counts": sorted_counter(blockers),
        "gate_reasons": sorted(blockers),
        "source_hashes": source_hashes,
        "oracle_hashes": oracle_hashes,
        "oracle_canonical_result_hashes": oracle_canonical_hashes,
        "oracle_provenance_hashes": oracle_provenance_hashes,
        "oracle_provenance_artifact_hashes": oracle_provenance_artifact_hashes,
        "source_identities": source_identities,
        "dataset_identities": dataset_identities,
        "engine_identities": engine_identities,
        "component_identities": component_identities,
        "materialization_identities": materialization_identities,
        "profile_context_identities": profile_context_identities,
        "canonicalizer_identities": canonicalizer_identities,
        "oracle_identities": oracle_identities,
        "timing_provenance": timing_summary(rows),
    }


def load_results(run_dir: Path) -> tuple[list[dict[str, Any]], list[str]]:
    rows: list[dict[str, Any]] = []
    errors: list[str] = []
    excluded = {"run_metadata.json", "summary.json"}
    for path in sorted(run_dir.glob("*.json")):
        if path.name in excluded:
            continue
        try:
            value = json.loads(path.read_text(encoding="utf-8"))
        except (OSError, json.JSONDecodeError) as exc:
            errors.append(f"{path.name}: {exc}")
            continue
        if not isinstance(value, dict) or value.get("schema_version") != 1 or not nonempty(value.get("case_id")):
            errors.append(f"{path.name}: invalid result record")
            continue
        rows.append(value)
    return rows, errors


def load_metadata(run_dir: Path) -> dict[str, Any]:
    path = run_dir / "run_metadata.json"
    if not path.exists():
        return {}
    try:
        value = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError):
        return {}
    return value if isinstance(value, dict) else {}


def markdown_value(value: Any) -> str:
    if value is None or value == "":
        return "unavailable"
    if isinstance(value, (dict, list)):
        value = json.dumps(value, sort_keys=True, separators=(",", ":"))
    return text(value).replace("|", "\\|").replace("\n", " ")


def row_source_label(row: dict[str, Any]) -> str:
    return f"{text(row.get('source_query'))} ({text(row.get('source_corpus'))}@{text(row.get('source_revision'))})"


def row_dataset_label(row: dict[str, Any]) -> str:
    identity = dataset_identity(row)
    return f"{identity['dataset_id']} ({identity['dataset_sha256']})"


def row_engine_label(row: dict[str, Any], metadata: dict[str, Any]) -> str:
    return markdown_value(engine_identity(row, metadata))


def row_canonicalizer_label(row: dict[str, Any], metadata: dict[str, Any]) -> str:
    identity = canonicalizer_identity(row, metadata)
    return f"{identity['name']}@{identity['version']}"


def row_oracle_label(row: dict[str, Any]) -> str:
    oracle = nested_object(row, "oracle")
    oracle_class = text(oracle.get("class") or oracle.get("oracle_class"), "unavailable")
    return f"{text(oracle.get('status'), 'unavailable')} ({oracle_class})"


def row_oracle_provenance_label(row: dict[str, Any]) -> str:
    oracle = nested_object(row, "oracle")
    return (
        f"id={text(oracle.get('oracle_id'), 'unavailable')}; "
        f"provenance={text(oracle.get('provenance'), 'unavailable')}; "
        f"match={text(oracle.get('match_reason'), 'unavailable')}"
    )


def row_oracle_evidence_label(row: dict[str, Any]) -> str:
    oracle = nested_object(row, "oracle")
    evidence = {
        "provenance_artifact": oracle.get("provenance_artifact"),
        "proof_artifact": oracle.get("proof_artifact"),
        "audit_artifact": oracle.get("audit_artifact"),
        "audit_status": oracle.get("audit_status"),
        "proof_sha256": oracle.get("proof_artifact_sha256"),
        "audit_sha256": oracle.get("audit_artifact_sha256"),
        "pipeline": oracle.get("oracle_pipeline_version"),
    }
    return markdown_value({key: value for key, value in evidence.items() if value is not None})


def write_row_table(handle: Any, rows: list[dict[str, Any]], metadata: dict[str, Any], include_profile: bool) -> None:
    columns = [
        "Case",
        "Profile",
        "Source",
        "Source hash",
        "Dataset",
        "Engine",
        "Canonicalizer",
        "Oracle",
        "Oracle provenance",
        "Oracle provenance hash",
        "Oracle artifact hash",
        "Oracle canonical hash",
        "Oracle evidence",
        "Execution",
        "Result",
        "Correctness",
        "Load (ms)",
        "Query (ms)",
    ]
    if not include_profile:
        columns.remove("Profile")
    handle.write("| " + " | ".join(columns) + " |\n")
    handle.write("|" + "---|" * len(columns) + "\n")
    for row in rows:
        values = [
            text(row.get("case_id")),
            text(row.get("query_profile"), "unknown"),
            row_source_label(row),
            source_query_hash(row),
            row_dataset_label(row),
            row_engine_label(row, metadata),
            row_canonicalizer_label(row, metadata),
            row_oracle_label(row),
            row_oracle_provenance_label(row),
            nested_object(row, "oracle").get("provenance_sha256", ""),
            oracle_hash(row),
            oracle_canonical_hash(row),
            row_oracle_evidence_label(row),
            text(row.get("execution_status"), "unknown"),
            text(row.get("result_status"), "unknown"),
            text(row.get("correctness_status"), "unknown"),
            text(row.get("load_time_ms"), "-1"),
            text(row.get("query_time_ms"), "-1"),
        ]
        if not include_profile:
            values.pop(1)
        handle.write("| " + " | ".join(markdown_value(value) for value in values) + " |\n")


def write_profile_report(
    path: Path,
    profile: str,
    summary: dict[str, Any],
    rows: list[dict[str, Any]],
    metadata: dict[str, Any],
) -> None:
    with path.open("w", encoding="utf-8") as handle:
        handle.write(f"# OWL2Bench Query Report — {profile}\n\n")
        handle.write(f"- Profile: **{profile}**\n")
        handle.write(f"- Profile correctness gate: **{summary['correctness_gate']}**\n")
        handle.write(f"- Rows: {summary['case_count']} / {summary['expected_case_count']} required\n")
        handle.write(
            f"- Performance/scalability claims: **{'allowed' if summary['performance_claims_allowed'] else 'suppressed'}**\n\n"
        )
        handle.write("## Status counts\n\n")
        handle.write("| Execution status | Count |\n|---|---:|\n")
        for status, count in summary["execution_status_counts"].items():
            handle.write(f"| {markdown_value(status)} | {count} |\n")
        handle.write("\n")
        handle.write("| Result status | Count |\n|---|---:|\n")
        for status, count in summary["result_status_counts"].items():
            handle.write(f"| {markdown_value(status)} | {count} |\n")
        handle.write("\n| Correctness status | Count |\n|---|---:|\n")
        for status, count in summary["correctness_status_counts"].items():
            handle.write(f"| {markdown_value(status)} | {count} |\n")
        handle.write("\n| Oracle status | Count |\n|---|---:|\n")
        for status, count in summary["oracle_status_counts"].items():
            handle.write(f"| {markdown_value(status)} | {count} |\n")
        handle.write("\n| Component status | Count |\n|---|---:|\n")
        for status, count in summary["component_status_counts"].items():
            handle.write(f"| {markdown_value(status)} | {count} |\n")
        handle.write("\n| Materialization status | Count |\n|---|---:|\n")
        for status, count in summary["materialization_status_counts"].items():
            handle.write(f"| {markdown_value(status)} | {count} |\n")
        handle.write("\n| Engine provenance status | Count |\n|---|---:|\n")
        for status, count in summary["engine_provenance_status_counts"].items():
            handle.write(f"| {markdown_value(status)} | {count} |\n")

        handle.write("\n## Provenance\n\n")
        handle.write(f"- Split-stack requested: **{summary['split_stack_requested']}**\n")
        handle.write(f"- Source hashes: {markdown_value(summary['source_hashes'])}\n")
        handle.write(f"- Oracle artifact hashes: {markdown_value(summary['oracle_hashes'])}\n")
        handle.write(
            f"- Oracle canonical result hashes: {markdown_value(summary['oracle_canonical_result_hashes'])}\n"
        )
        handle.write(
            f"- Oracle provenance hashes: {markdown_value(summary['oracle_provenance_hashes'])}\n"
        )
        handle.write(
            f"- Oracle provenance-artifact hashes: {markdown_value(summary['oracle_provenance_artifact_hashes'])}\n"
        )
        handle.write(f"- Dataset identities: {markdown_value(summary['dataset_identities'])}\n")
        handle.write(f"- Engine identities: {markdown_value(summary['engine_identities'])}\n")
        handle.write(f"- Component identities: {markdown_value(summary['component_identities'])}\n")
        handle.write(
            f"- Materialization identities: {markdown_value(summary['materialization_identities'])}\n"
        )
        handle.write(
            f"- Profile-context identities: {markdown_value(summary['profile_context_identities'])}\n"
        )
        handle.write(f"- Canonicalizer identities: {markdown_value(summary['canonicalizer_identities'])}\n")
        handle.write(f"- Timing provenance: {markdown_value(summary['timing_provenance'])}\n")
        if summary["gate_blocker_counts"]:
            handle.write(f"- Gate blockers: {markdown_value(summary['gate_blocker_counts'])}\n")
        handle.write("\n## Profile-qualified rows\n\n")
        write_row_table(handle, rows, metadata, include_profile=False)


def write_overall_report(
    path: Path,
    summary: dict[str, Any],
    rows: list[dict[str, Any]],
    metadata: dict[str, Any],
) -> None:
    with path.open("w", encoding="utf-8") as handle:
        handle.write("# OWL2Bench Query Report\n\n")
        handle.write(f"- Query rows: {summary['query_count']} / {summary['expected_query_count']} required\n")
        handle.write(f"- Overall query correctness gate: **{summary['query_gate']}**\n")
        variant_control = summary["variant_control"]
        handle.write(f"- Variant revision: `{variant_control['revision_id']}`\n")
        handle.write(f"- Variant category SHA: `{variant_control['category_sha256']}`\n")
        handle.write(
            "- Variant official QL status counts: "
            f"{markdown_value(variant_control['official_status_counts'])}\n"
        )
        handle.write(
            f"- Implementation completeness: **{variant_control['implementation_completeness']}**\n"
        )
        handle.write(f"- Release readiness: **{variant_control['release_readiness']}**\n")
        handle.write(
            f"- Execution availability: **{variant_control['execution_availability']}**\n"
        )
        handle.write(f"- Semantic correctness: **{variant_control['semantic_correctness']}**\n")
        handle.write(
            f"- Performance/scalability claims: **{'allowed' if summary['performance_claims_allowed'] else 'suppressed'}**\n\n"
        )
        handle.write("## Conjunctive profile gate\n\n")
        handle.write("| Profile | Rows | Required | Gate | Result statuses | Oracle statuses |\n")
        handle.write("|---|---:|---:|---|---|---|\n")
        for profile in PROFILES:
            profile_summary_value = summary["profiles"][profile]
            handle.write(
                f"| {profile} | {profile_summary_value['case_count']} | {profile_summary_value['expected_case_count']} | "
                f"{profile_summary_value['correctness_gate']} | "
                f"{markdown_value(profile_summary_value['result_status_counts'])} | "
                f"{markdown_value(profile_summary_value['oracle_status_counts'])} |\n"
            )
        gate_terms = " AND ".join(
            f"{profile} **{summary['profile_gates'][profile]}**" for profile in PROFILES
        )
        handle.write(f"\nOverall query gate: {gate_terms}\n\n")
        handle.write(
            "The overall gate is the conjunction of the four profile gates. "
            "Aggregate counts below are diagnostic and do not replace profile decisions.\n\n"
        )
        handle.write("## Aggregate diagnostic counts\n\n")
        handle.write("| Execution status | Count |\n|---|---:|\n")
        for status, count in summary["execution_status_counts"].items():
            handle.write(f"| {markdown_value(status)} | {count} |\n")
        handle.write("\n| Result status | Count |\n|---|---:|\n")
        for status, count in summary["result_status_counts"].items():
            handle.write(f"| {markdown_value(status)} | {count} |\n")
        handle.write("\n## Source and oracle coverage\n\n")
        handle.write(f"- Split-stack requested: **{summary['split_stack_requested']}**\n")
        handle.write(f"- Source hashes: {markdown_value(summary['source_hashes'])}\n")
        handle.write(f"- Oracle artifact hashes: {markdown_value(summary['oracle_hashes'])}\n")
        handle.write(
            f"- Component status counts: {markdown_value(summary['component_status_counts'])}\n"
        )
        handle.write(
            f"- Materialization status counts: {markdown_value(summary['materialization_status_counts'])}\n"
        )
        handle.write(
            "- Engine provenance status counts: "
            f"{markdown_value(summary['engine_provenance_status_counts'])}\n"
        )
        handle.write(f"- Component identities: {markdown_value(summary['component_identities'])}\n")
        handle.write(
            f"- Materialization identities: {markdown_value(summary['materialization_identities'])}\n"
        )
        handle.write(f"- Unclassified rows: {summary['unclassified_count']}\n")
        if summary["input_errors"]:
            handle.write(f"- Invalid result records: {markdown_value(summary['input_errors'])}\n")
        if summary["gate_blocker_counts"]:
            handle.write(f"- Overall gate blockers: {markdown_value(summary['gate_blocker_counts'])}\n")
        handle.write("\n## All profile-qualified rows\n\n")
        write_row_table(handle, rows, metadata, include_profile=True)


def build_reports(run_dir: Path) -> dict[str, Any]:
    metadata = load_metadata(run_dir)
    rows, input_errors = load_results(run_dir)
    rows_by_profile = {profile: [] for profile in PROFILES}
    unclassified_rows: list[dict[str, Any]] = []
    for row in rows:
        profile = text(row.get("query_profile"), "unknown")
        if profile in rows_by_profile:
            rows_by_profile[profile].append(row)
        else:
            unclassified_rows.append(row)

    case_ids = [text(row.get("case_id")) for row in rows]
    duplicate_case_ids = {case_id for case_id in case_ids if case_ids.count(case_id) > 1}
    profile_summaries = {
        profile: profile_summary(profile, rows_by_profile[profile], metadata, duplicate_case_ids)
        for profile in PROFILES
    }
    profile_gates = {
        profile: profile_summaries[profile]["correctness_gate"] for profile in PROFILES
    }

    overall_blockers = Counter()
    if len(rows) != EXPECTED_QUERY_COUNT:
        overall_blockers["query_case_count_mismatch"] += 1
    if unclassified_rows:
        overall_blockers["unclassified_profile_rows"] += len(unclassified_rows)
    if duplicate_case_ids:
        overall_blockers["duplicate_case_ids"] += len(duplicate_case_ids)
    if input_errors:
        overall_blockers["invalid_result_records"] += len(input_errors)
    for profile in PROFILES:
        if profile_gates[profile] != "passed":
            overall_blockers[f"profile_gate_blocked:{profile}"] += 1

    overall_gate = "passed" if not overall_blockers else "blocked"
    result_counts = Counter()
    for row in rows:
        result_counts[text(row.get("result_status"), "unknown")] += 1
    all_source_hashes = unique_strings(source_query_hash(row) for row in rows)
    all_oracle_hashes = unique_strings(oracle_hash(row) for row in rows)
    split_requested = split_stack_requested(metadata)
    summary = {
        "report_schema_version": 1,
        "generated_at": datetime.now(timezone.utc).isoformat(),
        "run_dir": str(run_dir),
        "variant_control": variant_control_statuses(metadata, run_dir),
        "split_stack_requested": split_requested,
        "query_count": len(rows),
        "expected_query_count": EXPECTED_QUERY_COUNT,
        "profile_case_counts": {
            profile: len(rows_by_profile[profile]) for profile in PROFILES
        },
        "expected_profile_case_counts": EXPECTED_PROFILE_COUNTS,
        "unclassified_count": len(unclassified_rows),
        "duplicate_case_ids": sorted(duplicate_case_ids),
        "input_errors": input_errors,
        "profiles": profile_summaries,
        "profile_gates": profile_gates,
        "query_gate": overall_gate,
        "correctness_gate": overall_gate,
        "status": overall_gate,
        "performance_claims_allowed": overall_gate == "passed",
        "result_status_counts": sorted_counter(result_counts),
        "execution_status_counts": counter_for(rows, "execution_status"),
        "source_hashes": all_source_hashes,
        "oracle_hashes": all_oracle_hashes,
        "component_status_counts": nested_status_counter(
            rows,
            "component_manifest",
            "not_available" if split_requested else "not_requested",
        ),
        "materialization_status_counts": nested_status_counter(
            rows,
            "materialization",
            "not_available" if split_requested else "not_requested",
        ),
        "engine_provenance_status_counts": engine_provenance_status_counter(rows),
        "component_identities": unique_objects(component_identity(row) for row in rows),
        "materialization_identities": unique_objects(
            materialization_identity(row) for row in rows
        ),
        "gate_blocker_counts": sorted_counter(overall_blockers),
    }

    profiles_dir = run_dir / "profiles"
    profiles_dir.mkdir(parents=True, exist_ok=True)
    for existing_profile_dir in profiles_dir.iterdir():
        if not existing_profile_dir.is_dir():
            continue
        marker = existing_profile_dir / ".owl2bench-query-report-owned"
        if marker.is_file():
            for generated_name in ("report.md", "summary.json"):
                generated_path = existing_profile_dir / generated_name
                if generated_path.exists():
                    generated_path.unlink()
    for profile in PROFILES:
        profile_dir = profiles_dir / profile
        profile_dir.mkdir(parents=True, exist_ok=True)
        (profile_dir / ".owl2bench-query-report-owned").write_text(
            "Generated by report_profiles.py; safe to replace.\n", encoding="utf-8"
        )
        profile_summary_value = profile_summaries[profile]
        (profile_dir / "summary.json").write_text(
            json.dumps(
                {
                    **profile_summary_value,
                    "run_dir": str(run_dir),
                    "run_metadata": str(run_dir / "run_metadata.json"),
                },
                indent=2,
                sort_keys=True,
            )
            + "\n",
            encoding="utf-8",
        )
        write_profile_report(
            profile_dir / "report.md",
            profile,
            profile_summary_value,
            sorted(rows_by_profile[profile], key=lambda row: text(row.get("case_id"))),
            metadata,
        )

    (run_dir / "summary.json").write_text(
        json.dumps(summary, indent=2, sort_keys=True) + "\n", encoding="utf-8"
    )
    write_overall_report(
        run_dir / "report.md",
        summary,
        sorted(rows, key=lambda row: (text(row.get("query_profile"), "unknown"), text(row.get("case_id")))),
        metadata,
    )
    return summary


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("run_dir", type=Path)
    args = parser.parse_args()
    run_dir = args.run_dir.resolve()
    if not run_dir.is_dir():
        parser.error(f"run directory not found: {run_dir}")
    summary = build_reports(run_dir)
    print(f"[owl2bench-query-report] summary={run_dir / 'summary.json'}")
    print(f"[owl2bench-query-report] report={run_dir / 'report.md'}")
    print(f"[owl2bench-query-report] query_gate={summary['query_gate']}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
