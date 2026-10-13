#!/usr/bin/env python3
"""Create an ELK 0.6.0 OWL2Bench EL profile identity manifest.

The manifest is intentionally shaped for profile_materialization.py. It pins
the ELK runtime, the exporter command, the read-only query backend, the frozen
EL dataset identity, and the EL query workload without creating oracle or
answer-key fields.
"""

from __future__ import annotations

import argparse
import csv
import hashlib
import json
import os
import shutil
import subprocess
import sys
from pathlib import Path
from typing import Any

from resource_variant import resource_variant

EXPECTED_VERSION = "0.6.0"
SCRIPT_DIR = Path(__file__).resolve().parent


class IdentityError(ValueError):
    """The requested ELK profile identity cannot be created."""


def file_digest(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return "sha256:" + digest.hexdigest()


def probe_java_runtime(java_bin: str) -> dict[str, str]:
    resolved = Path(java_bin).expanduser()
    if not resolved.is_absolute() and "/" not in java_bin:
        located = shutil.which(java_bin)
        if located is None:
            raise IdentityError(f"Java executable is missing: {java_bin}")
        resolved = Path(located)
    resolved = require_file(resolved, "Java executable")
    result = subprocess.run(
        [str(resolved), "-version"],
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
        check=False,
    )
    output = (result.stdout + result.stderr).strip()
    if result.returncode != 0 or not output:
        raise IdentityError("Java version probe failed: " + output)
    version = output.splitlines()[0].strip()
    return {
        "name": "java",
        "version": version,
        "revision": "java-version-output:" + hashlib.sha256(output.encode("utf-8")).hexdigest(),
        "path": str(resolved),
        "digest": file_digest(resolved),
    }


def require_file(path: Path, label: str) -> Path:
    resolved = path.expanduser().resolve()
    if not resolved.is_file():
        raise IdentityError(f"{label} is missing: {resolved}")
    return resolved


def require_executable(path: Path, label: str) -> Path:
    resolved = require_file(path, label)
    if not os.access(resolved, os.X_OK):
        raise IdentityError(f"{label} is not executable: {resolved}")
    return resolved


def read_json(path: Path, label: str) -> dict[str, Any]:
    try:
        value = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, UnicodeDecodeError, json.JSONDecodeError) as error:
        raise IdentityError(f"{label} is malformed: {error}") from error
    if not isinstance(value, dict):
        raise IdentityError(f"{label} root must be an object")
    return value


def resolve_dataset_root(explicit: str | None) -> Path:
    candidates: list[Path] = []
    if explicit:
        candidates.append(Path(explicit))
    env_root = os.environ.get("OWL2BENCH_PROFILE_DATASETS_ROOT")
    if env_root:
        candidates.append(Path(env_root))
    for candidate in candidates:
        resolved = candidate.expanduser().resolve()
        if (resolved / "profiles" / "EL" / "profile-identity.json").is_file():
            return resolved
    raise IdentityError(
        "EL profile dataset root is missing; pass --dataset-root or set "
        "OWL2BENCH_PROFILE_DATASETS_ROOT"
    )


def candidate_jars(explicit: str | None) -> list[Path]:
    candidates: list[Path] = []
    if explicit:
        candidates.append(Path(explicit))
    for variable in ("ELK_EL_JAR", "OWL2BENCH_ELK_JAR"):
        value = os.environ.get(variable)
        if value:
            candidates.append(Path(value))
    for variable in ("ELK_EL_HOME", "OWL2BENCH_ELK_HOME", "OWL2BENCH_ELK_ARTIFACT_ROOT"):
        value = os.environ.get(variable)
        if not value:
            continue
        root = Path(value)
        candidates.extend(
            [
                root / "elk.jar",
                root / "cli" / "elk-distribution-cli-0.6.0" / "elk.jar",
            ]
        )
    deduped: list[Path] = []
    seen: set[str] = set()
    for candidate in candidates:
        key = str(candidate.expanduser())
        if key not in seen:
            seen.add(key)
            deduped.append(candidate)
    return deduped


def resolve_elk_jar(explicit: str | None) -> Path:
    for candidate in candidate_jars(explicit):
        resolved = candidate.expanduser().resolve()
        if resolved.is_file():
            return resolved
    raise IdentityError(
        "ELK 0.6.0 elk.jar is missing; pass --elk-jar or set "
        "OWL2BENCH_ELK_ARTIFACT_ROOT"
    )


