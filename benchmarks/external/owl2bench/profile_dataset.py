#!/usr/bin/env python3
"""Freeze reproducible OWL2Bench profile datasets and their identities.

This command owns benchmark input preparation only. It runs the official
OWL2Bench generator with an explicit profile, university count, and seed,
keeps the generated RDF/XML dataset, extracts its explicit ABox, converts both
artifacts to N-Quads, and records hashes for every input and conversion step.
It never reasons over the dataset and never creates an answer key or oracle.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path
from typing import Any, Iterable

from query.resource_variant import resource_variant

PROFILES = ("DL", "EL", "QL", "RL")
SCHEMA_VERSION = 1
SENTINEL = ".owl2bench-profile-dataset-owned"
TOOL_SOURCE = Path(__file__).with_name("OWL2BenchDatasetTool.java")
CANONICALIZER = Path(__file__).with_name("query") / "rdf_canonicalization.py"


class DatasetError(RuntimeError):
    """The dataset package cannot satisfy its reproducibility contract."""


class SourceUnavailableError(DatasetError):
    """An external benchmark source or prerequisite cannot be verified."""


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return "sha256:" + digest.hexdigest()


def sha256_text(value: str) -> str:
    return "sha256:" + hashlib.sha256(value.encode("utf-8")).hexdigest()


def json_digest(value: Any) -> str:
    payload = json.dumps(value, ensure_ascii=False, sort_keys=True, separators=(",", ":"))
    return sha256_text(payload)


def write_json(path: Path, value: dict[str, Any]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(
        json.dumps(value, ensure_ascii=False, indent=2, sort_keys=True) + "\n",
        encoding="utf-8",
    )


def require_file(path: Path, label: str) -> Path:
    resolved = path.expanduser().resolve()
    if not resolved.is_file() or not os.access(resolved, os.R_OK):
        raise SourceUnavailableError(f"{label} is missing or unreadable: {resolved}")
    return resolved


def require_positive_int(value: str, label: str) -> int:
    try:
        parsed = int(value)
    except ValueError as error:
        raise DatasetError(f"{label} must be a positive integer") from error
    if parsed <= 0:
        raise DatasetError(f"{label} must be a positive integer")
    return parsed


def require_nonnegative_int(value: str, label: str) -> int:
    try:
        parsed = int(value)
    except ValueError as error:
        raise DatasetError(f"{label} must be a non-negative integer") from error
    if parsed < 0:
        raise DatasetError(f"{label} must be a non-negative integer")
    return parsed


def parse_profiles(value: str) -> tuple[str, ...]:
    profiles = tuple(part.strip().upper() for part in value.split(",") if part.strip())
    if not profiles or any(profile not in PROFILES for profile in profiles):
        raise DatasetError(f"profiles must contain only {','.join(PROFILES)}")
    if len(set(profiles)) != len(profiles):
        raise DatasetError("profiles must not contain duplicates")
    return profiles


def git_revision(source_dir: Path, explicit: str) -> str:
    if explicit.strip():
        return explicit.strip()
    try:
        result = subprocess.run(
            ["git", "-C", str(source_dir), "rev-parse", "HEAD"],
            check=False,
            capture_output=True,
            text=True,
            timeout=10,
        )
    except (OSError, subprocess.TimeoutExpired) as error:
        raise SourceUnavailableError("cannot determine OWL2Bench source revision") from error
    revision = result.stdout.strip()
    if result.returncode != 0 or not revision:
        raise SourceUnavailableError(
            "OWL2Bench source is not a repository; pass --source-revision explicitly"
        )
    return revision


def locate_asset(source_dir: Path, filename: str) -> Path:
    candidates = (source_dir / filename, source_dir / "OWL2Bench" / filename)
    for candidate in candidates:
        if candidate.is_file():
            return candidate.resolve()
    raise SourceUnavailableError(
        f"official OWL2Bench asset {filename} was not found under {source_dir}"
    )


def artifact(path: Path, role: str, *, revision: str | None = None, format: str | None = None) -> dict[str, Any]:
    record: dict[str, Any] = {
        "id": f"{role}:{sha256_file(path)}",
        "role": role,
        "path": str(path.resolve()),
        "sha256": sha256_file(path),
        "bytes": path.stat().st_size,
    }
    if revision is not None:
        record["revision"] = revision
    if format is not None:
        record["format"] = format
    return record


def stable_profile_identity(record: dict[str, Any]) -> dict[str, Any]:
    """Return only content identities, never run-directory paths."""

    dataset = record.get("dataset", {})
    return {
        "profile": record.get("profile"),
        "source_revision": record.get("source_revision"),
        "generator_sha256": record.get("generator", {}).get("artifact_digest"),
        "configuration_sha256": record.get("generator_configuration", {}).get("sha256"),
        "random_names_sha256": record.get("random_names", {}).get("sha256"),
        "generated_dataset_sha256": record.get("generated_dataset", {}).get("sha256"),
        "source_nquads_sha256": record.get("conversion_artifact", {}).get("sha256"),
        "tbox_sha256": record.get("profile_tbox", {}).get("sha256"),
        "abox_sha256": record.get("profile_abox", {}).get("sha256"),
        "dataset_id": dataset.get("id"),
        "dataset_sha256": dataset.get("sha256"),
        "dataset_source_ontology_sha256": dataset.get("source_ontology_sha256"),
        "dataset_tbox_sha256": dataset.get("tbox_sha256"),
        "dataset_abox_sha256": dataset.get("abox_sha256"),
        "universities": dataset.get("universities"),
        "seed": dataset.get("seed"),
        "profile_context": record.get("profile_context", {}).get("identity"),
    }


def remove_owned_output(output_dir: Path) -> None:
    output_dir.mkdir(parents=True, exist_ok=True)
    sentinel = output_dir / SENTINEL
    existing = list(output_dir.iterdir())
    if existing and not sentinel.is_file():
        raise DatasetError(
            f"refusing to clean unstamped dataset directory: {output_dir}"
        )
    if sentinel.is_file():
        for child in existing:
            if child == sentinel:
                continue
            if child.is_dir() and not child.is_symlink():
                shutil.rmtree(child)
            else:
                child.unlink()
    sentinel.write_text("owned-by-owl2bench-profile-dataset/1\n", encoding="utf-8")


def copy_asset(source: Path, destination: Path) -> Path:
    destination.parent.mkdir(parents=True, exist_ok=True)
    shutil.copy2(source, destination)
    return destination.resolve()


def compile_tool(
    java_bin: str, javac_bin: str, generator_jar: Path, runtime_dir: Path
) -> tuple[Path, dict[str, Any]]:
    require_file(TOOL_SOURCE, "dataset conversion tool source")
    runtime_dir.mkdir(parents=True, exist_ok=True)
    compiled = runtime_dir / "OWL2BenchDatasetTool.class"
    command = [
        javac_bin,
        "-cp",
        str(generator_jar),
        "-d",
        str(runtime_dir),
        str(TOOL_SOURCE),
    ]
    result = subprocess.run(command, capture_output=True, text=True, check=False)
    if result.returncode != 0 or not compiled.is_file():
        raise DatasetError(
            "javac could not compile the official OWL2Bench conversion bridge: "
            + (result.stderr.strip() or f"exit {result.returncode}")
        )
    return compiled, {
        "name": "OWL2BenchDatasetTool",
        "version": "1",
        "revision": sha256_file(TOOL_SOURCE),
        "source": artifact(TOOL_SOURCE, "conversion-tool-source"),
        "compiled": artifact(compiled, "conversion-tool-class"),
        "java": java_bin,
        "javac": javac_bin,
    }


def run_tool(
    java_bin: str,
    runtime_dir: Path,
    generator_jar: Path,
    command: str,
    input_path: Path,
    output_path: Path,
    timeout_seconds: int,
) -> None:
    result = subprocess.run(
        [
            java_bin,
            "-cp",
            f"{runtime_dir}{os.pathsep}{generator_jar}",
            "OWL2BenchDatasetTool",
            command,
            str(input_path),
            str(output_path),
        ],
        capture_output=True,
        text=True,
        timeout=timeout_seconds,
        check=False,
    )
    if result.returncode != 0 or not output_path.is_file():
        diagnostic = result.stderr.strip() or result.stdout.strip() or f"exit {result.returncode}"
        raise DatasetError(f"dataset conversion {command} failed: {diagnostic}")


def canonicalize_nquads(input_path: Path, output_path: Path) -> None:
    require_file(CANONICALIZER, "RDF dataset canonicalizer")
    result = subprocess.run(
        [
            sys.executable,
            str(CANONICALIZER),
            "canonicalize",
            "--input",
            str(input_path),
            "--output",
            str(output_path),
        ],
        capture_output=True,
        text=True,
        check=False,
    )
    if result.returncode != 0 or not output_path.is_file():
        diagnostic = result.stderr.strip() or result.stdout.strip() or f"exit {result.returncode}"
        raise DatasetError(f"RDF dataset canonicalization failed: {diagnostic}")


def generator_workspace(
    source_assets: dict[str, Path], workspace: Path
) -> dict[str, Path]:
    workspace.mkdir(parents=True, exist_ok=True)
    copied: dict[str, Path] = {}
    for name, source in source_assets.items():
        copied[name] = copy_asset(source, workspace / name)
    return copied


def run_generator(
    java_bin: str,
    generator_jar: Path,
    source_assets: dict[str, Path],
    profile: str,
    universities: int,
    seed: int,
    timeout_seconds: int,
    diagnostics_path: Path,
) -> Path:
    with tempfile.TemporaryDirectory(prefix=f"owl2bench-{profile.lower()}-") as temporary:
        workspace = Path(temporary)
        copied = generator_workspace(source_assets, workspace)
        result = subprocess.run(
            [
                java_bin,
                "-jar",
                str(copied["OWL2Bench.jar"]),
                str(universities),
                profile,
                str(seed),
            ],
            cwd=workspace,
            stdout=subprocess.PIPE,
            stderr=subprocess.STDOUT,
            timeout=timeout_seconds,
            check=False,
            text=True,
        )
        diagnostics_path.parent.mkdir(parents=True, exist_ok=True)
        diagnostics_path.write_text(result.stdout, encoding="utf-8")
        if result.returncode != 0:
            raise DatasetError(
                f"official OWL2Bench generator failed for {profile}: exit {result.returncode}"
            )
        expected = workspace / f"OWL2{profile}-{universities}.owl"
        if not expected.is_file():
            candidates = sorted(workspace.glob(f"OWL2{profile}-{universities}.*"))
            if len(candidates) != 1:
                raise DatasetError(
                    f"generator did not produce one expected {profile} dataset"
                )
            expected = candidates[0]
        retained = diagnostics_path.parent / "dataset.owl"
        shutil.copy2(expected, retained)
        return retained.resolve()


def profile_identity(
    profile: str,
    profile_dir: Path,
    dataset_path: Path,
    source_nq: Path,
    tbox_path: Path,
    abox_nq: Path,
    diagnostics_path: Path,
    source_revision: str,
    generator: dict[str, Any],
    configuration: dict[str, Any],
    random_names: dict[str, Any],
    conversion: dict[str, Any],
    universities: int,
    seed: int,
    resource_limits: dict[str, Any],
) -> dict[str, Any]:
    dataset_id = f"owl2bench:{profile}:universities={universities}:seed={seed}"
    context_id = f"owl2bench-context:{profile}:universities={universities}:seed={seed}"
    dataset_record = artifact(source_nq, "profile-source-nquads", revision=source_revision, format="N-Quads")
    record: dict[str, Any] = {
        "schema_version": SCHEMA_VERSION,
        "profile": profile,
        "status": "complete",
        "source_corpus": "owl2bench",
        "source_revision": source_revision,
        "source_ontology": artifact(dataset_path, "generated-dataset", revision=source_revision, format="RDF/XML"),
        "profile_tbox": artifact(tbox_path, "profile-tbox", revision=source_revision, format="RDF/XML"),
        "profile_abox": artifact(abox_nq, "profile-abox", revision=source_revision, format="N-Quads"),
        "generated_dataset": artifact(dataset_path, "generated-dataset", revision=source_revision, format="RDF/XML"),
        "conversion_artifact": dataset_record,
        "dataset_artifact": dataset_record,
        "diagnostics": artifact(diagnostics_path, "generator-diagnostics"),
        "dataset": {
            "id": dataset_id,
            "profile": profile,
            "artifact_id": dataset_record["id"],
            "artifact_path": dataset_record["path"],
            "artifact_revision": source_revision,
            "sha256": dataset_record["sha256"],
            "source_ontology_sha256": sha256_file(dataset_path),
            "tbox_id": f"owl2bench-tbox:{profile}",
            "tbox_sha256": sha256_file(tbox_path),
            "abox_id": f"owl2bench-abox:{profile}:universities={universities}:seed={seed}",
            "abox_sha256": sha256_file(abox_nq),
            "universities": universities,
            "seed": seed,
        },
        "generator": generator,
        "generator_configuration": configuration,
        "random_names": random_names,
        "conversion": conversion,
        "implementation": {
            "name": "owl2bench-profile-dataset-freezer",
            "version": "1",
            "path": str(Path(__file__).resolve()),
            "sha256": sha256_file(Path(__file__).resolve()),
        },
        "profile_context": {
            "id": context_id,
            "directory": str(profile_dir.resolve()),
            "identity": f"profile={profile};universities={universities};seed={seed};source_revision={source_revision}",
            "sha256": sha256_text(
                f"profile={profile};universities={universities};seed={seed};source_revision={source_revision}"
            ),
        },
        "resource_limits": resource_limits,
    }
    record["identity_sha256"] = json_digest(stable_profile_identity(record))
    return record


def overall_status(statuses: Iterable[str]) -> str:
    values = tuple(statuses)
    if values and all(value == "complete" for value in values):
        return "complete"
    if any(value == "failed" for value in values):
        return "failed"
    if any(value == "complete" for value in values):
        return "partial"
    return "not_available"


def build_manifest(
    output_dir: Path,
    source_dir: Path,
    source_revision: str,
    source_artifacts: dict[str, Path],
    generator_artifacts: dict[str, Path],
    profiles: dict[str, dict[str, Any]],
    universities: int,
    seed: int,
    resource_limits: dict[str, Any],
    query_manifest: Path | None,
) -> dict[str, Any]:
    source_records = {
        name: artifact(path, f"official-source:{name}", revision=source_revision)
        for name, path in source_artifacts.items()
    }
    generator_records = {
        name: artifact(path, f"generator-input:{name}", revision=source_revision)
        for name, path in generator_artifacts.items()
    }
    source_identity = {
        "revision": source_revision,
        "artifacts": {
            name: {"role": value["role"], "sha256": value["sha256"]}
            for name, value in source_records.items()
        },
    }
    manifest: dict[str, Any] = {
        "schema_version": SCHEMA_VERSION,
        "manifest_id": f"owl2bench-profile-datasets:universities={universities}:seed={seed}",
        "status": overall_status(profile["status"] for profile in profiles.values()),
        "source": {
            "corpus": "owl2bench",
            "repository": "https://github.com/kracr/owl2bench",
            "directory": str(source_dir),
            "revision": source_revision,
            "artifacts": source_records,
            "identity_sha256": json_digest(source_identity),
        },
        "generator": {
            "name": "OWL2Bench",
            "version": "official-jar-artifact",
            "revision": source_revision,
            "command": "java -jar OWL2Bench.jar <universities> <PROFILE> <seed>",
            "universities": universities,
            "seed": seed,
            "artifacts": generator_records,
            "artifact_digest": generator_records["OWL2Bench.jar"]["sha256"],
        },
        "resource_limits": resource_limits,
        "profiles": {
            profile: profiles.get(
                profile,
                {
                    "profile": profile,
                    "status": "not_available",
                    "reason": "profile was not requested by this preparation run",
                },
            )
            for profile in PROFILES
        },
        "query_manifest": (
            artifact(query_manifest, "official-query-manifest") if query_manifest else None
        ),
        "no_answer_keys": True,
    }
    manifest["identity_sha256"] = json_digest(
        {
            "manifest_id": manifest["manifest_id"],
            "source": manifest["source"]["identity_sha256"],
            "generator": manifest["generator"]["artifact_digest"],
            "profiles": {
                profile: value.get("identity_sha256")
                for profile, value in manifest["profiles"].items()
            },
            "resource_limits": resource_limits,
        }
    )
    return manifest


def prepare(args: argparse.Namespace) -> int:
    source_dir = require_file(args.source_dir / "README.md", "OWL2Bench source README").parent
    source_revision = git_revision(source_dir, args.source_revision)
    profiles = parse_profiles(args.profiles)
    universities = require_positive_int(str(args.universities), "universities")
    seed = require_nonnegative_int(str(args.seed), "seed")
    timeout_seconds = require_positive_int(str(args.timeout_seconds), "timeout_seconds")
    threads = require_positive_int(str(args.threads), "threads")
    query_manifest = args.query_manifest.expanduser().resolve() if args.query_manifest else None
    if query_manifest is not None:
        require_file(query_manifest, "query manifest")

    tboxes = {
        profile: locate_asset(source_dir, f"UNIV-BENCH-OWL2{profile}.owl")
        for profile in PROFILES
    }
    generator_jar = locate_asset(source_dir, "OWL2Bench.jar")
    config = locate_asset(source_dir, "config.properties")
    random_names = locate_asset(source_dir, "RandomNames.xlsx")
    source_assets = {
        "OWL2Bench.jar": generator_jar,
        "config.properties": config,
        "RandomNames.xlsx": random_names,
        **{f"UNIV-BENCH-OWL2{profile}.owl": tboxes[profile] for profile in PROFILES},
    }

    output_dir = args.output_dir.expanduser().resolve()
    remove_owned_output(output_dir)
    generator_dir = output_dir / "generator"
    copied_generator_assets = {
        name: copy_asset(source, generator_dir / name)
        for name, source in source_assets.items()
    }
    runtime_dir = output_dir / "runtime"
    _compiled_tool, conversion_tool = compile_tool(
        args.java_bin, args.javac_bin, copied_generator_assets["OWL2Bench.jar"], runtime_dir
    )
    resource_limits = {
        "timeout_seconds": timeout_seconds,
        "memory": args.memory,
        "threads": threads,
        "cpu": args.cpu,
        "disk": args.disk,
        "network": args.network,
    }
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

    profile_records: dict[str, dict[str, Any]] = {}
    for profile in profiles:
        profile_dir = output_dir / "profiles" / profile
        profile_dir.mkdir(parents=True, exist_ok=True)
        tbox_path = copy_asset(tboxes[profile], profile_dir / "profile-tbox.owl")
        diagnostics_path = profile_dir / "generator.log"
        try:
            dataset_path = run_generator(
                args.java_bin,
                copied_generator_assets["OWL2Bench.jar"],
                copied_generator_assets,
                profile,
                universities,
                seed,
                timeout_seconds,
                diagnostics_path,
            )
            raw_source_nq = profile_dir / "source.raw.nq"
            abox_rdf = profile_dir / "abox.owl"
            raw_abox_nq = profile_dir / "abox.raw.nq"
            source_nq = profile_dir / "source.nq"
            abox_nq = profile_dir / "abox.nq"
            run_tool(
                args.java_bin,
                runtime_dir,
                copied_generator_assets["OWL2Bench.jar"],
                "rdfxml-to-nquads",
                dataset_path,
                raw_source_nq,
                timeout_seconds,
            )
            run_tool(
                args.java_bin,
                runtime_dir,
                copied_generator_assets["OWL2Bench.jar"],
                "extract-abox",
                dataset_path,
                abox_rdf,
                timeout_seconds,
            )
            run_tool(
                args.java_bin,
                runtime_dir,
                copied_generator_assets["OWL2Bench.jar"],
                "rdfxml-to-nquads",
                abox_rdf,
                raw_abox_nq,
                timeout_seconds,
            )
            canonicalize_nquads(raw_source_nq, source_nq)
            canonicalize_nquads(raw_abox_nq, abox_nq)
            generator_record = artifact(
                copied_generator_assets["OWL2Bench.jar"], "OWL2Bench-generator", revision=source_revision
            )
            config_record = artifact(
                copied_generator_assets["config.properties"],
                "OWL2Bench-generator-config",
                revision=source_revision,
                format="properties",
            )
            random_names_record = artifact(
                copied_generator_assets["RandomNames.xlsx"],
                "OWL2Bench-random-names",
                revision=source_revision,
            )
            conversion = {
                "tool": conversion_tool,
                "input_format": "RDF/XML",
                "output_format": "N-Quads",
                "source_graph": artifact(source_nq, "converted-source-graph", format="N-Quads"),
                "abox_graph": artifact(abox_nq, "converted-abox-graph", format="N-Quads"),
            }
            profile_records[profile] = profile_identity(
                profile,
                profile_dir,
                dataset_path,
                source_nq,
                tbox_path,
                abox_nq,
                diagnostics_path,
                source_revision,
                generator_record,
                config_record,
                random_names_record,
                conversion,
                universities,
                seed,
                resource_limits,
            )
            write_json(profile_dir / "profile-identity.json", profile_records[profile])
        except subprocess.TimeoutExpired as error:
            profile_records[profile] = {
                "profile": profile,
                "status": "timeout",
                "reason": f"dataset preparation exceeded {timeout_seconds} seconds: {error}",
            }
        except DatasetError as error:
            profile_records[profile] = {
                "profile": profile,
                "status": "failed",
                "reason": str(error),
            }

    source_artifacts = {f"tbox_{profile}": tboxes[profile] for profile in PROFILES}
    manifest = build_manifest(
        output_dir,
        source_dir,
        source_revision,
        source_artifacts,
        source_assets,
        profile_records,
        universities,
        seed,
        resource_limits,
        query_manifest,
    )
    manifest_path = output_dir / "dataset-manifest.json"
    write_json(manifest_path, manifest)
    validation = validate_manifest(manifest_path)
    manifest["validation"] = {
        "status": validation["status"],
        "errors": validation["errors"],
        "profile_statuses": validation["profile_statuses"],
    }
    write_json(manifest_path, manifest)
    final_status = (
        "complete"
        if manifest["status"] == "complete" and validation["status"] == "complete"
        else manifest["status"] if manifest["status"] != "complete" else validation["status"]
    )
    print(json.dumps({"status": final_status, "manifest": str(manifest_path)}, sort_keys=True))
    return 0 if final_status == "complete" else 1


def load_manifest(path: Path) -> dict[str, Any]:
    try:
        value = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, UnicodeDecodeError, json.JSONDecodeError) as error:
        raise DatasetError(f"dataset manifest is malformed: {error}") from error
    if not isinstance(value, dict):
        raise DatasetError("dataset manifest root must be an object")
    return value


def validate_manifest(manifest_path: Path) -> dict[str, Any]:
    manifest_path = require_file(manifest_path, "dataset manifest")
    manifest = load_manifest(manifest_path)
    errors: list[str] = []
    if manifest.get("schema_version") != SCHEMA_VERSION:
        errors.append("schema_version_invalid")
    if manifest.get("no_answer_keys") is not True:
        errors.append("answer_key_policy_missing")
    profiles = manifest.get("profiles")
    if not isinstance(profiles, dict):
        errors.append("profiles_missing")
        profiles = {}
    for forbidden in find_forbidden_fields(manifest):
        errors.append(f"forbidden_field:{forbidden}")

    def validate_artifact_record(value: Any, label: str) -> None:
        if not isinstance(value, dict):
            errors.append(f"artifact_missing:{label}")
            return
        path = value.get("path")
        expected = value.get("sha256")
        if not isinstance(path, str) or not isinstance(expected, str):
            errors.append(f"identity_missing:{label}")
            return
        candidate = Path(path)
        if not candidate.is_file():
            errors.append(f"artifact_missing:{label}")
        elif sha256_file(candidate) != expected:
            errors.append(f"artifact_hash_mismatch:{label}")

    source = manifest.get("source")
    if not isinstance(source, dict):
        errors.append("source_missing")
    else:
        source_artifacts = source.get("artifacts")
        if not isinstance(source_artifacts, dict):
            errors.append("source_artifacts_missing")
        else:
            for name, value in source_artifacts.items():
                validate_artifact_record(value, f"source:{name}")
    generator = manifest.get("generator")
    if not isinstance(generator, dict):
        errors.append("generator_missing")
    else:
        generator_artifacts = generator.get("artifacts")
        if not isinstance(generator_artifacts, dict):
            errors.append("generator_artifacts_missing")
        else:
            for name, value in generator_artifacts.items():
                validate_artifact_record(value, f"generator:{name}")
    resource_limits = manifest.get("resource_limits")
    if not isinstance(resource_limits, dict):
        errors.append("resource_limits_missing")
    else:
        if not isinstance(resource_limits.get("timeout_seconds"), int) or resource_limits["timeout_seconds"] <= 0:
            errors.append("resource_limits_timeout_invalid")
        if not isinstance(resource_limits.get("threads"), int) or resource_limits["threads"] <= 0:
            errors.append("resource_limits_threads_invalid")
        if not isinstance(resource_limits.get("memory"), str) or not resource_limits["memory"]:
            errors.append("resource_limits_memory_missing")
        if not isinstance(resource_limits.get("variant"), str) or not resource_limits["variant"]:
            errors.append("resource_limits_variant_missing")
        for field in ("cpu", "disk", "network"):
            if not isinstance(resource_limits.get(field), str) or not resource_limits[field]:
                errors.append(f"resource_limits_{field}_missing")
    for profile in PROFILES:
        record = profiles.get(profile)
        if not isinstance(record, dict):
            errors.append(f"profile_missing:{profile}")
            continue
        if record.get("status") != "complete":
            errors.append(f"profile_status:{profile}:{record.get('status')}")
            continue
        required = (
            ("source_ontology", "generated-dataset"),
            ("profile_tbox", "profile-tbox"),
            ("profile_abox", "profile-abox"),
            ("generated_dataset", "generated-dataset"),
            ("conversion_artifact", "profile-source-nquads"),
            ("dataset_artifact", "profile-source-nquads"),
            ("diagnostics", "generator-diagnostics"),
        )
        for field, role in required:
            value = record.get(field)
            if not isinstance(value, dict):
                errors.append(f"field_missing:{profile}:{field}")
                continue
            path = value.get("path")
            expected = value.get("sha256")
            if not isinstance(path, str) or not isinstance(expected, str):
                errors.append(f"identity_missing:{profile}:{field}")
                continue
            candidate = Path(path)
            if not candidate.is_file():
                errors.append(f"artifact_missing:{profile}:{field}")
            elif sha256_file(candidate) != expected:
                errors.append(f"artifact_hash_mismatch:{profile}:{field}")
            if value.get("role") != role:
                errors.append(f"artifact_role_mismatch:{profile}:{field}")
        dataset = record.get("dataset")
        if isinstance(dataset, dict):
            for field, source_field in (
                ("sha256", "conversion_artifact"),
                ("source_ontology_sha256", "generated_dataset"),
                ("tbox_sha256", "profile_tbox"),
                ("abox_sha256", "profile_abox"),
            ):
                nested = record.get(source_field)
                if isinstance(nested, dict) and dataset.get(field) != nested.get("sha256"):
                    errors.append(f"dataset_hash_mismatch:{profile}:{field}")
            if dataset.get("artifact_id") != record.get("dataset_artifact", {}).get("id"):
                errors.append(f"dataset_artifact_identity_mismatch:{profile}")
        if not isinstance(dataset, dict):
            errors.append(f"dataset_identity_missing:{profile}")
        else:
            for field in ("id", "profile", "artifact_id", "artifact_path", "artifact_revision", "sha256", "source_ontology_sha256", "tbox_id", "tbox_sha256", "abox_id", "abox_sha256"):
                if not isinstance(dataset.get(field), str) or not dataset[field]:
                    errors.append(f"dataset_field_missing:{profile}:{field}")
        context = record.get("profile_context")
        if not isinstance(context, dict) or not context.get("id") or not context.get("directory"):
            errors.append(f"context_identity_missing:{profile}")
        if record.get("identity_sha256") != json_digest(stable_profile_identity(record)):
            errors.append(f"profile_identity_hash_mismatch:{profile}")
    identities = [
        profiles[profile].get("identity_sha256")
        for profile in PROFILES
        if isinstance(profiles.get(profile), dict)
        and profiles[profile].get("identity_sha256")
    ]
    if len(identities) != len(set(identities)):
        errors.append("profile_identity_reused")
    status = "complete" if not errors else "failed"
    return {
        "schema_version": SCHEMA_VERSION,
        "manifest": str(manifest_path),
        "status": status,
        "errors": errors,
        "profile_statuses": {
            profile: profiles.get(profile, {}).get("status") if isinstance(profiles.get(profile), dict) else "not_available"
            for profile in PROFILES
        },
    }


def find_forbidden_fields(value: Any, prefix: str = "") -> list[str]:
    forbidden: list[str] = []
    if isinstance(value, dict):
        for key, nested in value.items():
            path = f"{prefix}.{key}" if prefix else key
            normalized = key.lower().replace("-", "_")
            if normalized in {"oracle", "oracles", "answer_key", "answer_keys", "answer_file", "answer_sha256", "oracle_key"}:
                forbidden.append(path)
            forbidden.extend(find_forbidden_fields(nested, path))
    elif isinstance(value, list):
        for index, nested in enumerate(value):
            forbidden.extend(find_forbidden_fields(nested, f"{prefix}[{index}]"))
    return forbidden


def compare_reproducibility(first_path: Path, second_path: Path) -> dict[str, Any]:
    first = load_manifest(require_file(first_path, "first dataset manifest"))
    second = load_manifest(require_file(second_path, "second dataset manifest"))
    differences: list[str] = []
    first_profiles = first.get("profiles", {})
    second_profiles = second.get("profiles", {})
    for profile in PROFILES:
        first_record = first_profiles.get(profile, {})
        second_record = second_profiles.get(profile, {})
        first_status = first_record.get("status") if isinstance(first_record, dict) else None
        second_status = second_record.get("status") if isinstance(second_record, dict) else None
        if first_status != second_status:
            differences.append(f"profile_status:{profile}")
        first_identity = first_record.get("identity_sha256") if isinstance(first_record, dict) else None
        second_identity = second_record.get("identity_sha256") if isinstance(second_record, dict) else None
        if first_identity != second_identity:
            differences.append(f"profile_identity:{profile}")
    for section in ("source", "generator", "resource_limits"):
        left = first.get(section)
        right = second.get(section)
        if section == "source" and isinstance(left, dict) and isinstance(right, dict):
            left = {"revision": left.get("revision"), "identity_sha256": left.get("identity_sha256")}
            right = {"revision": right.get("revision"), "identity_sha256": right.get("identity_sha256")}
        if section == "generator" and isinstance(left, dict) and isinstance(right, dict):
            left = {"revision": left.get("revision"), "artifact_digest": left.get("artifact_digest")}
            right = {"revision": right.get("revision"), "artifact_digest": right.get("artifact_digest")}
        if left != right:
            differences.append(f"{section}_identity")
    return {
        "status": "complete" if not differences else "failed",
        "first": str(first_path.expanduser().resolve()),
        "second": str(second_path.expanduser().resolve()),
        "differences": differences,
    }


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    subparsers = parser.add_subparsers(dest="command", required=True)
    prepare_parser = subparsers.add_parser("prepare")
    prepare_parser.add_argument("--source-dir", type=Path, required=True)
    prepare_parser.add_argument("--output-dir", type=Path, required=True)
    prepare_parser.add_argument("--profiles", default=','.join(PROFILES))
    prepare_parser.add_argument("--universities", type=int, default=1)
    prepare_parser.add_argument("--seed", type=int, default=1)
    prepare_parser.add_argument("--source-revision", default="")
    prepare_parser.add_argument("--query-manifest", type=Path)
    prepare_parser.add_argument("--java-bin", default="java")
    prepare_parser.add_argument("--javac-bin", default="javac")
    prepare_parser.add_argument("--timeout-seconds", type=int, default=600)
    prepare_parser.add_argument("--memory", default="24GiB")
    prepare_parser.add_argument("--threads", type=int, default=1)
    prepare_parser.add_argument("--cpu", default="1")
    prepare_parser.add_argument("--disk", default="10GiB")
    prepare_parser.add_argument("--network", default="local-only")

    validate_parser = subparsers.add_parser("validate")
    validate_parser.add_argument("--manifest", type=Path, required=True)

    compare_parser = subparsers.add_parser("compare")
    compare_parser.add_argument("--first", type=Path, required=True)
    compare_parser.add_argument("--second", type=Path, required=True)
    compare_parser.add_argument("--output", type=Path, required=True)

    args = parser.parse_args(argv)
    try:
        if args.command == "prepare":
            return prepare(args)
        if args.command == "validate":
            result = validate_manifest(args.manifest)
            print(json.dumps(result, ensure_ascii=False, indent=2, sort_keys=True))
            return 0 if result["status"] == "complete" else 1
        if args.command == "compare":
            result = compare_reproducibility(args.first, args.second)
            write_json(args.output, result)
            print(json.dumps(result, ensure_ascii=False, indent=2, sort_keys=True))
            return 0 if result["status"] == "complete" else 1
    except SourceUnavailableError as error:
        print(
            json.dumps(
                {
                    "status": "not_available",
                    "reason": str(error),
                    "answer_key_policy": "not_permitted",
                },
                ensure_ascii=False,
                sort_keys=True,
            )
        )
        print(f"profile dataset freezer not_available: {error}", file=sys.stderr)
        return 125
    except (DatasetError, OSError, subprocess.TimeoutExpired) as error:
        print(f"profile dataset freezer failed: {error}", file=sys.stderr)
        return 1
    return 2


if __name__ == "__main__":
    raise SystemExit(main())
