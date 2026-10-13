"""Openllet QL immutable variant revision and release-control validation."""

from __future__ import annotations

import hashlib
import json
import re
from pathlib import Path
from typing import Any


EXPECTED_QUERY_COUNT = 10
SHA256_PATTERN = re.compile(r"^sha256:[0-9a-f]{64}$")
REVISION_ID_PATTERN = re.compile(r"^openllet-ql/revision-[1-9][0-9]*$")
SCRIPT_DIR = Path(__file__).resolve().parent
REVISION_LOCK_PATH = SCRIPT_DIR / "openllet_ql_variant_revision_lock.json"
ACCEPTANCE_LOCK_PATH = SCRIPT_DIR / "openllet_ql_acceptance_lock.json"
FORBIDDEN_REVIEW_KEYS = {
    "legal_document",
    "legal_documents",
    "legal_text",
    "license_text",
    "secret",
    "secrets",
    "token",
    "password",
    "private_key",
}


def is_digest(value: Any) -> bool:
    return isinstance(value, str) and SHA256_PATTERN.fullmatch(value) is not None


def normalized_sha256(value: Any) -> str:
    if not isinstance(value, str):
        return ""
    return value.strip().removeprefix("sha256:").lower()


def canonical_digest(value: Any) -> str:
    encoded = json.dumps(value, sort_keys=True, separators=(",", ":")).encode("utf-8")
    return "sha256:" + hashlib.sha256(encoded).hexdigest()


def file_digest(path: Path) -> str:
    hasher = hashlib.sha256()
    with path.open("rb") as handle:
        for block in iter(lambda: handle.read(1024 * 1024), b""):
            hasher.update(block)
    return "sha256:" + hasher.hexdigest()


def object_at(value: dict[str, Any] | None, key: str) -> dict[str, Any]:
    if not isinstance(value, dict):
        return {}
    item = value.get(key)
    return item if isinstance(item, dict) else {}


def list_at(value: dict[str, Any], key: str) -> list[Any]:
    item = value.get(key)
    return item if isinstance(item, list) else []


def variant_categories(identity: dict[str, Any]) -> dict[str, Any]:
    component = object_at(identity, "component")
    runtime = object_at(identity, "runtime")
    inputs = object_at(identity, "inputs")
    return {
        "engine": {
            "implementation": component.get("implementation"),
            "openllet_version": component.get("openllet_version"),
            "owlapi_version": component.get("owlapi_version"),
            "runtime_revision": runtime.get("variant_revision"),
        },
        "base_image": object_at(runtime, "base_image"),
        "image": object_at(runtime, "image"),
        "dependencies": (
            runtime.get("dependencies") if isinstance(runtime.get("dependencies"), list) else []
        ),
        "classpath": object_at(identity, "classpath"),
        "exporter": object_at(inputs, "exporter"),
        "configuration": object_at(inputs, "configuration"),
        "resource": object_at(identity, "resource_limits"),
        "graph_contract": object_at(identity, "graph_contract"),
    }


def declared_variant(identity: dict[str, Any], category_sha256: str) -> tuple[dict[str, Any], list[str]]:
    declaration = object_at(identity, "variant_control")
    reasons: list[str] = []
    revision_id = declaration.get("revision_id")
    declared_sha = declaration.get("category_sha256")
    if not declaration:
        reasons.append("variant_control.declaration_missing")
    if not isinstance(revision_id, str) or not revision_id.strip():
        reasons.append("variant_control.revision_id_invalid")
    elif REVISION_ID_PATTERN.fullmatch(revision_id) is None:
        reasons.append("variant_control.revision_id_invalid")
    if not is_digest(declared_sha):
        reasons.append("variant_control.category_sha256_invalid")
    elif declared_sha != category_sha256:
        reasons.append("variant_control.category_sha256_mismatch")
    revision_lock, revision_lock_errors = read_json_file(REVISION_LOCK_PATH)
    if revision_lock_errors or revision_lock is None:
        reasons.append("variant_control.revision_lock_unavailable")
    else:
        accepted = object_at(revision_lock, "accepted_revisions")
        accepted_sha = accepted.get(revision_id)
        if accepted_sha is None:
            reasons.append("variant_control.revision_id_unregistered")
        elif accepted_sha != category_sha256:
            reasons.append("variant_control.revision_id_reused")
    return {
        "revision_id": revision_id,
        "category_sha256": declared_sha,
        "computed_category_sha256": category_sha256,
    }, sorted(set(reasons))


