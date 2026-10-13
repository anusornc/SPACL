#!/usr/bin/env python3
"""Run a pinned EL, QL, or RL materialization/export handoff.

The profile materializer is deliberately an adapter seam.  A provisioned
reasoner/ruleset exporter is supplied as an executable with the contract
``export <source-nquads>`` and writes candidate N-Quads to stdout.  This
module owns identity checks, canonicalization, a second deterministic export,
independent structural validation, and the read-only graph handoff.  It never
creates or accepts an answer key.
"""

from __future__ import annotations

import argparse
import json
import subprocess
import time
from pathlib import Path
from typing import Any

from materialization_manifest import file_digest
from materialization_pipeline import (
    MissingInputError,
    PipelineError,
    load_identity,
    require_file,
    run_export_materialization,
    write_json,
)
from materialization_validator import handoff, validate
from rdf_canonicalization import canonicalize_file, write_canonical


PROFILE_COUNTS = {"EL": 10, "QL": 10, "RL": 18}
PROFILE_COMPONENTS = {
    "EL": "ELK",
    "QL": "QL-ruleset-engine",
    "RL": "RL-ruleset-engine",
}
NOT_AVAILABLE_EXIT_CODE = 125
UNSUPPORTED_EXIT_CODE = 123
TIMEOUT_EXIT_CODE = 124


def digest_value(value: Any) -> str:
    return str(value).strip().removeprefix("sha256:").lower()


def nonempty(value: Any) -> bool:
    return isinstance(value, str) and bool(value.strip())


def require_digest(value: Any, label: str) -> str:
    digest = str(value).strip() if isinstance(value, str) else ""
    if not digest_value(digest) or len(digest_value(digest)) != 64:
        raise PipelineError(f"{label} must be a SHA-256 digest")
    if any(character not in "0123456789abcdef" for character in digest_value(digest).lower()):
        raise PipelineError(f"{label} must be a SHA-256 digest")
    return digest


def resolve_path(value: Any, label: str) -> Path:
    if not nonempty(value):
        raise MissingInputError(f"{label} path is missing")
    return require_file(Path(str(value)), label)


def validate_query_workload(identity: dict[str, Any], profile: str) -> None:
    workload = identity.get("query_workload")
    if not isinstance(workload, dict):
        raise PipelineError("query_workload identity is required")
    expected_count = PROFILE_COUNTS[profile]
    if workload.get("query_count") != expected_count:
        raise PipelineError(
            f"query_workload.query_count must be {expected_count} for {profile}"
        )
    queries = workload.get("queries")
    if not isinstance(queries, list) or len(queries) != expected_count:
        raise PipelineError(
            f"query_workload must contain exactly {expected_count} queries for {profile}"
        )
    for index, query in enumerate(queries):
        if not isinstance(query, dict):
            raise PipelineError(f"query_workload query {index} is invalid")
        path = resolve_path(query.get("path"), f"query_workload query {index}")
        actual = file_digest(str(path))
        if digest_value(query.get("sha256")) != digest_value(actual):
            raise PipelineError(f"query_workload query {index} hash does not match its path")


