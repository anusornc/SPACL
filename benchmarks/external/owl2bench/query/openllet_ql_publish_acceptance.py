#!/usr/bin/env python3
"""Publish claim-safe Openllet QL integration acceptance evidence."""

from __future__ import annotations

import argparse
import hashlib
import json
from collections import Counter
from datetime import datetime, timezone
from pathlib import Path
from typing import Any

from rdf_canonicalization import CanonicalizationError, Term, load_nquads


EXPECTED_PROFILE_COUNTS = {"DL": 22, "EL": 10, "QL": 10, "RL": 18}
EXPECTED_NON_QL_PROFILE_COUNTS = {"DL": 22, "EL": 10, "RL": 18}
EXPECTED_TOTAL = 60
EXPECTED_INVENTORY_TOTAL = 29
EXPECTED_INVENTORY_GENERATED_BY = "openllet_ql_inventory_receipt.py"
EXPECTED_ISSUES = {
    25,
    36,
    46,
    50,
    62,
    69,
}
EXPECTED_TEST_PATHS = (
    "benchmarks/external/owl2bench/query/test_canonicalize_result.sh",
    "benchmarks/external/owl2bench/query/test_elk_el_engine.sh",
    "benchmarks/external/owl2bench/query/test_elk_el_export_boundary.sh",
    "benchmarks/external/owl2bench/query/test_elk_el_materializer.sh",
    "benchmarks/external/owl2bench/query/test_graphdb_graph_filter.sh",
    "benchmarks/external/owl2bench/query/test_graphdb_ql_materializer.sh",
    "benchmarks/external/owl2bench/query/test_graphdb_rl_boundary.sh",
    "benchmarks/external/owl2bench/query/test_hermit_dl_engine.sh",
    "benchmarks/external/owl2bench/query/test_hermit_dl_materializer.sh",
    "benchmarks/external/owl2bench/query/test_konclude_unified.sh",
    "benchmarks/external/owl2bench/query/test_materialization_manifest.sh",
    "benchmarks/external/owl2bench/query/test_materialization_pipeline.sh",
    "benchmarks/external/owl2bench/query/test_materialized_graph_oxigraph_engine.sh",
    "benchmarks/external/owl2bench/query/test_openllet_ql_materialization.sh",
    "benchmarks/external/owl2bench/query/test_openllet_ql_runtime_identity.sh",
    "benchmarks/external/owl2bench/query/test_openllet_ql_split_stack_runner.sh",
    "benchmarks/external/owl2bench/query/test_oracle_matching.sh",
    "benchmarks/external/owl2bench/query/test_profile_materialization.sh",
    "benchmarks/external/owl2bench/query/test_profile_result_contract.sh",
    "benchmarks/external/owl2bench/query/test_profile_stack_assembly.sh",
    "benchmarks/external/owl2bench/query/test_ql_ruleset_engine.sh",
    "benchmarks/external/owl2bench/query/test_query006_projection_regression.sh",
    "benchmarks/external/owl2bench/query/test_query_lane.sh",
    "benchmarks/external/owl2bench/query/test_real_smoke.sh",
    "benchmarks/external/owl2bench/query/test_report.sh",
    "benchmarks/external/owl2bench/query/test_rl_ruleset_engine.sh",
    "benchmarks/external/owl2bench/query/test_run_profile_query_smoke.sh",
    "benchmarks/external/owl2bench/query/test_split_profile_engine.sh",
    "benchmarks/external/owl2bench/query/test_split_stack_manifest.sh",
)


def digest(path: Path) -> str:
    hasher = hashlib.sha256()
    with path.open("rb") as handle:
        for block in iter(lambda: handle.read(1024 * 1024), b""):
            hasher.update(block)
    return "sha256:" + hasher.hexdigest()


def canonical_digest(value: Any) -> str:
    encoded = json.dumps(value, sort_keys=True, separators=(",", ":")).encode("utf-8")
    return "sha256:" + hashlib.sha256(encoded).hexdigest()


def load_json(path: Path, errors: list[str], label: str) -> dict[str, Any]:
    if not path.is_file():
        errors.append(f"{label}.missing")
        return {}
    try:
        value = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, UnicodeDecodeError, json.JSONDecodeError):
        errors.append(f"{label}.malformed")
        return {}
    if not isinstance(value, dict):
        errors.append(f"{label}.not_object")
        return {}
    return value


def object_at(value: dict[str, Any], key: str) -> dict[str, Any]:
    item = value.get(key)
    return item if isinstance(item, dict) else {}


def content_ref(path: Path, errors: list[str], label: str) -> dict[str, str]:
    if not path.is_file():
        errors.append(f"{label}.missing")
        return {"artifact_id": label}
    return {"artifact_id": label, "sha256": digest(path)}


def stable_fields(value: dict[str, Any], keys: tuple[str, ...]) -> dict[str, Any]:
    return {key: value.get(key) for key in keys if key in value}


def category_artifact_ref(value: dict[str, Any], label: str) -> dict[str, Any]:
    artifact_id = value.get("artifact_id") or value.get("path") or value.get("name") or label
    ref = {"artifact_id": artifact_id}
    if "sha256" in value:
        ref["sha256"] = value["sha256"]
    elif "artifact_digest" in value:
        ref["sha256"] = value["artifact_digest"]
    return ref


def is_digest(value: Any) -> bool:
    return isinstance(value, str) and len(value) == 71 and value.startswith("sha256:") and all(
        character in "0123456789abcdef" for character in value.removeprefix("sha256:")
    )