def invariant_reasons(identity: dict[str, Any], categories: dict[str, Any]) -> list[str]:
    runtime = object_at(identity, "runtime")
    archive = object_at(runtime, "archive")
    license_identity = object_at(identity, "license")
    reasons: list[str] = []
    required = {
        "engine": ("implementation", "openllet_version", "owlapi_version", "runtime_revision"),
        "base_image": ("reference", "digest"),
        "image": ("reference", "digest", "platform"),
        "classpath": ("path", "lock_sha256", "jar_count", "drift_policy"),
        "exporter": ("path", "sha256"),
        "configuration": ("path", "sha256"),
        "resource": ("timeout_seconds", "memory", "threads", "cpu", "disk", "network", "variant"),
        "graph_contract": (
            "profile",
            "execution_variant",
            "materialization_revision",
            "source_domain",
            "source_included",
            "finite_named_entailments",
            "anonymous_witness_policy",
            "query_count",
        ),
    }
    for category, fields in required.items():
        if not categories.get(category):
            reasons.append(f"{category}.missing")
        for field in fields:
            value = object_at(categories, category).get(field)
            if value is None or value == "":
                reasons.append(f"{category}.{field}_missing")
    dependencies = categories.get("dependencies")
    if not isinstance(dependencies, list) or not dependencies:
        reasons.append("dependencies.missing")
    else:
        names: set[str] = set()
        for index, dependency in enumerate(dependencies):
            if not isinstance(dependency, dict):
                reasons.append(f"dependencies[{index}].invalid")
                continue
            name = dependency.get("name")
            if not isinstance(name, str) or not name:
                reasons.append(f"dependencies[{index}].name_missing")
            elif name in names:
                reasons.append(f"dependencies[{index}].duplicate_name")
            else:
                names.add(name)
            if not isinstance(dependency.get("version"), str) or not dependency["version"]:
                reasons.append(f"dependencies[{index}].version_missing")
            if not is_digest(dependency.get("digest")):
                reasons.append(f"dependencies[{index}].digest_invalid")
    for category in ("base_image", "image"):
        if not is_digest(object_at(categories, category).get("digest")):
            reasons.append(f"{category}.digest_invalid")
    for category in ("classpath", "exporter", "configuration"):
        field = "lock_sha256" if category == "classpath" else "sha256"
        if not is_digest(object_at(categories, category).get(field)):
            reasons.append(f"{category}.{field}_invalid")
    for category in ("base_image", "image"):
        reference = str(object_at(categories, category).get("reference") or "")
        digest = object_at(categories, category).get("digest")
        if "@sha256:" not in reference:
            reasons.append(f"{category}.reference_mutable")
        elif reference.rsplit("@", 1)[-1] != digest:
            reasons.append(f"{category}.reference_digest_mismatch")
    if "benchmark-only" not in str(object_at(identity, "component").get("boundary", "")):
        reasons.append("runtime_control.benchmark_only_failed")
    if (
        runtime.get("execution_network") != "none"
        or runtime.get("materialization_network") != "none"
        or object_at(identity, "resource_limits").get("network") != "disabled"
    ):
        reasons.append("runtime_control.network_disabled_failed")
    if runtime.get("service_endpoint") != "none":
        reasons.append("runtime_control.service_endpoint_absent_failed")
    if archive.get("committed") is not False or archive.get("published") is not False:
        reasons.append("runtime_control.archive_absent_failed")
    if license_identity.get("distribution_review_required") is not True:
        reasons.append("agpl.distribution_review_contract_missing")
    return sorted(set(reasons))


def read_json_file(path: Path) -> tuple[dict[str, Any] | None, list[str]]:
    try:
        value = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, UnicodeDecodeError, json.JSONDecodeError):
        return None, ["json_malformed"]
    if not isinstance(value, dict):
        return None, ["json_not_object"]
    return value, []


def resolve_manifest_path(manifest_path: Path, value: Any) -> Path | None:
    if not isinstance(value, str) or not value.strip():
        return None
    path = Path(value)
    if not path.is_absolute():
        path = manifest_path.parent / path
    return path.resolve()


def load_manifest_artifact(
    manifest_path: Path,
    manifest: dict[str, Any],
    key: str,
) -> tuple[dict[str, Any] | None, dict[str, Any], list[str]]:
    record = object_at(manifest, key)
    path = resolve_manifest_path(manifest_path, record.get("path"))
    declared_sha = record.get("sha256")
    reasons: list[str] = []
    if path is None:
        return None, {}, [f"{key}.path_missing"]
    artifact_ref = {"path": str(path)}
    if not path.is_file():
        reasons.append(f"{key}.file_missing")
        return None, artifact_ref, reasons
    actual_sha = file_digest(path)
    artifact_ref["sha256"] = actual_sha
    if not is_digest(declared_sha):
        reasons.append(f"{key}.sha256_invalid")
    elif declared_sha != actual_sha:
        reasons.append(f"{key}.sha256_mismatch")
    artifact, artifact_errors = read_json_file(path)
    reasons.extend(f"{key}.{error}" for error in artifact_errors)
    return artifact, artifact_ref, reasons


def load_manifest_text_artifact(
    manifest_path: Path,
    manifest: dict[str, Any],
    key: str,
) -> tuple[str | None, dict[str, Any], list[str]]:
    record = object_at(manifest, key)
    path = resolve_manifest_path(manifest_path, record.get("path"))
    declared_sha = record.get("sha256")
    reasons: list[str] = []
    if path is None:
        return None, {}, [f"{key}.path_missing"]
    artifact_ref = {"path": str(path)}
    if not path.is_file():
        return None, artifact_ref, [f"{key}.file_missing"]
    actual_sha = file_digest(path)
    artifact_ref["sha256"] = actual_sha
    if not is_digest(declared_sha):
        reasons.append(f"{key}.sha256_invalid")
    elif declared_sha != actual_sha:
        reasons.append(f"{key}.sha256_mismatch")
    try:
        content = path.read_text(encoding="utf-8")
    except (OSError, UnicodeDecodeError):
        reasons.append(f"{key}.text_unavailable")
        return None, artifact_ref, reasons
    return content, artifact_ref, reasons


