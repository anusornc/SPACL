#!/usr/bin/env python3
"""Acceptance-publication tests for the Openllet QL integration evidence seam."""

from __future__ import annotations

import json
import subprocess
import sys
import tempfile
from pathlib import Path


SCRIPT_DIR = Path(__file__).resolve().parent
sys.path.insert(0, str(SCRIPT_DIR))

from openllet_ql_publish_acceptance import (  # noqa: E402
    EXPECTED_INVENTORY_GENERATED_BY,
    EXPECTED_ISSUES,
    EXPECTED_TEST_PATHS,
    digest,
    repo_root,
)


PUBLISHER = SCRIPT_DIR / "openllet_ql_publish_acceptance.py"
GOOD_DIGEST = "sha256:" + "1" * 64
GENERATED_AT = "2026-09-01T11:36:31Z"


def write_json(path: Path, value: dict) -> Path:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    return path


def write_text(path: Path, value: str) -> Path:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(value, encoding="utf-8")
    return path


def build_inventory_receipt(path: Path, mutate: str | None = None) -> Path:
    root = repo_root()
    tests = [
        {
            "script": script,
            "script_sha256": digest(root / script),
            "exit_code": 0,
            "status": "complete",
            "stdout_sha256": "sha256:" + "2" * 64,
            "stderr_sha256": "sha256:" + "3" * 64,
        }
        for script in EXPECTED_TEST_PATHS
    ]
    if mutate == "duplicate":
        tests.append(dict(tests[0]))
    elif mutate == "missing":
        tests.pop()
    elif mutate == "tampered":
        tests[0]["script_sha256"] = "sha256:" + "4" * 64
    elif mutate == "failed":
        tests[0]["exit_code"] = 1
        tests[0]["status"] = "failed"
    elif mutate == "missing_output_hash":
        tests[0].pop("stdout_sha256")
    generated_by = EXPECTED_INVENTORY_GENERATED_BY
    runner_sha256 = digest(SCRIPT_DIR / EXPECTED_INVENTORY_GENERATED_BY)
    if mutate == "bad_runner":
        runner_sha256 = "sha256:" + "5" * 64
    generated_at = "2026-09-01T11:37:31Z" if mutate == "time_mismatch" else GENERATED_AT
    return write_json(
        path,
        {
            "schema_version": 1,
            "generated_at": generated_at,
            "generated_by": generated_by,
            "runner_sha256": runner_sha256,
            "tests": tests,
        },
    )


def build_issue_snapshot(path: Path, mutate: str | None = None) -> Path:
    issues = [
        {
            "number": number,
            "title": f"Issue {number}",
            "state": "OPEN",
            "updated_at": "2026-09-01T10:00:00Z",
        }
        for number in sorted(EXPECTED_ISSUES)
    ]
    if mutate == "closed":
        issues[0]["state"] = "CLOSED"
    elif mutate == "missing":
        issues.pop()
    operator = {
        "attestor": "test",
        "attested_at": GENERATED_AT,
        "broader_issues_mutated": False,
    }
    if mutate == "operator_missing":
        operator = {}
    return write_json(
        path,
        {
            "schema_version": 1,
            "captured_at": GENERATED_AT,
            "issues": issues,
            "operator_attestation": operator,
        },
    )


