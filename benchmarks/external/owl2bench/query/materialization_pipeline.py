#!/usr/bin/env python3
"""Run profile materialization through a testable public seam.

The shared runner accepts a candidate graph from an external exporter, writes
an immutable canonical graph artifact, and records only execution provenance.
It does not reason, query, or create an oracle.
"""

from __future__ import annotations

import argparse
import json
import subprocess
import sys
from pathlib import Path
from typing import Any

from materialization_manifest import file_digest as file_hash
from rdf_canonicalization import canonicalize_file, write_canonical
from split_stack_manifest import forbidden_field_paths


PROFILES = {"DL", "EL", "QL", "RL"}
NOT_AVAILABLE_EXIT_CODE = 125
UNSUPPORTED_EXIT_CODE = 123
TIMEOUT_EXIT_CODE = 124
REQUIRED_IDENTITY_SECTIONS = (
    "component",
    "exporter",
    "query_backend",
    "configuration",
    "derivation",
    "runtime",
    "resource_limits",
    "profile_context",
)


class PipelineError(ValueError):
    """The fake pipeline input does not satisfy its execution contract."""


class MissingInputError(PipelineError):
    """A required external input is absent and cannot be executed."""


def nonempty(value: Any) -> bool:
    return isinstance(value, str) and bool(value.strip())


def load_identity(path: Path) -> dict[str, Any]:
    try:
        document = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, UnicodeDecodeError, json.JSONDecodeError) as error:
        raise PipelineError(f"identity manifest is malformed: {error}") from error
    if not isinstance(document, dict):
        raise PipelineError("identity manifest root must be an object")
    forbidden = forbidden_field_paths(document)
    if forbidden:
        raise PipelineError(f"identity contains forbidden fields: {', '.join(forbidden)}")
    missing = [section for section in REQUIRED_IDENTITY_SECTIONS if section not in document]
    if missing:
        raise PipelineError(f"identity sections are missing: {', '.join(missing)}")
    backend = document.get("query_backend")
    if not isinstance(backend, dict) or backend.get("read_only") is not True:
        raise PipelineError("query_backend.read_only must be true")
    for section in ("component", "exporter", "query_backend"):
        artifact = document.get(section)
        if not isinstance(artifact, dict):
            raise PipelineError(f"{section} identity must be an object")
        artifact_path = artifact.get("path")
        if not isinstance(artifact_path, str) or not artifact_path.strip():
            raise PipelineError(f"{section}.path is required")
        actual_digest = file_hash(str(require_file(Path(artifact_path), f"{section} artifact")))
        if actual_digest != artifact.get("artifact_digest"):
            raise PipelineError(f"{section}.artifact_digest does not match its path")
    for section, path_key, digest_key in (
        ("source_document", "path", "sha256"),
        ("generator", "artifact_path", "artifact_digest"),
        ("implementation", "path", "sha256"),
        ("profile_tbox", "path", "sha256"),
        ("profile_abox", "path", "sha256"),
        ("dataset_artifact", "path", "sha256"),
        ("generated_dataset", "path", "sha256"),
        ("generator_configuration", "path", "sha256"),
    ):
        artifact = document.get(section)
        if artifact is None:
            continue
        if not isinstance(artifact, dict):
            raise PipelineError(f"{section} identity must be an object")
        artifact_path = artifact.get(path_key)
        if not isinstance(artifact_path, str) or not artifact_path.strip():
            raise PipelineError(f"{section}.{path_key} is required")
        actual_digest = file_hash(
            str(require_file(Path(artifact_path), f"{section} artifact"))
        )
        if actual_digest != artifact.get(digest_key):
            raise PipelineError(f"{section}.{digest_key} does not match its path")
    if document.get("component", {}).get("name") == "HermiT":
        for section in (
            "profile_tbox",
            "profile_abox",
            "dataset_artifact",
            "generator_configuration",
        ):
            artifact = document.get(section)
            if not isinstance(artifact, dict) or not nonempty(artifact.get("revision")):
                raise PipelineError(f"{section}.revision is required")
        backend_manifest = document.get("query_backend", {}).get("identity_manifest")
        if not isinstance(backend_manifest, dict):
            raise PipelineError("query_backend.identity_manifest is required")
        manifest_path = backend_manifest.get("path")
        if not isinstance(manifest_path, str) or not manifest_path.strip():
            raise PipelineError("query_backend.identity_manifest.path is required")
        manifest_file = require_file(Path(manifest_path), "query backend identity manifest")
        if file_hash(str(manifest_file)) != backend_manifest.get("sha256"):
            raise PipelineError(
                "query_backend.identity_manifest.sha256 does not match its path"
            )
    classpath = document.get("runtime_classpath")
    if classpath is not None:
        if not isinstance(classpath, dict) or not isinstance(classpath.get("artifacts"), list):
            raise PipelineError("runtime_classpath artifacts are required")
        for index, artifact in enumerate(classpath["artifacts"]):
            if not isinstance(artifact, dict):
                raise PipelineError(f"runtime_classpath artifact {index} is invalid")
            for field in ("name", "version", "revision", "path", "sha256"):
                if not isinstance(artifact.get(field), str) or not artifact[field].strip():
                    raise PipelineError(f"runtime_classpath artifact {index}.{field} is required")
            actual_digest = file_hash(
                str(require_file(Path(artifact["path"]), "runtime classpath artifact"))
            )
            if actual_digest != artifact["sha256"]:
                raise PipelineError(
                    f"runtime_classpath artifact {index}.sha256 does not match its path"
                )
    return document