def probe_version(java_bin: str, elk_jar: Path) -> str:
    result = subprocess.run(
        [java_bin, "-jar", str(elk_jar), "--version"],
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
        check=False,
    )
    if result.returncode != 0:
        raise IdentityError(
            "ELK version probe failed: " + (result.stderr.strip() or result.stdout.strip())
        )
    output = result.stdout.strip()
    if EXPECTED_VERSION not in output:
        raise IdentityError(f"ELK runtime version is not {EXPECTED_VERSION}: {output}")
    return output


def artifact(record: dict[str, Any], fallback_path: Path, fallback_revision: str) -> dict[str, Any]:
    path = require_file(Path(str(record.get("path", fallback_path))), "dataset artifact")
    return {
        "id": record.get("id", f"artifact:{file_digest(path)}"),
        "path": str(path),
        "revision": record.get("revision", fallback_revision),
        "sha256": file_digest(path),
    }


def load_el_queries(manifest_path: Path) -> list[dict[str, Any]]:
    manifest = require_file(manifest_path, "query manifest")
    queries: list[dict[str, Any]] = []
    with manifest.open("r", encoding="utf-8", newline="") as handle:
        reader = csv.DictReader(handle, delimiter="\t")
        for row in reader:
            profiles = {
                part.strip().upper()
                for part in str(row.get("profile", "")).split(",")
                if part.strip()
            }
            if "EL" not in profiles:
                continue
            query_path = require_file(Path(str(row.get("staged_query", ""))), "EL staged query")
            queries.append(
                {
                    "id": row.get("query_id", ""),
                    "source_query": row.get("source_query", ""),
                    "path": str(query_path),
                    "sha256": file_digest(query_path),
                    "source_corpus": row.get("source_corpus", ""),
                    "source_revision": row.get("source_revision", ""),
                }
            )
    if len(queries) != 10:
        raise IdentityError(f"EL query workload must contain exactly 10 queries, found {len(queries)}")
    return queries