def build_fixture(tmp: Path) -> tuple[Path, Path, Path, Path, Path]:
    materialization_root = tmp / "materialization"
    query_run = tmp / "query-run"
    control_root = tmp / "variant-control"
    inventory_receipt = build_inventory_receipt(tmp / "inventory-receipt.json")
    issue_snapshot = build_issue_snapshot(tmp / "issue-snapshot.json")
    root = repo_root()
    identity = json.loads((root / "benchmarks/external/owl2bench/query/openllet_ql_runtime_identity.json").read_text(encoding="utf-8"))
    acceptance_lock = json.loads((root / "benchmarks/external/owl2bench/query/openllet_ql_acceptance_lock.json").read_text(encoding="utf-8"))
    workload = acceptance_lock["query_workload"]
    runtime_identity = identity["runtime"]
    classpath_identity = identity["classpath"]
    classpath_entries = []
    for line in (root / classpath_identity["path"]).read_text(encoding="utf-8").splitlines():
        jar_sha, jar_name = line.split()
        classpath_entries.append({"name": jar_name, "sha256": "sha256:" + jar_sha})
    categories = {
        "engine": {
            "implementation": identity["component"]["implementation"],
            "openllet_version": identity["component"]["openllet_version"],
            "owlapi_version": identity["component"]["owlapi_version"],
            "runtime_revision": runtime_identity["variant_revision"],
        },
        "base_image": runtime_identity["base_image"],
        "image": runtime_identity["image"],
        "dependencies": runtime_identity["dependencies"],
        "classpath": classpath_identity,
        "exporter": identity["inputs"]["exporter"],
        "configuration": identity["inputs"]["configuration"],
        "resource": identity["resource_limits"],
        "graph_contract": identity["graph_contract"],
    }
    source_graph = write_text(
        materialization_root / "diagnostic" / "source.nq",
        "<urn:a> <urn:p> <urn:b> .\n<urn:a> <urn:type> <urn:C> .\n",
    )
    candidate_graph = write_text(
        materialization_root / "diagnostic" / "candidate_graph.nq",
        "<urn:a> <urn:p> <urn:b> .\n<urn:a> <urn:type> <urn:C> .\n<urn:b> <urn:type> <urn:C> .\n",
    )
    graph = write_text(materialization_root / "diagnostic" / "graph.nq", candidate_graph.read_text(encoding="utf-8"))
    second_graph = write_text(materialization_root / "diagnostic" / "determinism" / "graph.nq", graph.read_text(encoding="utf-8"))
    official_graph_sha = "sha256:" + "4" * 64
    write_json(
        materialization_root / "diagnostic" / "validation.json",
        {
            "schema_version": 1,
            "status": "complete",
            "reasons": [],
            "source": {"path": str(source_graph), "sha256": digest(source_graph)},
            "candidate_graph": {"path": str(candidate_graph), "sha256": digest(candidate_graph)},
            "graph": {"path": str(graph), "sha256": digest(graph)},
            "determinism": {
                "runs": 2,
                "first_graph": {"path": str(graph), "sha256": digest(graph)},
                "second_graph": {"path": str(second_graph), "sha256": digest(second_graph)},
            },
        },
    )

    write_json(
        materialization_root / "runtime" / "attestation.json",
        {
            "status": "available",
            "inputs": identity["inputs"],
            "runtime": {
                **runtime_identity,
                "image": {**runtime_identity["image"], "local_attestation": "matched"},
                "execution_network": "none",
                "materialization_network": "none",
                "service_endpoint": "none",
                "archive": {"committed": False, "published": False},
            },
            "classpath": {
                **classpath_identity,
                "jars": classpath_entries,
            },
        },
    )
    write_json(
        materialization_root / "openllet_ql_materialization_summary.json",
        {
            "status": "complete",
            "runtime_attestation": {"artifact_id": "runtime_attestation", "sha256": GOOD_DIGEST},
            "diagnostic": {
                "status": "complete",
                "source": {"path": str(source_graph), "sha256": digest(source_graph)},
                "candidate": {"path": str(candidate_graph), "sha256": digest(candidate_graph)},
                "graph": {"path": str(graph), "sha256": digest(graph)},
                "determinism": {
                    "runs": 2,
                    "first_graph_sha256": digest(graph),
                    "second_graph_sha256": digest(second_graph),
                },
                "observations": {
                    "class": {"status": "observed"},
                    "object_property": {"status": "observed"},
                },
                "independent_obligations": {
                    "expected_inferred_count": 2,
                    "expected_inferred_assertion_counts": {"class": 1, "object_property": 1},
                },
            },
            "official": {
                "execution_status": "complete",
                "status": "oracle_unavailable",
                "source": {"sha256": "sha256:" + "5" * 64},
                "canonical_graph": {"sha256": official_graph_sha},
            },
        },
    )
    write_json(
        query_run / "summary.json",
        {
            "query_count": 60,
            "profile_case_counts": {"DL": 22, "EL": 10, "QL": 10, "RL": 18},
            "query_gate": "blocked",
            "correctness_gate": "blocked",
            "performance_claims_allowed": False,
            "variant_control": {
                "status": "complete",
                "implementation_completeness": "complete",
                "execution_availability": "complete",
                "release_readiness": "blocked",
                "semantic_correctness": "blocked",
                "revision_id": "openllet-ql/revision-1",
                "category_sha256": GOOD_DIGEST,
                "official_status_counts": {"oracle_unavailable": 10},
            },
        },
    )
    write_text(query_run / "report.md", "# OWL2Bench Query Report\n")
    ql_source_hashes = {
        query["id"]: query["sha256"].removeprefix("sha256:")
        for query in workload["queries"]
    }
    non_ql_rows = json.loads(
        (root / "benchmarks/external/owl2bench/query/openllet_ql_non_ql_projection_lock.json").read_text(encoding="utf-8")
    )["projection"]["rows"]
    for profile, count in {"DL": 22, "EL": 10, "QL": 10, "RL": 18}.items():
        for index in range(count):
            query_id = sorted(ql_source_hashes)[index] if profile == "QL" else f"{profile}-{index:02d}"
            case_id = f"{profile}-{index:02d}"
            row = {
                "case_id": case_id,
                "status": "success" if profile == "QL" else "not_available",
                "query_profile": profile,
                "query_id": query_id,
                "source_query_sha256": ql_source_hashes[query_id] if profile == "QL" else "not-used",
                "source_corpus": workload["source_corpus"],
                "source_revision": workload["source_revision"],
                "execution_status": "success" if profile == "QL" else "not_available",
                "result_status": "oracle_unavailable" if profile == "QL" else "not_available",
                "engine_provenance": {"adapter": "openllet-ql"} if profile == "QL" else None,
                "component_manifest": (
                    {
                        "status": "available",
                        "execution_variant": "openllet-ql",
                        "manifest_identity": "sha256:" + "6" * 64,
                    }
                    if profile == "QL"
                    else {"status": "not_available"}
                ),
                "materialization": (
                    {
                        "status": "complete",
                        "execution_variant": "openllet-ql",
                        "materialization_identity": "sha256:" + "7" * 64,
                        "output_sha256": official_graph_sha,
                        "output_graph": {"sha256": official_graph_sha},
                        "runtime": {"image": runtime_identity["image"], "dependencies": runtime_identity["dependencies"]},
                    }
                    if profile == "QL"
                    else {"status": "not_available"}
                ),
                "materialization_handoff": None,
            }
            if profile != "QL":
                locked = non_ql_rows.pop(0)
                row.update(
                    {
                        "case_id": locked["case_id"],
                        "query_id": locked["query_id"],
                        "query_profile": locked["profile"],
                        "source_query_sha256": locked["source_query_sha256"],
                        "source_corpus": locked["source_corpus"],
                        "source_revision": locked["source_revision"],
                    }
                )
            write_json(query_run / f"{profile}-{index:02d}.json", row)
    write_json(
        control_root / "variant-control-final.json",
        {
            "status": "complete",
            "implementation_completeness": "complete",
            "execution_availability": "complete",
            "semantic_correctness": "blocked",
            "release_readiness": "blocked",
            "variant_revision": {
                "revision_id": "openllet-ql/revision-1",
                "category_sha256": GOOD_DIGEST,
                "categories": categories,
            },
            "evidence": {
                "status": "complete",
                "core_status": "complete",
                "report_status": "complete",
                "release_comparison": {
                    "image_digest": runtime_identity["image"]["digest"],
                    "dependency_lock_sha256": classpath_identity["lock_sha256"],
                    "category_sha256": GOOD_DIGEST,
                    "build_inputs_sha256": "ignored-until-reset-below",
                    "image_input_sha256": "ignored-until-reset-below",
                    "official_status_counts": {"oracle_unavailable": 10},
                },
            },
            "pre_release_gates": {
                "agpl_review": {"status": "blocked", "reason": "pending_human_review"},
                "second_host_rebuild": {"status": "blocked", "reason": "second_host_evidence_manifest.missing"},
            },
        },
    )
    control = json.loads((control_root / "variant-control-final.json").read_text(encoding="utf-8"))
    control["variant_revision"]["category_sha256"] = "sha256:" + __import__("hashlib").sha256(
        json.dumps(categories, sort_keys=True, separators=(",", ":")).encode("utf-8")
    ).hexdigest()
    release = control["evidence"]["release_comparison"]
    release["category_sha256"] = control["variant_revision"]["category_sha256"]
    release["build_inputs_sha256"] = "sha256:" + __import__("hashlib").sha256(
        json.dumps(
            {
                key: categories[key]
                for key in ("base_image", "dependencies", "classpath", "exporter", "configuration")
            },
            sort_keys=True,
            separators=(",", ":"),
        ).encode("utf-8")
    ).hexdigest()
    release["image_input_sha256"] = "sha256:" + __import__("hashlib").sha256(
        json.dumps({"base_image": categories["base_image"], "image": categories["image"]}, sort_keys=True, separators=(",", ":")).encode("utf-8")
    ).hexdigest()
    write_json(control_root / "variant-control-final.json", control)
    write_json(control_root / "evidence-manifest.json", {"revision_id": "openllet-ql/revision-1", "category_sha256": GOOD_DIGEST})
    return materialization_root, query_run, control_root, inventory_receipt, issue_snapshot


