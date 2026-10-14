#!/usr/bin/env python3
"""Assemble and verify profile-qualified OWL2Bench materialization evidence.

The assembler combines the frozen dataset package, one identity per profile,
and one optional materialization result per profile into the existing
component/materialization contracts. It only copies provenance fields; it
never creates RDF graphs, query answers, or oracle records.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import subprocess
import sys
from pathlib import Path
from typing import Any


PROFILES = ("DL", "EL", "QL", "RL")
EXPECTED_COUNTS = {"DL": 22, "EL": 10, "QL": 10, "RL": 18}
SCRIPT_DIR = Path(__file__).resolve().parent
VALIDATOR = SCRIPT_DIR / "materialization_manifest.py"
GRAPH_VALIDATOR = SCRIPT_DIR / "materialization_validator.py"
FORBIDDEN_KEYS = {
    "oracle",
    "oracles",
    "oracle_key",
    "answer_key",
    "answer_keys",
    "answer_file",
    "answer_sha256",
}


class AssemblyError(ValueError):
    """The profile stack cannot be assembled without weakening its contract."""


def digest(path: Path) -> str:
    hasher = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            hasher.update(chunk)
    return "sha256:" + hasher.hexdigest()


def load_json(path: Path, label: str) -> dict[str, Any]:
    resolved = path.expanduser().resolve()
    if not resolved.is_file():
        raise AssemblyError(f"{label} is missing: {resolved}")
    try:
        value = json.loads(resolved.read_text(encoding="utf-8"))
    except (OSError, UnicodeDecodeError, json.JSONDecodeError) as error:
        raise AssemblyError(f"{label} is malformed: {resolved}: {error}") from error
    if not isinstance(value, dict):
        raise AssemblyError(f"{label} root must be an object: {resolved}")
    return value


def write_json(path: Path, value: dict[str, Any]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, ensure_ascii=False, indent=2, sort_keys=True) + "\n", encoding="utf-8")


def nonempty(value: Any) -> bool:
    return isinstance(value, str) and bool(value.strip())


def valid_digest(value: Any) -> bool:
    normalized = str(value or "").strip().removeprefix("sha256:")
    return len(normalized) == 64 and all(character in "0123456789abcdefABCDEF" for character in normalized)


def digest_value(value: Any) -> str:
    return str(value or "").strip().removeprefix("sha256:").lower()


def normalize_license_record(value: Any) -> dict[str, Any]:
    if value is None:
        return {
            "status": "not_available",
            "id": "runtime-license",
            "reason": "license identity was not supplied by the profile adapter",
        }
    if not isinstance(value, dict):
        raise AssemblyError("license identity must be an object")
    record = dict(value)
    status = record.get("status")
    if status == "recorded":
        raise AssemblyError(
            "license status 'recorded' is not a component-manifest status; "
            "use available with id/digest, not_required, or not_available"
        )
    if status not in {"available", "not_required", "not_available"}:
        raise AssemblyError(f"license status is missing or invalid: {status}")
    if status == "available":
        if not nonempty(record.get("id")) or not valid_digest(record.get("digest")):
            raise AssemblyError("available license identity requires id and SHA-256 digest")
    return record


def validate_frozen_dataset(document: dict[str, Any]) -> dict[str, Any]:
    """Validate the provenance envelope before accepting a profile stack."""

    errors: list[str] = []
    if document.get("no_answer_keys") is not True:
        errors.append("no_answer_keys_policy_missing")
    for forbidden in forbidden_paths(document):
        errors.append(f"forbidden_field:{forbidden}")
    source = document.get("source")
    if not isinstance(source, dict) or not nonempty(source.get("revision")):
        errors.append("source_revision_missing")
    generator = document.get("generator")
    if not isinstance(generator, dict):
        errors.append("generator_missing")
    else:
        seed = generator.get("seed")
        if isinstance(seed, bool) or not isinstance(seed, int) or seed < 0:
            errors.append("generator_seed_invalid")
        if not nonempty(generator.get("revision")):
            errors.append("generator_revision_missing")
        if not nonempty(generator.get("artifact_digest")):
            errors.append("generator_digest_missing")
    resources = document.get("resource_limits")
    if not isinstance(resources, dict):
        errors.append("resource_limits_missing")
    else:
        if not isinstance(resources.get("timeout_seconds"), int) or resources["timeout_seconds"] <= 0:
            errors.append("resource_limits_timeout_invalid")
        if not isinstance(resources.get("threads"), int) or resources["threads"] <= 0:
            errors.append("resource_limits_threads_invalid")
        if not nonempty(resources.get("memory")):
            errors.append("resource_limits_memory_missing")
        if not nonempty(resources.get("variant")):
            errors.append("resource_limits_variant_missing")
        for field in ("cpu", "disk", "network"):
            if not nonempty(resources.get(field)):
                errors.append(f"resource_limits_{field}_missing")
    profiles = document.get("profiles")
    if not isinstance(profiles, dict):
        errors.append("profiles_missing")
        profiles = {}
    identities: list[str] = []
    for profile in PROFILES:
        record = profiles.get(profile)
        if not isinstance(record, dict):
            errors.append(f"profile_missing:{profile}")
            continue
        if record.get("status") != "complete":
            errors.append(f"profile_status:{profile}:{record.get('status')}")
        identity = record.get("identity_sha256")
        if not nonempty(identity):
            errors.append(f"profile_identity_missing:{profile}")
        else:
            identities.append(str(identity))
        dataset = record.get("dataset")
        if not isinstance(dataset, dict):
            errors.append(f"profile_dataset_missing:{profile}")
        else:
            for field in ("id", "sha256", "tbox_id", "tbox_sha256", "abox_id", "abox_sha256"):
                if not nonempty(dataset.get(field)):
                    errors.append(f"profile_dataset_field_missing:{profile}:{field}")
        context = record.get("profile_context")
        if not isinstance(context, dict) or not nonempty(context.get("id")) or not nonempty(context.get("directory")):
            errors.append(f"profile_context_missing:{profile}")
    if len(identities) != len(set(identities)):
        errors.append("profile_identity_reused")
    validation = document.get("validation")
    if not isinstance(validation, dict):
        errors.append("dataset_validation_missing")
    elif validation.get("status") != "complete":
        errors.append(f"dataset_validation_status:{validation.get('status')}")
    elif validation.get("errors") not in (None, []):
        errors.append("dataset_validation_errors_present")
    return {
        "status": "complete" if not errors else "failed",
        "errors": errors,
        "profile_statuses": {
            profile: profiles.get(profile, {}).get("status")
            if isinstance(profiles.get(profile), dict)
            else "not_available"
            for profile in PROFILES
        },
    }


def frozen_identity_errors(
    profile: str, identity: dict[str, Any], dataset_manifest: dict[str, Any]
) -> list[str]:
    errors: list[str] = []
    frozen_profiles = dataset_manifest.get("profiles")
    frozen_profile = frozen_profiles.get(profile) if isinstance(frozen_profiles, dict) else None
    if not isinstance(frozen_profile, dict):
        return [f"dataset_manifest_profile_missing:{profile}"]
    frozen_dataset = frozen_profile.get("dataset")
    identity_dataset = identity.get("dataset")
    if not isinstance(frozen_dataset, dict) or not isinstance(identity_dataset, dict):
        return [f"dataset_identity_missing:{profile}"]
    for field in (
        "id",
        "artifact_id",
        "artifact_path",
        "artifact_revision",
        "sha256",
        "source_ontology_sha256",
        "tbox_id",
        "tbox_sha256",
        "abox_id",
        "abox_sha256",
    ):
        frozen_value = frozen_dataset.get(field)
        identity_value = identity_dataset.get(field)
        if field.endswith("sha256"):
            matches = valid_digest(frozen_value) and valid_digest(identity_value) and digest_value(frozen_value) == digest_value(identity_value)
        elif field == "artifact_path":
            matches = (
                nonempty(frozen_value)
                and nonempty(identity_value)
                and Path(str(frozen_value)).expanduser().resolve()
                == Path(str(identity_value)).expanduser().resolve()
            )
        else:
            matches = frozen_value == identity_value
        if not matches:
            errors.append(f"dataset_identity_mismatch:{field}")

    identity_source = identity.get("source_document")
    allowed_source_hashes = {
        digest_value(value)
        for value in (frozen_dataset.get("sha256"), frozen_dataset.get("source_ontology_sha256"))
        if valid_digest(value)
    }
    if not isinstance(identity_source, dict) or digest_value(identity_source.get("sha256")) not in allowed_source_hashes:
        errors.append("source_document_not_in_frozen_dataset")

    frozen_context = frozen_profile.get("profile_context")
    identity_context = identity.get("profile_context")
    if not isinstance(frozen_context, dict) or not isinstance(identity_context, dict):
        errors.append("profile_context_missing")
    else:
        if frozen_context.get("id") != identity_context.get("id"):
            errors.append("profile_context_id_mismatch")
        if (
            nonempty(frozen_context.get("directory"))
            and nonempty(identity_context.get("directory"))
            and Path(str(frozen_context["directory"])).expanduser().resolve()
            != Path(str(identity_context["directory"])).expanduser().resolve()
        ):
            errors.append("profile_context_directory_mismatch")

    frozen_generator = dataset_manifest.get("generator")
    identity_generator = identity.get("generator")
    frozen_generator_digest = frozen_generator.get("artifact_digest") if isinstance(frozen_generator, dict) else None
    identity_generator_digest = identity_generator.get("artifact_digest") if isinstance(identity_generator, dict) else None
    if not valid_digest(frozen_generator_digest) or not valid_digest(identity_generator_digest):
        errors.append("generator_artifact_digest_missing")
    elif digest_value(frozen_generator_digest) != digest_value(identity_generator_digest):
        errors.append("generator_artifact_digest_mismatch")

    frozen_resources = dataset_manifest.get("resource_limits")
    identity_resources = identity.get("resource_limits")
    for field in ("timeout_seconds", "memory", "threads", "cpu", "disk", "network", "variant"):
        if not isinstance(frozen_resources, dict) or not isinstance(identity_resources, dict):
            errors.append(f"resource_limit_missing:{field}")
        elif frozen_resources.get(field) != identity_resources.get(field):
            errors.append(f"resource_limit_mismatch:{field}")
    return errors


def bindings(values: list[str], label: str) -> dict[str, Path]:
    result: dict[str, Path] = {}
    for value in values:
        profile, separator, raw_path = value.partition("=")
        profile = profile.strip().upper()
        if separator != "=" or profile not in PROFILES or not raw_path.strip():
            raise AssemblyError(f"{label} must use PROFILE=PATH for {','.join(PROFILES)}")
        if profile in result:
            raise AssemblyError(f"duplicate {label} binding for {profile}")
        result[profile] = Path(raw_path).expanduser().resolve()
    return result


def forbidden_paths(value: Any, prefix: str = "") -> list[str]:
    paths: list[str] = []
    if isinstance(value, dict):
        for key, child in value.items():
            path = f"{prefix}.{key}" if prefix else str(key)
            if key.lower().replace("-", "_") in FORBIDDEN_KEYS:
                paths.append(path)
            paths.extend(forbidden_paths(child, path))
    elif isinstance(value, list):
        for index, child in enumerate(value):
            paths.extend(forbidden_paths(child, f"{prefix}[{index}]"))
    return paths


def component_part(identity: dict[str, Any], section: str) -> dict[str, Any]:
    value = identity.get(section)
    if not isinstance(value, dict):
        raise AssemblyError(f"identity section is missing: {section}")
    required = ("name", "version", "revision", "artifact_digest")
    missing = [field for field in required if not nonempty(value.get(field))]
    if missing:
        raise AssemblyError(f"identity {section} is missing: {','.join(missing)}")
    return {field: value[field] for field in required}


def dataset_record(identity: dict[str, Any]) -> dict[str, Any]:
    source = identity.get("source_document")
    dataset = identity.get("dataset")
    if not isinstance(source, dict) or not nonempty(source.get("sha256")):
        raise AssemblyError("profile identity source_document.sha256 is missing")
    if not isinstance(dataset, dict):
        raise AssemblyError("profile identity dataset is missing")
    result = dict(dataset)
    for field in ("id", "tbox_id", "tbox_sha256", "abox_id", "abox_sha256"):
        if not nonempty(result.get(field)):
            raise AssemblyError(f"profile identity dataset.{field} is missing")
    if not valid_digest(result.get("sha256")):
        raise AssemblyError("profile identity dataset.sha256 is missing or invalid")
    if not valid_digest(source.get("sha256")):
        raise AssemblyError("profile identity source_document.sha256 is missing or invalid")
    selected = {
        field: result[field]
        for field in (
            "id",
            "sha256",
            "tbox_id",
            "tbox_sha256",
            "abox_id",
            "abox_sha256",
        )
    }
    if nonempty(result.get("source_ontology_sha256")):
        selected["source_ontology_sha256"] = result["source_ontology_sha256"]
    return selected


def resource_limits(identity: dict[str, Any]) -> dict[str, Any]:
    value = identity.get("resource_limits")
    if not isinstance(value, dict):
        raise AssemblyError("profile identity resource_limits is missing")
    result = dict(value)
    if not isinstance(result.get("timeout_seconds"), int) or result["timeout_seconds"] <= 0:
        raise AssemblyError("resource_limits.timeout_seconds must be positive")
    if not isinstance(result.get("threads"), int) or result["threads"] <= 0:
        raise AssemblyError("resource_limits.threads must be positive")
    if not nonempty(result.get("memory")):
        raise AssemblyError("resource_limits.memory is missing")
    if not nonempty(result.get("variant")):
        raise AssemblyError("resource_limits.variant is missing")
    for field in ("cpu", "disk", "network"):
        if not nonempty(result.get(field)):
            raise AssemblyError(f"resource_limits.{field} is missing")
    return result


def profile_component_record(
    profile: str,
    identity: dict[str, Any],
    materialization: dict[str, Any],
) -> dict[str, Any]:
    if identity.get("profile") != profile:
        raise AssemblyError(f"identity profile mismatch: expected {profile}")
    if forbidden_paths(identity):
        raise AssemblyError(f"identity contains forbidden answer/oracle fields: {forbidden_paths(identity)}")
    component = component_part(identity, "component")
    exporter = component_part(identity, "exporter")
    backend = component_part(identity, "query_backend")
    configuration = identity.get("configuration")
    runtime = identity.get("runtime")
    context = identity.get("profile_context")
    derivation = identity.get("derivation")
    generator = identity.get("generator")
    if not isinstance(configuration, dict) or not nonempty(configuration.get("id")):
        raise AssemblyError(f"{profile} configuration identity is missing")
    configuration_digest = configuration.get("digest") or configuration.get("sha256")
    if not nonempty(configuration_digest):
        raise AssemblyError(f"{profile} configuration digest is missing")
    if not isinstance(runtime, dict) or not nonempty(runtime.get("id")):
        raise AssemblyError(f"{profile} runtime identity is missing")
    if not isinstance(runtime.get("dependencies"), list) or not runtime["dependencies"]:
        raise AssemblyError(f"{profile} runtime dependencies are missing")
    if not isinstance(context, dict) or not nonempty(context.get("id")) or not nonempty(context.get("directory")):
        raise AssemblyError(f"{profile} profile context identity is missing")
    if not isinstance(derivation, dict):
        derivation = {"version": "profile-materialization/unknown", "revision": "unknown"}
    if not isinstance(generator, dict):
        raise AssemblyError(f"{profile} generator identity is missing")

    materialization_provenance_errors: list[str] = []
    source_document = identity.get("source_document")
    dataset = identity.get("dataset")
    expected_source_hashes = {
        value
        for value in (
            source_document.get("sha256") if isinstance(source_document, dict) else None,
            dataset.get("sha256") if isinstance(dataset, dict) else None,
        )
        if valid_digest(value)
    }
    materialization_source = materialization.get("source_ontology")
    if isinstance(materialization_source, dict) and valid_digest(materialization_source.get("sha256")):
        if expected_source_hashes and not any(
            digest_value(materialization_source["sha256"]) == digest_value(expected)
            for expected in expected_source_hashes
        ):
            materialization_provenance_errors.append("source_ontology_sha256_not_in_frozen_identity")
    input_hash = materialization.get("input_sha256")
    if valid_digest(input_hash) and expected_source_hashes and not any(
        digest_value(input_hash) == digest_value(expected) for expected in expected_source_hashes
    ):
        materialization_provenance_errors.append("input_sha256_not_in_frozen_identity")
    materialization_dataset = materialization.get("dataset")
    if (
        isinstance(materialization_dataset, dict)
        and valid_digest(materialization_dataset.get("sha256"))
        and isinstance(dataset, dict)
        and valid_digest(dataset.get("sha256"))
        and digest_value(materialization_dataset["sha256"]) != digest_value(dataset["sha256"])
    ):
        materialization_provenance_errors.append("dataset_sha256_mismatch")
    if materialization_provenance_errors:
        materialization["status"] = "failed"
        materialization["provenance_errors"] = materialization_provenance_errors
        materialization["reason"] = (
            "materialization provenance does not match the frozen profile identity: "
            + ", ".join(materialization_provenance_errors)
        )
    credentials = identity.get("credentials", {"status": "not_required"})
    license_record = normalize_license_record(identity.get("license"))
    record: dict[str, Any] = {
        "components": {
            "engine": exporter,
            "materializer": component,
            "query": backend,
        },
        "configuration": {
            "id": configuration["id"],
            "digest": configuration_digest,
        },
        "runtime": runtime,
        "credentials": credentials,
        "license": license_record,
        "capability": identity.get(
            "capability", {"status": "supported", "supports_query": True}
        ),
        "resource_limits": resource_limits(identity),
        "dataset": dataset_record(identity),
        "generator": {
            "id": generator.get("id", "OWL2Bench-generator"),
            "revision": generator.get("revision", "unknown"),
            "seed": generator.get("seed", 1),
            "configuration_id": generator.get("configuration_id", "OWL2Bench-generator-config"),
            "configuration_sha256": generator.get("configuration_sha256", ""),
        },
        "profile_context": {"id": context["id"], "directory": context["directory"]},
    }
    if profile in {"QL", "RL"}:
        ruleset = identity.get("ruleset")
        if not isinstance(ruleset, dict):
            raise AssemblyError(f"{profile} ruleset identity is missing")
        record["ruleset"] = {
            field: ruleset.get(field)
            for field in ("id", "version", "revision", "artifact_digest", "conformance_boundary")
        }
    return record


def materialization_record(profile: str, path: Path | None) -> dict[str, Any]:
    if path is None:
        return {
            "profile": profile,
            "status": "not_available",
            "reason": "no profile materialization manifest was supplied",
        }
    document = load_json(path, f"{profile} materialization manifest")
    if forbidden_paths(document):
        raise AssemblyError(
            f"{profile} materialization manifest contains forbidden answer/oracle fields: {forbidden_paths(document)}"
        )
    profiles = document.get("profiles")
    if not isinstance(profiles, dict) or not isinstance(profiles.get(profile), dict):
        raise AssemblyError(f"{profile} materialization manifest has no {profile} profile record")
    record = dict(profiles[profile])
    if record.get("profile") not in {None, profile}:
        raise AssemblyError(f"{profile} materialization record profile mismatch")
    record["profile"] = profile
    return record


def enrich_materialization_record(
    record: dict[str, Any], identity: dict[str, Any] | None
) -> dict[str, Any]:
    """Retain full runtime identity when a producer emitted only graph fields."""

    if identity is None:
        return record
    enriched = dict(record)
    for field in (
        "exporter",
        "query_backend",
        "runtime",
        "credentials",
        "license",
        "dataset",
        "generator",
        "generator_configuration",
        "profile_tbox",
        "profile_abox",
        "dataset_artifact",
        "implementation",
        "query_workload",
    ):
        if field not in enriched and field in identity:
            enriched[field] = identity[field]
    if "component" not in enriched and "component" in identity:
        enriched["component"] = identity["component"]
    for section in ("component", "configuration", "profile_context", "resource_limits"):
        current = enriched.get(section)
        source = identity.get(section)
        if isinstance(current, dict) and isinstance(source, dict):
            merged = dict(current)
            for key, value in source.items():
                if key not in merged or merged[key] is None or merged[key] == "":
                    merged[key] = value
            enriched[section] = merged
    return enriched


def graph_validation(
    profile: str,
    record: dict[str, Any],
    materialization_manifest: Path,
    output_dir: Path,
) -> dict[str, Any]:
    """Run the independent graph validator for every declared complete graph."""

    declared_status = record.get("status")
    if declared_status != "complete":
        return {
            "profile": profile,
            "status": declared_status or "not_available",
            "reasons": ["materialization_not_declared_complete"],
        }
    source = record.get("source_ontology", {})
    candidate = record.get("candidate_graph", {})
    graph = record.get("output_graph", {})
    paths = {
        "source": source.get("path") if isinstance(source, dict) else None,
        "candidate": candidate.get("path") if isinstance(candidate, dict) else None,
        "graph": graph.get("path") if isinstance(graph, dict) else None,
    }
    if not all(nonempty(value) for value in paths.values()):
        return {
            "profile": profile,
            "status": "not_available",
            "reasons": ["graph_validation_inputs_missing"],
        }
    validation_path = output_dir / f"graph-validation-{profile.lower()}.json"
    try:
        validation_path.unlink()
    except FileNotFoundError:
        pass
    except OSError as error:
        return {
            "profile": profile,
            "status": "failed",
            "reasons": [f"graph_validator_output_reset_failed:{error}"],
        }
    command = [
        sys.executable,
        str(GRAPH_VALIDATOR),
        "validate",
        "--profile",
        profile,
        "--source",
        str(paths["source"]),
        "--candidate-graph",
        str(paths["candidate"]),
        "--graph",
        str(paths["graph"]),
        "--manifest",
        str(materialization_manifest),
        "--output",
        str(validation_path),
    ]
    try:
        result = subprocess.run(command, check=False)
    except OSError as error:
        return {
            "profile": profile,
            "status": "failed",
            "reasons": [f"graph_validator_launch_failed:{error}"],
        }
    if not validation_path.is_file():
        return {
            "profile": profile,
            "status": "failed",
            "reasons": ["graph_validator_did_not_write_output"],
        }
    validation = load_json(validation_path, f"{profile} graph validation")
    validation_status = validation.get("status", "failed")
    validation_reasons = list(validation.get("reasons", []))
    if result.returncode != 0:
        if validation_status == "complete":
            validation_status = "failed"
        validation_reasons.append(f"graph_validator_exit:{result.returncode}")
    return {
        "profile": profile,
        "status": validation_status,
        "reasons": validation_reasons,
        "validation": {
            "path": str(validation_path),
            "sha256": digest(validation_path),
        },
        "returncode": result.returncode,
    }


def combined_status(base_status: str, graph_validations: dict[str, dict[str, Any]]) -> str:
    if base_status != "complete":
        return base_status
    statuses = [record.get("status", "failed") for record in graph_validations.values()]
    for status in ("failed", "timeout", "not_available", "unsupported"):
        if status in statuses:
            return status
    return "complete"


def assemble(args: argparse.Namespace) -> int:
    dataset_manifest = load_json(args.dataset_manifest, "dataset manifest")
    if dataset_manifest.get("status") != "complete":
        raise AssemblyError(
            f"dataset manifest is not complete: {dataset_manifest.get('status')}"
        )
    dataset_validation = validate_frozen_dataset(dataset_manifest)
    if dataset_validation["status"] != "complete":
        raise AssemblyError(
            "dataset manifest provenance is incomplete: "
            + ", ".join(dataset_validation["errors"])
        )
    identity_paths = bindings(args.profile_identity, "profile identity")
    materialization_paths = bindings(args.profile_materialization, "profile materialization")
    identities = {
        profile: load_json(path, f"{profile} profile identity")
        for profile, path in identity_paths.items()
    }
    component_profiles: dict[str, dict[str, Any]] = {}
    materialization_profiles: dict[str, dict[str, Any]] = {}
    identity_validations: dict[str, dict[str, Any]] = {}
    for profile in PROFILES:
        identity = identities.get(profile)
        materialization_profiles[profile] = materialization_record(
            profile, materialization_paths.get(profile)
        )
        materialization_profiles[profile] = enrich_materialization_record(
            materialization_profiles[profile], identity
        )
        materialization = materialization_profiles[profile]
        if identity is None:
            component_profiles[profile] = {
                "status": "not_available",
                "reason": "profile identity was not supplied",
            }
        else:
            identity_errors = frozen_identity_errors(profile, identity, dataset_manifest)
            identity_validations[profile] = {
                "status": "complete" if not identity_errors else "failed",
                "errors": identity_errors,
            }
            try:
                component_profiles[profile] = profile_component_record(
                    profile, identity, materialization
                )
            except AssemblyError as error:
                component_profiles[profile] = {
                    "status": "failed",
                    "reason": str(error),
                }
            if identity_errors:
                component_profiles[profile]["provenance_errors"] = identity_errors

    component_manifest = {
        "schema_version": 1,
        "manifest_id": "owl2bench-real-profile-component-stack",
        "matrix": {
            "expansion": "query_profile",
            "expected_query_count": 60,
            "expected_profile_case_counts": EXPECTED_COUNTS,
        },
        "profiles": component_profiles,
    }
    materialization_manifest = {
        "schema_version": 1,
        "manifest_id": "owl2bench-real-profile-materialization-stack",
        "profiles": materialization_profiles,
    }
    component_path = args.output_dir / "component.json"
    materialization_path = args.output_dir / "materialization.json"
    validation_path = args.output_dir / "stack-validation.json"
    write_json(component_path, component_manifest)
    write_json(materialization_path, materialization_manifest)
    graph_validations = {
        profile: graph_validation(
            profile,
            materialization_profiles[profile],
            materialization_path,
            args.output_dir,
        )
        for profile in PROFILES
    }
    command = [
        sys.executable,
        str(VALIDATOR),
        "verify-stack",
        "--component-manifest",
        str(component_path),
        "--materialization-manifest",
        str(materialization_path),
        "--query-manifest",
        str(args.query_manifest),
        "--output",
        str(validation_path),
    ]
    result = subprocess.run(command, check=False)
    if not validation_path.is_file():
        raise AssemblyError("materialization stack validator did not write a validation artifact")
    validation = load_json(validation_path, "stack validation")
    validation["graph_validations"] = graph_validations
    validation["dataset_validation"] = dataset_validation
    validation["frozen_identity_validations"] = {
        profile: identity_validations.get(
            profile,
            {"status": "not_available", "errors": ["profile identity was not supplied"]},
        )
        for profile in PROFILES
    }
    for profile in PROFILES:
        profile_validation = validation.get("profiles", {}).get(profile)
        if not isinstance(profile_validation, dict):
            continue
        graph_validation_record = graph_validations[profile]
        profile_validation["graph_validation"] = graph_validation_record
        if (
            profile_validation.get("status") == "complete"
            and graph_validation_record.get("status") != "complete"
        ):
            profile_validation["status"] = graph_validation_record.get("status", "failed")
            reasons = profile_validation.setdefault("cross_contract_errors", [])
            for reason in graph_validation_record.get("reasons", []):
                marker = f"graph_validation:{reason}"
                if marker not in reasons:
                    reasons.append(marker)
        identity_validation_record = validation["frozen_identity_validations"][profile]
        if identity_validation_record.get("status") != "complete":
            identity_status = identity_validation_record.get("status")
            profile_validation["status"] = (
                "not_available" if identity_status == "not_available" else "failed"
            )
            reasons = profile_validation.setdefault("cross_contract_errors", [])
            for reason in identity_validation_record.get("errors", []):
                marker = f"frozen_identity:{reason}"
                if marker not in reasons:
                    reasons.append(marker)
    validation["status"] = combined_status(validation.get("status", "failed"), graph_validations)
    identity_statuses = {
        record.get("status") for record in validation["frozen_identity_validations"].values()
    }
    if "failed" in identity_statuses:
        validation["status"] = "failed"
    elif "not_available" in identity_statuses and validation.get("status") == "complete":
        validation["status"] = "not_available"
    write_json(validation_path, validation)
    stack = {
        "schema_version": 1,
        "stack_id": "owl2bench-real-profile-materialization-stack",
        "assembly_status": "complete",
        "status": validation.get("status", "failed"),
        "dataset_manifest": {"path": str(args.dataset_manifest), "sha256": digest(args.dataset_manifest)},
        "dataset_validation": dataset_validation,
        "component_manifest": {"path": str(component_path), "sha256": digest(component_path)},
        "materialization_manifest": {"path": str(materialization_path), "sha256": digest(materialization_path)},
        "validation": {"path": str(validation_path), "sha256": digest(validation_path)},
        "profiles": {
            profile: {
                "component_status": validation.get("profiles", {}).get(profile, {}).get("component", {}).get("status", "not_available"),
                "materialization_status": validation.get("profiles", {}).get(profile, {}).get("materialization", {}).get("status", "not_available"),
                "status": validation.get("profiles", {}).get(profile, {}).get("status", "not_available"),
                "graph_validation_status": graph_validations[profile]["status"],
                "graph_validation_reasons": graph_validations[profile].get("reasons", []),
            }
            for profile in PROFILES
        },
        "answer_key_policy": "not_permitted",
    }
    write_json(args.output_dir / "stack.json", stack)
    return 0 if result.returncode == 0 and validation.get("status") == "complete" else 1


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    subparsers = parser.add_subparsers(dest="command", required=True)
    assemble_parser = subparsers.add_parser("assemble")
    assemble_parser.add_argument("--dataset-manifest", type=Path, required=True)
    assemble_parser.add_argument("--query-manifest", type=Path, required=True)
    assemble_parser.add_argument("--output-dir", type=Path, required=True)
    assemble_parser.add_argument("--profile-identity", action="append", default=[])
    assemble_parser.add_argument("--profile-materialization", action="append", default=[])
    args = parser.parse_args(argv)
    try:
        if args.command == "assemble":
            args.output_dir = args.output_dir.expanduser().resolve()
            args.output_dir.mkdir(parents=True, exist_ok=True)
            return assemble(args)
    except (AssemblyError, OSError, subprocess.SubprocessError) as error:
        print(f"profile stack assembly failed: {error}", file=sys.stderr)
        return 1
    return 2


if __name__ == "__main__":
    raise SystemExit(main())
