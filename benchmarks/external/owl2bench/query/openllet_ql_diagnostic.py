"""Diagnostic handoff and observation flow for Openllet QL."""

from __future__ import annotations

import copy
import os
import subprocess
from pathlib import Path
from typing import Any, Iterable

from canonicalize_result import canonicalize as canonicalize_query_result
from materialization_pipeline import run_export_materialization, write_json
from materialization_validator import handoff, validate
from profile_materialization import verify_determinism

from openllet_ql_contract import (
    EXECUTION_VARIANT,
    NOT_AVAILABLE_EXIT_CODE,
    PROFILE,
    SCRIPT_DIR,
    OpenlletQlError,
    OpenlletQlTimeout,
    OpenlletQlUnavailable,
    digest,
    load_json,
    require_directory,
    require_file,
    write_query_iris,
)


def diagnostic_queries() -> list[dict[str, Any]]:
    diagnostic_dir = require_directory(
        SCRIPT_DIR / "openllet_ql_diagnostic", "Openllet QL diagnostic fixture"
    )
    values = []
    for identifier, filename in (
        ("class", "class.rq"),
        ("object_property", "object-property.rq"),
    ):
        path = require_file(diagnostic_dir / filename, f"diagnostic {identifier} query")
        values.append({"id": identifier, "path": str(path), "sha256": digest(path)})
    return values


def merge_graph_files(output: Path, inputs: Iterable[Path]) -> None:
    lines: set[str] = set()
    for path in inputs:
        current = require_file(path, "diagnostic source input")
        lines.update(
            line.strip()
            for line in current.read_text(encoding="utf-8").splitlines()
            if line.strip()
        )
    output.write_text("\n".join(sorted(lines)) + "\n", encoding="utf-8")


def diagnostic_identity(
    official: dict[str, Any], source: Path, extension: Path, queries: list[dict[str, Any]]
) -> dict[str, Any]:
    identity = copy.deepcopy(official)
    source_hash = digest(source)
    extension_hash = digest(extension)
    identity["source_document"] = {
        "path": str(source),
        "sha256": source_hash,
        "revision": f"diagnostic-extension:{extension_hash}",
    }
    identity["dataset"]["id"] = f"openllet-ql-diagnostic:{source_hash}"
    identity["dataset"]["artifact_id"] = f"diagnostic-source:{source_hash}"
    identity["dataset"]["artifact_path"] = str(source)
    identity["dataset"]["artifact_revision"] = f"diagnostic-extension:{extension_hash}"
    identity["dataset"]["sha256"] = source_hash
    identity["dataset_artifact"] = {
        "id": f"diagnostic-source:{source_hash}",
        "path": str(source),
        "sha256": source_hash,
        "revision": f"diagnostic-extension:{extension_hash}",
    }
    identity["derivation"]["revision"] = extension_hash
    identity["query_workload"] = {
        "id": "openllet-ql-entailment-diagnostic",
        "profile": PROFILE,
        "query_count": len(queries),
        "queries": queries,
        "evidence_scope": "diagnostic_observation_only",
    }
    identity["diagnostic_extension"] = {
        "path": str(extension),
        "sha256": extension_hash,
        "official_corpus": False,
    }
    identity["evidence_scope"] = "diagnostic_observation_only"
    return identity


def run_query_observation(
    backend: Path,
    query: dict[str, Any],
    handoff_path: Path,
    graph_hash: str,
    output_dir: Path,
    timeout_seconds: int,
) -> dict[str, Any]:
    identifier = str(query["id"])
    raw = output_dir / f"{identifier}.tsv"
    diagnostics = output_dir / f"{identifier}.diagnostics"
    canonical = output_dir / f"{identifier}.canonical.json"
    environment = os.environ.copy()
    environment.update(
        {
            "OWL2BENCH_QUERY_MATERIALIZATION_HANDOFF": str(handoff_path),
            "OWL2BENCH_QUERY_MATERIALIZATION_GRAPH_SHA256": graph_hash,
        }
    )
    try:
        executed = subprocess.run(
            [str(backend), "query", str(query["path"])],
            env=environment,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            timeout=timeout_seconds,
            check=False,
        )
    except subprocess.TimeoutExpired as error:
        raise OpenlletQlTimeout(f"diagnostic query timed out: {identifier}") from error
    raw.write_bytes(executed.stdout)
    diagnostics.write_bytes(executed.stderr)
    if executed.returncode == NOT_AVAILABLE_EXIT_CODE:
        raise OpenlletQlUnavailable(f"diagnostic query backend is unavailable: {identifier}")
    if executed.returncode != 0:
        raise OpenlletQlError(
            f"diagnostic query failed with status {executed.returncode}: {identifier}"
        )
    try:
        canonical_document = canonicalize_query_result(
            Path(str(query["path"])), raw
        )
    except (OSError, UnicodeDecodeError, ValueError) as error:
        raise OpenlletQlError(f"diagnostic result is malformed: {identifier}: {error}") from error
    write_json(canonical, canonical_document)
    row_count = canonical_document.get("row_count")
    if isinstance(row_count, bool) or not isinstance(row_count, int) or row_count < 1:
        raise OpenlletQlError(f"diagnostic query did not observe its obligation: {identifier}")
    return {
        "status": "observed",
        "row_count": row_count,
        "canonical_result": {
            "path": str(canonical),
            "sha256": digest(canonical),
            "result_hash": canonical_document.get("result_hash"),
        },
    }