def validate_report_markdown(
    content: str | None,
    revision_id: str | None,
    category_sha256: str,
    projection: dict[str, str],
) -> list[str]:
    if content is None:
        return []
    required_markers = {
        "title_missing": "# OWL2Bench Query Report",
        "revision_mismatch": f"- Variant revision: `{revision_id}`",
        "category_sha256_mismatch": f"- Variant category SHA: `{category_sha256}`",
        "implementation_completeness_mismatch": (
            "- Implementation completeness: "
            f"**{projection['implementation_completeness']}**"
        ),
        "release_readiness_mismatch": (
            f"- Release readiness: **{projection['release_readiness']}**"
        ),
        "execution_availability_mismatch": (
            f"- Execution availability: **{projection['execution_availability']}**"
        ),
        "semantic_correctness_mismatch": (
            f"- Semantic correctness: **{projection['semantic_correctness']}**"
        ),
        "official_ql_status_mismatch": (
            '- Variant official QL status counts: {"oracle_unavailable":10}'
        ),
    }
    return [
        f"report_markdown.{reason}"
        for reason, marker in required_markers.items()
        if marker not in content
    ]


def validate_diagnostic(artifact: dict[str, Any] | None) -> list[str]:
    if artifact is None:
        return []
    reasons: list[str] = []
    if artifact.get("status") != "complete":
        reasons.append("diagnostic.status_not_complete")
    obligations = object_at(artifact, "independent_obligations")
    counts = object_at(obligations, "expected_inferred_assertion_counts")
    if obligations.get("expected_inferred_count", 0) < 2:
        reasons.append("diagnostic.expected_inferred_count_below_2")
    if counts.get("class", 0) < 1:
        reasons.append("diagnostic.required_class_obligation_missing")
    if counts.get("object_property", 0) < 1:
        reasons.append("diagnostic.required_object_obligation_missing")
    observations = object_at(artifact, "observations")
    if object_at(observations, "class").get("status") != "observed":
        reasons.append("diagnostic.class_observation_missing")
    if object_at(observations, "object_property").get("status") != "observed":
        reasons.append("diagnostic.object_observation_missing")
    return reasons


def validate_runtime_attestation(
    artifact: dict[str, Any] | None,
    categories: dict[str, Any],
) -> list[str]:
    if artifact is None:
        return []
    reasons: list[str] = []
    component = object_at(artifact, "component")
    engine = object_at(categories, "engine")
    runtime = object_at(artifact, "runtime")
    inputs = object_at(artifact, "inputs")
    if artifact.get("status") != "available":
        reasons.append("runtime_attestation.status_not_available")
    engine_checks = {
        "implementation": component.get("implementation"),
        "openllet_version": component.get("openllet_version"),
        "owlapi_version": component.get("owlapi_version"),
        "runtime_revision": runtime.get("variant_revision"),
    }
    if engine_checks != engine:
        reasons.append("runtime_attestation.engine_mismatch")
    if object_at(runtime, "base_image") != object_at(categories, "base_image"):
        reasons.append("runtime_attestation.base_image_mismatch")
    expected_image = object_at(categories, "image")
    actual_image = object_at(runtime, "image")
    if {key: actual_image.get(key) for key in expected_image} != expected_image:
        reasons.append("runtime_attestation.image_mismatch")
    if runtime.get("dependencies") != categories.get("dependencies"):
        reasons.append("runtime_attestation.dependencies_mismatch")
    expected_classpath = object_at(categories, "classpath")
    actual_classpath = object_at(artifact, "classpath")
    if {key: actual_classpath.get(key) for key in expected_classpath} != expected_classpath:
        reasons.append("runtime_attestation.classpath_mismatch")
    if object_at(inputs, "exporter") != object_at(categories, "exporter"):
        reasons.append("runtime_attestation.exporter_mismatch")
    if object_at(inputs, "configuration") != object_at(categories, "configuration"):
        reasons.append("runtime_attestation.configuration_mismatch")
    if object_at(artifact, "resource_limits") != object_at(categories, "resource"):
        reasons.append("runtime_attestation.resource_mismatch")
    if (
        runtime.get("execution_network") != "none"
        or runtime.get("materialization_network") != "none"
        or runtime.get("service_endpoint") != "none"
    ):
        reasons.append("runtime_attestation.runtime_controls_mismatch")
    archive = object_at(runtime, "archive")
    if archive.get("committed") is not False or archive.get("published") is not False:
        reasons.append("runtime_attestation.archive_policy_mismatch")
    return reasons