def validate_profile_identity(
    profile: str, source: Path, exporter: Path, identity_path: Path
) -> dict[str, Any]:
    identity = load_identity(require_file(identity_path, "profile identity manifest"))
    if identity.get("profile") != profile:
        raise PipelineError("profile identity does not match the requested profile")
    component = identity.get("component")
    if not isinstance(component, dict) or component.get("name") != PROFILE_COMPONENTS[profile]:
        raise PipelineError(
            f"{profile} component must be {PROFILE_COMPONENTS[profile]}"
        )
    identity_exporter = resolve_path(identity.get("exporter", {}).get("path"), "exporter")
    if identity_exporter != exporter.resolve():
        raise PipelineError("identity exporter path does not match --exporter")
    source = source.resolve()
    source_document = identity.get("source_document")
    if not isinstance(source_document, dict):
        raise PipelineError("source_document identity is required")
    source_identity_path = resolve_path(source_document.get("path"), "source_document")
    if source_identity_path != source:
        raise PipelineError("source_document path does not match --source")
    source_hash = file_digest(str(source))
    if digest_value(source_document.get("sha256")) != digest_value(source_hash):
        raise PipelineError("source_document hash does not match --source")

    required_sections = (
        "dataset",
        "generator",
        "generator_configuration",
        "profile_tbox",
        "profile_abox",
        "dataset_artifact",
        "implementation",
    )
    missing = [section for section in required_sections if section not in identity]
    if missing:
        raise PipelineError("profile identity sections are missing: " + ", ".join(missing))
    context = identity.get("profile_context")
    if not isinstance(context, dict) or not nonempty(context.get("directory")):
        raise PipelineError("profile_context.directory is required")
    context_dir = Path(str(context["directory"])).expanduser().resolve()
    if not context_dir.is_dir() or context_dir.name != profile:
        raise MissingInputError(f"profile context is missing or misnamed: {context_dir}")
    dataset = identity.get("dataset")
    if not isinstance(dataset, dict) or dataset.get("profile") != profile:
        raise PipelineError("dataset profile identity is invalid")
    for field in ("id", "artifact_id", "artifact_path", "artifact_revision", "tbox_id", "abox_id"):
        if not nonempty(dataset.get(field)):
            raise PipelineError(f"dataset.{field} is required")
    for field in ("sha256", "source_ontology_sha256", "tbox_sha256", "abox_sha256"):
        require_digest(dataset.get(field), f"dataset.{field}")
    dataset_artifact = identity.get("dataset_artifact")
    if not isinstance(dataset_artifact, dict):
        raise PipelineError("dataset_artifact identity is required")
    dataset_artifact_path = resolve_path(dataset_artifact.get("path"), "dataset_artifact")
    if digest_value(dataset_artifact.get("sha256")) != digest_value(file_digest(str(dataset_artifact_path))):
        raise PipelineError("dataset_artifact hash does not match its path")
    if digest_value(dataset.get("sha256")) != digest_value(dataset_artifact.get("sha256")):
        raise PipelineError("dataset hash does not match dataset_artifact")
    if digest_value(dataset.get("sha256")) != digest_value(source_hash):
        raise PipelineError("dataset conversion hash does not match --source")

    generator = identity.get("generator")
    if not isinstance(generator, dict) or not nonempty(generator.get("revision")):
        raise PipelineError("generator.revision is required")
    seed = generator.get("seed")
    if isinstance(seed, bool) or not isinstance(seed, int) or seed < 0:
        raise PipelineError("generator.seed must be a non-negative integer")
    if profile in {"QL", "RL"}:
        ruleset = identity.get("ruleset")
        if not isinstance(ruleset, dict):
            raise PipelineError(f"{profile} ruleset identity is required")
        for field in ("id", "version", "revision", "conformance_boundary"):
            if not nonempty(ruleset.get(field)):
                raise PipelineError(f"ruleset.{field} is required")
        require_digest(ruleset.get("artifact_digest"), "ruleset.artifact_digest")
        if ruleset.get("path") is not None:
            ruleset_path = resolve_path(ruleset.get("path"), "ruleset")
            if digest_value(ruleset.get("artifact_digest")) != digest_value(file_digest(str(ruleset_path))):
                raise PipelineError("ruleset hash does not match its path")
    validate_query_workload(identity, profile)
    identity["two_run_determinism_required"] = True
    identity["profile_capability_boundary"] = profile
    if profile in {"QL", "RL"}:
        identity["ruleset_required"] = True
    return identity