def parse_generated_at(value: str | None, errors: list[str]) -> str:
    if value is None:
        errors.append("generated_at.missing")
        return "1970-01-01T00:00:00Z"
    try:
        parsed = datetime.strptime(value, "%Y-%m-%dT%H:%M:%SZ").replace(tzinfo=timezone.utc)
    except ValueError:
        errors.append("generated_at.invalid")
        return value
    return parsed.strftime("%Y-%m-%dT%H:%M:%SZ")


def result_rows(run_dir: Path, errors: list[str]) -> list[dict[str, Any]]:
    rows: list[dict[str, Any]] = []
    for path in sorted(run_dir.glob("*.json")):
        if path.name in {"run_metadata.json", "summary.json"}:
            continue
        row = load_json(path, errors, f"result.{path.name}")
        if row:
            row["_artifact_id"] = f"query_result:{path.stem}"
            row["_sha256"] = digest(path)
            rows.append(row)
    return rows


def validate_runtime(attestation: dict[str, Any], errors: list[str]) -> dict[str, Any]:
    runtime = object_at(attestation, "runtime")
    archive = object_at(runtime, "archive")
    classpath = object_at(attestation, "classpath")
    checks = {
        "attestation_status_available": attestation.get("status") == "available",
        "pinned_image_digest_present": bool(object_at(runtime, "image").get("digest")),
        "runtime_network_disabled": runtime.get("execution_network") == "none",
        "materialization_network_disabled": runtime.get("materialization_network") == "none",
        "service_endpoint_absent": runtime.get("service_endpoint") == "none",
        "archive_absent": archive.get("committed") is False and archive.get("published") is False,
        "classpath_78_jars": classpath.get("jar_count") == 78,
    }
    for name, ok in checks.items():
        if not ok:
            errors.append(f"runtime.{name}")
    return checks


def validate_materialization(summary: dict[str, Any], errors: list[str]) -> dict[str, Any]:
    diagnostic = object_at(summary, "diagnostic")
    official = object_at(summary, "official")
    observations = object_at(diagnostic, "observations")
    obligations = object_at(diagnostic, "independent_obligations")
    obligation_counts = object_at(obligations, "expected_inferred_assertion_counts")
    checks = {
        "summary_complete": summary.get("status") == "complete",
        "diagnostic_complete": diagnostic.get("status") == "complete",
        "diagnostic_graph_not_source_equal": object_at(diagnostic, "graph").get("sha256")
        != object_at(diagnostic, "source").get("sha256"),
        "diagnostic_class_observed": object_at(observations, "class").get("status") == "observed",
        "diagnostic_object_observed": object_at(observations, "object_property").get("status") == "observed",
        "diagnostic_class_obligation": obligation_counts.get("class", 0) >= 1,
        "diagnostic_object_obligation": obligation_counts.get("object_property", 0) >= 1,
        "official_complete": official.get("execution_status") == "complete",
        "official_oracle_unavailable": official.get("status") == "oracle_unavailable",
    }
    for name, ok in checks.items():
        if not ok:
            errors.append(f"materialization.{name}")
    return checks


def validate_query_run(
    query_summary: dict[str, Any],
    rows: list[dict[str, Any]],
    errors: list[str],
) -> dict[str, Any]:
    profile_counts = Counter(str(row.get("query_profile") or "") for row in rows)
    ql_rows = [row for row in rows if row.get("query_profile") == "QL"]
    ql_execution_counts = Counter(str(row.get("execution_status") or "") for row in ql_rows)
    ql_result_counts = Counter(str(row.get("result_status") or "") for row in ql_rows)
    non_ql_openllet = [
        row for row in rows
        if row.get("query_profile") != "QL"
        and (
            object_at(row, "engine_provenance").get("adapter") == "openllet-ql"
            or object_at(row, "component_manifest").get("execution_variant") == "openllet-ql"
        )
    ]
    graphdb_fallback = [
        row for row in rows
        if "graphdb" in json.dumps(row, sort_keys=True).lower()
    ]
    checks = {
        "query_count_60": query_summary.get("query_count") == EXPECTED_TOTAL and len(rows) == EXPECTED_TOTAL,
        "profile_case_counts": query_summary.get("profile_case_counts") == EXPECTED_PROFILE_COUNTS
        and {profile: profile_counts[profile] for profile in EXPECTED_PROFILE_COUNTS} == EXPECTED_PROFILE_COUNTS,
        "query_gate_blocked": query_summary.get("query_gate") == "blocked",
        "correctness_gate_blocked": query_summary.get("correctness_gate") == "blocked",
        "performance_claims_suppressed": query_summary.get("performance_claims_allowed") is False,
        "ql_count_10": len(ql_rows) == 10,
        "ql_execution_success_10": dict(ql_execution_counts) == {"success": 10},
        "ql_result_oracle_unavailable_10": dict(ql_result_counts) == {"oracle_unavailable": 10},
        "only_ql_selects_openllet": len(non_ql_openllet) == 0,
        "no_graphdb_fallback": len(graphdb_fallback) == 0,
    }
    for name, ok in checks.items():
        if not ok:
            errors.append(f"query_run.{name}")
    if not checks["ql_execution_success_10"]:
        errors.append("official_ql.execution_status_counts_not_success")
    if not checks["ql_result_oracle_unavailable_10"]:
        errors.append("official_ql.result_status_counts_not_oracle_unavailable")
    return {
        "profile_case_counts": {profile: profile_counts[profile] for profile in EXPECTED_PROFILE_COUNTS},
        "ql_count": len(ql_rows),
        "execution_status_counts": dict(sorted(ql_execution_counts.items())),
        "result_status_counts": dict(sorted(ql_result_counts.items())),
        "non_ql_openllet_selection_count": len(non_ql_openllet),
        "graphdb_fallback_count": len(graphdb_fallback),
        "checks": checks,
    }