def validate_component_manifest(
    artifact: dict[str, Any] | None,
    categories: dict[str, Any],
    runtime_attestation_sha256: str,
) -> list[str]:
    if artifact is None:
        return []
    reasons: list[str] = []
    profile = object_at(object_at(artifact, "profiles"), "QL")
    runtime = object_at(profile, "runtime")
    engine = object_at(object_at(profile, "components"), "engine")
    expected_engine = object_at(categories, "engine")
    if profile.get("execution_variant") != "openllet-ql":
        reasons.append("component_manifest.variant_not_openllet_ql")
    if engine.get("name") != "Openllet" or engine.get("version") != expected_engine.get(
        "openllet_version"
    ):
        reasons.append("component_manifest.engine_mismatch")
    if runtime.get("variant_revision") != expected_engine.get("runtime_revision"):
        reasons.append("component_manifest.runtime_revision_mismatch")
    if object_at(runtime, "base_image") != object_at(categories, "base_image"):
        reasons.append("component_manifest.base_image_mismatch")
    expected_image = object_at(categories, "image")
    actual_image = object_at(runtime, "image")
    if {key: actual_image.get(key) for key in expected_image} != expected_image:
        reasons.append("component_manifest.image_mismatch")
    if runtime.get("dependencies") != categories.get("dependencies"):
        reasons.append("component_manifest.dependencies_mismatch")
    runtime_classpath = object_at(runtime, "classpath")
    expected_classpath = object_at(categories, "classpath")
    if (
        runtime_classpath.get("lock_sha256") != expected_classpath.get("lock_sha256")
        or runtime_classpath.get("jar_count") != expected_classpath.get("jar_count")
    ):
        reasons.append("component_manifest.classpath_mismatch")
    if object_at(profile, "configuration").get("digest") != object_at(
        categories, "configuration"
    ).get("sha256"):
        reasons.append("component_manifest.configuration_mismatch")
    ruleset = object_at(profile, "ruleset")
    if (
        ruleset.get("revision") != expected_engine.get("runtime_revision")
        or ruleset.get("artifact_digest") != expected_image.get("digest")
    ):
        reasons.append("component_manifest.ruleset_mismatch")
    if normalized_sha256(object_at(runtime, "attestation").get("sha256")) != normalized_sha256(
        runtime_attestation_sha256
    ):
        reasons.append("component_manifest.runtime_attestation_mismatch")
    return reasons


def validate_materialization(
    artifact: dict[str, Any] | None,
    diagnostic: dict[str, Any] | None,
    component_manifest_sha256: str,
    runtime_attestation_sha256: str,
) -> list[str]:
    if artifact is None:
        return []
    reasons: list[str] = []
    if artifact.get("status") != "complete":
        reasons.append("materialization.status_not_complete")
    if artifact.get("execution_variant") != "openllet-ql":
        reasons.append("materialization.variant_not_openllet_ql")
    official = object_at(artifact, "official")
    if official.get("execution_status") != "complete":
        reasons.append("materialization.official_execution_not_complete")
    if official.get("status") != "oracle_unavailable":
        reasons.append("materialization.result_not_oracle_unavailable")
    if not is_digest(object_at(official, "canonical_graph").get("sha256")):
        reasons.append("materialization.graph_digest_invalid")
    if object_at(artifact, "diagnostic").get("status") != "complete":
        reasons.append("materialization.diagnostic_not_complete")
    if diagnostic is not None and object_at(artifact, "diagnostic") != diagnostic:
        reasons.append("materialization.diagnostic_evidence_mismatch")
    if normalized_sha256(object_at(artifact, "component_manifest").get("sha256")) != normalized_sha256(
        component_manifest_sha256
    ):
        reasons.append("materialization.component_manifest_mismatch")
    if normalized_sha256(object_at(artifact, "runtime_attestation").get("sha256")) != normalized_sha256(
        runtime_attestation_sha256
    ):
        reasons.append("materialization.runtime_attestation_mismatch")
    return reasons


def validate_result_contract(artifact: dict[str, Any] | None) -> list[str]:
    if artifact is None:
        return []
    reasons: list[str] = []
    if artifact.get("status") != "valid":
        reasons.append("result_contract.not_valid")
    if artifact.get("errors") != []:
        reasons.append("result_contract.errors_not_empty")
    if artifact.get("validator") != "owl2bench-profile-result-contract/1":
        reasons.append("result_contract.validator_mismatch")
    if artifact.get("result_count") != 60:
        reasons.append("result_contract.result_count_not_60")
    if object_at(artifact, "result_status_counts") != {
        "not_available": 50,
        "oracle_unavailable": EXPECTED_QUERY_COUNT,
    }:
        reasons.append("result_contract.status_counts_mismatch")
    return reasons


def validate_result_contract_lineage(
    artifact: dict[str, Any] | None,
    contract_ref: dict[str, Any],
    ql_refs: list[dict[str, Any]],
    report_ref: dict[str, Any],
) -> list[str]:
    if artifact is None:
        return []
    run_dir_value = artifact.get("run_dir")
    if not isinstance(run_dir_value, str) or not run_dir_value.strip():
        return ["result_contract.run_dir_missing"]
    run_dir = Path(run_dir_value).expanduser().resolve()
    reasons: list[str] = []
    if not run_dir.is_dir():
        reasons.append("result_contract.run_dir_missing")
    contract_path_value = contract_ref.get("path")
    if not isinstance(contract_path_value, str) or not Path(
        contract_path_value
    ).resolve().is_relative_to(run_dir):
        reasons.append("result_contract.path_outside_run_dir")
    for index, ref in enumerate(ql_refs):
        path_value = ref.get("path")
        if not isinstance(path_value, str) or Path(path_value).resolve().parent != run_dir:
            reasons.append(f"ql_results[{index}].outside_result_contract_run_dir")
    report_path_value = report_ref.get("path")
    if isinstance(report_path_value, str) and Path(report_path_value).resolve() != (
        run_dir / "report.md"
    ):
        reasons.append("report_markdown.outside_result_contract_run_dir")
    return reasons


