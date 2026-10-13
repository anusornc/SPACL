#!/usr/bin/env python3
"""Produce the pinned Openllet QL graph handoff and diagnostic evidence.

The command keeps the existing profile materialization pipeline as the graph
boundary.  Openllet is used only to export finite named/source-domain
entailments; the shared validator independently checks the graph and creates
the read-only handoff consumed by the existing materialized-graph backend.
"""

from __future__ import annotations

import argparse
import json
import os
import sys
from pathlib import Path
from types import SimpleNamespace
from typing import Any

from materialization_pipeline import PipelineError, write_json
from profile_materialization import run_profile

from openllet_ql_contract import (
    EXECUTION_VARIANT,
    EXPECTED_QUERY_COUNT,
    NOT_AVAILABLE_EXIT_CODE,
    PROFILE,
    SCRIPT_DIR,
    TIMEOUT_EXIT_CODE,
    OpenlletQlError,
    OpenlletQlTimeout,
    OpenlletQlUnavailable,
    attest_runtime,
    build_identity,
    collect_queries,
    component_manifest,
    digest,
    ql_dataset_paths,
    require_file,
    runtime_version,
    validate_acceptance_lock,
    validate_frozen_profile,
    write_query_iris,
)
from openllet_ql_diagnostic import run_diagnostic
from openllet_ql_exporter import export_graph


def run_materialization(args: argparse.Namespace) -> dict[str, Any]:
    output_dir = args.output_dir.expanduser().resolve()
    if output_dir.exists() and any(output_dir.iterdir()):
        raise OpenlletQlError(f"output directory must be empty: {output_dir}")
    output_dir.mkdir(parents=True, exist_ok=True)
    paths = ql_dataset_paths(args.dataset_root)
    frozen = validate_frozen_profile(paths)
    query_manifest = require_file(args.query_manifest, "official query manifest")
    queries = collect_queries(query_manifest, EXPECTED_QUERY_COUNT)
    acceptance_lock = validate_acceptance_lock(
        paths, frozen, query_manifest, queries
    )
    exporter = require_file(
        SCRIPT_DIR / "openllet_ql_materializer.sh", "Openllet QL exporter", executable=True
    )
    query_backend = require_file(
        args.query_backend, "materialized-graph query backend", executable=True
    )
    runtime_dir = output_dir / "runtime"
    attestation = attest_runtime(args.docker_bin, args.runtime_identity, runtime_dir)
    official_dir = output_dir / "official"
    official_dir.mkdir(parents=True, exist_ok=True)
    official_query_iris = official_dir / "query-iris.txt"
    write_query_iris(official_query_iris, queries)
    identity = build_identity(
        paths,
        frozen,
        queries,
        query_manifest,
        attestation,
        exporter,
        query_backend,
        acceptance_lock,
    )
    identity_path = official_dir / "identity.json"
    write_json(identity_path, identity)
    write_json(output_dir / "component-manifest.json", component_manifest(identity, query_manifest))

    environment = os.environ.copy()
    environment.update(
        {
            "OPENLLET_QL_DOCKER_BIN": str(args.docker_bin),
            "OPENLLET_QL_RUNTIME_ATTESTATION": str(runtime_dir / "attestation.json"),
            "OPENLLET_QL_QUERY_IRIS": str(official_query_iris),
        }
    )
    previous_environment = os.environ.copy()
    os.environ.update(environment)
    try:
        official = run_profile(
            SimpleNamespace(
                profile=PROFILE,
                source=paths["source"],
                exporter=exporter,
                identity=identity_path,
                output_dir=official_dir,
            )
        )
    finally:
        os.environ.clear()
        os.environ.update(previous_environment)

    diagnostic: dict[str, Any]
    if official.get("execution_status") == "complete":
        diagnostic = run_diagnostic(
            identity,
            paths["source"],
            exporter,
            query_backend,
            output_dir / "diagnostic",
            environment,
        )
    else:
        diagnostic = {
            "schema_version": 1,
            "status": "not_available",
            "evidence_scope": "diagnostic_observation_only",
            "reason": "official QL materialization did not produce a validated handoff",
        }
        diagnostic_dir = output_dir / "diagnostic"
        diagnostic_dir.mkdir(parents=True, exist_ok=True)
        write_json(diagnostic_dir / "result.json", diagnostic)

    status = (
        "complete"
        if official.get("execution_status") == "complete"
        and diagnostic.get("status") == "complete"
        else str(official.get("execution_status") or diagnostic.get("status") or "failed")
    )
    summary = {
        "schema_version": 1,
        "pipeline": "owl2bench-openllet-ql-materialization/1",
        "status": status,
        "execution_variant": EXECUTION_VARIANT,
        "official": official,
        "diagnostic": diagnostic,
        "runtime_attestation": {
            "path": str(runtime_dir / "attestation.json"),
            "sha256": digest(runtime_dir / "attestation.json"),
        },
        "component_manifest": {
            "path": str(output_dir / "component-manifest.json"),
            "sha256": digest(output_dir / "component-manifest.json"),
        },
        "claim_boundary": {
            "correctness_claim": "not_made",
            "performance_claim": "not_made",
            "official_result_status": "oracle_unavailable",
            "diagnostic_evidence_scope": "non_oracle_regression_only",
        },
        "release_gates": {
            "agpl_review": "pending",
            "second_host_rebuild": "required",
            "image_archive_committed": False,
            "image_archive_published": False,
        },
    }
    write_json(output_dir / "openllet_ql_materialization_summary.json", summary)
    return summary