def run_publisher(tmp: Path, *args: str, suffix: str = "") -> tuple[int, dict, str]:
    output_json = tmp / f"acceptance{suffix}.json"
    output_markdown = tmp / f"acceptance{suffix}.md"
    status = subprocess.run(
        [
            "python3",
            str(PUBLISHER),
            "--output-json",
            str(output_json),
            "--output-markdown",
            str(output_markdown),
            *args,
        ],
        check=False,
    ).returncode
    return status, json.loads(output_json.read_text(encoding="utf-8")), output_markdown.read_text(encoding="utf-8")


def mutate_json(path: Path, mutator) -> None:
    value = json.loads(path.read_text(encoding="utf-8"))
    mutator(value)
    write_json(path, value)


def publisher_args(
    materialization_root: Path,
    query_run: Path,
    control_root: Path,
    inventory_receipt: Path,
    issue_snapshot: Path,
) -> list[str]:
    return [
        "--materialization-root",
        str(materialization_root),
        "--query-run",
        str(query_run),
        "--variant-control-root",
        str(control_root),
        "--inventory-receipt",
        str(inventory_receipt),
        "--issue-state-snapshot",
        str(issue_snapshot),
        "--generated-at",
        GENERATED_AT,
    ]


def assert_no_host_paths(summary: dict, markdown: str) -> None:
    serialized = json.dumps(summary, sort_keys=True)
    assert "/tmp/" not in serialized
    assert str(Path(tempfile.gettempdir())) not in serialized
    assert '"path"' not in serialized
    assert "/tmp/" not in markdown