def export_again(
    exporter: Path,
    source: Path,
    output_dir: Path,
    timeout_seconds: int,
) -> tuple[str, Path, Path | None, str]:
    output_dir.mkdir(parents=True, exist_ok=True)
    candidate = output_dir / "candidate_graph.nq"
    diagnostics = output_dir / "export.diagnostics"
    try:
        result = subprocess.run(
            [str(exporter), "export", str(source)],
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            timeout=timeout_seconds,
            check=False,
        )
    except subprocess.TimeoutExpired:
        return "timeout", diagnostics, None, "profile exporter exceeded the declared timeout"
    diagnostics.write_bytes(result.stderr)
    if result.returncode == TIMEOUT_EXIT_CODE:
        return "timeout", diagnostics, None, "profile exporter reported a timeout"
    if result.returncode == NOT_AVAILABLE_EXIT_CODE:
        return "not_available", diagnostics, None, "profile exporter declared runtime unavailable"
    if result.returncode == UNSUPPORTED_EXIT_CODE:
        return "unsupported", diagnostics, None, "profile exporter declared capability unsupported"
    if result.returncode != 0:
        return "failed", diagnostics, None, f"profile exporter failed with status {result.returncode}"
    if not result.stdout.strip():
        return "failed", diagnostics, None, "profile exporter returned an empty graph"
    candidate.write_bytes(result.stdout)
    return "complete", diagnostics, candidate, ""


def verify_determinism(
    profile: str,
    exporter: Path,
    source: Path,
    output_dir: Path,
    first_graph: Path,
    timeout_seconds: int,
) -> dict[str, Any]:
    status, diagnostics, candidate, reason = export_again(
        exporter, source, output_dir / "determinism", timeout_seconds
    )
    record: dict[str, Any] = {
        "profile": profile,
        "runs": 2,
        "status": status,
        "first_graph": {"path": str(first_graph), "sha256": file_digest(str(first_graph))},
        "diagnostics": str(diagnostics),
    }
    if candidate is None:
        record["reason"] = reason
        return record
    second_graph = output_dir / "determinism" / "graph.nq"
    try:
        second_bytes = canonicalize_file(candidate)
        second_hash = write_canonical(second_graph, second_bytes)
    except (OSError, UnicodeDecodeError, ValueError) as error:
        record["status"] = "failed"
        record["reason"] = f"second exporter graph could not be canonicalized: {error}"
        return record
    first_bytes = first_graph.read_bytes()
    record["second_graph"] = {"path": str(second_graph), "sha256": second_hash}
    if first_bytes != second_bytes:
        record["status"] = "failed"
        record["reason"] = "canonical graph differs between the two exporter runs"
    else:
        record["status"] = "complete"
    return record


def blocked_summary(
    profile: str, output_dir: Path, status: str, reason: str, source: Path
) -> dict[str, Any]:
    manifest_path = output_dir / "materialization.json"
    summary_path = output_dir / "profile_materialization_summary.json"
    write_json(
        manifest_path,
        {
            "schema_version": 1,
            "manifest_id": f"{profile.lower()}-materialization",
            "profiles": {profile: {"profile": profile, "status": status, "reason": reason}},
        },
    )
    summary = {
        "schema_version": 1,
        "pipeline": f"owl2bench-{profile.lower()}-materialization/1",
        "profile": profile,
        "status": status,
        "execution_status": status,
        "reason": reason,
        "source": {"path": str(source.expanduser().resolve())},
        "manifest": str(manifest_path),
        "validator": "not_ready",
    }
    write_json(summary_path, summary)
    return summary