def validate_variant_control(control: dict[str, Any], errors: list[str]) -> dict[str, Any]:
    gates = object_at(control, "pre_release_gates")
    agpl = object_at(gates, "agpl_review")
    second_host = object_at(gates, "second_host_rebuild")
    evidence = object_at(control, "evidence")
    checks = {
        "validation_complete": control.get("status") == "complete",
        "implementation_complete": control.get("implementation_completeness") == "complete",
        "execution_complete": control.get("execution_availability") == "complete",
        "semantic_blocked": control.get("semantic_correctness") == "blocked",
        "release_blocked": control.get("release_readiness") == "blocked",
        "evidence_complete": evidence.get("status") == "complete",
        "official_ql_oracle_unavailable_10": object_at(
            object_at(evidence, "release_comparison"), "official_status_counts"
        ) == {"oracle_unavailable": 10},
        "agpl_pending": agpl.get("status") == "blocked" and agpl.get("reason") == "pending_human_review",
        "second_host_pending": second_host.get("status") == "blocked",
    }
    for name, ok in checks.items():
        if not ok:
            errors.append(f"variant_control.{name}")
    revision = object_at(control, "variant_revision")
    return {
        "status": control.get("status"),
        "revision_id": revision.get("revision_id"),
        "category_sha256": revision.get("category_sha256"),
        "implementation_completeness": control.get("implementation_completeness"),
        "execution_availability": control.get("execution_availability"),
        "semantic_correctness": control.get("semantic_correctness"),
        "release_readiness": control.get("release_readiness"),
        "official_status_counts": object_at(
            object_at(evidence, "release_comparison"), "official_status_counts"
        ),
        "agpl_review": agpl,
        "second_host_rebuild": {
            "status": second_host.get("status"),
            "reason": second_host.get("reason"),
            "rewrite_expected_results_allowed": second_host.get("rewrite_expected_results_allowed", False),
            "rewrite_oracle_data_allowed": second_host.get("rewrite_oracle_data_allowed", False),
        },
        "checks": checks,
    }


def repo_root() -> Path:
    return Path(__file__).resolve().parents[4]


def resolve_artifact_path(record: dict[str, Any], errors: list[str], label: str) -> Path | None:
    raw_path = record.get("path")
    if not isinstance(raw_path, str) or not raw_path:
        errors.append(f"{label}.path_missing")
        return None
    return Path(raw_path).expanduser().resolve()


def validated_file_ref(path: Path | None, declared_sha: Any, errors: list[str], label: str) -> dict[str, Any]:
    ref = {"artifact_id": label}
    if path is None:
        return ref
    if not path.is_file():
        errors.append(f"{label}.missing")
        return ref
    actual_sha = digest(path)
    ref["sha256"] = actual_sha
    if declared_sha != actual_sha:
        errors.append(f"{label}.sha256_mismatch")
    return ref


def nquad_terms(path: Path, errors: list[str], label: str) -> set[Term]:
    if not path.is_file():
        errors.append(f"{label}.missing")
        return set()
    try:
        return {term for quad in load_nquads(path) for term in quad.terms()}
    except (OSError, UnicodeDecodeError, CanonicalizationError):
        errors.append(f"{label}.nquads_malformed")
        return set()


def validate_diagnostic_evidence(materialization_root: Path, summary: dict[str, Any], errors: list[str]) -> dict[str, Any]:
    diagnostic_summary = object_at(summary, "diagnostic")
    validation_path = materialization_root / "diagnostic" / "validation.json"
    validation = load_json(validation_path, errors, "diagnostic_validation")
    source_record = object_at(validation, "source") or object_at(diagnostic_summary, "source")
    candidate_record = object_at(validation, "candidate_graph") or object_at(diagnostic_summary, "candidate")
    graph_record = object_at(validation, "graph") or object_at(diagnostic_summary, "graph")
    determinism = object_at(validation, "determinism")
    first_record = object_at(determinism, "first_graph")
    second_record = object_at(determinism, "second_graph")
    source_path = resolve_artifact_path(source_record, errors, "diagnostic_source")
    candidate_path = resolve_artifact_path(candidate_record, errors, "diagnostic_candidate_graph")
    graph_path = resolve_artifact_path(graph_record, errors, "diagnostic_graph")
    first_path = resolve_artifact_path(first_record, errors, "diagnostic_determinism.first_graph")
    second_path = resolve_artifact_path(second_record, errors, "diagnostic_determinism.second_graph")
    source_ref = validated_file_ref(source_path, source_record.get("sha256"), errors, "diagnostic_source")
    candidate_ref = validated_file_ref(candidate_path, candidate_record.get("sha256"), errors, "diagnostic_candidate_graph")
    graph_ref = validated_file_ref(graph_path, graph_record.get("sha256"), errors, "diagnostic_graph")
    first_ref = validated_file_ref(first_path, first_record.get("sha256"), errors, "diagnostic_determinism_first_graph")
    second_ref = validated_file_ref(second_path, second_record.get("sha256"), errors, "diagnostic_determinism_second_graph")
    if validation.get("status") != "complete":
        errors.append("diagnostic_validation.status_not_complete")
    if validation.get("reasons") != []:
        errors.append("diagnostic_validation.reasons_not_empty")
    graph_sha = graph_ref.get("sha256")
    first_sha = first_ref.get("sha256")
    second_sha = second_ref.get("sha256")
    if determinism.get("runs") != 2:
        errors.append("diagnostic_determinism.runs_not_2")
    if not graph_sha or first_sha != graph_sha or second_sha != graph_sha:
        errors.append("diagnostic_determinism.graph_sha_mismatch")
    if candidate_ref.get("sha256") != graph_sha:
        errors.append("diagnostic_candidate_graph.graph_sha_mismatch")
    source_terms = nquad_terms(source_path, errors, "diagnostic_source") if source_path else set()
    candidate_terms = nquad_terms(candidate_path, errors, "diagnostic_candidate_graph") if candidate_path else set()
    extra_terms = sorted(
        candidate_terms - source_terms,
        key=lambda term: (term.kind, term.value, term.datatype or "", term.language or ""),
    )
    extra_blank_nodes = [term for term in extra_terms if term.kind == "blank_node"]
    extra_iris = [term for term in extra_terms if term.kind == "iri"]
    extra_literals = [term for term in extra_terms if term.kind == "literal"]
    if extra_terms:
        errors.append("diagnostic.generated_terms_present")
    return {
        "validation": content_ref(validation_path, errors, "diagnostic_validation"),
        "status": validation.get("status"),
        "reasons": validation.get("reasons", []),
        "determinism": {
            "runs": determinism.get("runs"),
            "first_graph": first_ref,
            "second_graph": second_ref,
            "validated_graph": graph_ref,
            "first_equals_second_equals_validated": first_sha == graph_sha and second_sha == graph_sha,
        },
        "term_domain": {
            "source": source_ref,
            "candidate": candidate_ref,
            "source_term_count": len(source_terms),
            "candidate_term_count": len(candidate_terms),
            "extra_term_count": len(extra_terms),
            "extra_blank_node_count": len(extra_blank_nodes),
            "extra_iri_count": len(extra_iris),
            "extra_literal_count": len(extra_literals),
        },
    }