def main() -> int:
    with tempfile.TemporaryDirectory() as tmp_name:
        tmp = Path(tmp_name)
        materialization_root, query_run, control_root, inventory_receipt, issue_snapshot = build_fixture(tmp)
        args = publisher_args(materialization_root, query_run, control_root, inventory_receipt, issue_snapshot)
        status, summary, markdown = run_publisher(tmp, *args, suffix="-one")
        status_2, summary_2, markdown_2 = run_publisher(tmp, *args, suffix="-two")
        assert status == 0
        assert status_2 == 0
        assert summary == summary_2
        assert markdown == markdown_2
        assert_no_host_paths(summary, markdown)
        assert summary["generated_at"] == GENERATED_AT
        assert summary["status"] == "complete"
        assert summary["claim_boundary"] == "execution_available_only"
        assert summary["semantic_correctness"] == "blocked"
        assert summary["release_readiness"] == "blocked"
        assert summary["official_ql"]["execution_status_counts"] == {"success": 10}
        assert summary["official_ql"]["result_status_counts"] == {"oracle_unavailable": 10}
        assert summary["split_stack"]["profile_case_counts"] == {"DL": 22, "EL": 10, "QL": 10, "RL": 18}
        assert summary["split_stack"]["non_ql_openllet_selection_count"] == 0
        assert summary["diagnostic_evidence"]["determinism"]["runs"] == 2
        assert summary["diagnostic_evidence"]["determinism"]["first_equals_second_equals_validated"] is True
        assert summary["diagnostic_evidence"]["term_domain"]["extra_term_count"] == 0
        assert summary["diagnostic_evidence"]["term_domain"]["extra_blank_node_count"] == 0
        assert summary["diagnostic_evidence"]["term_domain"]["extra_iri_count"] == 0
        assert summary["diagnostic_evidence"]["term_domain"]["extra_literal_count"] == 0
        assert summary["runtime_source"]["checks"]["attestation_inputs_match_identity"] is True
        assert summary["runtime_source"]["checks"]["build_inputs_sha_matches_release_comparison"] is True
        assert summary["runtime_source"]["checks"]["image_input_sha_matches_release_comparison"] is True
        assert summary["runtime_source"]["checks"]["category_sha_matches_revision"] is True
        assert summary["non_ql_projection"]["row_count"] == 50
        assert summary["non_ql_projection"]["checks"]["projection_digest_matches_lock"] is True
        assert summary["claim_controls"]["answer_key_created"] is False
        assert summary["claim_controls"]["performance_claims_allowed"] is False
        assert summary["identities"]["decision"]["execution_variant"] == "openllet-ql"
        assert summary["identities"]["component"]["implementation"] == "Openllet"
        assert len(summary["identities"]["dependencies"]) == 5
        assert len(summary["identities"]["query"]) == 10
        assert summary["identities"]["query"][0]["artifact_id"].startswith("query_result:")
        assert summary["official_ql_bindings"]["lock"]["artifact_id"] == "ql_acceptance_lock"
        assert summary["identities"]["query"][0]["graph"]["sha256"] == "sha256:" + "4" * 64
        assert summary["regression_inventory"]["observed_passed"] == 29
        assert summary["issue_state"]["issues"][0]["state"] == "OPEN"
        assert "parent_issues_unmodified" not in summary
        assert "github_mutation_performed" not in json.dumps(summary, sort_keys=True)
        assert summary["issue_state"]["operator_attestation"]["broader_issues_mutated"] is False
        assert "Execution availability only" in markdown
        assert "Semantic correctness: blocked" in markdown

        validation_path = materialization_root / "diagnostic" / "validation.json"
        for term_name, extra_line in {
            "literal": '<urn:a> <urn:p> "generated literal" .\n',
            "blank": "_:generatedBlank <urn:p> <urn:a> .\n",
        }.items():
            original_graph = (materialization_root / "diagnostic" / "graph.nq").read_text(encoding="utf-8")
            generated_graph = original_graph + extra_line
            for graph_path in (
                materialization_root / "diagnostic" / "candidate_graph.nq",
                materialization_root / "diagnostic" / "graph.nq",
                materialization_root / "diagnostic" / "determinism" / "graph.nq",
            ):
                write_text(graph_path, generated_graph)
            mutate_json(
                validation_path,
                lambda value: (
                    value["candidate_graph"].update({"sha256": digest(materialization_root / "diagnostic" / "candidate_graph.nq")}),
                    value["graph"].update({"sha256": digest(materialization_root / "diagnostic" / "graph.nq")}),
                    value["determinism"]["first_graph"].update({"sha256": digest(materialization_root / "diagnostic" / "graph.nq")}),
                    value["determinism"]["second_graph"].update({"sha256": digest(materialization_root / "diagnostic" / "determinism" / "graph.nq")}),
                ),
            )
            status, failed_diagnostic, _ = run_publisher(tmp, *args, suffix=f"-diagnostic-{term_name}")
            assert status == 1
            assert "diagnostic.generated_terms_present" in failed_diagnostic["errors"]
            expected_count = "extra_literal_count" if term_name == "literal" else "extra_blank_node_count"
            assert failed_diagnostic["diagnostic_evidence"]["term_domain"][expected_count] == 1
            materialization_root, query_run, control_root, inventory_receipt, issue_snapshot = build_fixture(tmp / f"reset-{term_name}")
            args = publisher_args(materialization_root, query_run, control_root, inventory_receipt, issue_snapshot)

        mutate_json(
            materialization_root / "runtime" / "attestation.json",
            lambda value: value["inputs"]["dockerfile"].update({"sha256": "sha256:" + "8" * 64}),
        )
        status, failed_runtime, _ = run_publisher(tmp, *args, suffix="-runtime-input")
        assert status == 1
        assert "runtime_source.attestation_inputs_match_identity" in failed_runtime["errors"]
        materialization_root, query_run, control_root, inventory_receipt, issue_snapshot = build_fixture(tmp / "reset-runtime")
        args = publisher_args(materialization_root, query_run, control_root, inventory_receipt, issue_snapshot)

        mutate_json(
            control_root / "variant-control-final.json",
            lambda value: value["evidence"]["release_comparison"].update({"build_inputs_sha256": "sha256:" + "9" * 64}),
        )
        status, failed_release_hash, _ = run_publisher(tmp, *args, suffix="-release-hash")
        assert status == 1
        assert "runtime_source.build_inputs_sha_matches_release_comparison" in failed_release_hash["errors"]
        materialization_root, query_run, control_root, inventory_receipt, issue_snapshot = build_fixture(tmp / "reset-release")
        args = publisher_args(materialization_root, query_run, control_root, inventory_receipt, issue_snapshot)

        non_ql_result = next(
            path
            for path in sorted(query_run.glob("*.json"))
            if path.name != "summary.json"
            and json.loads(path.read_text(encoding="utf-8")).get("query_profile") != "QL"
        )
        mutate_json(non_ql_result, lambda value: value.update({"source_query_sha256": "tampered"}))
        status, failed_non_ql, _ = run_publisher(tmp, *args, suffix="-non-ql-source")
        assert status == 1
        assert "non_ql_projection.projection_digest_matches_lock" in failed_non_ql["errors"]
        materialization_root, query_run, control_root, inventory_receipt, issue_snapshot = build_fixture(tmp / "reset-nonql")
        args = publisher_args(materialization_root, query_run, control_root, inventory_receipt, issue_snapshot)

        ql_result = query_run / "QL-00.json"
        row = json.loads(ql_result.read_text(encoding="utf-8"))
        row["result_status"] = "passed"
        write_json(ql_result, row)
        status, failed, _ = run_publisher(tmp, *args, suffix="-bad-result")
        assert status == 1
        assert "official_ql.result_status_counts_not_oracle_unavailable" in failed["errors"]

        row["result_status"] = "oracle_unavailable"
        write_json(ql_result, row)
        mutate_json(ql_result, lambda value: value["materialization"].update({"output_sha256": "sha256:" + "a" * 64}))
        status, failed_ql_graph, _ = run_publisher(tmp, *args, suffix="-ql-graph")
        assert status == 1
        assert "official_ql.QL-00.official_graph_matches" in failed_ql_graph["errors"]
        mutate_json(ql_result, lambda value: value["materialization"].update({"output_sha256": "sha256:" + "4" * 64}))
        mutate_json(ql_result, lambda value: value.update({"source_query_sha256": "tampered"}))
        status, failed_ql_source, _ = run_publisher(tmp, *args, suffix="-ql-source")
        assert status == 1
        assert "official_ql.QL-00.source_hash_matches" in failed_ql_source["errors"]
        mutate_json(ql_result, lambda value: value.update({"source_query_sha256": summary["identities"]["query"][0]["source_query_sha256"]}))
        for mutation, expected_error in {
            "duplicate": "inventory_receipt.duplicate_scripts",
            "missing": "inventory_receipt.script_set_mismatch",
            "tampered": "inventory_receipt.tests[0].script_sha256_mismatch",
            "failed": "inventory_receipt.tests[0].not_complete",
            "bad_runner": "inventory_receipt.runner_sha256_mismatch",
            "missing_output_hash": "inventory_receipt.tests[0].stdout_sha256_invalid",
            "time_mismatch": "inventory_receipt.generated_at_mismatch",
        }.items():
            bad_receipt = build_inventory_receipt(tmp / f"inventory-{mutation}.json", mutation)
            status, failed_inventory, _ = run_publisher(
                tmp,
                *publisher_args(materialization_root, query_run, control_root, bad_receipt, issue_snapshot),
                suffix=f"-inventory-{mutation}",
            )
            assert status == 1
            assert expected_error in failed_inventory["errors"]

        for mutation, expected_error in {
            "closed": "issue_state_snapshot.issues[0].not_open",
            "missing": "issue_state_snapshot.issue_set_mismatch",
            "operator_missing": "issue_state_snapshot.broader_issue_mutation_attestation_missing",
        }.items():
            bad_snapshot = build_issue_snapshot(tmp / f"issue-{mutation}.json", mutation)
            status, failed_issue_state, _ = run_publisher(
                tmp,
                *publisher_args(materialization_root, query_run, control_root, inventory_receipt, bad_snapshot),
                suffix=f"-issue-{mutation}",
            )
            assert status == 1
            assert expected_error in failed_issue_state["errors"]

        status, failed_time, _ = run_publisher(
            tmp,
            *args[:-1],
            "2026-09-01 11:36:31",
            suffix="-bad-time",
        )
        assert status == 1
        assert "generated_at.invalid" in failed_time["errors"]
    print("openllet ql acceptance publication tests passed")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
