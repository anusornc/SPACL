#!/usr/bin/env python3
"""Run and validate the real four-profile OWL2Bench Konclude smoke campaign.

This command is intentionally execution-only.  It removes oracle inputs from
the two runner environments, preserves the runner's raw/canonical artifacts,
and reports correctness as blocked until an accepted oracle exists.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import shutil
import subprocess
import sys
from collections import Counter
from datetime import datetime, timezone
from pathlib import Path
from typing import Any


PROFILES = ("DL", "EL", "QL", "RL")
EXPECTED_PROFILE_COUNTS = {"DL": 22, "EL": 10, "QL": 10, "RL": 18}
EXPECTED_QUERY_COUNT = 60
SHA256_RE = re.compile(r"^(?:sha256:)?[0-9a-fA-F]{64}$")
ORACLE_ENVIRONMENT_KEYS = (
    "OWL2BENCH_QUERY_ORACLE_DIR",
    "OWL2BENCH_QUERY_ORACLE_MANIFEST",
    "OWL2BENCH_QUERY_ORACLE_LOOKUP",
    "OWL2BENCH_QUERY_DATASET_METADATA",
    "OWL2BENCH_QUERY_DATASET_ID",
    "OWL2BENCH_QUERY_DATASET_SHA256",
)


def text(value: Any, default: str = "") -> str:
    if value is None:
        return default
    return str(value)


def is_hash(value: Any) -> bool:
    return isinstance(value, str) and SHA256_RE.fullmatch(value) is not None


def sha256_file(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def load_json(path: Path) -> dict[str, Any]:
    value = json.loads(path.read_text(encoding="utf-8"))
    return value if isinstance(value, dict) else {}


def write_json(path: Path, value: dict[str, Any]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, indent=2, sort_keys=True) + "\n", encoding="utf-8")


def artifact(path_value: str | Path | None, role: str) -> dict[str, Any]:
    if not path_value:
        return {"role": role, "status": "not_available", "path": None}
    path = Path(path_value).expanduser()
    if not path.is_file():
        return {"role": role, "status": "not_available", "path": str(path)}
    return {
        "role": role,
        "status": "recorded",
        "path": str(path.resolve()),
        "sha256": sha256_file(path),
        "bytes": path.stat().st_size,
    }


def resolve_path(value: str) -> Path | None:
    if not value:
        return None
    candidate = Path(value).expanduser()
    if candidate.is_file():
        return candidate.resolve()
    located = shutil.which(value)
    return Path(located).resolve() if located else None


def run_command(command: list[str], *, cwd: Path, stdout: Path, stderr: Path, env: dict[str, str]) -> int:
    stdout.parent.mkdir(parents=True, exist_ok=True)
    with stdout.open("w", encoding="utf-8") as stdout_handle, stderr.open(
        "w", encoding="utf-8"
    ) as stderr_handle:
        completed = subprocess.run(
            command,
            cwd=cwd,
            env=env,
            stdout=stdout_handle,
            stderr=stderr_handle,
            check=False,
        )
    return completed.returncode


def manifest_provenance(staged_dir: Path) -> dict[str, Any]:
    manifest = staged_dir / "manifest.tsv"
    result: dict[str, Any] = {
        "status": "not_available",
        "manifest": str(manifest),
        "manifest_sha256": None,
        "source_corpus": [],
        "source_revision": [],
        "query_file_hashes": {},
        "declared_profile_counts": {profile: 0 for profile in PROFILES},
    }
    if not manifest.is_file():
        return result
    result["manifest_sha256"] = sha256_file(manifest)
    rows = []
    for line in manifest.read_text(encoding="utf-8").splitlines()[1:]:
        if not line:
            continue
        fields = line.split("\t")
        if len(fields) != 7:
            continue
        query_path, _source_query, _query_id, profile, _workload, corpus, revision = fields
        rows.append((query_path, profile, corpus, revision))
        for value, key in ((corpus, "source_corpus"), (revision, "source_revision")):
            if value and value not in result[key]:
                result[key].append(value)
        query_file = Path(query_path)
        if query_file.is_file():
            result["query_file_hashes"][str(query_file.resolve())] = sha256_file(query_file)
        profiles = [part.strip().upper() for part in profile.split(",") if part.strip()]
        for profile_name in profiles:
            if profile_name in result["declared_profile_counts"]:
                result["declared_profile_counts"][profile_name] += 1
    result["status"] = "recorded"
    result["manifest_row_count"] = len(rows)
    result["expanded_profile_counts"] = {
        profile: result["declared_profile_counts"][profile] for profile in PROFILES
    }
    return result


def generator_provenance() -> dict[str, Any]:
    generator_dir_value = os.environ.get("OWL2BENCH_SMOKE_GENERATOR_DIR", "")
    generator_dir = Path(generator_dir_value).expanduser() if generator_dir_value else None
    result: dict[str, Any] = {
        "status": "not_available",
        "directory": str(generator_dir) if generator_dir else None,
        "revision": os.environ.get("OWL2BENCH_SMOKE_GENERATOR_REVISION", ""),
        "command": os.environ.get(
            "OWL2BENCH_SMOKE_GENERATOR_COMMAND", "java -jar OWL2Bench.jar 1 <PROFILE> 1"
        ),
        "artifacts": {},
    }
    if generator_dir is None or not generator_dir.is_dir():
        return result
    if not result["revision"]:
        try:
            result["revision"] = subprocess.check_output(
                ["git", "-C", str(generator_dir), "rev-parse", "HEAD"],
                text=True,
                stderr=subprocess.DEVNULL,
            ).strip()
        except (OSError, subprocess.CalledProcessError):
            result["revision"] = "unavailable"
    for name in ("OWL2Bench.jar", "config.properties", "RandomNames.xlsx"):
        result["artifacts"][name] = artifact(generator_dir / name, f"generator:{name}")
    if (
        result["revision"] not in {"", "unavailable"}
        and all(value["status"] == "recorded" for value in result["artifacts"].values())
    ):
        result["status"] = "recorded"
    return result


def resolve_engine_paths(engine_bin: str) -> dict[str, Path | None]:
    actual_binary = (
        os.environ.get("KONCLUDE_UNIFIED_BIN")
        or os.environ.get("KONCLUDE_BIN")
        or (str(Path(os.environ["KONCLUDE_HOME"]) / "Konclude") if os.environ.get("KONCLUDE_HOME") else "")
    )
    config = os.environ.get("KONCLUDE_UNIFIED_CONFIG") or os.environ.get("KONCLUDE_CONFIG")
    converter = os.environ.get("KONCLUDE_UNIFIED_CONVERTER") or os.environ.get("KONCLUDE_CONVERTER")
    return {
        "adapter": resolve_path(engine_bin),
        "binary": resolve_path(actual_binary),
        "config": resolve_path(config),
        "converter": resolve_path(converter),
    }


def runtime_provenance(smoke_dir: Path, run_id: str, binary: Path | None) -> dict[str, Any]:
    supplied = os.environ.get("OWL2BENCH_SMOKE_RUNTIME_PROVENANCE_FILE", "")
    dependency_artifact = os.environ.get("KONCLUDE_UNIFIED_RUNTIME_ARTIFACT", "")
    if supplied:
        record = artifact(supplied, "runtime-provenance")
        record["status"] = "recorded" if record["status"] == "recorded" else "not_available"
        record["library_path"] = os.environ.get("LD_LIBRARY_PATH", "")
        record["dependency_artifact"] = artifact(dependency_artifact, "runtime-dependency")
        return record
    if binary is None:
        return {
            "role": "runtime-provenance",
            "status": "not_available",
            "path": None,
            "reason": "no Konclude binary was supplied for runtime inspection",
            "dependency_artifact": artifact(dependency_artifact, "runtime-dependency"),
        }
    output = smoke_dir / f"{run_id}.runtime-provenance.txt"
    try:
        ldd = subprocess.run(
            ["ldd", str(binary)], capture_output=True, text=True, check=False
        )
        package = subprocess.run(
            ["dpkg-query", "-W", "-f=${Package} ${Version} ${Architecture}\\n", "libpcre3"],
            capture_output=True,
            text=True,
            check=False,
        )
        content = "# ldd\n" + ldd.stdout + ldd.stderr + "# package\n" + package.stdout + package.stderr
    except OSError as exc:
        return {"role": "runtime-provenance", "status": "not_available", "path": None, "reason": str(exc)}
    output.write_text(content, encoding="utf-8")
    return {
        "role": "runtime-provenance",
        "status": "recorded" if content else "not_available",
        "path": str(output),
        "sha256": sha256_file(output),
        "binary": str(binary),
        "library_path": os.environ.get("LD_LIBRARY_PATH", ""),
        "ldd_exit": ldd.returncode,
        "package_query_exit": package.returncode,
        "dependency_artifact": artifact(dependency_artifact, "runtime-dependency"),
    }


def profile_dataset_provenance(data_dir: Path | None) -> dict[str, Any]:
    records: dict[str, Any] = {}
    for profile in PROFILES:
        context = data_dir / profile if data_dir else None
        record: dict[str, Any] = {
            "profile": profile,
            "context_dir": str(context) if context else None,
            "status": "not_available",
            "input_files": [],
            "provenance_file": None,
        }
        if context is None or not context.is_dir():
            records[profile] = record
            continue
        record["input_files"] = [
            artifact(path, f"dataset:{profile}:{path.name}")
            for path in sorted(context.iterdir())
            if path.is_file() and path.suffix.lower() in {".ofn", ".omn", ".fss", ".functional", ".owl", ".rdf", ".xml"}
        ]
        provenance_path = context / ".konclude-unified" / "provenance.json"
        record["provenance_file"] = str(provenance_path)
        if provenance_path.is_file():
            try:
                provenance = load_json(provenance_path)
            except (OSError, json.JSONDecodeError):
                provenance = {}
            record["provenance"] = provenance
            record["status"] = "recorded" if provenance.get("prepared_sha256") else "not_available"
        records[profile] = record
    return records


def artifact_from_metadata(value: Any, role: str, run_dir: Path) -> dict[str, Any]:
    if not value:
        return artifact(None, role)
    candidate = Path(str(value)).expanduser()
    if not candidate.is_absolute():
        candidates = (Path.cwd() / candidate, run_dir.parent / candidate)
        candidate = next((path for path in candidates if path.is_file()), candidate)
    return artifact(candidate, role)


def split_stack_provenance(run_dir: Path) -> dict[str, Any]:
    """Capture the validated split-stack inputs without copying secrets."""

    metadata = load_json(run_dir / "run_metadata.json")
    result: dict[str, Any] = {"requested": False}
    for section_name, manifest_role, validation_role in (
        ("component_manifest", "split-stack-component-manifest", "split-stack-component-validation"),
        ("materialization", "split-stack-materialization-manifest", "split-stack-materialization-validation"),
    ):
        section = metadata.get(section_name)
        if not isinstance(section, dict):
            result[section_name] = {
                "requested": False,
                "status": "not_available",
                "profiles": {},
            }
            continue
        result["requested"] = result["requested"] or section.get("requested") is True
        result[section_name] = {
            "requested": section.get("requested") is True,
            "status": section.get("status", "not_available"),
            "manifest": artifact_from_metadata(section.get("manifest"), manifest_role, run_dir),
            "validation": artifact_from_metadata(section.get("validation_file"), validation_role, run_dir),
            "profiles": section.get("profiles", {}) if isinstance(section.get("profiles"), dict) else {},
        }
    return result


def engine_provenance(
    engine_bin: str, capabilities_file: Path, run_rows: list[dict[str, Any]]
) -> dict[str, Any]:
    try:
        capabilities = load_json(capabilities_file)
    except (OSError, json.JSONDecodeError):
        capabilities = {}
    observed = []
    for row in run_rows:
        provenance = row.get("engine_provenance")
        if isinstance(provenance, dict) and provenance not in observed:
            observed.append(provenance)
    paths = resolve_engine_paths(engine_bin)
    if observed:
        if paths["binary"] is None:
            paths["binary"] = resolve_path(text(observed[0].get("binary_path")))
        if paths["config"] is None:
            paths["config"] = resolve_path(text(observed[0].get("config_path")))
        if paths["converter"] is None:
            paths["converter"] = resolve_path(text(observed[0].get("converter_path")))
    converter_record = artifact(paths["converter"], "owl2bench-converter")
    if paths["converter"] is None and observed and all(
        value.get("source_kind") == "functional-style" for value in observed
    ):
        converter_record = {
            "role": "owl2bench-converter",
            "status": "not_used_native_input",
            "path": None,
            "declared_sha256": capabilities.get("converter_sha256"),
        }
    return {
        "adapter": artifact(paths["adapter"], "query-adapter"),
        "binary": artifact(paths["binary"], "konclude-executable"),
        "configuration": artifact(paths["config"], "konclude-configuration"),
        "converter": converter_record,
        "capabilities": artifact(capabilities_file, "query-capabilities"),
        "declared": capabilities,
        "observed_profile_provenance": observed,
        "variant": os.environ.get("KONCLUDE_UNIFIED_VARIANT", "konclude-unified"),
        "threads": os.environ.get("KONCLUDE_UNIFIED_THREADS", "1"),
        "memory_limit": os.environ.get("KONCLUDE_UNIFIED_MEMORY_LIMIT", "24GiB"),
    }


def safe_case_id(case_id: str) -> str:
    return re.sub(r"[^A-Za-z0-9._-]", "_", case_id)


def read_rows(run_dir: Path) -> tuple[list[dict[str, Any]], list[str]]:
    rows: list[dict[str, Any]] = []
    errors: list[str] = []
    for path in sorted(run_dir.glob("*.json")):
        if path.name in {"run_metadata.json", "summary.json"}:
            continue
        try:
            value = load_json(path)
        except (OSError, json.JSONDecodeError) as exc:
            errors.append(f"{path.name}: {exc}")
            continue
        if value.get("schema_version") != 1 or not value.get("case_id"):
            errors.append(f"{path.name}: malformed result record")
            continue
        rows.append(value)
    return rows, errors


def numeric(value: Any) -> bool:
    return isinstance(value, (int, float)) and not isinstance(value, bool)


def validate_run(run_dir: Path, data_dir: Path | None) -> dict[str, Any]:
    errors: list[str] = []
    metadata_path = run_dir / "run_metadata.json"
    summary_path = run_dir / "summary.json"
    try:
        metadata = load_json(metadata_path)
    except (OSError, json.JSONDecodeError):
        metadata = {}
        errors.append("run_metadata.json is missing or malformed")
    try:
        summary = load_json(summary_path)
    except (OSError, json.JSONDecodeError):
        summary = {}
        errors.append("summary.json is missing or malformed")
    rows, row_errors = read_rows(run_dir)
    errors.extend(row_errors)
    counts = Counter(text(row.get("result_status"), "unknown") for row in rows)
    profile_counts = Counter(text(row.get("query_profile"), "unknown") for row in rows)
    if len(rows) != EXPECTED_QUERY_COUNT:
        errors.append(f"result row count is {len(rows)}, expected {EXPECTED_QUERY_COUNT}")
    actual_profile_counts = {profile: profile_counts.get(profile, 0) for profile in PROFILES}
    if actual_profile_counts != EXPECTED_PROFILE_COUNTS:
        errors.append(f"profile counts are {actual_profile_counts}, expected {EXPECTED_PROFILE_COUNTS}")
    if any(profile not in PROFILES for profile in profile_counts):
        errors.append("unclassified profile row exists")
    case_ids = [text(row.get("case_id")) for row in rows]
    if len(case_ids) != len(set(case_ids)):
        errors.append("duplicate case IDs exist")
    if summary.get("unclassified_count", 0) != 0:
        errors.append("report recorded unclassified rows")
    if metadata.get("data_mode") != "directory":
        errors.append("run did not use directory data mode")
    profile_contexts = metadata.get("profile_contexts")
    if not isinstance(profile_contexts, dict) or set(profile_contexts) != set(PROFILES):
        errors.append("run metadata does not contain exactly four profile contexts")
    component_metadata = metadata.get("component_manifest")
    materialization_metadata = metadata.get("materialization")
    if not isinstance(component_metadata, dict) or component_metadata.get("requested") is not True:
        errors.append("run metadata does not request the split-stack component manifest")
    elif component_metadata.get("status") != "available":
        errors.append("split-stack component manifest is not available")
    if not isinstance(materialization_metadata, dict) or materialization_metadata.get("requested") is not True:
        errors.append("run metadata does not request the split-stack materialization manifest")
    elif materialization_metadata.get("status") != "complete":
        errors.append("split-stack materialization is not complete")

    completed = 0
    completed_without_oracle = 0
    canonical_hashes: dict[str, str] = {}
    materialization_hash_sets: dict[str, set[str]] = {profile: set() for profile in PROFILES}
    for row in rows:
        case_id = text(row.get("case_id"))
        profile = text(row.get("query_profile"), "unknown")
        if profile not in PROFILES:
            continue
        context = row.get("profile_context") if isinstance(row.get("profile_context"), dict) else {}
        expected_context = str((data_dir / profile).resolve()) if data_dir else None
        if expected_context and context.get("directory") != expected_context:
            errors.append(f"{case_id}: profile context fallback or mismatch")
        if context.get("profile") != profile:
            errors.append(f"{case_id}: profile context identity mismatch")
        provenance = row.get("engine_provenance")
        if not isinstance(provenance, dict) or provenance.get("profile") != profile:
            errors.append(f"{case_id}: engine provenance is missing or mixed")
        component = row.get("component_manifest") if isinstance(row.get("component_manifest"), dict) else {}
        if component.get("profile") != profile or component.get("status") != "available":
            errors.append(f"{case_id}: split-stack component provenance is missing or unavailable")
        elif not is_hash(component.get("manifest_identity")):
            errors.append(f"{case_id}: split-stack component identity is missing or invalid")
        materialization = row.get("materialization") if isinstance(row.get("materialization"), dict) else {}
        if materialization.get("profile") != profile or materialization.get("status") != "complete":
            errors.append(f"{case_id}: split-stack materialization is missing or incomplete")
        elif not is_hash(materialization.get("materialization_identity")):
            errors.append(f"{case_id}: split-stack materialization identity is missing or invalid")
        else:
            output_graph = materialization.get("output_graph")
            graph_hash = output_graph.get("sha256") if isinstance(output_graph, dict) else None
            if not is_hash(graph_hash):
                errors.append(f"{case_id}: canonical materialization graph hash is missing or invalid")
            else:
                materialization_hash_sets[profile].add(graph_hash)
        if not is_hash(row.get("source_query_sha256")):
            errors.append(f"{case_id}: persisted source hash is missing or invalid")
        oracle = row.get("oracle") if isinstance(row.get("oracle"), dict) else {}
        canonical = row.get("canonicalization") if isinstance(row.get("canonicalization"), dict) else {}
        execution_status = text(row.get("execution_status"), "unknown")
        result_status = text(row.get("result_status"), "unknown")
        if execution_status == "success":
            completed += 1
            if result_status != "oracle_unavailable" or oracle.get("status") == "available":
                errors.append(f"{case_id}: completed case made an oracle/correctness claim")
            else:
                completed_without_oracle += 1
            if canonical.get("status") != "available" or not is_hash(canonical.get("result_hash")):
                errors.append(f"{case_id}: canonical result hash is missing")
            else:
                semantic = row.get("semantic_result") or row.get("reasoning_result")
                if semantic != canonical.get("result_hash"):
                    errors.append(f"{case_id}: semantic result is not the canonical hash")
                canonical_hashes[case_id] = canonical["result_hash"]
            if not numeric(row.get("query_time_ms")) or row.get("query_time_ms", -1) < 0:
                errors.append(f"{case_id}: query timing is missing")
            load_valid = numeric(row.get("load_time_ms")) and row.get("load_time_ms", -1) >= 0
            load_not_applicable = row.get("load_time_ms") == -1 and row.get("load_status") == "not_applicable"
            if not load_valid and not load_not_applicable:
                errors.append(f"{case_id}: load timing is missing")
            raw_output = run_dir / "raw" / f"{safe_case_id(case_id)}.output"
            if not raw_output.is_file():
                errors.append(f"{case_id}: raw output artifact is missing")
            diagnostics = row.get("diagnostics_file")
            if not diagnostics or not Path(diagnostics).is_file():
                errors.append(f"{case_id}: diagnostics artifact is missing")
            canonical_file = canonical.get("canonical_result_file")
            if not canonical_file or not Path(canonical_file).is_file():
                errors.append(f"{case_id}: canonical result artifact is missing")
        elif result_status not in {"not_available", "unsupported", "failed", "timeout"}:
            errors.append(f"{case_id}: non-completed case has no explicit result status")

    materialization_hashes: dict[str, str] = {}
    for profile, hashes in materialization_hash_sets.items():
        if len(hashes) > 1:
            errors.append(f"{profile}: materialization graph hash changed within the run")
        elif len(hashes) == 1:
            materialization_hashes[profile] = next(iter(hashes))

    validation_status = "passed" if not errors and completed > 0 else "blocked"
    return {
        "run_id": metadata.get("run_id", run_dir.name),
        "run_dir": str(run_dir),
        "query_count": len(rows),
        "profile_case_counts": actual_profile_counts,
        "result_status_counts": dict(sorted(counts.items())),
        "completed_case_count": completed,
        "completed_without_oracle": completed_without_oracle,
        "canonical_hashes": canonical_hashes,
        "materialization_hashes": materialization_hashes,
        "validation_status": validation_status,
        "errors": errors,
        "metadata": metadata,
    }


def run_once(
    *,
    repo_root: Path,
    run_sh: Path,
    run_id: str,
    results_root: Path,
    staged_dir: Path,
    data_dir: Path | None,
    engine_bin: str,
    capabilities_file: Path,
    smoke_dir: Path,
    timeout_seconds: str,
) -> tuple[int, Path]:
    env = os.environ.copy()
    env["RUN_ID"] = run_id
    env["OWL2BENCH_QUERY_RESULTS_ROOT"] = str(results_root)
    env["OWL2BENCH_QUERY_STAGED_DIR"] = str(staged_dir)
    env["OWL2BENCH_QUERY_CAPABILITIES_FILE"] = str(capabilities_file)
    env["TIMEOUT_SECONDS"] = timeout_seconds
    if engine_bin:
        env["OWL2BENCH_QUERY_ENGINE_BIN"] = engine_bin
    else:
        env.pop("OWL2BENCH_QUERY_ENGINE_BIN", None)
    if data_dir:
        env["OWL2BENCH_QUERY_DATA_DIR"] = str(data_dir)
    else:
        env.pop("OWL2BENCH_QUERY_DATA_DIR", None)
    # This runner is the split-stack smoke contract, so never inherit a
    # caller's legacy single-engine mode.  Manifest paths may be supplied via
    # the smoke-specific aliases or the normal query-runner variables.
    env["OWL2BENCH_QUERY_SPLIT_STACK"] = "1"
    for query_key, smoke_key in (
        (
            "OWL2BENCH_QUERY_COMPONENT_MANIFEST",
            "OWL2BENCH_SMOKE_COMPONENT_MANIFEST",
        ),
        (
            "OWL2BENCH_QUERY_MATERIALIZATION_MANIFEST",
            "OWL2BENCH_SMOKE_MATERIALIZATION_MANIFEST",
        ),
    ):
        manifest = os.environ.get(smoke_key) or os.environ.get(query_key)
        if manifest:
            manifest_path = Path(manifest).expanduser()
            if not manifest_path.is_absolute():
                manifest_path = (repo_root / manifest_path).resolve()
            env[query_key] = str(manifest_path)
        else:
            env.pop(query_key, None)
    for key in ORACLE_ENVIRONMENT_KEYS:
        env.pop(key, None)
    run_dir = results_root / run_id
    return_code = run_command(
        [str(run_sh)],
        cwd=repo_root,
        stdout=smoke_dir / f"{run_id}.runner.stdout",
        stderr=smoke_dir / f"{run_id}.runner.stderr",
        env=env,
    )
    return return_code, run_dir


def add_smoke_metadata(run_dir: Path, smoke_metadata: dict[str, Any]) -> None:
    metadata_path = run_dir / "run_metadata.json"
    metadata = load_json(metadata_path)
    metadata["real_smoke"] = smoke_metadata
    write_json(metadata_path, metadata)


def stable_hashes(first: dict[str, Any], second: dict[str, Any]) -> dict[str, Any]:
    first_hashes = first.get("canonical_hashes", {})
    second_hashes = second.get("canonical_hashes", {})
    mismatches = []
    if set(first_hashes) != set(second_hashes):
        mismatches.append(
            {
                "kind": "case_set",
                "first_only": sorted(set(first_hashes) - set(second_hashes)),
                "second_only": sorted(set(second_hashes) - set(first_hashes)),
            }
        )
    for case_id in sorted(set(first_hashes) & set(second_hashes)):
        if first_hashes[case_id] != second_hashes[case_id]:
            mismatches.append(
                {"kind": "canonical_hash", "case_id": case_id, "first": first_hashes[case_id], "second": second_hashes[case_id]}
            )
    first_materialization_hashes = first.get("materialization_hashes", {})
    second_materialization_hashes = second.get("materialization_hashes", {})
    if set(first_materialization_hashes) != set(second_materialization_hashes):
        mismatches.append(
            {
                "kind": "materialization_graph_case_set",
                "first_only": sorted(set(first_materialization_hashes) - set(second_materialization_hashes)),
                "second_only": sorted(set(second_materialization_hashes) - set(first_materialization_hashes)),
            }
        )
    for profile in sorted(set(first_materialization_hashes) & set(second_materialization_hashes)):
        if first_materialization_hashes[profile] != second_materialization_hashes[profile]:
            mismatches.append(
                {
                    "kind": "materialization_graph_hash",
                    "profile": profile,
                    "first": first_materialization_hashes[profile],
                    "second": second_materialization_hashes[profile],
                }
            )
    status = (
        "passed"
        if first_hashes
        and second_hashes
        and first_materialization_hashes
        and second_materialization_hashes
        and not mismatches
        else "blocked"
    )
    return {
        "status": status,
        "first_completed_case_count": len(first_hashes),
        "second_completed_case_count": len(second_hashes),
        "first_materialization_profile_count": len(first_materialization_hashes),
        "second_materialization_profile_count": len(second_materialization_hashes),
        "mismatches": mismatches,
    }


def split_stack_provenance_complete(value: Any) -> bool:
    if not isinstance(value, dict) or value.get("requested") is not True:
        return False
    expectations = (
        ("component_manifest", "available", "manifest_identity"),
        ("materialization", "complete", "materialization_identity"),
    )
    for section_name, expected_status, identity_field in expectations:
        section = value.get(section_name)
        if not isinstance(section, dict):
            return False
        if section.get("requested") is not True or section.get("status") != expected_status:
            return False
        manifest = section.get("manifest")
        validation = section.get("validation")
        if (
            not isinstance(manifest, dict)
            or manifest.get("status") != "recorded"
            or not is_hash(manifest.get("sha256"))
        ):
            return False
        if (
            not isinstance(validation, dict)
            or validation.get("status") != "recorded"
            or not is_hash(validation.get("sha256"))
        ):
            return False
        profiles = section.get("profiles")
        if not isinstance(profiles, dict) or set(profiles) != set(PROFILES):
            return False
        for profile in PROFILES:
            record = profiles.get(profile)
            if not isinstance(record, dict) or record.get("profile") != profile:
                return False
            if record.get("status") != expected_status or not is_hash(record.get(identity_field)):
                return False
    return True


def run_campaign(repo_root: Path, query_run_dir: Path, output_dir: Path) -> dict[str, Any]:
    env = os.environ.copy()
    env["CAMPAIGN_OUTPUT_DIR"] = str(output_dir)
    env["OWL2BENCH_QUERY_RUN_DIR"] = str(query_run_dir)
    for key in ("W3C_RUN_DIR", "ORE_RUN_DIR", "OWL2BENCH_RUN_DIR"):
        if key not in os.environ:
            env.pop(key, None)
    command = [str(repo_root / "benchmarks/campaign/report.sh")]
    stdout = output_dir / "campaign.stdout"
    stderr = output_dir / "campaign.stderr"
    return_code = run_command(command, cwd=repo_root, stdout=stdout, stderr=stderr, env=env)
    summary_path = output_dir / "summary.json"
    if not summary_path.is_file():
        return {"status": "blocked", "return_code": return_code, "error": "campaign summary is missing"}
    summary = load_json(summary_path)
    query_suite = summary.get("suites", {}).get("owl2bench-query", {})
    status = (
        "passed"
        if return_code == 0
        and summary.get("correctness_gate") == "blocked"
        and summary.get("performance_claims_allowed") is False
        and query_suite.get("correctness_gate") == "blocked"
        else "blocked"
    )
    return {
        "status": status,
        "return_code": return_code,
        "output_dir": str(output_dir),
        "correctness_gate": summary.get("correctness_gate", "blocked"),
        "performance_claims_allowed": summary.get("performance_claims_allowed", False),
        "query_suite": query_suite,
    }


def write_smoke_report(path: Path, summary: dict[str, Any]) -> None:
    lines = [
        "# OWL2Bench Konclude Real Smoke Campaign",
        "",
        f"- Execution smoke gate: **{summary['smoke_gate']}**",
        "- Correctness gate: **blocked** (no accepted oracle is attached)",
        "- Performance/scalability claims: **suppressed**",
        "- No historical Stardog or official-oracle correctness claim is made.",
        "",
        "## Profile counts",
        "",
        "| Profile | Required | Run 1 | Run 2 |",
        "|---|---:|---:|---:|",
    ]
    for profile in PROFILES:
        lines.append(
            f"| {profile} | {EXPECTED_PROFILE_COUNTS[profile]} | "
            f"{summary['runs'][0]['profile_case_counts'].get(profile, 0)} | "
            f"{summary['runs'][1]['profile_case_counts'].get(profile, 0)} |"
        )
    lines.extend(
        [
            "",
            "## Status and stability",
            "",
            f"- Run 1 validation: **{summary['runs'][0]['validation_status']}**",
            f"- Run 2 validation: **{summary['runs'][1]['validation_status']}**",
            f"- Stable canonical hashes: **{summary['stability']['status']}**",
            "- Materialization graph hashes: **"
            + summary["stability"]["status"]
            + "** (compared independently per profile)",
            f"- Completed cases without oracle: **{summary['correctness']['completed_without_oracle']}**",
            f"- Oracle-unavailable result statuses: **{summary['correctness']['oracle_unavailable']}**",
            "",
            "## Provenance",
            "",
            f"- Engine: `{summary['provenance']['engine'].get('declared', {}).get('engine', 'unavailable')}` "
            f"`{summary['provenance']['engine'].get('declared', {}).get('engine_version', 'unavailable')}`",
            f"- Query manifest: `{summary['provenance']['query_source'].get('manifest_sha256', 'unavailable')}`",
            f"- Generator status: **{summary['provenance']['generator']['status']}**",
            f"- Runtime status: **{summary['provenance']['runtime']['status']}**",
            f"- Split-stack provenance: **{'recorded' if split_stack_provenance_complete(summary['provenance'].get('split_stack')) else 'blocked'}**",
            "",
            "## Campaign",
            "",
            f"- Campaign correctness gate: **{summary['campaign'].get('correctness_gate', 'blocked')}**",
            f"- Campaign performance claims: **{summary['campaign'].get('performance_claims_allowed', False)}**",
            "",
            "Raw outputs, diagnostics, canonical result artifacts, and timing fields remain under each run directory.",
        ]
    )
    path.write_text("\n".join(lines) + "\n", encoding="utf-8")


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--repo-root", type=Path, required=True)
    parser.add_argument("--staged-dir", type=Path)
    parser.add_argument("--data-dir", type=Path)
    parser.add_argument("--engine-bin")
    parser.add_argument("--capabilities-file", type=Path)
    parser.add_argument("--results-root", type=Path)
    parser.add_argument("--run-id")
    parser.add_argument("--timeout-seconds")
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    repo_root = args.repo_root.resolve()
    script_dir = repo_root / "benchmarks/external/owl2bench/query"
    staged_dir = (args.staged_dir or Path(os.environ.get("OWL2BENCH_QUERY_STAGED_DIR", repo_root / "benchmarks/ontologies/external/owl2bench/query-staged"))).expanduser().resolve()
    data_dir_value = args.data_dir or (Path(os.environ["OWL2BENCH_QUERY_DATA_DIR"]) if os.environ.get("OWL2BENCH_QUERY_DATA_DIR") else None)
    data_dir = data_dir_value.expanduser().resolve() if data_dir_value else None
    engine_bin = args.engine_bin or os.environ.get("OWL2BENCH_QUERY_ENGINE_BIN", str(script_dir / "konclude_unified_engine.sh"))
    capabilities_file = (args.capabilities_file or Path(os.environ.get("OWL2BENCH_QUERY_CAPABILITIES_FILE", script_dir / "konclude-unified-capabilities.json"))).expanduser().resolve()
    results_root = (args.results_root or Path(os.environ.get("OWL2BENCH_SMOKE_RESULTS_ROOT", repo_root / "benchmarks/external/owl2bench/query/results/history"))).expanduser().resolve()
    base_id = args.run_id or os.environ.get("OWL2BENCH_SMOKE_RUN_ID", f"owl2bench_konclude_smoke_{datetime.now().strftime('%Y%m%d_%H%M%S')}")
    timeout_seconds = args.timeout_seconds or os.environ.get("OWL2BENCH_SMOKE_TIMEOUT_SECONDS", "600")
    if not re.fullmatch(r"[A-Za-z0-9_-]+", base_id):
        print(f"[owl2bench-real-smoke][error] unsafe run id: {base_id}", file=sys.stderr)
        return 2
    smoke_dir = results_root / f"{base_id}_smoke"
    marker = smoke_dir / ".owl2bench-real-smoke-owned"
    if smoke_dir.exists() and not marker.exists() and any(smoke_dir.iterdir()):
        print(f"[owl2bench-real-smoke][error] refusing unstamped smoke directory: {smoke_dir}", file=sys.stderr)
        return 2
    smoke_dir.mkdir(parents=True, exist_ok=True)
    marker.write_text("Generated by run_real_smoke.py.\n", encoding="utf-8")

    run_ids = (f"{base_id}_r1", f"{base_id}_r2")
    run_results = []
    run_return_codes = []
    for run_id in run_ids:
        return_code, run_dir = run_once(
            repo_root=repo_root,
            run_sh=script_dir / "run.sh",
            run_id=run_id,
            results_root=results_root,
            staged_dir=staged_dir,
            data_dir=data_dir,
            engine_bin=engine_bin,
            capabilities_file=capabilities_file,
            smoke_dir=smoke_dir,
            timeout_seconds=timeout_seconds,
        )
        run_return_codes.append(return_code)
        if run_dir.is_dir():
            rows, _ = read_rows(run_dir)
            smoke_metadata = {
                "run_id": run_id,
                "variant": os.environ.get("KONCLUDE_UNIFIED_VARIANT", "konclude-unified"),
                "historical_stardog_reproduction": False,
                "official_oracle_claim": False,
                "oracle_policy": "oracle_unavailable_without_accepted_manifest",
                "resource_limits": {
                    "timeout_seconds": int(timeout_seconds),
                    "threads": os.environ.get("KONCLUDE_UNIFIED_THREADS", "1"),
                    "memory_limit": os.environ.get("KONCLUDE_UNIFIED_MEMORY_LIMIT", "24GiB"),
                },
                "engine": engine_provenance(engine_bin, capabilities_file, rows),
                "query_source": manifest_provenance(staged_dir),
                "generator": generator_provenance(),
                "profile_datasets": profile_dataset_provenance(data_dir),
                "runtime": runtime_provenance(smoke_dir, run_id, resolve_engine_paths(engine_bin)["binary"]),
                "split_stack": split_stack_provenance(run_dir),
            }
            add_smoke_metadata(run_dir, smoke_metadata)
            report_env = os.environ.copy()
            report_env["OWL2BENCH_QUERY_RESULTS_ROOT"] = str(results_root)
            report_result = subprocess.run(
                [str(script_dir / "report.sh"), run_id],
                cwd=repo_root,
                env=report_env,
                capture_output=True,
                text=True,
                check=False,
            )
            (smoke_dir / f"{run_id}.report.stdout").write_text(report_result.stdout, encoding="utf-8")
            (smoke_dir / f"{run_id}.report.stderr").write_text(report_result.stderr, encoding="utf-8")
            validation = validate_run(run_dir, data_dir)
            if return_code != 0:
                validation["errors"].append(f"query runner exited with status {return_code}")
                validation["validation_status"] = "blocked"
            if report_result.returncode != 0:
                validation["errors"].append(
                    f"query report exited with status {report_result.returncode}"
                )
                validation["validation_status"] = "blocked"
            run_results.append(validation)
        else:
            run_results.append(
                {
                    "run_id": run_id,
                    "run_dir": str(run_dir),
                    "query_count": 0,
                    "profile_case_counts": {profile: 0 for profile in PROFILES},
                    "result_status_counts": {},
                    "completed_case_count": 0,
                    "completed_without_oracle": 0,
                    "canonical_hashes": {},
                    "validation_status": "blocked",
                    "errors": ["runner did not create a run directory"],
                }
            )

    stability = stable_hashes(run_results[0], run_results[1])
    campaign = run_campaign(repo_root, results_root / run_ids[0], smoke_dir / "campaign")
    baseline_metadata = run_results[0].get("metadata", {})
    smoke_metadata = baseline_metadata.get("real_smoke", {}) if isinstance(baseline_metadata, dict) else {}
    provenance = {
        "engine": smoke_metadata.get("engine", engine_provenance(engine_bin, capabilities_file, [])),
        "query_source": smoke_metadata.get("query_source", manifest_provenance(staged_dir)),
        "generator": smoke_metadata.get("generator", generator_provenance()),
        "profile_datasets": smoke_metadata.get("profile_datasets", profile_dataset_provenance(data_dir)),
        "runtime": smoke_metadata.get("runtime", {"status": "not_available"}),
        "resource_limits": smoke_metadata.get("resource_limits", {"timeout_seconds": int(timeout_seconds)}),
        "split_stack": smoke_metadata.get("split_stack", split_stack_provenance(results_root / run_ids[0])),
    }
    completed_without_oracle = sum(result.get("completed_without_oracle", 0) for result in run_results[:1])
    oracle_unavailable = sum(
        result.get("result_status_counts", {}).get("oracle_unavailable", 0) for result in run_results[:1]
    )
    validation_errors = {
        result["run_id"]: result.get("errors", []) for result in run_results if result.get("errors")
    }
    split_stack_ok = split_stack_provenance_complete(provenance["split_stack"])
    provenance_ok = (
        split_stack_ok
        and provenance["engine"].get("declared", {}).get("adapter") == "konclude-unified"
        and provenance["engine"].get("adapter", {}).get("status") == "recorded"
        and provenance["engine"].get("binary", {}).get("status") == "recorded"
        and provenance["engine"].get("configuration", {}).get("status") == "recorded"
        and provenance["engine"].get("capabilities", {}).get("status") == "recorded"
        and provenance["engine"].get("converter", {}).get("status")
        in {"recorded", "not_used_native_input"}
        and all(
            provenance["engine"].get("declared", {}).get(field)
            for field in (
                "engine_version",
                "release_commit",
                "release_asset_sha256",
                "executable_sha256",
                "query_config_sha256",
                "converter_sha256",
            )
        )
        and all(
            is_hash(provenance["engine"].get("declared", {}).get(field))
            for field in (
                "release_asset_sha256",
                "executable_sha256",
                "query_config_sha256",
                "converter_sha256",
            )
        )
        and all(
            is_hash(provenance["engine"].get(name, {}).get("sha256"))
            for name in ("adapter", "binary", "configuration", "capabilities")
        )
        and len(provenance["engine"].get("observed_profile_provenance", [])) == 4
        and all(
            is_hash(value.get("binary_sha256"))
            and is_hash(value.get("config_sha256"))
            and is_hash(value.get("prepared_sha256"))
            for value in provenance["engine"].get("observed_profile_provenance", [])
        )
        and provenance["query_source"].get("status") == "recorded"
        and is_hash(provenance["query_source"].get("manifest_sha256"))
        and all(
            is_hash(value)
            for value in provenance["query_source"].get("query_file_hashes", {}).values()
        )
        and provenance["generator"].get("status") == "recorded"
        and all(
            value.get("status") == "recorded"
            and is_hash(value.get("sha256"))
            for value in provenance["generator"].get("artifacts", {}).values()
        )
        and provenance["runtime"].get("status") == "recorded"
        and is_hash(provenance["runtime"].get("sha256"))
        and all(
            record.get("status") == "recorded"
            and isinstance(record.get("provenance"), dict)
            and is_hash(record["provenance"].get("source_sha256"))
            and is_hash(record["provenance"].get("prepared_sha256"))
            and len(record.get("input_files", [])) == 1
            and all(
                input_file.get("status") == "recorded" and is_hash(input_file.get("sha256"))
                for input_file in record["input_files"]
            )
            for record in provenance["profile_datasets"].values()
        )
    )
    smoke_gate = (
        "passed"
        if all(result.get("validation_status") == "passed" for result in run_results)
        and stability["status"] == "passed"
        and campaign.get("status") == "passed"
        and provenance_ok
        else "blocked"
    )
    summary = {
        "schema_version": 1,
        "generated_at": datetime.now(timezone.utc).isoformat(),
        "campaign": "owl2bench-konclude-unified-real-smoke",
        "variant": provenance["engine"].get("variant", "konclude-unified"),
        "historical_stardog_reproduction": False,
        "official_oracle_claim": False,
        "expected_profile_case_counts": EXPECTED_PROFILE_COUNTS,
        "smoke_gate": smoke_gate,
        "correctness_gate": "blocked",
        "performance_claims_allowed": False,
        "runs": run_results,
        "stability": stability,
        "correctness": {
            "completed_without_oracle": completed_without_oracle,
            "oracle_unavailable": oracle_unavailable,
            "gate": "blocked",
        },
        "provenance": provenance,
        "campaign_report": campaign,
        "campaign": campaign,
        "validation_errors": validation_errors,
        "runner_return_codes": dict(zip(run_ids, run_return_codes)),
    }
    write_json(smoke_dir / "summary.json", summary)
    write_smoke_report(smoke_dir / "report.md", summary)
    print(f"[owl2bench-real-smoke] summary={smoke_dir / 'summary.json'}")
    print(f"[owl2bench-real-smoke] report={smoke_dir / 'report.md'}")
    print(f"[owl2bench-real-smoke] smoke_gate={smoke_gate}")
    return 0 if smoke_gate == "passed" else 1


if __name__ == "__main__":
    raise SystemExit(main())
