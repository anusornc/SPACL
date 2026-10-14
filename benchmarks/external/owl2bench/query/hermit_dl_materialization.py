#!/usr/bin/env python3
"""Run the pinned HermiT DL materialization through the shared graph seam."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import shutil
import subprocess
import zipfile
from pathlib import Path
from typing import Any

from materialization_manifest import file_digest
from materialization_pipeline import PipelineError, run_fake, write_json
from materialization_validator import handoff, validate
from rdf_canonicalization import canonicalize_file, write_canonical
from split_stack_manifest import forbidden_field_paths
from resource_variant import resource_variant


PROFILE = "DL"
NOT_AVAILABLE_EXIT_CODE = 125
UNSUPPORTED_EXIT_CODE = 123
MATERIALIZER = Path(__file__).with_name("hermit_dl_materializer.sh")
JAVA_SOURCE = Path(__file__).with_name("HermitDlMaterializer.java")


class MissingInputError(PipelineError):
    """A required external artifact is absent."""


def require_file(path: Path, label: str, *, executable: bool = False) -> Path:
    resolved = path.expanduser().resolve()
    if not resolved.is_file() or not os.access(resolved, os.R_OK):
        raise MissingInputError(f"{label} is missing or unreadable: {resolved}")
    if executable and not os.access(resolved, os.X_OK):
        raise MissingInputError(f"{label} is not executable: {resolved}")
    return resolved


def require_directory(path: Path, label: str) -> Path:
    resolved = path.expanduser().resolve()
    if not resolved.is_dir() or not os.access(resolved, os.R_OK):
        raise MissingInputError(f"{label} is missing or unreadable: {resolved}")
    return resolved


def digest(path: Path) -> str:
    value = file_digest(str(path))
    if value is None:
        raise MissingInputError(f"artifact digest is unavailable: {path}")
    return value


def nonempty(value: Any) -> bool:
    return isinstance(value, str) and bool(value.strip())


def digest_value(value: Any) -> str:
    return str(value).strip().removeprefix("sha256:").lower()


def classpath_artifacts(
    classpath: str, hermit_jar: Path, hermit_version: str, hermit_revision: str
) -> list[dict[str, str]]:
    entries: list[Path] = []
    for raw_entry in classpath.split(os.pathsep):
        entry = raw_entry.strip()
        if not entry:
            raise PipelineError("HermiT classpath contains an empty entry")
        if entry.endswith("*"):
            directory = require_directory(Path(entry[:-1]), "HermiT classpath directory")
            jars = sorted(path for path in directory.iterdir() if path.is_file() and path.suffix == ".jar")
            if not jars:
                raise MissingInputError(f"HermiT classpath directory contains no jars: {directory}")
            entries.extend(jars)
        else:
            path = Path(entry)
            if path.is_dir():
                raise PipelineError(
                    f"HermiT classpath directory must use a trailing '*': {path}"
                )
            entries.append(require_file(path, "HermiT classpath artifact"))
    if not entries:
        raise MissingInputError("HermiT classpath contains no artifacts")

    artifacts: list[dict[str, str]] = []
    for path in entries:
        is_primary = path.resolve() == hermit_jar.resolve()
        version = ""
        try:
            with zipfile.ZipFile(path) as archive:
                manifest = archive.read("META-INF/MANIFEST.MF").decode("utf-8", errors="replace")
                if is_primary:
                    properties_name = "META-INF/maven/com.hermit-reasoner/org.semanticweb.hermit/pom.properties"
                    properties_text = archive.read(properties_name).decode("utf-8", errors="replace")
                    version_match = re.search(r"^version=(.+)$", properties_text, re.MULTILINE)
                    version = version_match.group(1).strip() if version_match else ""
            if not is_primary:
                for key in ("Implementation-Version", "Bundle-Version", "Specification-Version"):
                    match = re.search(rf"^{re.escape(key)}:\s*(.+)$", manifest, re.MULTILINE)
                    if match:
                        version = match.group(1).strip()
                        break
            if is_primary and version == "":
                raise KeyError("HermiT artifact version metadata is empty")
        except (OSError, zipfile.BadZipFile, KeyError):
            version = ""
        if not version:
            raise MissingInputError(f"HermiT classpath artifact has no exact version metadata: {path}")
        if is_primary and version != hermit_version:
            raise PipelineError(
                f"HermiT configuration version {hermit_version} does not match artifact metadata {version}"
            )
        artifacts.append(
            {
                "name": path.stem,
                "version": version,
                "revision": hermit_revision if path.resolve() == hermit_jar.resolve() else digest(path),
                "path": str(path),
                "sha256": digest(path),
            }
        )
    return artifacts


def directory_digest(directory: Path) -> str:
    hasher = hashlib.sha256()
    files = sorted(path for path in directory.rglob("*") if path.is_file())
    for path in files:
        relative = path.relative_to(directory).as_posix().encode("utf-8")
        hasher.update(relative)
        hasher.update(bytes([0]))
        hasher.update(path.read_bytes())
        hasher.update(bytes([0]))
    return "sha256:" + hasher.hexdigest()


def properties(path: Path) -> dict[str, str]:
    values: dict[str, str] = {}
    try:
        lines = path.read_text(encoding="utf-8").splitlines()
    except (OSError, UnicodeDecodeError) as error:
        raise PipelineError(f"HermiT configuration is unreadable: {error}") from error
    for line in lines:
        stripped = line.strip()
        if not stripped or stripped.startswith(("#", "!")):
            continue
        if "=" not in stripped:
            raise PipelineError(f"HermiT configuration line is malformed: {line}")
        key, value = stripped.split("=", 1)
        values[key.strip()] = value.strip()
    required = {
        "engine.id": "HermiT",
        "hermit.version": None,
        "materialization.mode": "finite_query_complete",
    }
    for key, expected in required.items():
        value = values.get(key, "")
        if not value:
            raise MissingInputError(f"HermiT configuration pin is missing {key}")
        if expected is not None and value != expected:
            expected_text = expected or "a non-empty value"
            raise PipelineError(f"HermiT configuration requires {key}={expected_text}")
    return values


def query_backend_identity(
    manifest_path: Path,
    backend_path: Path,
    backend_name: str,
    backend_version: str,
    backend_revision: str,
) -> dict[str, Any]:
    try:
        manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
    except (OSError, UnicodeDecodeError, json.JSONDecodeError) as error:
        raise PipelineError(f"query backend identity manifest is malformed: {error}") from error
    if not isinstance(manifest, dict):
        raise PipelineError("query backend identity manifest must be an object")
    forbidden = forbidden_field_paths(manifest)
    if forbidden:
        raise PipelineError(
            "query backend identity contains forbidden fields: " + ", ".join(forbidden)
        )
    for field in ("name", "version", "revision", "path", "sha256"):
        if not nonempty(manifest.get(field)):
            raise MissingInputError(f"query backend identity is missing {field}")
    if (
        manifest["name"] != backend_name
        or manifest["version"] != backend_version
        or manifest["revision"] != backend_revision
        or Path(str(manifest["path"])).expanduser().resolve() != backend_path.resolve()
    ):
        raise PipelineError("query backend identity does not match its CLI pins")
    if digest_value(manifest["sha256"]) != digest_value(digest(backend_path)):
        raise PipelineError("query backend identity hash does not match its executable")
    configuration = manifest.get("configuration")
    if not isinstance(configuration, dict):
        raise MissingInputError("query backend configuration identity is missing")
    if configuration.get("status") != "not_required":
        for field in ("id", "path", "revision", "sha256"):
            if not nonempty(configuration.get(field)):
                raise MissingInputError(f"query backend configuration is missing {field}")
        configuration_path = require_file(
            Path(str(configuration["path"])), "query backend configuration"
        )
        if digest_value(configuration["sha256"]) != digest_value(digest(configuration_path)):
            raise PipelineError("query backend configuration hash does not match its path")
    dependencies = manifest.get("dependencies")
    if not isinstance(dependencies, list):
        raise MissingInputError("query backend dependency identity is missing")
    normalized_dependencies: list[dict[str, str]] = []
    for index, dependency in enumerate(dependencies):
        if not isinstance(dependency, dict):
            raise PipelineError(f"query backend dependency {index} is invalid")
        for field in ("name", "version", "revision", "path", "sha256"):
            if not nonempty(dependency.get(field)):
                raise MissingInputError(f"query backend dependency {index} is missing {field}")
        dependency_path = require_file(
            Path(str(dependency["path"])), "query backend dependency"
        )
        if digest_value(dependency["sha256"]) != digest_value(digest(dependency_path)):
            raise PipelineError(
                f"query backend dependency {index} hash does not match its path"
            )
        normalized_dependencies.append(dict(dependency))
    return {
        "name": manifest["name"],
        "version": manifest["version"],
        "revision": manifest["revision"],
        "path": str(backend_path),
        "artifact_digest": digest(backend_path),
        "read_only": True,
        "configuration": configuration,
        "dependencies": normalized_dependencies,
        "identity_manifest": {
            "path": str(manifest_path),
            "sha256": digest(manifest_path),
        },
    }


def executable_identity(command: str, version_args: tuple[str, ...]) -> dict[str, str]:
    path_value = shutil.which(command)
    if path_value is None:
        raise MissingInputError(f"{command} runtime is missing")
    path = require_file(Path(path_value), f"{command} runtime", executable=True)
    try:
        probe = subprocess.run(
            [str(path), *version_args],
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            timeout=10,
            check=False,
        )
    except (OSError, subprocess.TimeoutExpired) as error:
        raise PipelineError(f"{command} version probe failed: {error}") from error
    output = probe.stdout.decode("utf-8", errors="replace").strip()
    if probe.returncode != 0 or not output:
        raise PipelineError(f"{command} version probe failed with status {probe.returncode}")
    return {
        "name": command,
        "version": output.splitlines()[0],
        "path": str(path),
        "digest": digest(path),
    }


def query_workload_identity(
    query_directory: Path, query_manifest: Path, expected_count: int
) -> dict[str, Any]:
    query_files = sorted(query_directory.glob("*.rq"))
    if len(query_files) != expected_count:
        raise PipelineError(
            f"DL query directory contains {len(query_files)} queries; expected {expected_count}"
        )
    try:
        lines = query_manifest.read_text(encoding="utf-8").splitlines()
    except (OSError, UnicodeDecodeError) as error:
        raise PipelineError(f"query manifest is unreadable: {error}") from error
    if not lines or lines[0].split("\t")[:1] != ["staged_query"]:
        raise PipelineError("query manifest header is malformed")
    header = lines[0].split("\t")
    try:
        profile_index = header.index("profile")
        revision_index = header.index("source_revision")
    except ValueError as error:
        raise PipelineError("query manifest lacks profile/source_revision columns") from error
    rows = [line.split("\t") for line in lines[1:] if line.strip()]
    dl_rows = [
        row
        for row in rows
        if len(row) > profile_index
        and "DL" in {value.strip().upper() for value in row[profile_index].split(",")}
    ]
    revisions = {
        row[revision_index]
        for row in dl_rows
        if len(row) > revision_index and row[revision_index]
    }
    if not dl_rows or len(revisions) != 1:
        raise PipelineError("query manifest must provide one DL source revision")
    manifest_paths = {
        Path(row[0]).expanduser().resolve()
        for row in dl_rows
        if len(row) > 0 and row[0]
    }
    query_paths = {query_file.resolve() for query_file in query_files}
    if len(dl_rows) != expected_count or manifest_paths != query_paths:
        raise PipelineError("query manifest DL rows do not match the query directory")
    return {
        "id": "owl2bench-query-workload:DL",
        "manifest_path": str(query_manifest),
        "manifest_sha256": digest(query_manifest),
        "directory": str(query_directory),
        "directory_sha256": directory_digest(query_directory),
        "source_revision": next(iter(revisions)),
        "query_count": len(query_files),
        "queries": [
            {
                "path": str(query_file),
                "sha256": digest(query_file),
            }
            for query_file in query_files
        ],
    }


def write_blocked_artifacts(
    output_dir: Path,
    status: str,
    reason: str,
    ontology: Path,
) -> dict[str, Any]:
    manifest_path = output_dir / "materialization.json"
    summary_path = output_dir / "hermit_materialization_summary.json"
    record = {"profile": PROFILE, "status": status, "reason": reason}
    write_json(
        manifest_path,
        {
            "schema_version": 1,
            "manifest_id": "hermit-dl-materialization",
            "profiles": {PROFILE: record},
        },
    )
    summary = {
        "schema_version": 1,
        "pipeline": "owl2bench-hermit-dl-materialization/1",
        "profile": PROFILE,
        "status": status,
        "execution_status": status,
        "reason": reason,
        "source_document": {"path": str(ontology.expanduser().resolve())},
        "manifest": str(manifest_path),
        "validator": "not_ready",
    }
    write_json(summary_path, summary)
    return summary


def verify_two_run_determinism(
    materializer: Path,
    ontology: Path,
    source_graph: Path,
    output_dir: Path,
    environment: dict[str, str],
    timeout_seconds: int,
    first_graph: Path,
) -> dict[str, Any]:
    """Run the exporter again and compare canonical graph bytes and hashes."""

    verification_dir = output_dir / "determinism"
    verification_dir.mkdir(parents=True, exist_ok=True)
    second_source = verification_dir / "source_graph.nq"
    second_candidate = verification_dir / "candidate_graph.nq"
    second_graph = verification_dir / "graph.nq"
    second_obligation_graph = verification_dir / "obligations.nq"
    second_obligation_manifest = verification_dir / "obligations.json"
    source_diagnostics = verification_dir / "source.diagnostics"
    diagnostics = verification_dir / "export.diagnostics"
    verification_environment = environment.copy()
    verification_environment.update(
        {
            "HERMIT_DL_BUILD_DIR": str(verification_dir / ".hermit-build"),
            "HERMIT_DL_OBLIGATION_OUTPUT": str(second_obligation_graph),
            "HERMIT_DL_OBLIGATION_MANIFEST": str(second_obligation_manifest),
        }
    )
    try:
        source_result = subprocess.run(
            [str(materializer), "source", str(ontology)],
            env=verification_environment,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            timeout=timeout_seconds,
            check=False,
        )
    except subprocess.TimeoutExpired:
        return {
            "status": "timeout",
            "runs": 2,
            "reason": "HermiT second source-graph export exceeded the declared timeout",
        }
    source_diagnostics.write_bytes(source_result.stderr)
    if source_result.returncode == NOT_AVAILABLE_EXIT_CODE:
        return {
            "status": "not_available",
            "runs": 2,
            "reason": "HermiT second source-graph export runtime is unavailable",
        }
    if source_result.returncode != 0:
        return {
            "status": "failed",
            "runs": 2,
            "reason": f"HermiT second source-graph export failed with status {source_result.returncode}",
        }
    if not source_result.stdout.strip():
        return {
            "status": "failed",
            "runs": 2,
            "reason": "HermiT second source-graph export returned an empty graph",
        }
    second_source.write_bytes(source_result.stdout)
    try:
        first_source_bytes = canonicalize_file(source_graph)
        second_source_bytes = canonicalize_file(second_source)
    except (OSError, UnicodeDecodeError, ValueError) as error:
        return {
            "status": "failed",
            "runs": 2,
            "reason": f"HermiT source graphs could not be canonicalized: {error}",
        }
    if first_source_bytes != second_source_bytes:
        return {
            "status": "failed",
            "runs": 2,
            "reason": "source graph differs between the two materialization runs",
            "first_source_graph": {"path": str(source_graph), "sha256": digest(source_graph)},
            "second_source_graph": {"path": str(second_source), "sha256": digest(second_source)},
            "source_diagnostics": str(source_diagnostics),
        }
    try:
        exported = subprocess.run(
            [str(materializer), "export", str(second_source)],
            env=verification_environment,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            timeout=timeout_seconds,
            check=False,
        )
    except subprocess.TimeoutExpired:
        return {
            "status": "timeout",
            "runs": 2,
            "reason": "HermiT second export exceeded the declared timeout",
        }
    diagnostics.write_bytes(exported.stderr)
    if exported.returncode == NOT_AVAILABLE_EXIT_CODE:
        return {
            "status": "not_available",
            "runs": 2,
            "reason": "HermiT second export runtime is unavailable",
        }
    if exported.returncode != 0:
        return {
            "status": "failed",
            "runs": 2,
            "reason": f"HermiT second export failed with status {exported.returncode}",
        }
    if not exported.stdout.strip():
        return {
            "status": "failed",
            "runs": 2,
            "reason": "HermiT second export returned an empty graph",
        }
    second_candidate.write_bytes(exported.stdout)
    try:
        first_bytes = first_graph.read_bytes()
        second_bytes = canonicalize_file(second_candidate)
        second_digest = write_canonical(second_graph, second_bytes)
    except (OSError, UnicodeDecodeError, ValueError) as error:
        return {
            "status": "failed",
            "runs": 2,
            "reason": f"HermiT determinism verification could not canonicalize: {error}",
        }
    first_digest = digest(first_graph)
    if first_bytes != second_bytes or first_digest != second_digest:
        return {
            "status": "failed",
            "runs": 2,
            "reason": "canonical graph differs between the two exporter runs",
            "first_graph": {"path": str(first_graph), "sha256": first_digest},
            "second_graph": {"path": str(second_graph), "sha256": second_digest},
            "first_source_graph": {"path": str(source_graph), "sha256": digest(source_graph)},
            "second_source_graph": {"path": str(second_source), "sha256": digest(second_source)},
            "second_obligation_graph": {
                "path": str(second_obligation_graph),
                "sha256": digest(second_obligation_graph),
            },
            "second_obligation_manifest": {
                "path": str(second_obligation_manifest),
                "sha256": digest(second_obligation_manifest),
            },
            "second_candidate_graph": str(second_candidate),
            "source_diagnostics": str(source_diagnostics),
            "diagnostics": str(diagnostics),
        }
    return {
        "status": "complete",
        "runs": 2,
        "first_graph": {"path": str(first_graph), "sha256": first_digest},
        "second_graph": {"path": str(second_graph), "sha256": second_digest},
        "first_source_graph": {"path": str(source_graph), "sha256": digest(source_graph)},
        "second_source_graph": {"path": str(second_source), "sha256": digest(second_source)},
        "second_obligation_graph": {
            "path": str(second_obligation_graph),
            "sha256": digest(second_obligation_graph),
        },
        "second_obligation_manifest": {
            "path": str(second_obligation_manifest),
            "sha256": digest(second_obligation_manifest),
        },
        "second_candidate_graph": str(second_candidate),
        "source_diagnostics": str(source_diagnostics),
        "diagnostics": str(diagnostics),
    }


def run_materialization(args: argparse.Namespace) -> dict[str, Any]:
    output_dir = args.output_dir.expanduser().resolve()
    output_dir.mkdir(parents=True, exist_ok=True)
    ontology = require_file(args.ontology, "DL source ontology")
    query_directory = require_directory(args.query_dir, "DL query directory")
    query_manifest = require_file(args.query_manifest, "DL query manifest")
    hermit_jar = require_file(args.hermit_jar, "HermiT jar")
    config = require_file(args.config, "HermiT configuration")
    query_backend = require_file(args.query_backend, "DL query backend", executable=True)
    query_backend_manifest = require_file(
        args.query_backend_manifest, "DL query backend identity manifest"
    )
    profile_context = require_directory(args.profile_context, "DL profile context")
    conversion_artifact = require_file(
        profile_context / "source.nq", "DL frozen conversion artifact"
    )
    profile_tbox = require_file(args.profile_tbox, "DL profile TBox")
    profile_abox = require_file(args.profile_abox, "DL profile ABox")
    dataset_artifact = require_file(args.dataset_artifact, "DL generated dataset artifact")
    if dataset_artifact != ontology:
        raise PipelineError(
            "DL generated dataset artifact must be the ontology consumed by HermiT"
        )
    generator_artifact = require_file(args.generator_artifact, "OWL2Bench generator artifact")
    generator_configuration = require_file(
        args.generator_config, "OWL2Bench generator configuration"
    )
    materializer = require_file(MATERIALIZER, "DL materializer", executable=True)
    implementation = require_file(JAVA_SOURCE, "DL materializer implementation")
    if args.timeout_seconds <= 0 or args.threads <= 0 or args.expected_query_count <= 0:
        raise PipelineError("timeout, threads, and expected query count must be positive")

    config_values = properties(config)
    hermit_classpath = args.hermit_classpath or str(hermit_jar)
    classpath = classpath_artifacts(
        hermit_classpath,
        hermit_jar,
        config_values["hermit.version"],
        args.hermit_revision,
    )
    hermit_artifact = next(
        (
            artifact
            for artifact in classpath
            if Path(artifact["path"]).resolve() == hermit_jar.resolve()
        ),
        None,
    )
    if hermit_artifact is None:
        raise PipelineError("HermiT artifact is absent from the runtime classpath")
    workload = query_workload_identity(
        query_directory, query_manifest, args.expected_query_count
    )
    backend_identity = query_backend_identity(
        query_backend_manifest,
        query_backend,
        args.query_backend_name,
        args.query_backend_version,
        args.query_backend_revision,
    )
    java_identity = executable_identity("java", ("-version",))
    javac_identity = executable_identity("javac", ("-version",))
    ontology_digest = digest(ontology)
    profile_tbox_digest = digest(profile_tbox)
    profile_abox_digest = digest(profile_abox)
    generator_configuration_digest = digest(generator_configuration)
    generated_dataset_digest = digest(dataset_artifact)
    conversion_digest = digest(conversion_artifact)
    source_graph = output_dir / "source_graph.nq"
    source_diagnostics = output_dir / "source_graph.diagnostics"
    obligation_graph = output_dir / "obligations.nq"
    obligation_manifest = output_dir / "obligations.json"
    identity_path = output_dir / "identity.json"

    identity = {
        "profile": PROFILE,
        "component": {
            "name": "HermiT",
            "version": hermit_artifact["version"],
            "revision": args.hermit_revision,
            "artifact_digest": digest(hermit_jar),
            "path": str(hermit_jar),
        },
        "exporter": {
            "name": "tableauxx-hermit-dl-exporter",
            "version": "1.0.0",
            "revision": digest(implementation),
            "artifact_digest": digest(materializer),
            "path": str(materializer),
        },
        "query_backend": backend_identity,
        "configuration": {
            "id": "hermit-dl-materialization-config",
            "path": str(config),
            "sha256": digest(config),
        },
        "derivation": {
            "version": "tableauxx-hermit-dl-materializer/1",
            "revision": digest(implementation),
        },
        "runtime": {
            "id": "java-hermit-dl-runtime",
            "dependencies": [
                {
                    "name": java_identity["name"],
                    "version": java_identity["version"],
                    "digest": java_identity["digest"],
                },
                {
                    "name": javac_identity["name"],
                    "version": javac_identity["version"],
                    "digest": javac_identity["digest"],
                },
                *[
                    {
                        "name": artifact["name"],
                        "version": artifact["version"],
                        "digest": artifact["sha256"],
                    }
                    for artifact in classpath
                ],
            ],
        },
        "runtime_classpath": {
            "requested": hermit_classpath,
            "artifacts": classpath,
        },
        "resource_limits": {
            "timeout_seconds": args.timeout_seconds,
            "memory": args.memory,
            "threads": args.threads,
            "cpu": "1",
            "disk": "10GiB",
            "network": "local-only",
        },
        "profile_context": {
            "id": f"owl2bench-context:DL:universities={args.university_count}:seed={args.generator_seed}",
            "directory": str(profile_context),
            "sha256": directory_digest(profile_context),
        },
        "profile_tbox": {
            "id": f"owl2bench-dl-tbox:{profile_tbox_digest}",
            "path": str(profile_tbox),
            "revision": args.profile_tbox_revision,
            "sha256": profile_tbox_digest,
        },
        "profile_abox": {
            "id": f"owl2bench-dl-abox:{profile_abox_digest}",
            "path": str(profile_abox),
            "revision": args.profile_abox_revision,
            "sha256": profile_abox_digest,
        },
        "dataset_artifact": {
            "id": f"profile-source-nquads:{conversion_digest}",
            "path": str(conversion_artifact),
            "revision": args.dataset_artifact_revision,
            "sha256": conversion_digest,
        },
        "generated_dataset": {
            "id": f"owl2bench-dl-generated-dataset:{generated_dataset_digest}",
            "path": str(dataset_artifact),
            "revision": args.dataset_artifact_revision,
            "sha256": generated_dataset_digest,
        },
        "source_document": {
            "id": f"owl2bench-dl-source:{ontology_digest}",
            "path": str(ontology),
            "sha256": ontology_digest,
            "format": "RDF/XML",
        },
        "query_workload": workload,
        "dataset": {
            "id": args.dataset_id,
            "sha256": conversion_digest,
            "profile": PROFILE,
            "source_ontology_sha256": ontology_digest,
            "artifact_id": f"profile-source-nquads:{conversion_digest}",
            "artifact_path": str(conversion_artifact),
            "artifact_revision": args.dataset_artifact_revision,
            "tbox_id": f"owl2bench-dl-tbox:{profile_tbox_digest}",
            "tbox_sha256": profile_tbox_digest,
            "abox_id": f"owl2bench-dl-abox:{profile_abox_digest}",
            "abox_sha256": profile_abox_digest,
            "seed": args.generator_seed,
            "university_count": args.university_count,
        },
        "generator": {
            "id": "owl2bench-generator",
            "revision": args.generator_revision,
            "seed": args.generator_seed,
            "university_count": args.university_count,
            "artifact_path": str(generator_artifact),
            "artifact_digest": digest(generator_artifact),
            "configuration_id": f"owl2bench-generator-config:{generator_configuration_digest}",
            "configuration_sha256": generator_configuration_digest,
        },
        "generator_configuration": {
            "id": f"owl2bench-generator-config:{generator_configuration_digest}",
            "path": str(generator_configuration),
            "revision": args.generator_config_revision,
            "sha256": generator_configuration_digest,
        },
        "implementation": {
            "path": str(implementation),
            "sha256": digest(implementation),
        },
        "obligations": {
            "manifest_path": str(obligation_manifest),
            "graph_path": str(obligation_graph),
        },
        "credentials": {"status": "not_required"},
        "license": {
            "status": "not_required",
            "id": "LGPL-3.0-or-later",
            "provenance": "SPDX license identifier; no runtime license artifact is required",
        },
    }
    identity["resource_limits"]["variant"] = resource_variant(
        identity["resource_limits"],
        {
            "timeout_seconds": 600,
            "memory": "24GiB",
            "threads": 1,
            "cpu": "1",
            "disk": "10GiB",
            "network": "local-only",
        },
    )
    write_json(identity_path, identity)

    runtime_environment = os.environ.copy()
    runtime_environment.update(
        {
            "HERMIT_DL_JAR": str(hermit_jar),
            "HERMIT_DL_CLASSPATH": hermit_classpath,
            "HERMIT_DL_CONFIG": str(config),
            "HERMIT_DL_SOURCE_ONTOLOGY": str(ontology),
            "HERMIT_DL_QUERY_DIR": str(query_directory),
            "HERMIT_DL_BUILD_DIR": str(output_dir / ".hermit-build"),
            "HERMIT_DL_OBLIGATION_OUTPUT": str(obligation_graph),
            "HERMIT_DL_OBLIGATION_MANIFEST": str(obligation_manifest),
            "HERMIT_DL_JAVA_OPTS": "",
        }
    )
    try:
        source_result = subprocess.run(
            [str(materializer), "source", str(ontology)],
            env=runtime_environment,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            timeout=args.timeout_seconds,
            check=False,
        )
    except subprocess.TimeoutExpired:
        return write_blocked_artifacts(
            output_dir,
            "timeout",
            "HermiT source graph export exceeded the declared timeout",
            ontology,
        )
    source_diagnostics.write_bytes(source_result.stderr)
    if source_result.returncode == NOT_AVAILABLE_EXIT_CODE:
        return write_blocked_artifacts(
            output_dir,
            "not_available",
            "HermiT source graph runtime is unavailable",
            ontology,
        )
    if source_result.returncode == UNSUPPORTED_EXIT_CODE:
        return write_blocked_artifacts(
            output_dir,
            "unsupported",
            "HermiT source graph capability is unsupported",
            ontology,
        )
    if source_result.returncode != 0 or not source_result.stdout.strip():
        return write_blocked_artifacts(
            output_dir,
            "failed",
            "HermiT source graph export failed",
            ontology,
        )
    source_graph.write_bytes(source_result.stdout)
    try:
        source_graph.write_bytes(canonicalize_file(source_graph))
    except (OSError, UnicodeDecodeError, ValueError) as error:
        return write_blocked_artifacts(
            output_dir,
            "failed",
            f"HermiT source graph could not be canonicalized: {error}",
            ontology,
        )

    previous_environment = os.environ.copy()
    os.environ.update(runtime_environment)
    try:
        pipeline_summary = run_fake(
            PROFILE,
            source_graph,
            materializer,
            identity_path,
            output_dir,
        )
    finally:
        os.environ.clear()
        os.environ.update(previous_environment)

    manifest_path = output_dir / "materialization.json"
    validation_path = output_dir / "validation.json"
    handoff_path = output_dir / "handoff.json"
    candidate_graph = output_dir / "candidate_graph.nq"
    canonical_graph = output_dir / "graph.nq"
    if pipeline_summary.get("status") == "candidate" and canonical_graph.is_file():
        determinism = verify_two_run_determinism(
            materializer,
            ontology,
            source_graph,
            output_dir,
            runtime_environment,
            args.timeout_seconds,
            canonical_graph,
        )
        manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
        manifest["profiles"][PROFILE]["determinism"] = determinism
        write_json(manifest_path, manifest)
    else:
        determinism = {
            "status": "not_available",
            "runs": 2,
            "reason": "first HermiT materialization did not produce a candidate graph",
        }
    validation = validate(
        PROFILE,
        source_graph,
        candidate_graph,
        canonical_graph,
        manifest_path,
    )
    write_json(validation_path, validation)
    handoff_record: dict[str, Any] | None = None
    if validation["status"] == "complete":
        handoff_record = handoff(validation_path, canonical_graph)
        write_json(handoff_path, handoff_record)
    status = validation["status"]
    if handoff_record is not None and handoff_record.get("status") != "ready":
        status = handoff_record.get("status", "failed")
    execution_status = status
    reported_status = "oracle_unavailable" if status == "complete" else status
    summary = {
        "schema_version": 1,
        "pipeline": "owl2bench-hermit-dl-materialization/1",
        "profile": PROFILE,
        "status": reported_status,
        "execution_status": execution_status,
        "pipeline_summary": pipeline_summary,
        "source_document": identity["source_document"],
        "source_graph": {
            "path": str(source_graph),
            "sha256": digest(source_graph),
        },
        "source_diagnostics": str(source_diagnostics),
        "query_workload": identity["query_workload"],
        "dataset": identity["dataset"],
        "generator": identity["generator"],
        "obligations": {
            "manifest": str(obligation_manifest),
            "graph": str(obligation_graph),
        },
        "determinism": determinism,
        "identity": str(identity_path),
        "manifest": str(manifest_path),
        "validation": str(validation_path),
        "handoff": str(handoff_path) if handoff_record is not None else None,
    }
    write_json(output_dir / "hermit_materialization_summary.json", summary)
    return summary


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("command", choices=("run",))
    parser.add_argument("--ontology", required=True, type=Path)
    parser.add_argument("--query-dir", required=True, type=Path)
    parser.add_argument("--query-manifest", required=True, type=Path)
    parser.add_argument("--hermit-jar", required=True, type=Path)
    parser.add_argument("--hermit-classpath", default="")
    parser.add_argument("--config", required=True, type=Path)
    parser.add_argument("--query-backend", required=True, type=Path)
    parser.add_argument("--query-backend-manifest", required=True, type=Path)
    parser.add_argument("--query-backend-name", required=True)
    parser.add_argument("--query-backend-version", required=True)
    parser.add_argument("--query-backend-revision", required=True)
    parser.add_argument("--profile-context", required=True, type=Path)
    parser.add_argument("--profile-tbox", required=True, type=Path)
    parser.add_argument("--profile-tbox-revision", required=True)
    parser.add_argument("--profile-abox", required=True, type=Path)
    parser.add_argument("--profile-abox-revision", required=True)
    parser.add_argument("--generator-artifact", required=True, type=Path)
    parser.add_argument("--generator-revision", required=True)
    parser.add_argument("--generator-config", required=True, type=Path)
    parser.add_argument("--generator-config-revision", required=True)
    parser.add_argument("--generator-seed", required=True, type=int)
    parser.add_argument("--university-count", required=True, type=int)
    parser.add_argument("--dataset-id", required=True)
    parser.add_argument("--dataset-artifact", required=True, type=Path)
    parser.add_argument("--dataset-artifact-revision", required=True)
    parser.add_argument("--hermit-revision", required=True)
    parser.add_argument("--expected-query-count", type=int, default=22)
    parser.add_argument("--timeout-seconds", type=int, default=600)
    parser.add_argument("--memory", default="24GiB")
    parser.add_argument("--threads", type=int, default=1)
    parser.add_argument("--output-dir", required=True, type=Path)
    args = parser.parse_args(argv)
    try:
        summary = run_materialization(args)
    except MissingInputError as error:
        output_dir = args.output_dir.expanduser().resolve()
        output_dir.mkdir(parents=True, exist_ok=True)
        summary = write_blocked_artifacts(
            output_dir, "not_available", str(error), args.ontology
        )
    except (OSError, PipelineError, ValueError) as error:
        output_dir = args.output_dir.expanduser().resolve()
        output_dir.mkdir(parents=True, exist_ok=True)
        summary = write_blocked_artifacts(
            output_dir, "failed", str(error), args.ontology
        )
    print(json.dumps(summary, ensure_ascii=False, sort_keys=True))
    return 0 if summary.get("execution_status") == "complete" else 1


if __name__ == "__main__":
    raise SystemExit(main())