def parse_classpath_lock(path: Path, errors: list[str]) -> list[dict[str, str]]:
    entries: list[dict[str, str]] = []
    if not path.is_file():
        errors.append("runtime_source.classpath_lock_missing")
        return entries
    for index, line in enumerate(path.read_text(encoding="utf-8").splitlines(), start=1):
        parts = line.split()
        if len(parts) != 2 or len(parts[0]) != 64:
            errors.append(f"runtime_source.classpath_lock[{index}].invalid")
            continue
        entries.append({"name": parts[1], "sha256": "sha256:" + parts[0]})
    return entries


def validate_runtime_source(runtime_attestation: dict[str, Any], variant_control: dict[str, Any], errors: list[str]) -> dict[str, Any]:
    root = repo_root()
    identity_path = root / "benchmarks/external/owl2bench/query/openllet_ql_runtime_identity.json"
    identity = load_json(identity_path, errors, "runtime_identity")
    runtime = object_at(runtime_attestation, "runtime")
    attestation_inputs = object_at(runtime_attestation, "inputs")
    categories = object_at(object_at(variant_control, "variant_revision"), "categories")
    revision = object_at(variant_control, "variant_revision")
    release_comparison = object_at(object_at(variant_control, "evidence"), "release_comparison")
    checks: dict[str, bool] = {}
    source_refs = {"runtime_identity": content_ref(identity_path, errors, "runtime_identity")}
    input_records_match = True
    for key in ("dockerfile", "pom", "configuration", "exporter"):
        record = object_at(object_at(identity, "inputs"), key)
        if object_at(attestation_inputs, key) != record:
            input_records_match = False
        source_path = root / str(record.get("path", ""))
        source_refs[key] = validated_file_ref(source_path, record.get("sha256"), errors, f"runtime_source.{key}")
    classpath_identity = object_at(identity, "classpath")
    classpath_path = root / str(classpath_identity.get("path", ""))
    classpath_entries = parse_classpath_lock(classpath_path, errors)
    classpath_ref = validated_file_ref(classpath_path, classpath_identity.get("lock_sha256"), errors, "runtime_source.classpath_lock")
    source_refs["classpath_lock"] = classpath_ref
    attestation_classpath = object_at(runtime_attestation, "classpath")
    attestation_jars = attestation_classpath.get("jars") if isinstance(attestation_classpath.get("jars"), list) else []
    checks["identity_variant_revision_matches_attestation"] = object_at(identity, "runtime").get("variant_revision") == runtime.get("variant_revision")
    checks["runtime_variant_revision_pinned"] = runtime.get("variant_revision") == "openllet-ql-runtime-v1"
    checks["base_image_matches"] = object_at(object_at(identity, "runtime"), "base_image") == object_at(runtime, "base_image") == object_at(categories, "base_image")
    checks["image_matches"] = object_at(object_at(identity, "runtime"), "image") == {
        key: object_at(runtime, "image").get(key)
        for key in ("reference", "mutable_tag", "digest", "platform")
    } == object_at(categories, "image")
    checks["image_local_attestation_matched"] = object_at(runtime, "image").get("local_attestation") == "matched"
    checks["dependencies_match"] = object_at(identity, "runtime").get("dependencies") == runtime.get("dependencies") == categories.get("dependencies")
    checks["classpath_lock_sha_matches"] = classpath_identity.get("lock_sha256") == attestation_classpath.get("lock_sha256") == object_at(categories, "classpath").get("lock_sha256") == release_comparison.get("dependency_lock_sha256")
    checks["classpath_78_entries"] = len(classpath_entries) == 78 and attestation_classpath.get("jar_count") == 78 and object_at(categories, "classpath").get("jar_count") == 78
    checks["classpath_entries_match_attestation"] = classpath_entries == attestation_jars
    checks["engine_versions_match"] = {
        "implementation": object_at(identity, "component").get("implementation"),
        "openllet_version": object_at(identity, "component").get("openllet_version"),
        "owlapi_version": object_at(identity, "component").get("owlapi_version"),
        "runtime_revision": object_at(identity, "runtime").get("variant_revision"),
    } == object_at(categories, "engine")
    checks["configuration_hash_matches_category"] = object_at(object_at(identity, "inputs"), "configuration") == object_at(categories, "configuration")
    checks["exporter_hash_matches_category"] = object_at(object_at(identity, "inputs"), "exporter") == object_at(categories, "exporter")
    checks["graph_contract_matches_category"] = object_at(identity, "graph_contract") == object_at(categories, "graph_contract")
    checks["resource_matches_category"] = object_at(identity, "resource_limits") == object_at(categories, "resource")
    checks["image_digest_matches_release_comparison"] = object_at(runtime, "image").get("digest") == release_comparison.get("image_digest")
    checks["attestation_inputs_match_identity"] = input_records_match
    category_sha256 = canonical_digest(categories)
    build_inputs_sha256 = canonical_digest(
        {
            key: categories.get(key)
            for key in (
                "base_image",
                "dependencies",
                "classpath",
                "exporter",
                "configuration",
            )
        }
    )
    image_input_sha256 = canonical_digest(
        {
            "base_image": categories.get("base_image"),
            "image": categories.get("image"),
        }
    )
    checks["category_sha_matches_revision"] = category_sha256 == revision.get("category_sha256") == release_comparison.get("category_sha256")
    checks["build_inputs_sha_matches_release_comparison"] = build_inputs_sha256 == release_comparison.get("build_inputs_sha256")
    checks["image_input_sha_matches_release_comparison"] = image_input_sha256 == release_comparison.get("image_input_sha256")
    for name, ok in checks.items():
        if not ok:
            errors.append(f"runtime_source.{name}")
    return {
        "artifacts": source_refs,
        "computed_hashes": {
            "category_sha256": category_sha256,
            "build_inputs_sha256": build_inputs_sha256,
            "image_input_sha256": image_input_sha256,
        },
        "variant_revision": runtime.get("variant_revision"),
        "image": stable_fields(object_at(runtime, "image"), ("reference", "digest", "platform", "local_attestation")),
        "base_image": object_at(runtime, "base_image"),
        "classpath": {
            "jar_count": len(classpath_entries),
            "lock_sha256": classpath_ref.get("sha256"),
            "attestation_lock_sha256": attestation_classpath.get("lock_sha256"),
        },
        "checks": checks,
    }