def validate_ql_result(
    artifact: dict[str, Any] | None,
    index: int,
    expected_graph_sha256: str,
    expected_image_digest: str,
    expected_query_sha256: str,
    expected_source_corpus: str,
    expected_source_revision: str,
    categories: dict[str, Any],
) -> list[str]:
    if artifact is None:
        return []
    reasons: list[str] = []
    prefix = f"ql_results[{index}]"
    if artifact.get("query_profile") != "QL":
        reasons.append(f"{prefix}.profile_not_ql")
    if artifact.get("execution_status") != "success":
        reasons.append(f"{prefix}.execution_not_success")
    if artifact.get("result_status") != "oracle_unavailable":
        reasons.append(f"{prefix}.result_not_oracle_unavailable")
    if normalized_sha256(artifact.get("source_query_sha256")) != normalized_sha256(
        expected_query_sha256
    ):
        reasons.append(f"{prefix}.source_query_sha256_mismatch")
    if artifact.get("source_corpus") != expected_source_corpus:
        reasons.append(f"{prefix}.source_corpus_mismatch")
    if artifact.get("source_revision") != expected_source_revision:
        reasons.append(f"{prefix}.source_revision_mismatch")
    provenance = object_at(artifact, "engine_provenance")
    if provenance.get("adapter") != "openllet-ql":
        reasons.append(f"{prefix}.engine_not_openllet")
    component = object_at(artifact, "component_manifest")
    materialization = object_at(artifact, "materialization")
    if component.get("execution_variant") != "openllet-ql":
        reasons.append(f"{prefix}.component_variant_not_openllet")
    expected_engine = object_at(categories, "engine")
    engine = object_at(object_at(component, "components"), "engine")
    if (
        engine.get("name") != "Openllet"
        or engine.get("version") != expected_engine.get("openllet_version")
    ):
        reasons.append(f"{prefix}.component_engine_not_openllet")
    runtime = object_at(component, "runtime")
    runtime_image = object_at(runtime, "image")
    if (
        runtime_image.get("digest") != expected_image_digest
        or runtime_image.get("reference") != object_at(categories, "image").get("reference")
    ):
        reasons.append(f"{prefix}.component_image_digest_mismatch")
    if runtime.get("dependencies") != categories.get("dependencies"):
        reasons.append(f"{prefix}.component_dependencies_mismatch")
    if object_at(component, "configuration").get("digest") != object_at(
        categories, "configuration"
    ).get("sha256"):
        reasons.append(f"{prefix}.component_configuration_mismatch")
    ruleset = object_at(component, "ruleset")
    if (
        ruleset.get("revision") != expected_engine.get("runtime_revision")
        or ruleset.get("artifact_digest") != expected_image_digest
    ):
        reasons.append(f"{prefix}.component_ruleset_mismatch")
    if materialization.get("execution_variant") != "openllet-ql":
        reasons.append(f"{prefix}.materialization_variant_not_openllet")
    if object_at(materialization, "output_graph").get("sha256") != expected_graph_sha256:
        reasons.append(f"{prefix}.graph_digest_mismatch")
    if object_at(artifact, "canonicalization").get("status") != "available":
        reasons.append(f"{prefix}.canonicalization_not_available")
    if not isinstance(artifact.get("query_time_ms"), (int, float)) or artifact["query_time_ms"] < 0:
        reasons.append(f"{prefix}.query_time_invalid")
    return reasons