def require_file(path: Path, label: str) -> Path:
    resolved = path.expanduser().resolve()
    if not resolved.is_file():
        raise MissingInputError(f"{label} is missing: {resolved}")
    return resolved


def write_json(path: Path, document: dict[str, Any]) -> None:
    path.write_text(
        json.dumps(document, ensure_ascii=False, indent=2, sort_keys=True) + "\n",
        encoding="utf-8",
    )


def graph_contract(identity: dict[str, Any]) -> dict[str, Any]:
    contract = {
        "canonicalizer": {
            "name": "URDNA2015",
            "version": "2015",
            "serialization": "N-Quads",
        },
        "closure_scope": "finite_query_complete",
        "source_included": True,
        "required_assertion_families": [
            "class",
            "object_property",
            "data_property",
            "tbox",
            "rbox",
        ],
        "anonymous_witness_policy": "not_exported",
        "independent_obligation_check_required": True,
        # The low-level pipeline emits a first-pass candidate.  Callers that
        # perform the second export must opt into the stronger determinism
        # contract explicitly (profile_materialization.py does so).
        "two_run_determinism_required": False,
    }
    if identity.get("component", {}).get("name") == "HermiT":
        contract["independent_obligation_artifacts_required"] = True
        contract["independent_obligation_check_required"] = True
        contract["two_run_determinism_required"] = True
        contract["unavailable_assertion_families_status"] = "not_available"
    if identity.get("two_run_determinism_required") is True:
        contract["two_run_determinism_required"] = True
    if nonempty(identity.get("profile_capability_boundary")):
        contract["profile_capability_boundary"] = identity[
            "profile_capability_boundary"
        ]
    if identity.get("ruleset_required") is True:
        contract["ruleset_required"] = True
    return contract