def validate_non_ql_projection(rows: list[dict[str, Any]], errors: list[str]) -> dict[str, Any]:
    non_ql_rows = [row for row in rows if row.get("query_profile") in {"DL", "EL", "RL"}]
    profile_counts = Counter(str(row.get("query_profile") or "") for row in non_ql_rows)
    failed_rows: list[str] = []
    normalized_rows: list[dict[str, Any]] = []
    for row in non_ql_rows:
        case_id = str(row.get("case_id") or "")
        component = object_at(row, "component_manifest")
        materialization = object_at(row, "materialization")
        normalized_rows.append(
            {
                "case_id": row.get("case_id"),
                "query_id": row.get("query_id"),
                "profile": row.get("query_profile"),
                "source_query_sha256": row.get("source_query_sha256"),
                "source_corpus": row.get("source_corpus"),
                "source_revision": row.get("source_revision"),
                "status": row.get("status"),
                "execution_status": row.get("execution_status"),
                "result_status": row.get("result_status"),
                "engine_provenance_status": None if row.get("engine_provenance") is None else object_at(row, "engine_provenance").get("status"),
                "component_status": component.get("status"),
                "component_manifest_identity": component.get("manifest_identity"),
                "materialization_status": materialization.get("status"),
                "materialization_identity": materialization.get("materialization_identity"),
                "materialization_handoff": row.get("materialization_handoff"),
            }
        )
        row_ok = (
            row.get("status") == "not_available"
            and row.get("execution_status") == "not_available"
            and row.get("result_status") == "not_available"
            and row.get("engine_provenance") is None
            and object_at(row, "component_manifest").get("status") == "not_available"
            and object_at(row, "materialization").get("status") == "not_available"
            and row.get("materialization_handoff") is None
        )
        if not row_ok:
            failed_rows.append(case_id)
    projection = {
        "schema_version": 1,
        "profiles": {profile: profile_counts[profile] for profile in ("DL", "EL", "RL")},
        "row_count": len(non_ql_rows),
        "rows": sorted(normalized_rows, key=lambda item: str(item.get("case_id"))),
    }
    projection_digest = canonical_digest(projection)
    lock_path = repo_root() / "benchmarks/external/owl2bench/query/openllet_ql_non_ql_projection_lock.json"
    lock = load_json(lock_path, errors, "non_ql_projection_lock")
    expected_digest = lock.get("projection_sha256")
    checks = {
        "row_count_50": len(non_ql_rows) == 50,
        "profile_counts": projection["profiles"] == EXPECTED_NON_QL_PROFILE_COUNTS,
        "all_rows_not_available": not failed_rows,
        "projection_digest_matches_lock": projection_digest == expected_digest,
    }
    for name, ok in checks.items():
        if not ok:
            errors.append(f"non_ql_projection.{name}")
    return {
        "projection_sha256": projection_digest,
        "lock": content_ref(lock_path, errors, "non_ql_projection_lock"),
        "profile_counts": projection["profiles"],
        "row_count": len(non_ql_rows),
        "rows": projection["rows"],
        "failed_row_count": len(failed_rows),
        "checks": checks,
    }