def validate_evidence_manifest(
    manifest_path: Path | None,
    revision_id: str | None,
    category_sha256: str,
    categories: dict[str, Any],
) -> dict[str, Any]:
    if manifest_path is None:
        return {
            "status": "blocked",
            "core_status": "blocked",
            "report_status": "blocked",
            "selection_blocker": True,
            "reasons": ["evidence_manifest.missing"],
        }
    if not manifest_path.is_file():
        return {
            "status": "not_available",
            "core_status": "not_available",
            "report_status": "not_available",
            "selection_blocker": True,
            "reasons": ["evidence_manifest.file_missing"],
        }
    manifest, errors = read_json_file(manifest_path)
    if errors or manifest is None:
        return {
            "status": "not_available",
            "core_status": "not_available",
            "report_status": "not_available",
            "selection_blocker": True,
            "reasons": ["evidence_manifest.malformed"],
        }

    reasons: list[str] = []
    expected_manifest_fields = {
        "revision_id",
        "category_sha256",
        "diagnostic",
        "runtime_attestation",
        "component_manifest",
        "materialization",
        "result_contract",
        "report_markdown",
        "ql_results",
    }
    reasons.extend(
        f"evidence_manifest.unexpected_field:{field}"
        for field in sorted(set(manifest) - expected_manifest_fields)
    )
    if manifest.get("revision_id") != revision_id:
        reasons.append("evidence_manifest.revision_id_mismatch")
    if manifest.get("category_sha256") != category_sha256:
        reasons.append("evidence_manifest.category_sha256_mismatch")

    artifacts: dict[str, Any] = {}
    loaded: dict[str, dict[str, Any] | None] = {}
    for key in (
        "diagnostic",
        "runtime_attestation",
        "component_manifest",
        "materialization",
        "result_contract",
    ):
        artifact, ref, artifact_reasons = load_manifest_artifact(manifest_path, manifest, key)
        artifacts[key] = ref
        loaded[key] = artifact
        reasons.extend(artifact_reasons)
    report_content, report_ref, report_reasons = load_manifest_text_artifact(
        manifest_path, manifest, "report_markdown"
    )
    artifacts["report_markdown"] = report_ref
    reasons.extend(validate_diagnostic(loaded.get("diagnostic")))
    runtime_attestation_sha256 = str(
        artifacts.get("runtime_attestation", {}).get("sha256") or ""
    )
    component_manifest_sha256 = str(
        artifacts.get("component_manifest", {}).get("sha256") or ""
    )
    reasons.extend(validate_runtime_attestation(loaded.get("runtime_attestation"), categories))
    reasons.extend(
        validate_component_manifest(
            loaded.get("component_manifest"),
            categories,
            runtime_attestation_sha256,
        )
    )
    reasons.extend(
        validate_materialization(
            loaded.get("materialization"),
            loaded.get("diagnostic"),
            component_manifest_sha256,
            runtime_attestation_sha256,
        )
    )
    reasons.extend(validate_result_contract(loaded.get("result_contract")))
    materialization_official = object_at(loaded.get("materialization"), "official")
    expected_graph_sha256 = object_at(materialization_official, "canonical_graph").get(
        "sha256"
    )
    expected_image_digest = object_at(categories, "image").get("digest")
    acceptance_lock, acceptance_errors = read_json_file(ACCEPTANCE_LOCK_PATH)
    if acceptance_errors or acceptance_lock is None:
        reasons.append("ql_results.acceptance_lock_unavailable")
        expected_queries: dict[str, str] = {}
        expected_source_corpus = ""
        expected_source_revision = ""
    else:
        workload = object_at(acceptance_lock, "query_workload")
        expected_queries = {
            str(item.get("id")): str(item.get("sha256"))
            for item in list_at(workload, "queries")
            if isinstance(item, dict)
        }
        expected_source_corpus = str(workload.get("source_corpus") or "")
        expected_source_revision = str(workload.get("source_revision") or "")
        if len(expected_queries) != EXPECTED_QUERY_COUNT:
            reasons.append("ql_results.acceptance_lock_query_count_invalid")

    ql_records = list_at(manifest, "ql_results")
    if len(ql_records) != EXPECTED_QUERY_COUNT:
        reasons.append("ql_results.count_not_10")
    seen: set[str] = set()
    ql_artifacts: list[dict[str, Any]] = []
    official_status_counts: dict[str, int] = {}
    official_status_evidence: list[dict[str, Any]] = []
    for index, record in enumerate(ql_records):
        artifact, ref, artifact_reasons = load_manifest_artifact(manifest_path, {"item": record}, "item")
        ql_artifacts.append(ref)
        reasons.extend(reason.replace("item.", f"ql_results[{index}].") for reason in artifact_reasons)
        if artifact is None:
            continue
        query_id = str(artifact.get("query_id") or "")
        key = query_id or str(artifact.get("case_id") or ref.get("path"))
        if key in seen:
            reasons.append("ql_results.duplicate")
        seen.add(key)
        if query_id not in expected_queries:
            reasons.append(f"ql_results[{index}].query_id_not_official")
        result_status = str(artifact.get("result_status") or "")
        official_status_counts[result_status] = official_status_counts.get(result_status, 0) + 1
        official_status_evidence.append(
            {
                "query_id": query_id,
                "source_query_sha256": normalized_sha256(
                    artifact.get("source_query_sha256")
                ),
                "source_corpus": artifact.get("source_corpus"),
                "source_revision": artifact.get("source_revision"),
                "execution_status": artifact.get("execution_status"),
                "result_status": result_status,
                "graph_sha256": object_at(
                    object_at(artifact, "materialization"), "output_graph"
                ).get("sha256"),
                "image_digest": object_at(
                    object_at(object_at(artifact, "component_manifest"), "runtime"),
                    "image",
                ).get("digest"),
            }
        )
        reasons.extend(
            validate_ql_result(
                artifact,
                index,
                str(expected_graph_sha256 or ""),
                str(expected_image_digest or ""),
                expected_queries.get(query_id, ""),
                expected_source_corpus,
                expected_source_revision,
                categories,
            )
        )
    if seen != set(expected_queries):
        reasons.append("ql_results.official_query_set_mismatch")
    if official_status_counts != {"oracle_unavailable": EXPECTED_QUERY_COUNT}:
        reasons.append("ql_results.official_status_counts_mismatch")
    artifacts["ql_results"] = ql_artifacts
    lineage_reasons = validate_result_contract_lineage(
        loaded.get("result_contract"),
        artifacts.get("result_contract", {}),
        ql_artifacts,
        report_ref,
    )
    reasons.extend(
        reason for reason in lineage_reasons if not reason.startswith("report_markdown.")
    )
    report_reasons.extend(
        reason for reason in lineage_reasons if reason.startswith("report_markdown.")
    )

    release_comparison = {
        "revision_id": revision_id,
        "category_sha256": category_sha256,
        "build_inputs_sha256": canonical_digest({
            key: categories[key]
            for key in (
                "base_image",
                "dependencies",
                "classpath",
                "exporter",
                "configuration",
            )
        }),
        "dependency_lock_sha256": object_at(categories, "classpath").get("lock_sha256"),
        "image_input_sha256": canonical_digest({
            "base_image": categories["base_image"],
            "image": categories["image"],
        }),
        "image_digest": object_at(categories, "image").get("digest"),
        "diagnostic_hash": artifacts.get("diagnostic", {}).get("sha256"),
        "official_status_sha256": canonical_digest(
            sorted(official_status_evidence, key=lambda item: str(item.get("query_id")))
        ),
        "official_status_counts": dict(sorted(official_status_counts.items())),
    }
    if isinstance(loaded.get("result_contract"), dict):
        raw_run_dir = loaded["result_contract"].get("run_dir")
    else:
        raw_run_dir = None
    run_lineage: dict[str, str] = {}
    if isinstance(raw_run_dir, str) and raw_run_dir.strip():
        resolved_run_dir = Path(raw_run_dir).expanduser().resolve()
        run_lineage = {
            "run_dir": str(resolved_run_dir),
            "report_path": str(resolved_run_dir / "report.md"),
        }
    all_reasons = sorted(set(reasons + report_reasons))
    return {
        "status": "complete" if not all_reasons else "failed",
        "core_status": "complete" if not reasons else "failed",
        "report_status": "complete" if not report_reasons else "failed",
        "selection_blocker": bool(all_reasons),
        "reasons": all_reasons,
        "manifest": {"path": str(manifest_path), "sha256": file_digest(manifest_path)},
        "artifacts": artifacts,
        "run_lineage": run_lineage,
        "release_comparison": release_comparison,
        "_report_content": report_content,
        "_report_reasons": report_reasons,
    }