def status_record(
    profile: str,
    source: Path,
    identity: dict[str, Any],
    status: str,
    reason: str,
    source_digest: str,
    candidate_graph: Path | None = None,
    candidate_digest: str | None = None,
    canonical_graph: Path | None = None,
    graph_digest: str | None = None,
    diagnostics_path: Path | None = None,
) -> dict[str, Any]:
    configuration = dict(identity["configuration"])
    configuration.setdefault("sha256", configuration.get("digest"))
    record: dict[str, Any] = {
        "profile": profile,
        "status": status,
        "source_ontology": {
            "id": f"source:{source_digest}",
            "path": str(source),
            "sha256": source_digest,
        },
        "component": identity["component"],
        "exporter": identity["exporter"],
        "query_backend": identity["query_backend"],
        "configuration": configuration,
        "derivation": identity["derivation"],
        "runtime": identity["runtime"],
        "resource_limits": identity["resource_limits"],
        "profile_context": identity["profile_context"],
        "graph_contract": graph_contract(identity),
    }
    if nonempty(identity.get("execution_variant")):
        record["execution_variant"] = identity["execution_variant"]
    for provenance_section in ("credentials", "license"):
        if provenance_section in identity:
            record[provenance_section] = identity[provenance_section]
    if "ruleset" in identity:
        record["ruleset"] = identity["ruleset"]
    for provenance_section in (
        "source_document",
        "query_workload",
        "dataset",
        "generator",
        "generator_configuration",
        "dataset_artifact",
        "generated_dataset",
        "profile_tbox",
        "profile_abox",
        "implementation",
        "runtime_classpath",
        "obligations",
        "diagnostic_extension",
        "acceptance_lock",
    ):
        if provenance_section in identity:
            record[provenance_section] = identity[provenance_section]
    if reason:
        record["reason"] = reason
    if candidate_graph is not None and candidate_digest is not None:
        record["candidate_graph"] = {
            "id": f"candidate:{candidate_digest}",
            "path": str(candidate_graph),
            "sha256": candidate_digest,
            "immutable": True,
        }
    if canonical_graph is not None and graph_digest is not None:
        record["output_graph"] = {
            "id": f"canonical:{graph_digest}",
            "path": str(canonical_graph),
            "sha256": graph_digest,
            "immutable": True,
        }
        record["input_sha256"] = source_digest
        record["output_sha256"] = graph_digest
    if diagnostics_path is not None and diagnostics_path.is_file():
        diagnostics_digest = file_hash(str(diagnostics_path))
        record["diagnostics"] = {
            "path": str(diagnostics_path),
            "sha256": diagnostics_digest,
            "immutable": True,
        }
    return record