def validate_ql_bindings(rows: list[dict[str, Any]], official_graph_sha: Any, categories: dict[str, Any], errors: list[str]) -> dict[str, Any]:
    acceptance_lock_path = repo_root() / "benchmarks/external/owl2bench/query/openllet_ql_acceptance_lock.json"
    acceptance_lock = load_json(acceptance_lock_path, errors, "ql_acceptance_lock")
    workload = object_at(acceptance_lock, "query_workload")
    expected_queries = {
        query.get("id"): str(query.get("sha256", "")).removeprefix("sha256:")
        for query in workload.get("queries", [])
        if isinstance(query, dict)
    }
    ql_rows = sorted([row for row in rows if row.get("query_profile") == "QL"], key=lambda row: str(row.get("query_id")))
    if [row.get("query_id") for row in ql_rows] != sorted(expected_queries):
        errors.append("official_ql.query_id_set_mismatch")
    identities: list[dict[str, Any]] = []
    image = object_at(categories, "image")
    dependencies = categories.get("dependencies")
    for row in ql_rows:
        query_id = row.get("query_id")
        case_id = row.get("case_id")
        component = object_at(row, "component_manifest")
        materialization = object_at(row, "materialization")
        output_graph = object_at(materialization, "output_graph")
        runtime = object_at(materialization, "runtime")
        row_checks = {
            "source_hash_matches": row.get("source_query_sha256") == expected_queries.get(str(query_id)),
            "source_corpus_matches": row.get("source_corpus") == workload.get("source_corpus"),
            "source_revision_matches": row.get("source_revision") == workload.get("source_revision"),
            "official_graph_matches": materialization.get("output_sha256") == official_graph_sha and output_graph.get("sha256") == official_graph_sha,
            "component_available": component.get("status") == "available" and component.get("execution_variant") == "openllet-ql",
            "materialization_complete": materialization.get("status") == "complete" and materialization.get("execution_variant") == "openllet-ql",
            "image_matches": object_at(runtime, "image").get("digest") == image.get("digest")
            and object_at(runtime, "image").get("reference") == image.get("reference"),
            "dependencies_match": runtime.get("dependencies") == dependencies,
        }
        for name, ok in row_checks.items():
            if not ok:
                errors.append(f"official_ql.{case_id}.{name}")
        identities.append(
            {
                "case_id": case_id,
                "query_id": query_id,
                "artifact_id": row.get("_artifact_id"),
                "sha256": row.get("_sha256"),
                "source_query_sha256": row.get("source_query_sha256"),
                "graph": {"artifact_id": "official_graph", "sha256": official_graph_sha},
                "component": stable_fields(component, ("status", "execution_variant", "manifest_identity")),
                "materialization": stable_fields(materialization, ("status", "execution_variant", "materialization_identity", "output_sha256")),
                "image": stable_fields(object_at(runtime, "image"), ("reference", "digest")),
                "checks": row_checks,
            }
        )
    return {
        "lock": content_ref(acceptance_lock_path, errors, "ql_acceptance_lock"),
        "source_corpus": workload.get("source_corpus"),
        "source_revision": workload.get("source_revision"),
        "query_count": len(identities),
        "rows": identities,
    }


def validate_inventory(receipt_path: Path, publication_generated_at: str, errors: list[str]) -> dict[str, Any]:
    receipt = load_json(receipt_path, errors, "inventory_receipt")
    if receipt.get("schema_version") != 1:
        errors.append("inventory_receipt.schema_version_invalid")
    receipt_generated_at = parse_generated_at(
        receipt.get("generated_at") if isinstance(receipt.get("generated_at"), str) else None,
        errors,
    )
    if receipt_generated_at != publication_generated_at:
        errors.append("inventory_receipt.generated_at_mismatch")
    if receipt.get("generated_by") != EXPECTED_INVENTORY_GENERATED_BY:
        errors.append("inventory_receipt.generated_by_invalid")
    runner_path = Path(__file__).resolve().with_name(EXPECTED_INVENTORY_GENERATED_BY)
    if not runner_path.is_file():
        errors.append("inventory_receipt.runner_missing")
    elif receipt.get("runner_sha256") != digest(runner_path):
        errors.append("inventory_receipt.runner_sha256_mismatch")
    entries = receipt.get("tests") if isinstance(receipt.get("tests"), list) else []
    seen: set[str] = set()
    receipt_paths: list[str] = []
    pass_count = 0
    sanitized_entries: list[dict[str, Any]] = []
    root = repo_root()
    for index, entry in enumerate(entries):
        if not isinstance(entry, dict):
            errors.append(f"inventory_receipt.tests[{index}].not_object")
            continue
        script = entry.get("script")
        if not isinstance(script, str):
            errors.append(f"inventory_receipt.tests[{index}].script_missing")
            continue
        if script in seen:
            errors.append(f"inventory_receipt.tests[{index}].duplicate")
        seen.add(script)
        receipt_paths.append(script)
        if script not in EXPECTED_TEST_PATHS:
            errors.append(f"inventory_receipt.tests[{index}].unexpected_script")
            script_path = root / script
        else:
            script_path = root / script
        if not script_path.is_file():
            errors.append(f"inventory_receipt.tests[{index}].script_missing_on_disk")
        else:
            script_sha = digest(script_path)
            if entry.get("script_sha256") != script_sha:
                errors.append(f"inventory_receipt.tests[{index}].script_sha256_mismatch")
        for field in ("stdout_sha256", "stderr_sha256"):
            if not is_digest(entry.get(field)):
                errors.append(f"inventory_receipt.tests[{index}].{field}_invalid")
        if entry.get("exit_code") != 0 or entry.get("status") != "complete":
            errors.append(f"inventory_receipt.tests[{index}].not_complete")
        else:
            pass_count += 1
        sanitized_entries.append(
            {
                "script": script,
                "script_sha256": entry.get("script_sha256"),
                "exit_code": entry.get("exit_code"),
                "status": entry.get("status"),
                "stdout_sha256": entry.get("stdout_sha256"),
                "stderr_sha256": entry.get("stderr_sha256"),
            }
        )
    if tuple(receipt_paths) != EXPECTED_TEST_PATHS:
        errors.append("inventory_receipt.script_set_mismatch")
    if len(receipt_paths) != len(set(receipt_paths)):
        errors.append("inventory_receipt.duplicate_scripts")
    if len(entries) != EXPECTED_INVENTORY_TOTAL or pass_count != EXPECTED_INVENTORY_TOTAL:
        errors.append("inventory.not_29_of_29")
    return {
        "observed_passed": pass_count,
        "observed_total": len(entries),
        "observation_source": "structured_receipt",
        "receipt": content_ref(receipt_path, errors, "inventory_receipt"),
        "tests": sanitized_entries,
    }