def second_host_gate(
    primary: dict[str, Any],
    second_host_manifest_path: Path | None,
) -> dict[str, Any]:
    derived = object_at(primary, "release_comparison")
    required = (
        "revision_id",
        "category_sha256",
        "build_inputs_sha256",
        "dependency_lock_sha256",
        "image_input_sha256",
        "image_digest",
        "diagnostic_hash",
        "official_status_sha256",
        "official_status_counts",
    )
    if primary.get("core_status") != "complete" or not derived:
        return {
            "status": "blocked",
            "release_blocker": True,
            "missing": ["primary.release_comparison"],
            "rewrite_expected_results_allowed": False,
            "rewrite_oracle_data_allowed": False,
        }
    if second_host_manifest_path is None:
        return {
            "status": "blocked",
            "release_blocker": True,
            "reason": "second_host_evidence_manifest.missing",
            "required_evidence": list(required),
            "expected": derived,
            "rewrite_expected_results_allowed": False,
            "rewrite_oracle_data_allowed": False,
        }
    if not second_host_manifest_path.is_file():
        return {
            "status": "not_available",
            "release_blocker": True,
            "reason": "second_host_evidence_manifest.file_missing",
            "rewrite_expected_results_allowed": False,
            "rewrite_oracle_data_allowed": False,
        }
    manifest, manifest_errors = read_json_file(second_host_manifest_path)
    if manifest_errors or manifest is None:
        return {
            "status": "failed",
            "release_blocker": True,
            "reason": "second_host_evidence_manifest.malformed",
            "rewrite_expected_results_allowed": False,
            "rewrite_oracle_data_allowed": False,
        }
    second_host_evidence, comparison_ref, comparison_reasons = load_manifest_artifact(
        second_host_manifest_path,
        manifest,
        "comparison_report",
    )
    if comparison_reasons or second_host_evidence is None:
        return {
            "status": "failed",
            "release_blocker": True,
            "reasons": comparison_reasons,
            "comparison_report": comparison_ref,
            "rewrite_expected_results_allowed": False,
            "rewrite_oracle_data_allowed": False,
        }
    missing = [field for field in required if field not in second_host_evidence]
    mismatches = [
        field for field in required
        if field in second_host_evidence and second_host_evidence.get(field) != derived.get(field)
    ]
    if missing:
        return {
            "status": "blocked",
            "release_blocker": True,
            "missing": sorted(missing),
            "expected": derived,
            "rewrite_expected_results_allowed": False,
            "rewrite_oracle_data_allowed": False,
        }
    if mismatches:
        return {
            "status": "failed",
            "release_blocker": True,
            "mismatches": sorted(mismatches),
            "expected": derived,
            "rewrite_expected_results_allowed": False,
            "rewrite_oracle_data_allowed": False,
        }
    return {
        "status": "complete",
        "release_blocker": False,
        "expected": derived,
        "manifest": {
            "path": str(second_host_manifest_path),
            "sha256": file_digest(second_host_manifest_path),
        },
        "comparison_report": comparison_ref,
    }


def contains_forbidden_review_payload(value: Any) -> bool:
    if isinstance(value, dict):
        for key, nested in value.items():
            if str(key).lower() in FORBIDDEN_REVIEW_KEYS:
                return True
            if contains_forbidden_review_payload(nested):
                return True
    if isinstance(value, list):
        return any(contains_forbidden_review_payload(item) for item in value)
    return False


