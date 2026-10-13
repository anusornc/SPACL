#!/usr/bin/env python3
"""Focused CLI tests for Openllet QL variant-control gates."""

from __future__ import annotations

import hashlib
import json
import subprocess
import tempfile
from pathlib import Path


SCRIPT_DIR = Path(__file__).resolve().parent
ATTESTOR = SCRIPT_DIR / "openllet_ql_runtime_identity.py"
IDENTITY = SCRIPT_DIR / "openllet_ql_runtime_identity.json"
ACCEPTANCE_LOCK = SCRIPT_DIR / "openllet_ql_acceptance_lock.json"


def sha256(path: Path) -> str:
    return "sha256:" + hashlib.sha256(path.read_bytes()).hexdigest()


def write_json(path: Path, value: dict) -> Path:
    path.write_text(json.dumps(value, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    return path


def run_validate(tmp: Path, *args: str) -> tuple[int, dict]:
    output = tmp / f"validation-{len(list(tmp.glob('validation-*.json')))}.json"
    command = ["python3", str(ATTESTOR), "validate-variant", "--output", str(output), *args]
    status = subprocess.run(command, check=False).returncode
    return status, json.loads(output.read_text(encoding="utf-8"))


def compute_declared_identity(tmp: Path, identity: dict, revision_id: str) -> Path:
    candidate = dict(identity)
    candidate.pop("variant_control", None)
    path = write_json(tmp / f"{revision_id.replace('/', '-')}-undeclared.json", candidate)
    _, result = run_validate(tmp, "--identity", str(path))
    candidate["variant_control"] = {
        "revision_id": revision_id,
        "category_sha256": result["variant_revision"]["category_sha256"],
    }
    return write_json(tmp / f"{revision_id.replace('/', '-')}.json", candidate)


def write_evidence_manifest(
    tmp: Path,
    revision_id: str,
    category_sha256: str,
    identity: dict,
    release_readiness: str = "blocked",
) -> Path:
    acceptance = json.loads(ACCEPTANCE_LOCK.read_text(encoding="utf-8"))
    workload = acceptance["query_workload"]
    graph_sha256 = "sha256:" + "9" * 64
    diagnostic = write_json(
        tmp / "diagnostic.json",
        {
            "status": "complete",
            "independent_obligations": {
                "expected_inferred_count": 2,
                "expected_inferred_assertion_counts": {
                    "class": 1,
                    "object_property": 1,
                },
            },
            "observations": {
                "class": {"status": "observed"},
                "object_property": {"status": "observed"},
            },
        },
    )
    runtime_attestation = write_json(
        tmp / "runtime-attestation.json",
        {
            "status": "available",
            "component": identity["component"],
            "runtime": identity["runtime"],
            "classpath": identity["classpath"],
            "inputs": identity["inputs"],
            "resource_limits": identity["resource_limits"],
        },
    )
    component_manifest = write_json(
        tmp / "component-manifest.json",
        {
            "schema_version": 1,
            "profiles": {
                "QL": {
                    "execution_variant": "openllet-ql",
                    "components": {
                        "engine": {
                            "name": "Openllet",
                            "version": identity["component"]["openllet_version"],
                        }
                    },
                    "configuration": {
                        "digest": identity["inputs"]["configuration"]["sha256"]
                    },
                    "ruleset": {
                        "revision": identity["runtime"]["variant_revision"],
                        "artifact_digest": identity["runtime"]["image"]["digest"],
                    },
                    "runtime": {
                        "variant_revision": identity["runtime"]["variant_revision"],
                        "base_image": identity["runtime"]["base_image"],
                        "image": identity["runtime"]["image"],
                        "dependencies": identity["runtime"]["dependencies"],
                        "classpath": {
                            "lock_sha256": identity["classpath"]["lock_sha256"],
                            "jar_count": identity["classpath"]["jar_count"],
                        },
                        "attestation": {
                            "path": str(runtime_attestation),
                            "sha256": sha256(runtime_attestation),
                        },
                    },
                }
            },
        },
    )
    materialization = write_json(
        tmp / "materialization.json",
        {
            "status": "complete",
            "execution_variant": "openllet-ql",
            "official": {
                "execution_status": "complete",
                "status": "oracle_unavailable",
                "canonical_graph": {"sha256": graph_sha256},
            },
            "diagnostic": json.loads(diagnostic.read_text(encoding="utf-8")),
            "component_manifest": {
                "path": str(component_manifest),
                "sha256": sha256(component_manifest),
            },
            "runtime_attestation": {
                "path": str(runtime_attestation),
                "sha256": sha256(runtime_attestation),
            },
        },
    )
    result_contract = write_json(
        tmp / "result-contract.json",
        {
            "status": "valid",
            "errors": [],
            "validator": "owl2bench-profile-result-contract/1",
            "run_dir": str(tmp),
            "result_count": 60,
            "result_status_counts": {
                "not_available": 50,
                "oracle_unavailable": 10,
            },
        },
    )
    report = tmp / "report.md"
    report.write_text(
        "# OWL2Bench Query Report\n\n"
        f"- Variant revision: `{revision_id}`\n"
        f"- Variant category SHA: `{category_sha256}`\n"
        "- Implementation completeness: **complete**\n"
        f"- Release readiness: **{release_readiness}**\n"
        "- Execution availability: **complete**\n"
        "- Semantic correctness: **blocked**\n"
        '- Variant official QL status counts: {"oracle_unavailable":10}\n'
        '| QL | 10 | 10 | blocked | {"oracle_unavailable":10} |\n',
        encoding="utf-8",
    )
    ql_results = []
    for index, query in enumerate(workload["queries"]):
        result = write_json(
            tmp / f"ql-{index:02d}.json",
            {
                "case_id": f"{query['id']}__QL",
                "query_id": query["id"],
                "query_profile": "QL",
                "source_query_sha256": query["sha256"].removeprefix("sha256:"),
                "source_corpus": workload["source_corpus"],
                "source_revision": workload["source_revision"],
                "execution_status": "success",
                "result_status": "oracle_unavailable",
                "engine_provenance": {"adapter": "openllet-ql"},
                "component_manifest": {
                    "path": str(component_manifest),
                    "sha256": sha256(component_manifest).removeprefix("sha256:"),
                    "execution_variant": "openllet-ql",
                    "components": {
                        "engine": {
                            "name": "Openllet",
                            "version": identity["component"]["openllet_version"],
                        }
                    },
                    "configuration": {
                        "digest": identity["inputs"]["configuration"]["sha256"]
                    },
                    "ruleset": {
                        "revision": identity["runtime"]["variant_revision"],
                        "artifact_digest": identity["runtime"]["image"]["digest"],
                    },
                    "runtime": {
                        "image": identity["runtime"]["image"],
                        "dependencies": identity["runtime"]["dependencies"],
                    },
                },
                "materialization": {
                    "execution_variant": "openllet-ql",
                    "output_graph": {"sha256": graph_sha256},
                },
                "canonicalization": {"status": "available"},
                "query_time_ms": index,
            },
        )
        ql_results.append({"path": str(result), "sha256": sha256(result)})
    return write_json(
        tmp / "evidence-manifest.json",
        {
            "revision_id": revision_id,
            "category_sha256": category_sha256,
            "diagnostic": {"path": str(diagnostic), "sha256": sha256(diagnostic)},
            "runtime_attestation": {
                "path": str(runtime_attestation),
                "sha256": sha256(runtime_attestation),
            },
            "component_manifest": {
                "path": str(component_manifest),
                "sha256": sha256(component_manifest),
            },
            "materialization": {"path": str(materialization), "sha256": sha256(materialization)},
            "result_contract": {"path": str(result_contract), "sha256": sha256(result_contract)},
            "report_markdown": {"path": str(report), "sha256": sha256(report)},
            "ql_results": ql_results,
        },
    )


def write_second_host_manifest(tmp: Path, comparison: dict) -> Path:
    report = write_json(tmp / "second-host-comparison.json", comparison)
    return write_json(
        tmp / "second-host-evidence-manifest.json",
        {"comparison_report": {"path": str(report), "sha256": sha256(report)}},
    )


def main() -> int:
    source_identity = json.loads(IDENTITY.read_text(encoding="utf-8"))
    with tempfile.TemporaryDirectory() as tmp_name:
        tmp = Path(tmp_name)
        status, missing_evidence = run_validate(tmp, "--identity", str(IDENTITY))
        assert status == 1
        assert missing_evidence["status"] == "blocked"
        assert missing_evidence["selection"]["status"] == "blocked"
        assert "evidence_manifest.missing" in missing_evidence["selection"]["reasons"]

        mutation_cases = {
            "engine": ("component", "openllet_version", "2.6.6"),
            "base_image": ("runtime", "base_image", "digest", "sha256:" + "1" * 64),
            "image": ("runtime", "image", "platform", "linux/arm64"),
            "classpath": ("classpath", "jar_count", 79),
            "exporter": ("inputs", "exporter", "sha256", "sha256:" + "2" * 64),
            "configuration": ("inputs", "configuration", "sha256", "sha256:" + "3" * 64),
            "resource": ("resource_limits", "memory", "25GiB"),
            "graph_contract": ("graph_contract", "query_count", 9),
        }
        for category, mutation in mutation_cases.items():
            changed = json.loads(json.dumps(source_identity))
            target = changed
            for key in mutation[:-2]:
                target = target[key]
            target[mutation[-2]] = mutation[-1]
            changed_path = write_json(tmp / f"{category}-stale-declaration.json", changed)
            status, result = run_validate(tmp, "--identity", str(changed_path))
            assert status == 1
            assert result["status"] == "blocked"
            assert "variant_control.category_sha256_mismatch" in result["selection"]["reasons"]

        mutable_base = json.loads(json.dumps(source_identity))
        mutable_base["runtime"]["base_image"]["reference"] = (
            "maven:3.9-eclipse-temurin-17:latest"
        )
        mutable_base_path = write_json(tmp / "mutable-base-reference.json", mutable_base)
        status, mutable_base_result = run_validate(
            tmp, "--identity", str(mutable_base_path)
        )
        assert status == 1
        assert "base_image.reference_mutable" in mutable_base_result["selection"][
            "reasons"
        ]

        mismatched_base = json.loads(json.dumps(source_identity))
        mismatched_base["runtime"]["base_image"]["reference"] = (
            "maven:3.9-eclipse-temurin-17@sha256:" + "0" * 64
        )
        mismatched_base_path = write_json(
            tmp / "mismatched-base-reference.json", mismatched_base
        )
        status, mismatched_base_result = run_validate(
            tmp, "--identity", str(mismatched_base_path)
        )
        assert status == 1
        assert "base_image.reference_digest_mismatch" in mismatched_base_result[
            "selection"
        ]["reasons"]

        changed_dependency = json.loads(json.dumps(source_identity))
        changed_dependency["runtime"]["dependencies"][2]["digest"] = (
            "sha256:" + "4" * 64
        )
        changed_dependency_path = write_json(
            tmp / "changed-dependency.json", changed_dependency
        )
        status, changed_dependency_result = run_validate(
            tmp, "--identity", str(changed_dependency_path)
        )
        assert status == 1
        assert "variant_control.category_sha256_mismatch" in changed_dependency_result[
            "selection"
        ]["reasons"]

        changed = json.loads(json.dumps(source_identity))
        changed["component"]["openllet_version"] = "2.6.6"
        reused_revision = compute_declared_identity(
            tmp, changed, "openllet-ql/revision-1"
        )
        status, reused = run_validate(tmp, "--identity", str(reused_revision))
        assert status == 1
        assert "variant_control.revision_id_reused" in reused["selection"]["reasons"]
        declared_changed = compute_declared_identity(tmp, changed, "openllet-ql/revision-2")
        status, changed_no_evidence = run_validate(tmp, "--identity", str(declared_changed))
        assert status == 1
        assert "evidence_manifest.missing" in changed_no_evidence["selection"]["reasons"]
        assert "variant_control.revision_id_unregistered" in changed_no_evidence[
            "selection"
        ]["reasons"]
        changed_revision = changed_no_evidence["variant_revision"]
        unregistered_dir = tmp / "unregistered-revision-artifacts"
        unregistered_dir.mkdir()
        unregistered_evidence = write_evidence_manifest(
            unregistered_dir,
            changed_revision["revision_id"],
            changed_revision["category_sha256"],
            changed,
        )
        status, unregistered = run_validate(
            tmp,
            "--identity",
            str(declared_changed),
            "--evidence-manifest",
            str(unregistered_evidence),
        )
        assert status == 1
        assert "variant_control.revision_id_unregistered" in unregistered[
            "selection"
        ]["reasons"]

        revision = missing_evidence["variant_revision"]

        unofficial_dir = tmp / "unofficial-artifacts"
        unofficial_dir.mkdir()
        unofficial_evidence = write_evidence_manifest(
            unofficial_dir,
            revision["revision_id"],
            revision["category_sha256"],
            source_identity,
        )
        unofficial_manifest = json.loads(unofficial_evidence.read_text(encoding="utf-8"))
        first_result = Path(unofficial_manifest["ql_results"][0]["path"])
        first_document = json.loads(first_result.read_text(encoding="utf-8"))
        first_document["source_query_sha256"] = "sha256:" + "0" * 64
        write_json(first_result, first_document)
        unofficial_manifest["ql_results"][0]["sha256"] = sha256(first_result)
        write_json(unofficial_evidence, unofficial_manifest)
        status, unofficial = run_validate(
            tmp,
            "--identity",
            str(IDENTITY),
            "--evidence-manifest",
            str(unofficial_evidence),
        )
        assert status == 1
        assert any(
            reason.endswith("source_query_sha256_mismatch")
            for reason in unofficial["evidence"]["reasons"]
        )

        stale_runtime_dir = tmp / "stale-runtime-artifacts"
        stale_runtime_dir.mkdir()
        stale_runtime_evidence = write_evidence_manifest(
            stale_runtime_dir,
            revision["revision_id"],
            revision["category_sha256"],
            source_identity,
        )
        stale_runtime_manifest = json.loads(
            stale_runtime_evidence.read_text(encoding="utf-8")
        )
        runtime_attestation = Path(stale_runtime_manifest["runtime_attestation"]["path"])
        runtime_document = json.loads(runtime_attestation.read_text(encoding="utf-8"))
        runtime_document["inputs"]["configuration"]["sha256"] = "sha256:" + "0" * 64
        write_json(runtime_attestation, runtime_document)
        stale_runtime_manifest["runtime_attestation"]["sha256"] = sha256(
            runtime_attestation
        )
        write_json(stale_runtime_evidence, stale_runtime_manifest)
        status, stale_runtime = run_validate(
            tmp,
            "--identity",
            str(IDENTITY),
            "--evidence-manifest",
            str(stale_runtime_evidence),
        )
        assert status == 1
        assert "runtime_attestation.configuration_mismatch" in stale_runtime[
            "evidence"
        ]["reasons"]

        missing_counts_dir = tmp / "missing-official-counts"
        missing_counts_dir.mkdir()
        missing_counts_evidence = write_evidence_manifest(
            missing_counts_dir,
            revision["revision_id"],
            revision["category_sha256"],
            source_identity,
        )
        missing_counts_manifest = json.loads(
            missing_counts_evidence.read_text(encoding="utf-8")
        )
        first_result = Path(missing_counts_manifest["ql_results"][0]["path"])
        first_document = json.loads(first_result.read_text(encoding="utf-8"))
        first_document["result_status"] = "failed"
        write_json(first_result, first_document)
        missing_counts_manifest["ql_results"][0]["sha256"] = sha256(first_result)
        write_json(missing_counts_evidence, missing_counts_manifest)
        status, missing_counts = run_validate(
            tmp,
            "--identity",
            str(IDENTITY),
            "--evidence-manifest",
            str(missing_counts_evidence),
        )
        assert status == 1
        assert "ql_results[0].result_not_oracle_unavailable" in missing_counts[
            "evidence"
        ]["reasons"]
        assert "ql_results.official_status_counts_mismatch" in missing_counts[
            "evidence"
        ]["reasons"]

        valid_dir = tmp / "valid-artifacts"
        valid_dir.mkdir()
        evidence = write_evidence_manifest(
            valid_dir,
            revision["revision_id"],
            revision["category_sha256"],
            source_identity,
        )
        status, selected = run_validate(
            tmp,
            "--identity",
            str(IDENTITY),
            "--evidence-manifest",
            str(evidence),
        )
        assert status == 0
        assert selected["selection"]["status"] == "complete"
        assert selected["release_readiness"] == "blocked"

        missing_report_manifest = json.loads(evidence.read_text(encoding="utf-8"))
        del missing_report_manifest["report_markdown"]
        missing_report_path = write_json(
            tmp / "missing-report-evidence-manifest.json", missing_report_manifest
        )
        status, missing_report = run_validate(
            tmp,
            "--identity",
            str(IDENTITY),
            "--evidence-manifest",
            str(missing_report_path),
        )
        assert status == 1
        assert "report_markdown.path_missing" in missing_report["evidence"]["reasons"]

        foreign_report_dir = tmp / "foreign-report-artifacts"
        foreign_report_dir.mkdir()
        foreign_report_evidence = write_evidence_manifest(
            foreign_report_dir,
            revision["revision_id"],
            revision["category_sha256"],
            source_identity,
        )
        foreign_report_manifest = json.loads(
            foreign_report_evidence.read_text(encoding="utf-8")
        )
        original_report = Path(foreign_report_manifest["report_markdown"]["path"])
        foreign_report = tmp / "foreign-report.md"
        foreign_report.write_text(original_report.read_text(encoding="utf-8"), encoding="utf-8")
        foreign_report_manifest["report_markdown"] = {
            "path": str(foreign_report),
            "sha256": sha256(foreign_report),
        }
        write_json(foreign_report_evidence, foreign_report_manifest)
        status, foreign_report_result = run_validate(
            tmp,
            "--identity",
            str(IDENTITY),
            "--evidence-manifest",
            str(foreign_report_evidence),
        )
        assert status == 1
        assert "report_markdown.outside_result_contract_run_dir" in foreign_report_result[
            "evidence"
        ]["reasons"]
        assert foreign_report_result["evidence"]["core_status"] == "complete"
        assert foreign_report_result["evidence"]["report_status"] == "failed"

        unexpected_manifest = json.loads(evidence.read_text(encoding="utf-8"))
        unexpected_manifest["run_summary"] = {
            "path": str(evidence),
            "sha256": sha256(evidence),
        }
        unexpected_path = write_json(
            tmp / "unexpected-evidence-manifest.json", unexpected_manifest
        )
        status, unexpected = run_validate(
            tmp,
            "--identity",
            str(IDENTITY),
            "--evidence-manifest",
            str(unexpected_path),
        )
        assert status == 1
        assert "evidence_manifest.unexpected_field:run_summary" in unexpected[
            "evidence"
        ]["reasons"]

        wrong_report_dir = tmp / "wrong-report-artifacts"
        wrong_report_dir.mkdir()
        wrong_report_evidence = write_evidence_manifest(
            wrong_report_dir,
            revision["revision_id"],
            revision["category_sha256"],
            source_identity,
        )
        wrong_report_manifest = json.loads(
            wrong_report_evidence.read_text(encoding="utf-8")
        )
        wrong_report = Path(wrong_report_manifest["report_markdown"]["path"])
        wrong_report.write_text(
            wrong_report.read_text(encoding="utf-8").replace(
                revision["revision_id"], "openllet-ql/revision-999"
            ),
            encoding="utf-8",
        )
        wrong_report_manifest["report_markdown"]["sha256"] = sha256(wrong_report)
        write_json(wrong_report_evidence, wrong_report_manifest)
        status, wrong_report_result = run_validate(
            tmp,
            "--identity",
            str(IDENTITY),
            "--evidence-manifest",
            str(wrong_report_evidence),
        )
        assert status == 1
        assert "report_markdown.revision_mismatch" in wrong_report_result[
            "evidence"
        ]["reasons"]

        second_host = selected["evidence"]["release_comparison"]
        second_host_path = write_second_host_manifest(tmp, second_host)
        agpl_review = write_json(
            tmp / "agpl-review.json",
            {
                "status": "approved",
                "reviewer": "release-reviewer@example.test",
                "reviewed_at": "2026-09-01T00:00:00Z",
                "revision_id": revision["revision_id"],
                "category_sha256": revision["category_sha256"],
            },
        )
        release_ready_dir = tmp / "release-ready-artifacts"
        release_ready_dir.mkdir()
        release_ready_evidence = write_evidence_manifest(
            release_ready_dir,
            revision["revision_id"],
            revision["category_sha256"],
            source_identity,
            release_readiness="complete",
        )
        status, release_ready = run_validate(
            tmp,
            "--identity",
            str(IDENTITY),
            "--evidence-manifest",
            str(release_ready_evidence),
            "--second-host-evidence-manifest",
            str(second_host_path),
            "--agpl-review-evidence",
            str(agpl_review),
        )
        assert status == 0
        assert release_ready["release_readiness"] == "complete"
        assert release_ready["pre_release_gates"]["agpl_review"]["status"] == "complete"

        mismatch = dict(second_host)
        mismatch["build_inputs_sha256"] = "sha256:" + "a" * 64
        mismatch_dir = tmp / "second-host-mismatch"
        mismatch_dir.mkdir()
        mismatch_path = write_second_host_manifest(mismatch_dir, mismatch)
        status, second_host_failed = run_validate(
            tmp,
            "--identity",
            str(IDENTITY),
            "--evidence-manifest",
            str(evidence),
            "--second-host-evidence-manifest",
            str(mismatch_path),
        )
        assert status == 0
        assert second_host_failed["release_readiness"] == "blocked"
        gate = second_host_failed["pre_release_gates"]["second_host_rebuild"]
        assert gate["status"] == "failed"
        assert "build_inputs_sha256" in gate["mismatches"]
        assert gate["rewrite_expected_results_allowed"] is False
        assert gate["rewrite_oracle_data_allowed"] is False

        self_approved = json.loads(json.dumps(source_identity))
        self_approved["license"]["distribution_review_status"] = "approved"
        self_approved_path = write_json(tmp / "self-approved.json", self_approved)
        self_approved_artifacts = tmp / "self-approved-artifacts"
        self_approved_artifacts.mkdir()
        status, self_approved_result = run_validate(
            tmp,
            "--identity",
            str(self_approved_path),
            "--evidence-manifest",
            str(write_evidence_manifest(
                self_approved_artifacts,
                source_identity["variant_control"]["revision_id"],
                source_identity["variant_control"]["category_sha256"],
                source_identity,
            )),
        )
        assert status == 0
        assert self_approved_result["release_readiness"] == "blocked"
        assert self_approved_result["pre_release_gates"]["agpl_review"]["reason"] == "pending_human_review"

        forbidden_review = write_json(
            tmp / "forbidden-agpl-review.json",
            {
                "status": "approved",
                "reviewer": "release-reviewer@example.test",
                "reviewed_at": "2026-09-01T00:00:00Z",
                "revision_id": revision["revision_id"],
                "category_sha256": revision["category_sha256"],
                "Legal_Document": "must not be emitted",
            },
        )
        status, forbidden = run_validate(
            tmp,
            "--identity",
            str(IDENTITY),
            "--evidence-manifest",
            str(evidence),
            "--agpl-review-evidence",
            str(forbidden_review),
        )
        assert status == 0
        assert "forbidden_sensitive_or_legal_payload" in forbidden["pre_release_gates"]["agpl_review"]["reasons"]
        assert "Legal_Document" not in json.dumps(forbidden)

        malformed_manifest = tmp / "malformed-manifest.json"
        malformed_manifest.write_text("{bad json\n", encoding="utf-8")
        status, malformed = run_validate(
            tmp,
            "--identity",
            str(IDENTITY),
            "--evidence-manifest",
            str(malformed_manifest),
        )
        assert status == 1
        assert malformed["evidence"]["status"] == "not_available"
        assert "evidence_manifest.malformed" in malformed["evidence"]["reasons"]

        missing_identity = tmp / "missing-identity.json"
        status, missing = run_validate(tmp, "--identity", str(missing_identity))
        assert status == 125
        assert missing["status"] == "not_available"
        assert "identity.missing" in missing["reasons"]
    print("openllet ql variant-control tests passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