def run_profile(args: argparse.Namespace) -> dict[str, Any]:
    profile = args.profile
    output_dir = args.output_dir.expanduser().resolve()
    output_dir.mkdir(parents=True, exist_ok=True)
    source = require_file(args.source, f"{profile} source graph")
    exporter = require_file(args.exporter, f"{profile} materializer/exporter")
    identity = validate_profile_identity(profile, source, exporter, args.identity)
    resources = identity.get("resource_limits")
    if not isinstance(resources, dict):
        raise PipelineError("resource_limits identity is required")
    timeout_seconds = resources.get("timeout_seconds")
    if isinstance(timeout_seconds, bool) or not isinstance(timeout_seconds, int) or timeout_seconds <= 0:
        raise PipelineError("resource_limits.timeout_seconds must be a positive integer")
    materialization_started = time.monotonic()
    first = run_export_materialization(profile, source, exporter, args.identity, output_dir)
    materialization_time_ms = round((time.monotonic() - materialization_started) * 1000, 3)
    if first.get("status") != "candidate":
        return {
            **first,
            "execution_status": first.get("status"),
            "pipeline": f"owl2bench-{profile.lower()}-materialization/1",
            "timing": {"export_materialization_time_ms": materialization_time_ms},
        }
    candidate = output_dir / "candidate_graph.nq"
    graph = output_dir / "graph.nq"
    manifest = output_dir / "materialization.json"
    validation_path = output_dir / "validation.json"
    handoff_path = output_dir / "handoff.json"
    determinism_started = time.monotonic()
    determinism = verify_determinism(
        profile, exporter, source, output_dir, graph, timeout_seconds
    )
    determinism_time_ms = round((time.monotonic() - determinism_started) * 1000, 3)
    manifest_document = json.loads(manifest.read_text(encoding="utf-8"))
    record = manifest_document["profiles"][profile]
    record["determinism"] = determinism
    record["graph_contract"]["two_run_determinism_required"] = True
    if determinism.get("status") != "complete":
        record["status"] = determinism.get("status", "failed")
        record["reason"] = determinism.get("reason", "determinism verification failed")
    write_json(manifest, manifest_document)

    validation_started = time.monotonic()
    validation = validate(profile, source, candidate, graph, manifest)
    validation_time_ms = round((time.monotonic() - validation_started) * 1000, 3)
    write_json(validation_path, validation)
    handoff_record: dict[str, Any] | None = None
    if validation.get("status") == "complete":
        handoff_record = handoff(validation_path, graph)
        write_json(handoff_path, handoff_record)
    execution_status = validation.get("status", "failed")
    if handoff_record is not None and handoff_record.get("status") != "ready":
        execution_status = handoff_record.get("status", "failed")
    total_time_ms = round((time.monotonic() - materialization_started) * 1000, 3)
    timing = {
        "export_materialization_time_ms": materialization_time_ms,
        "determinism_time_ms": determinism_time_ms,
        "validation_time_ms": validation_time_ms,
        "total_time_ms": total_time_ms,
    }
    manifest_document = json.loads(manifest.read_text(encoding="utf-8"))
    manifest_document["profiles"][profile]["timing"] = timing
    write_json(manifest, manifest_document)
    summary = {
        "schema_version": 1,
        "pipeline": f"owl2bench-{profile.lower()}-materialization/1",
        "profile": profile,
        "status": "oracle_unavailable" if execution_status == "complete" else execution_status,
        "execution_status": execution_status,
        "source": {"path": str(source), "sha256": file_digest(str(source))},
        "candidate_graph": {"path": str(candidate), "sha256": file_digest(str(candidate))},
        "canonical_graph": {"path": str(graph), "sha256": file_digest(str(graph))},
        "determinism": determinism,
        "timing": timing,
        "manifest": str(manifest),
        "validation": str(validation_path),
        "handoff": str(handoff_path) if handoff_record is not None else None,
        "oracle_policy": "no_answer_key_oracle",
    }
    write_json(output_dir / "profile_materialization_summary.json", summary)
    return summary


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("run", choices=("run",))
    parser.add_argument("--profile", required=True, choices=sorted(PROFILE_COUNTS))
    parser.add_argument("--source", required=True, type=Path)
    parser.add_argument("--exporter", required=True, type=Path)
    parser.add_argument("--identity", required=True, type=Path)
    parser.add_argument("--output-dir", required=True, type=Path)
    args = parser.parse_args(argv)
    try:
        summary = run_profile(args)
    except MissingInputError as error:
        summary = blocked_summary(
            args.profile,
            args.output_dir.expanduser().resolve(),
            "not_available",
            str(error),
            args.source,
        )
    except (OSError, PipelineError, ValueError) as error:
        summary = blocked_summary(
            args.profile,
            args.output_dir.expanduser().resolve(),
            "failed",
            str(error),
            args.source,
        )
    print(json.dumps(summary, ensure_ascii=False, sort_keys=True))
    return 0 if summary.get("execution_status") == "complete" else 1


if __name__ == "__main__":
    raise SystemExit(main())