def run_diagnostic(
    official_identity: dict[str, Any],
    official_source: Path,
    exporter: Path,
    query_backend: Path,
    output_dir: Path,
    environment: dict[str, str],
) -> dict[str, Any]:
    output_dir.mkdir(parents=True, exist_ok=True)
    extension = require_file(
        SCRIPT_DIR / "openllet_ql_diagnostic" / "extension.nq",
        "Openllet QL diagnostic extension",
    )
    source = output_dir / "source.nq"
    merge_graph_files(source, (official_source, extension))
    queries = diagnostic_queries()
    query_iris_path = output_dir / "query-iris.txt"
    write_query_iris(query_iris_path, queries)
    identity = diagnostic_identity(official_identity, source, extension, queries)
    identity_path = output_dir / "identity.json"
    write_json(identity_path, identity)

    diagnostic_environment = environment.copy()
    diagnostic_environment["OPENLLET_QL_QUERY_IRIS"] = str(query_iris_path)
    previous_environment = os.environ.copy()
    os.environ.update(diagnostic_environment)
    try:
        first = run_export_materialization(
            PROFILE, source, exporter, identity_path, output_dir
        )
        if first.get("status") != "candidate":
            result = {
                "schema_version": 1,
                "status": first.get("status", "failed"),
                "evidence_scope": "diagnostic_observation_only",
                "reason": first.get("reason", "diagnostic export did not produce a candidate"),
            }
            write_json(output_dir / "result.json", result)
            return result
        graph = output_dir / "graph.nq"
        candidate = output_dir / "candidate_graph.nq"
        manifest = output_dir / "materialization.json"
        determinism = verify_determinism(
            PROFILE,
            exporter,
            source,
            output_dir,
            graph,
            int(identity["resource_limits"]["timeout_seconds"]),
        )
        manifest_document = load_json(manifest, "diagnostic materialization manifest")
        record = manifest_document["profiles"][PROFILE]
        record["determinism"] = determinism
        record["graph_contract"]["two_run_determinism_required"] = True
        record["execution_variant"] = EXECUTION_VARIANT
        record["diagnostic_extension"] = copy.deepcopy(identity["diagnostic_extension"])
        record["evidence_scope"] = "diagnostic_observation_only"
        if determinism.get("status") != "complete":
            record["status"] = determinism.get("status", "failed")
            record["reason"] = determinism.get("reason", "diagnostic determinism failed")
        manifest_document["manifest_id"] = "openllet-ql-entailment-diagnostic"
        write_json(manifest, manifest_document)
        validation_path = output_dir / "validation.json"
        validation = validate(PROFILE, source, candidate, graph, manifest)
        write_json(validation_path, validation)
        if validation.get("status") != "complete":
            result = {
                "schema_version": 1,
                "status": validation.get("status", "failed"),
                "evidence_scope": "diagnostic_observation_only",
                "reasons": validation.get("reasons", []),
            }
            write_json(output_dir / "result.json", result)
            return result
        handoff_path = output_dir / "handoff.json"
        handoff_record = handoff(validation_path, graph)
        write_json(handoff_path, handoff_record)
        if handoff_record.get("status") != "ready":
            raise OpenlletQlError("diagnostic handoff is not ready")
        graph_hash = str(handoff_record["graph_sha256"])
        observation_dir = output_dir / "query-results"
        observation_dir.mkdir(parents=True, exist_ok=True)
        observations = {
            str(query["id"]): run_query_observation(
                query_backend,
                query,
                handoff_path,
                graph_hash,
                observation_dir,
                int(identity["resource_limits"]["timeout_seconds"]),
            )
            for query in queries
        }
        result = {
            "schema_version": 1,
            "status": "complete",
            "evidence_scope": "diagnostic_observation_only",
            "extension": copy.deepcopy(identity["diagnostic_extension"]),
            "source": {"path": str(source), "sha256": digest(source)},
            "candidate": {"path": str(candidate), "sha256": digest(candidate)},
            "graph": {"path": str(graph), "sha256": digest(graph)},
            "determinism": {
                "runs": 2,
                "first_graph_sha256": determinism["first_graph"]["sha256"],
                "second_graph_sha256": determinism["second_graph"]["sha256"],
            },
            "independent_obligations": validation.get("independent_obligations", {}),
            "observations": observations,
            "handoff": {"path": str(handoff_path), "sha256": digest(handoff_path)},
        }
        write_json(output_dir / "result.json", result)
        return result
    finally:
        os.environ.clear()
        os.environ.update(previous_environment)