def run_export_materialization(
    profile: str,
    source_path: Path,
    exporter_path: Path,
    identity_path: Path,
    output_dir: Path,
) -> dict[str, Any]:
    if profile not in PROFILES:
        raise PipelineError(f"unknown profile: {profile}")

    output_dir = output_dir.expanduser().resolve()
    output_dir.mkdir(parents=True, exist_ok=True)
    manifest_path = output_dir / "materialization.json"
    summary_path = output_dir / "pipeline_summary.json"

    try:
        source = require_file(source_path, "source graph")
        exporter = require_file(exporter_path, "profile exporter")
        identity = load_identity(require_file(identity_path, "identity manifest"))
    except MissingInputError as error:
        return _write_input_status_artifacts(
            profile,
            source_path,
            exporter_path,
            "not_available",
            error,
            summary_path,
            manifest_path,
        )
    except PipelineError as error:
        return _write_input_status_artifacts(
            profile,
            source_path,
            exporter_path,
            "failed",
            error,
            summary_path,
            manifest_path,
        )

    resources = identity["resource_limits"]
    try:
        if not isinstance(resources, dict):
            raise PipelineError("resource_limits must be an object")
        timeout_seconds = resources.get("timeout_seconds")
        if isinstance(timeout_seconds, bool) or not isinstance(timeout_seconds, int) or timeout_seconds <= 0:
            raise PipelineError("resource_limits.timeout_seconds must be a positive integer")
    except PipelineError as error:
        return _write_input_status_artifacts(
            profile,
            source_path,
            exporter_path,
            "failed",
            error,
            summary_path,
            manifest_path,
        )

    candidate_graph = output_dir / "candidate_graph.nq"
    canonical_graph = output_dir / "graph.nq"
    diagnostics_path = output_dir / "export.diagnostics"
    manifest_path = output_dir / "materialization.json"
    summary_path = output_dir / "pipeline_summary.json"

    # The candidate is retained as immutable input to the independent
    # validator; the canonical graph is the only graph handed to the query
    # backend. The exporter is a process boundary so the fake path exercises
    # the same handoff as a real profile adapter.
    source_digest = file_hash(str(source))
    try:
        exported = subprocess.run(
            [str(exporter), "export", str(source)],
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            timeout=timeout_seconds,
            check=False,
        )
    except subprocess.TimeoutExpired as error:
        diagnostics_path.write_text(str(error) + "\n", encoding="utf-8")
        return _write_status_artifacts(
            profile,
            source,
            identity,
            "timeout",
            "profile exporter exceeded the declared timeout",
            source_digest,
            exporter,
            summary_path,
            manifest_path,
            diagnostics_path=diagnostics_path,
        )
    diagnostics_path.write_bytes(exported.stderr)
    if exported.returncode == TIMEOUT_EXIT_CODE:
        return _write_status_artifacts(
            profile,
            source,
            identity,
            "timeout",
            "profile exporter reported a timeout",
            source_digest,
            exporter,
            summary_path,
            manifest_path,
            diagnostics_path=diagnostics_path,
        )
    if exported.returncode == NOT_AVAILABLE_EXIT_CODE:
        return _write_status_artifacts(
            profile,
            source,
            identity,
            "not_available",
            "profile exporter declared the profile materialization unavailable",
            source_digest,
            exporter,
            summary_path,
            manifest_path,
            diagnostics_path=diagnostics_path,
        )
    if exported.returncode == UNSUPPORTED_EXIT_CODE:
        return _write_status_artifacts(
            profile,
            source,
            identity,
            "unsupported",
            "profile exporter declared the profile capability unsupported",
            source_digest,
            exporter,
            summary_path,
            manifest_path,
            diagnostics_path=diagnostics_path,
        )
    if exported.returncode != 0:
        return _write_status_artifacts(
            profile,
            source,
            identity,
            "failed",
            f"profile exporter failed with status {exported.returncode}",
            source_digest,
            exporter,
            summary_path,
            manifest_path,
            diagnostics_path=diagnostics_path,
        )
    if not exported.stdout.strip():
        return _write_status_artifacts(
            profile,
            source,
            identity,
            "failed",
            "profile exporter returned an empty graph",
            source_digest,
            exporter,
            summary_path,
            manifest_path,
            diagnostics_path=diagnostics_path,
        )
    candidate_graph.write_bytes(exported.stdout)
    try:
        canonical_bytes = canonicalize_file(candidate_graph)
        graph_digest = write_canonical(canonical_graph, canonical_bytes)
    except (OSError, UnicodeDecodeError, ValueError) as error:
        return _write_status_artifacts(
            profile,
            source,
            identity,
            "failed",
            f"exported graph could not be canonicalized: {error}",
            source_digest,
            exporter,
            summary_path,
            manifest_path,
            diagnostics_path=diagnostics_path,
        )
    candidate_digest = file_hash(str(candidate_graph))
    record = status_record(
        profile,
        source,
        identity,
        "complete",
        "",
        source_digest,
        candidate_graph,
        candidate_digest,
        canonical_graph,
        graph_digest,
        diagnostics_path=diagnostics_path,
    )
    manifest = {
        "schema_version": 1,
        "manifest_id": f"fake-materialization-{profile.lower()}",
        "profiles": {profile: record},
    }
    write_json(manifest_path, manifest)

    summary = {
        "schema_version": 1,
        "pipeline": "owl2bench-profile-materialization/1",
        "profile": profile,
        "status": "candidate",
        "exporter": str(exporter),
        "source": {"path": str(source), "sha256": source_digest},
        "candidate_graph": {"path": str(candidate_graph), "sha256": candidate_digest},
        "canonical_graph": {"path": str(canonical_graph), "sha256": graph_digest},
        "diagnostics": {"path": str(diagnostics_path), "sha256": file_hash(str(diagnostics_path))},
        "manifest": str(manifest_path),
        "validator": "pending",
    }
    write_json(summary_path, summary)
    return summary