def build_identity(args: argparse.Namespace) -> dict[str, Any]:
    output_dir = args.output.parent.expanduser().resolve()
    output_dir.mkdir(parents=True, exist_ok=True)
    dataset_root = resolve_dataset_root(args.dataset_root)
    profile_dir = (dataset_root / "profiles" / "EL").resolve()
    profile_identity = read_json(profile_dir / "profile-identity.json", "EL profile identity")
    if profile_identity.get("profile") != "EL" or profile_identity.get("status") != "complete":
        raise IdentityError("EL frozen profile identity must be a complete EL record")
    source = require_file(Path(args.source) if args.source else profile_dir / "source.nq", "EL source N-Quads")
    frozen_dataset = profile_identity.get("dataset")
    if not isinstance(frozen_dataset, dict):
        raise IdentityError("EL frozen dataset identity is missing")
    for field in (
        "id",
        "profile",
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
        if not isinstance(frozen_dataset.get(field), str) or not frozen_dataset[field].strip():
            raise IdentityError(f"EL frozen dataset.{field} is missing")
    if frozen_dataset["profile"] != "EL":
        raise IdentityError("EL frozen dataset profile does not match EL")
    frozen_source_path = frozen_dataset.get("artifact_path")
    if not isinstance(frozen_source_path, str) or not frozen_source_path.strip():
        raise IdentityError("EL frozen conversion artifact path is missing")
    if Path(frozen_source_path).expanduser().resolve() != source.resolve():
        raise IdentityError("EL source path does not match the frozen conversion artifact")
    if frozen_dataset.get("sha256") != file_digest(source):
        raise IdentityError("EL source hash does not match the frozen conversion artifact")
    exporter = require_executable(
        Path(args.exporter) if args.exporter else SCRIPT_DIR / "elk_el_materializer.sh",
        "ELK EL exporter",
    )
    query_backend = require_executable(Path(args.query_backend), "EL query backend")
    query_manifest = require_file(Path(args.query_manifest), "query manifest")
    elk_jar = resolve_elk_jar(args.elk_jar)
    java_bin = args.java or os.environ.get("ELK_EL_JAVA", "java")
    java_identity = probe_java_runtime(java_bin)
    version_output = probe_version(java_identity["path"], elk_jar)

    config_path = output_dir / "elk_el_config.json"
    config = {
        "schema_version": 1,
        "profile": "EL",
        "engine": "ELK",
        "engine_version": EXPECTED_VERSION,
        "java": java_bin,
        "elk_jar": str(elk_jar),
        "elk_jar_sha256": file_digest(elk_jar),
        "source_format": "N-Quads",
        "elk_cli_input_format": "OWL Functional Syntax",
        "rdf_conversion_boundary": "dataset freezer supplied canonical source.nq",
        "export_boundary": {
            "status": "not_available",
            "reason": "ELK 0.6.0 CLI/API artifacts do not provide complete finite RDF graph export",
            "classification_taxonomy_only": "insufficient_for_query_graph",
        },
    }
    config_path.write_text(json.dumps(config, ensure_ascii=False, indent=2, sort_keys=True) + "\n", encoding="utf-8")

    source_hash = file_digest(source)
    dataset = dict(frozen_dataset)

    source_revision = str(profile_identity.get("source_revision", "unknown"))
    generator_record = profile_identity.get("generator")
    if not isinstance(generator_record, dict):
        raise IdentityError("EL frozen generator identity is missing")
    for field in ("id", "revision", "path", "sha256"):
        if not isinstance(generator_record.get(field), str) or not generator_record[field].strip():
            raise IdentityError(f"EL frozen generator.{field} is missing")
    if isinstance(frozen_dataset.get("seed"), bool) or not isinstance(frozen_dataset.get("seed"), int) or frozen_dataset["seed"] < 0:
        raise IdentityError("EL frozen dataset seed is invalid")
    generator_path = require_file(Path(generator_record["path"]), "OWL2Bench generator")
    if file_digest(generator_path) != generator_record["sha256"]:
        raise IdentityError("EL frozen generator hash does not match its path")
    generator_config_record = profile_identity.get("generator_configuration")
    if not isinstance(generator_config_record, dict):
        raise IdentityError("EL frozen generator configuration identity is missing")
    for field in ("id", "revision", "path", "sha256"):
        if not isinstance(generator_config_record.get(field), str) or not generator_config_record[field].strip():
            raise IdentityError(f"EL frozen generator_configuration.{field} is missing")
    generator_config_path = require_file(
        Path(generator_config_record["path"]),
        "OWL2Bench generator configuration",
    )
    if file_digest(generator_config_path) != generator_config_record["sha256"]:
        raise IdentityError("EL frozen generator configuration hash does not match its path")
    profile_context_record = profile_identity.get("profile_context")
    if not isinstance(profile_context_record, dict) or not isinstance(profile_context_record.get("id"), str) or not profile_context_record["id"].strip():
        raise IdentityError("EL frozen profile context identity is missing")
    for section in ("profile_tbox", "profile_abox"):
        if not isinstance(profile_identity.get(section), dict):
            raise IdentityError(f"EL frozen {section} identity is missing")

    resource_record = profile_identity.get("resource_limits")
    if not isinstance(resource_record, dict):
        raise IdentityError("EL frozen resource limits are missing")
    resource_limits = dict(resource_record)
    resource_limits.setdefault("cpu", "1")
    resource_limits.setdefault("disk", "10GiB")
    resource_limits.setdefault("network", "local-only")
    resource_limits["variant"] = resource_variant(
        resource_limits,
        {
            "timeout_seconds": 600,
            "memory": "24GiB",
            "threads": 1,
            "cpu": "1",
            "disk": "10GiB",
            "network": "local-only",
        },
    )

    identity = {
        "schema_version": 1,
        "profile": "EL",
        "component": {
            "name": "ELK",
            "version": EXPECTED_VERSION,
            "revision": "io.github.liveontologies:elk-cli:0.6.0",
            "artifact_digest": file_digest(elk_jar),
            "path": str(elk_jar),
            "version_output": version_output,
        },
        "exporter": {
            "name": "tableauxx-elk-el-fail-closed-exporter",
            "version": "1",
            "revision": file_digest(exporter),
            "artifact_digest": file_digest(exporter),
            "path": str(exporter),
            "boundary": "export <source.nq>",
        },
        "query_backend": {
            "name": "owl2bench-el-read-only-query-backend",
            "version": "manifest-selected",
            "revision": file_digest(query_backend),
            "artifact_digest": file_digest(query_backend),
            "path": str(query_backend),
            "read_only": True,
        },
        "configuration": {
            "id": "elk-el-0.6.0-fail-closed-config",
            "path": str(config_path),
            "sha256": file_digest(config_path),
        },
        "derivation": {
            "version": "owl2bench-elk-el-materialization-boundary/1",
            "revision": file_digest(SCRIPT_DIR / "elk_el_export_boundary.py"),
        },
        "runtime": {
            "id": "elk-cli-0.6.0-java",
            "dependencies": [
                java_identity,
                {
                    "name": "elk-cli",
                    "version": EXPECTED_VERSION,
                    "revision": "io.github.liveontologies:elk-cli:0.6.0",
                    "path": str(elk_jar),
                    "digest": file_digest(elk_jar),
                },
            ],
        },
        "resource_limits": resource_limits,
        "profile_context": {
            "id": profile_context_record["id"],
            "directory": str(profile_dir),
        },
        "source_document": {
            "id": f"profile-source-nquads:{source_hash}",
            "path": str(source),
            "sha256": source_hash,
        },
        "query_workload": {
            "id": "owl2bench-el-query-workload",
            "manifest": {"path": str(query_manifest), "sha256": file_digest(query_manifest)},
            "query_count": 10,
            "queries": load_el_queries(query_manifest),
        },
        "dataset": dataset,
        "generator": {
            "id": generator_record.get("id", "OWL2Bench-generator"),
            "revision": generator_record.get("revision", source_revision),
            "seed": generator_record.get("seed", dataset.get("seed", 1)),
            "artifact_path": str(generator_path),
            "artifact_digest": file_digest(generator_path),
            "configuration_id": generator_config_record.get("id", "OWL2Bench-generator-config"),
            "configuration_sha256": file_digest(generator_config_path),
        },
        "generator_configuration": {
            "id": generator_config_record.get("id", "OWL2Bench-generator-config"),
            "path": str(generator_config_path),
            "revision": generator_config_record.get("revision", source_revision),
            "sha256": file_digest(generator_config_path),
        },
        "profile_tbox": artifact(
            dict(profile_identity["profile_tbox"]),
            profile_dir / "profile-tbox.owl",
            source_revision,
        ),
        "profile_abox": artifact(
            dict(profile_identity["profile_abox"]),
            profile_dir / "abox.nq",
            source_revision,
        ),
        "dataset_artifact": {
            "id": dataset["artifact_id"],
            "path": dataset["artifact_path"],
            "revision": dataset["artifact_revision"],
            "sha256": dataset["sha256"],
        },
        "implementation": {
            "name": "tableauxx-elk-el-boundary",
            "path": str(SCRIPT_DIR / "elk_el_export_boundary.py"),
            "sha256": file_digest(SCRIPT_DIR / "elk_el_export_boundary.py"),
        },
        "runtime_classpath": {
            "requested": str(elk_jar),
            "artifacts": [
                {
                    "name": "elk-cli",
                    "version": EXPECTED_VERSION,
                    "revision": "io.github.liveontologies:elk-cli:0.6.0",
                    "path": str(elk_jar),
                    "sha256": file_digest(elk_jar),
                }
            ]
        },
        "profile_capability_boundary": "ELK taxonomy/classification is insufficient for finite OWL2Bench query graph export",
        "two_run_determinism_required": True,
        "license": {"status": "not_required"},
    }
    return identity


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--dataset-root", help="frozen profile dataset root")
    parser.add_argument("--elk-jar", help="path to pinned ELK 0.6.0 elk.jar")
    parser.add_argument("--java", help="Java runtime for ELK version probing")
    parser.add_argument("--source", help="EL source N-Quads; defaults to <dataset-root>/profiles/EL/source.nq")
    parser.add_argument("--query-manifest", required=True)
    parser.add_argument("--query-backend", required=True)
    parser.add_argument("--exporter")
    parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args(argv)
    try:
        identity = build_identity(args)
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(
            json.dumps(identity, ensure_ascii=False, indent=2, sort_keys=True) + "\n",
            encoding="utf-8",
        )
    except (OSError, IdentityError) as error:
        print(f"[elk-el-identity][not_available] {error}", file=sys.stderr)
        return 125
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