def validate_issue_snapshot(snapshot_path: Path, errors: list[str]) -> dict[str, Any]:
    snapshot = load_json(snapshot_path, errors, "issue_state_snapshot")
    raw_issues = snapshot.get("issues") if isinstance(snapshot.get("issues"), list) else []
    issues: list[dict[str, Any]] = []
    seen: set[int] = set()
    for index, issue in enumerate(raw_issues):
        if not isinstance(issue, dict):
            errors.append(f"issue_state_snapshot.issues[{index}].not_object")
            continue
        number = issue.get("number")
        if not isinstance(number, int):
            errors.append(f"issue_state_snapshot.issues[{index}].number_missing")
            continue
        if number in seen:
            errors.append(f"issue_state_snapshot.issues[{index}].duplicate")
        seen.add(number)
        if number not in EXPECTED_ISSUES:
            errors.append(f"issue_state_snapshot.issues[{index}].unexpected_issue")
        if issue.get("state") != "OPEN":
            errors.append(f"issue_state_snapshot.issues[{index}].not_open")
        issues.append(stable_fields(issue, ("number", "title", "state", "updated_at")))
    if seen != EXPECTED_ISSUES:
        errors.append("issue_state_snapshot.issue_set_mismatch")
    operator = object_at(snapshot, "operator_attestation")
    if operator.get("broader_issues_mutated") is not False:
        errors.append("issue_state_snapshot.broader_issue_mutation_attestation_missing")
    attested_at = operator.get("attested_at")
    if not isinstance(attested_at, str) or not attested_at.endswith("Z"):
        errors.append("issue_state_snapshot.operator_attestation.attested_at_invalid")
    return {
        "snapshot": content_ref(snapshot_path, errors, "issue_state_snapshot"),
        "issues": sorted(issues, key=lambda item: item.get("number", 0)),
        "operator_attestation": stable_fields(
            operator,
            ("attestor", "attested_at", "broader_issues_mutated"),
        ),
    }


def build_summary(args: argparse.Namespace) -> dict[str, Any]:
    errors: list[str] = []
    generated_at = parse_generated_at(args.generated_at, errors)
    materialization_root = args.materialization_root.resolve()
    query_run = args.query_run.resolve()
    control_root = args.variant_control_root.resolve()
    materialization_summary_path = materialization_root / "openllet_ql_materialization_summary.json"
    runtime_attestation_path = materialization_root / "runtime" / "attestation.json"
    query_summary_path = query_run / "summary.json"
    report_path = query_run / "report.md"
    control_path = control_root / "variant-control-final.json"
    evidence_manifest_path = control_root / "evidence-manifest.json"

    materialization_summary = load_json(materialization_summary_path, errors, "materialization_summary")
    runtime_attestation = load_json(runtime_attestation_path, errors, "runtime_attestation")
    query_summary = load_json(query_summary_path, errors, "query_summary")
    variant_control = load_json(control_path, errors, "variant_control")
    rows = result_rows(query_run, errors) if query_run.is_dir() else []
    if not query_run.is_dir():
        errors.append("query_run.missing")

    runtime_checks = validate_runtime(runtime_attestation, errors)
    materialization_checks = validate_materialization(materialization_summary, errors)
    official_ql = validate_query_run(query_summary, rows, errors)
    control_summary = validate_variant_control(variant_control, errors)
    inventory = validate_inventory(args.inventory_receipt.resolve(), generated_at, errors)
    issue_state = validate_issue_snapshot(args.issue_state_snapshot.resolve(), errors)
    diagnostic_evidence = validate_diagnostic_evidence(materialization_root, materialization_summary, errors)
    runtime_source = validate_runtime_source(runtime_attestation, variant_control, errors)
    non_ql_projection = validate_non_ql_projection(rows, errors)

    artifacts = {
        "runtime_attestation": content_ref(runtime_attestation_path, errors, "runtime_attestation"),
        "materialization_summary": content_ref(materialization_summary_path, errors, "materialization_summary"),
        "query_summary": content_ref(query_summary_path, errors, "query_summary"),
        "query_report": content_ref(report_path, errors, "query_report"),
        "variant_control": content_ref(control_path, errors, "variant_control"),
        "evidence_manifest": content_ref(evidence_manifest_path, errors, "evidence_manifest"),
    }

    claim_controls = {
        "execution_availability_only": True,
        "answer_key_created": False,
        "oracle_approval_created": False,
        "expected_result_repaired": False,
        "docker_archive_committed_or_published": False,
        "performance_claims_allowed": False,
    }
    blockers = [
        "semantic_correctness_blocked",
        "release_readiness_blocked",
        "agpl_review_pending",
        "second_host_rebuild_required",
        "official_ql_oracle_unavailable",
    ]
    categories = object_at(object_at(variant_control, "variant_revision"), "categories")
    official = object_at(materialization_summary, "official")
    diagnostic = object_at(materialization_summary, "diagnostic")
    official_graph_sha = object_at(official, "canonical_graph").get("sha256")
    ql_bindings = validate_ql_bindings(rows, official_graph_sha, categories, errors)
    identities = {
        "source": stable_fields(object_at(official, "source"), ("id", "sha256")),
        "decision": {
            "execution_variant": "openllet-ql",
            "claim_boundary": "execution_available_only",
            "semantic_correctness": "blocked",
        },
        "component": object_at(categories, "engine"),
        "image": object_at(categories, "image"),
        "dependencies": categories.get("dependencies", []),
        "exporter": category_artifact_ref(object_at(categories, "exporter"), "exporter"),
        "configuration": category_artifact_ref(object_at(categories, "configuration"), "configuration"),
        "graph": {
            "official": stable_fields(object_at(official, "canonical_graph"), ("id", "sha256")),
            "diagnostic": stable_fields(object_at(diagnostic, "graph"), ("id", "sha256")),
        },
        "query": ql_bindings["rows"],
        "report": artifacts["query_report"],
        "tests": inventory.get("receipt", {}),
    }
    summary = {
        "schema_version": 1,
        "generated_at": generated_at,
        "status": "complete" if not errors else "failed",
        "errors": sorted(set(errors)),
        "claim_boundary": "execution_available_only",
        "summary": "Execution availability only; no semantic conformance claim.",
        "implementation_completeness": control_summary.get("implementation_completeness"),
        "execution_availability": control_summary.get("execution_availability"),
        "semantic_correctness": "blocked",
        "release_readiness": "blocked",
        "runtime": runtime_checks,
        "materialization": materialization_checks,
        "diagnostic_evidence": diagnostic_evidence,
        "runtime_source": runtime_source,
        "official_ql": official_ql,
        "official_ql_bindings": ql_bindings,
        "non_ql_projection": non_ql_projection,
        "split_stack": {
            "profile_case_counts": official_ql["profile_case_counts"],
            "query_gate": query_summary.get("query_gate"),
            "correctness_gate": query_summary.get("correctness_gate"),
            "performance_claims_allowed": query_summary.get("performance_claims_allowed"),
            "non_ql_openllet_selection_count": official_ql["non_ql_openllet_selection_count"],
            "graphdb_fallback_count": official_ql["graphdb_fallback_count"],
        },
        "variant_control": control_summary,
        "regression_inventory": inventory,
        "issue_state": issue_state,
        "artifacts": artifacts,
        "identities": identities,
        "claim_controls": claim_controls,
        "known_blockers": blockers,
        "known_validation_gaps": [
            "AGPL human distribution review evidence is not present.",
            "Second-host rebuild evidence is not present.",
            "No answer key or accepted oracle exists for QL semantic correctness.",
        ],
    }
    return summary