def agpl_review_gate(
    review: dict[str, Any] | None,
    revision_id: str | None,
    category_sha256: str,
) -> dict[str, Any]:
    if review is None:
        return {
            "status": "blocked",
            "release_blocker": True,
            "reason": "pending_human_review",
            "stores_legal_documents": False,
            "stores_secrets": False,
            "invented_approval": False,
        }
    reasons: list[str] = []
    if review.get("status") != "approved":
        reasons.append("status_not_approved")
    if not isinstance(review.get("reviewer"), str) or not review["reviewer"].strip():
        reasons.append("reviewer_missing")
    if not isinstance(review.get("reviewed_at"), str) or not review["reviewed_at"].strip():
        reasons.append("reviewed_at_missing")
    if review.get("revision_id") != revision_id:
        reasons.append("revision_id_mismatch")
    if review.get("category_sha256") != category_sha256:
        reasons.append("category_sha256_mismatch")
    if contains_forbidden_review_payload(review):
        reasons.append("forbidden_sensitive_or_legal_payload")
    if reasons:
        return {
            "status": "failed",
            "release_blocker": True,
            "reasons": sorted(set(reasons)),
            "stores_legal_documents": False,
            "stores_secrets": False,
            "invented_approval": False,
        }
    return {
        "status": "complete",
        "release_blocker": False,
        "reviewer": review["reviewer"],
        "reviewed_at": review["reviewed_at"],
        "revision_id": review["revision_id"],
        "category_sha256": review["category_sha256"],
        "stores_legal_documents": False,
        "stores_secrets": False,
        "invented_approval": False,
    }


def evaluate_variant_control(
    identity: dict[str, Any],
    evidence_manifest: Path | None = None,
    second_host_evidence_manifest: Path | None = None,
    agpl_review_evidence: dict[str, Any] | None = None,
) -> dict[str, Any]:
    categories = variant_categories(identity)
    category_hashes = {name: canonical_digest(value) for name, value in categories.items()}
    category_sha256 = canonical_digest(categories)
    declaration, declaration_reasons = declared_variant(identity, category_sha256)
    invariant_failures = invariant_reasons(identity, categories)
    revision_id = declaration.get("revision_id")
    identity_ok = not declaration_reasons and not invariant_failures
    evidence = validate_evidence_manifest(
        evidence_manifest if identity_ok else None,
        revision_id,
        category_sha256,
        categories,
    )
    report_content = evidence.pop("_report_content", None)
    report_reasons = list(evidence.pop("_report_reasons", []))
    runtime_controls = {
        "benchmark_only": "runtime_control.benchmark_only_failed" not in invariant_failures,
        "network_disabled": "runtime_control.network_disabled_failed" not in invariant_failures,
        "service_endpoint_absent": "runtime_control.service_endpoint_absent_failed" not in invariant_failures,
        "archive_absent": "runtime_control.archive_absent_failed" not in invariant_failures,
    }
    agpl_review = agpl_review_gate(agpl_review_evidence, revision_id, category_sha256)
    second_host = second_host_gate(evidence, second_host_evidence_manifest)
    core_ready = identity_ok and evidence.get("core_status") == "complete"
    execution_availability = "complete" if all(runtime_controls.values()) else "blocked"
    projected_release_blocked = (
        not core_ready
        or agpl_review.get("release_blocker") is True
        or second_host.get("release_blocker") is True
        or not all(runtime_controls.values())
    )
    report_projection = {
        "implementation_completeness": "complete",
        "release_readiness": "blocked" if projected_release_blocked else "complete",
        "execution_availability": execution_availability,
        "semantic_correctness": "blocked",
    }
    report_reasons.extend(
        validate_report_markdown(
            report_content,
            revision_id,
            category_sha256,
            report_projection,
        )
    )
    evidence_reasons = sorted(set(evidence.get("reasons", []) + report_reasons))
    report_ready = not report_reasons
    if "manifest" in evidence:
        evidence["report_status"] = "complete" if report_ready else "failed"
        evidence["status"] = (
            "complete"
            if evidence.get("core_status") == "complete" and report_ready
            else "failed"
        )
        evidence["selection_blocker"] = evidence["status"] != "complete"
    evidence["reasons"] = evidence_reasons

    selection_reasons = declaration_reasons + invariant_failures
    if evidence.get("selection_blocker"):
        selection_reasons.extend(evidence_reasons)
    selection_ready = identity_ok and evidence.get("status") == "complete"
    selection = {
        "status": "complete" if selection_ready else "blocked",
        "reason": "revision_and_evidence_current" if selection_ready else "variant_revision_or_evidence_blocked",
        "invalidated_evidence": ["diagnostic", "materialization", "official_ql", "report"],
        "required_reruns": ["diagnostic", "official_ql_10_cases", "profile_result_contract", "report"],
    }
    if selection_reasons:
        selection["reasons"] = sorted(set(selection_reasons))
    release_blocked = (
        not selection_ready
        or agpl_review.get("release_blocker") is True
        or second_host.get("release_blocker") is True
        or not all(runtime_controls.values())
    )
    return {
        "schema_version": 1,
        "status": "complete" if selection_ready else "blocked",
        "implementation_completeness": "complete",
        "execution_availability": execution_availability,
        "semantic_correctness": "blocked",
        "release_readiness": "blocked" if release_blocked else "complete",
        "report_projection": report_projection,
        "internal_use_allowed": True,
        "variant_revision": {
            "revision_id": revision_id,
            "category_sha256": category_sha256,
            "declaration": declaration,
            "categories": categories,
            "category_hashes": category_hashes,
        },
        "selection": selection,
        "evidence": evidence,
        "runtime_controls": runtime_controls,
        "pre_release_gates": {
            "agpl_review": agpl_review,
            "second_host_rebuild": second_host,
        },
    }