def write_blocked_summary(output_dir: Path, status: str, reason: str) -> dict[str, Any]:
    output_dir.mkdir(parents=True, exist_ok=True)
    summary = {
        "schema_version": 1,
        "pipeline": "owl2bench-openllet-ql-materialization/1",
        "status": status,
        "execution_variant": EXECUTION_VARIANT,
        "reason": reason,
        "claim_boundary": {
            "correctness_claim": "not_made",
            "performance_claim": "not_made",
        },
    }
    write_json(output_dir / "openllet_ql_materialization_summary.json", summary)
    return summary


def parser() -> argparse.ArgumentParser:
    main = argparse.ArgumentParser(description=__doc__)
    subparsers = main.add_subparsers(dest="command", required=True)

    export_parser = subparsers.add_parser(
        "export", help="export a source-inclusive finite Openllet QL graph"
    )
    export_parser.add_argument("source", type=Path)
    export_parser.add_argument(
        "--docker-bin", default=os.environ.get("OPENLLET_QL_DOCKER_BIN", "docker")
    )
    export_parser.add_argument("--runtime-attestation", default="")
    export_parser.add_argument("--query-iris", default="")
    export_parser.set_defaults(func=export_graph)

    run_parser = subparsers.add_parser(
        "run", help="produce official and diagnostic validated QL handoffs"
    )
    run_parser.add_argument("--dataset-root", required=True, type=Path)
    run_parser.add_argument("--query-manifest", required=True, type=Path)
    run_parser.add_argument(
        "--runtime-identity",
        type=Path,
        default=SCRIPT_DIR / "openllet_ql_runtime_identity.json",
    )
    run_parser.add_argument(
        "--query-backend",
        type=Path,
        default=SCRIPT_DIR / "openllet_ql_query_engine.sh",
    )
    run_parser.add_argument(
        "--docker-bin", default=os.environ.get("OPENLLET_QL_DOCKER_BIN", "docker")
    )
    run_parser.add_argument("--output-dir", required=True, type=Path)
    run_parser.set_defaults(func=run_materialization)

    version_parser = subparsers.add_parser(
        "version", help="print the exact attested Openllet runtime version"
    )
    version_parser.add_argument("--runtime-attestation", default="")
    version_parser.set_defaults(func=runtime_version)
    return main


def main(argv: list[str] | None = None) -> int:
    args = parser().parse_args(argv)
    if args.command in {"export", "version"}:
        try:
            return int(args.func(args))
        except OpenlletQlTimeout as error:
            print(f"[openllet-ql][timeout] {error}", file=sys.stderr)
            return TIMEOUT_EXIT_CODE
        except OpenlletQlUnavailable as error:
            print(f"[openllet-ql][not_available] {error}", file=sys.stderr)
            return NOT_AVAILABLE_EXIT_CODE
        except (OSError, OpenlletQlError, PipelineError, ValueError) as error:
            print(f"[openllet-ql][failed] {error}", file=sys.stderr)
            return 1

    try:
        summary = args.func(args)
    except OpenlletQlTimeout as error:
        summary = write_blocked_summary(args.output_dir.resolve(), "timeout", str(error))
    except OpenlletQlUnavailable as error:
        summary = write_blocked_summary(
            args.output_dir.resolve(), "not_available", str(error)
        )
    except (OSError, OpenlletQlError, PipelineError, ValueError) as error:
        summary = write_blocked_summary(args.output_dir.resolve(), "failed", str(error))
    print(json.dumps(summary, ensure_ascii=False, sort_keys=True))
    if summary.get("status") == "complete":
        return 0
    if summary.get("status") == "not_available":
        return NOT_AVAILABLE_EXIT_CODE
    if summary.get("status") == "timeout":
        return TIMEOUT_EXIT_CODE
    return 1


if __name__ == "__main__":
    raise SystemExit(main())