def run_fake(
    profile: str,
    source_path: Path,
    exporter_path: Path,
    identity_path: Path,
    output_dir: Path,
) -> dict[str, Any]:
    """Backward-compatible name for the dependency-free fake fixture."""

    return run_export_materialization(
        profile, source_path, exporter_path, identity_path, output_dir
    )


def _write_status_artifacts(
    profile: str,
    source: Path,
    identity: dict[str, Any],
    status: str,
    reason: str,
    source_digest: str,
    exporter: Path,
    summary_path: Path,
    manifest_path: Path,
    diagnostics_path: Path | None = None,
) -> dict[str, Any]:
    record = status_record(
        profile,
        source,
        identity,
        status,
        reason,
        source_digest,
        diagnostics_path=diagnostics_path,
    )
    write_json(
        manifest_path,
        {
            "schema_version": 1,
            "manifest_id": f"fake-materialization-{profile.lower()}",
            "profiles": {profile: record},
        },
    )
    summary = {
        "schema_version": 1,
        "pipeline": "owl2bench-profile-materialization/1",
        "profile": profile,
        "status": status,
        "reason": reason,
        "exporter": str(exporter),
        "source": {"path": str(source), "sha256": source_digest},
        "manifest": str(manifest_path),
        "diagnostics": (
            {"path": str(diagnostics_path), "sha256": file_hash(str(diagnostics_path))}
            if diagnostics_path is not None and diagnostics_path.is_file()
            else None
        ),
        "validator": "not_ready",
    }
    write_json(summary_path, summary)
    return summary


def _write_input_status_artifacts(
    profile: str,
    source_path: Path,
    exporter_path: Path,
    status: str,
    error: PipelineError,
    summary_path: Path,
    manifest_path: Path,
) -> dict[str, Any]:
    """Retain a structured result when preflight cannot start execution."""

    source = source_path.expanduser().resolve()
    exporter = exporter_path.expanduser().resolve()
    source_record: dict[str, Any] = {"path": str(source)}
    if source.is_file():
        source_record["sha256"] = file_hash(str(source))
    manifest = {
        "schema_version": 1,
        "manifest_id": f"fake-materialization-{profile.lower()}",
        "profiles": {
            profile: {
                "profile": profile,
                "status": status,
                "reason": str(error),
            }
        },
    }
    write_json(manifest_path, manifest)
    summary = {
        "schema_version": 1,
        "pipeline": "owl2bench-profile-materialization/1",
        "profile": profile,
        "status": status,
        "reason": str(error),
        "exporter": {"path": str(exporter)},
        "source": source_record,
        "manifest": str(manifest_path),
        "validator": "not_ready",
    }
    write_json(summary_path, summary)
    return summary


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    subparsers = parser.add_subparsers(dest="command", required=True)
    fake_parser = subparsers.add_parser("run-fake", help="run a fake profile exporter")
    fake_parser.add_argument("--profile", required=True, choices=sorted(PROFILES))
    fake_parser.add_argument("--source", required=True, type=Path)
    fake_parser.add_argument("--exporter", required=True, type=Path)
    fake_parser.add_argument("--identity", required=True, type=Path)
    fake_parser.add_argument("--output-dir", required=True, type=Path)
    args = parser.parse_args(argv)
    try:
        if args.command == "run-fake":
            summary = run_fake(
                args.profile,
                args.source,
                args.exporter,
                args.identity,
                args.output_dir,
            )
            print(json.dumps(summary, ensure_ascii=False, sort_keys=True))
            return 0 if summary["status"] == "candidate" else 1
    except (OSError, PipelineError, ValueError) as error:
        print(f"materialization pipeline failed: {error}", file=sys.stderr)
        return 1
    return 2


if __name__ == "__main__":
    raise SystemExit(main())