def write_json(path: Path, value: dict[str, Any]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, indent=2, sort_keys=True) + "\n", encoding="utf-8")


def write_markdown(path: Path, summary: dict[str, Any]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    lines = [
        "# Openllet QL Integration Evidence",
        "",
        "Execution availability only. This publication does not claim semantic conformance.",
        "",
        f"- Status: {summary['status']}",
        f"- Implementation completeness: {summary['implementation_completeness']}",
        f"- Execution availability: {summary['execution_availability']}",
        f"- Semantic correctness: {summary['semantic_correctness']}",
        f"- Release readiness: {summary['release_readiness']}",
        f"- Official QL execution statuses: {json.dumps(summary['official_ql']['execution_status_counts'], sort_keys=True, separators=(',', ':'))}",
        f"- Official QL result statuses: {json.dumps(summary['official_ql']['result_status_counts'], sort_keys=True, separators=(',', ':'))}",
        f"- Split-stack profile counts: {json.dumps(summary['split_stack']['profile_case_counts'], sort_keys=True, separators=(',', ':'))}",
        f"- Regression inventory: {summary['regression_inventory']['observed_passed']}/{summary['regression_inventory']['observed_total']}",
        "",
        "## Claim controls",
        "",
        f"- Answer key created: {str(summary['claim_controls']['answer_key_created']).lower()}",
        f"- Oracle approval created: {str(summary['claim_controls']['oracle_approval_created']).lower()}",
        f"- Expected result repaired: {str(summary['claim_controls']['expected_result_repaired']).lower()}",
        f"- Performance/scalability claims allowed: {str(summary['claim_controls']['performance_claims_allowed']).lower()}",
        "",
        "## Known blockers and gaps",
        "",
        *[f"- {item}" for item in summary["known_blockers"]],
        *[f"- {item}" for item in summary["known_validation_gaps"]],
        "",
    ]
    if summary["errors"]:
        lines.extend(["## Validation errors", "", *[f"- {error}" for error in summary["errors"]], ""])
    path.write_text("\n".join(lines), encoding="utf-8")


def parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--materialization-root", type=Path, required=True)
    parser.add_argument("--query-run", type=Path, required=True)
    parser.add_argument("--variant-control-root", type=Path, required=True)
    parser.add_argument("--inventory-receipt", type=Path, required=True)
    parser.add_argument("--issue-state-snapshot", type=Path, required=True)
    parser.add_argument("--generated-at", default=None)
    parser.add_argument("--output-json", type=Path, required=True)
    parser.add_argument("--output-markdown", type=Path, required=True)
    return parser


def main() -> int:
    args = parser().parse_args()
    summary = build_summary(args)
    write_json(args.output_json, summary)
    write_markdown(args.output_markdown, summary)
    return 0 if summary["status"] == "complete" else 1


if __name__ == "__main__":
    raise SystemExit(main())
