#!/usr/bin/env python3
"""ELK 0.6.0 finite RDF export boundary for the OWL2Bench EL lane.

ELK's official CLI artifact classifies EL ontologies and can write taxonomy
output.  That taxonomy is not a query-complete RDF graph for OWL2Bench.  This
adapter therefore accepts a separately pinned finite RDF exporter and otherwise
fails closed with ``not_available``.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import subprocess
import sys
import zipfile
from pathlib import Path
from typing import Any


PROFILE = "EL"
ENGINE = "ELK"
ENGINE_VERSION = "0.6.0"
NOT_AVAILABLE = 125
UNSUPPORTED = 123


class BoundaryError(Exception):
    def __init__(self, status: str, reason: str, exit_code: int = 1) -> None:
        super().__init__(reason)
        self.status = status
        self.reason = reason
        self.exit_code = exit_code


def digest(path: Path) -> str:
    hasher = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            hasher.update(chunk)
    return "sha256:" + hasher.hexdigest()


def digest_value(value: str | None) -> str:
    return str(value or "").strip().removeprefix("sha256:").lower()


def require_digest_match(path: Path, expected: str | None, label: str) -> str:
    actual = digest(path)
    if expected and digest_value(actual) != digest_value(expected):
        raise BoundaryError(
            "failed",
            f"{label} sha256 mismatch: expected {expected}, actual {actual}",
        )
    return actual


def require_file(value: str | None, label: str, *, executable: bool = False) -> Path:
    if not value or not value.strip():
        raise BoundaryError("not_available", f"{label} path is missing", NOT_AVAILABLE)
    path = Path(value).expanduser().resolve()
    if not path.is_file() or not os.access(path, os.R_OK):
        raise BoundaryError(
            "not_available", f"{label} is missing or unreadable: {path}", NOT_AVAILABLE
        )
    if executable and not os.access(path, os.X_OK):
        raise BoundaryError(
            "not_available", f"{label} is not executable: {path}", NOT_AVAILABLE
        )
    return path


def load_properties(path: Path | None) -> dict[str, str]:
    if path is None:
        return {}
    values: dict[str, str] = {}
    try:
        lines = path.read_text(encoding="utf-8").splitlines()
    except (OSError, UnicodeDecodeError) as error:
        raise BoundaryError("failed", f"ELK boundary config is unreadable: {error}") from error
    for line in lines:
        stripped = line.strip()
        if not stripped or stripped.startswith(("#", "!")):
            continue
        if "=" not in stripped:
            raise BoundaryError("failed", f"ELK boundary config line is malformed: {line}")
        key, value = stripped.split("=", 1)
        values[key.strip()] = value.strip()
    return values


def setting(values: dict[str, str], key: str, env_name: str, default: str = "") -> str:
    return values.get(key) or os.environ.get(env_name, default)


def artifact_version(path: Path, artifact_id: str) -> str:
    try:
        with zipfile.ZipFile(path) as archive:
            properties_name = f"META-INF/maven/io.github.liveontologies/{artifact_id}/pom.properties"
            try:
                text = archive.read(properties_name).decode("utf-8", errors="replace")
                for line in text.splitlines():
                    if line.startswith("version="):
                        return line.split("=", 1)[1].strip()
            except KeyError:
                pass
            try:
                manifest = archive.read("META-INF/MANIFEST.MF").decode("utf-8", errors="replace")
                for line in manifest.splitlines():
                    if line.startswith("Implementation-Version:"):
                        return line.split(":", 1)[1].strip()
            except KeyError:
                pass
    except (OSError, zipfile.BadZipFile) as error:
        raise BoundaryError("failed", f"{artifact_id} is not a readable jar: {error}") from error
    raise BoundaryError(
        "not_available",
        f"{artifact_id} exact version metadata is missing",
        NOT_AVAILABLE,
    )


def optional_file(value: str, label: str) -> Path | None:
    if not value:
        return None
    return require_file(value, label)


def load_boundary(config_path: Path | None) -> dict[str, Any]:
    values = load_properties(config_path)
    configured_version = setting(values, "elk.version", "ELK_EL_VERSION", ENGINE_VERSION)
    if configured_version != ENGINE_VERSION:
        raise BoundaryError(
            "failed",
            f"ELK version pin must be {ENGINE_VERSION}, got {configured_version}",
        )

    owlapi_path = setting(values, "elk.owlapi_jar", "ELK_EL_OWLAPI_JAR", "")
    cli_path = setting(values, "elk.cli_jar", "ELK_EL_CLI_JAR", "")

    owlapi_jar = require_file(owlapi_path, "ELK OWLAPI jar")
    cli_jar = optional_file(cli_path, "ELK CLI jar")
    owlapi_sha = require_digest_match(
        owlapi_jar, setting(values, "elk.owlapi_sha256", "ELK_EL_OWLAPI_SHA256"), "ELK OWLAPI jar"
    )
    owlapi_version = artifact_version(owlapi_jar, "elk-owlapi")
    if owlapi_version != ENGINE_VERSION:
        raise BoundaryError(
            "failed", f"ELK OWLAPI jar version must be {ENGINE_VERSION}, got {owlapi_version}"
        )
    cli_record = None
    if cli_jar is not None:
        cli_sha = require_digest_match(
            cli_jar, setting(values, "elk.cli_sha256", "ELK_EL_CLI_SHA256"), "ELK CLI jar"
        )
        cli_version = artifact_version(cli_jar, "elk-cli")
        if cli_version != ENGINE_VERSION:
            raise BoundaryError(
                "failed", f"ELK CLI jar version must be {ENGINE_VERSION}, got {cli_version}"
            )
        cli_record = {
            "path": str(cli_jar),
            "sha256": cli_sha,
            "version": cli_version,
            "capability": "classification_taxonomy_only",
        }

    exporter_path = setting(values, "exporter.path", "ELK_EL_FINITE_EXPORTER_BIN")
    exporter_boundary = setting(values, "exporter.boundary", "ELK_EL_EXPORTER_BOUNDARY")
    exporter = None
    exporter_sha = ""
    if exporter_path:
        exporter = require_file(exporter_path, "ELK finite RDF exporter", executable=True)
        exporter_sha = require_digest_match(
            exporter,
            setting(values, "exporter.sha256", "ELK_EL_FINITE_EXPORTER_SHA256"),
            "ELK finite RDF exporter",
        )
    config_sha = digest(config_path) if config_path is not None else None

    return {
        "schema_version": 1,
        "adapter": "elk-el-export-boundary",
        "profile": PROFILE,
        "engine": {
            "name": ENGINE,
            "version": ENGINE_VERSION,
            "revision": setting(values, "elk.revision", "ELK_EL_REVISION", "official-0.6.0"),
            "owlapi_jar": {"path": str(owlapi_jar), "sha256": owlapi_sha, "version": owlapi_version},
            "cli_jar": cli_record,
        },
        "configuration": (
            {"path": str(config_path), "sha256": config_sha}
            if config_path is not None
            else {"status": "environment_or_default_artifact_discovery"}
        ),
        "exporter": (
            {
                "path": str(exporter),
                "sha256": exporter_sha,
                "version": setting(values, "exporter.version", "ELK_EL_FINITE_EXPORTER_VERSION"),
                "revision": setting(values, "exporter.revision", "ELK_EL_FINITE_EXPORTER_REVISION"),
                "boundary": exporter_boundary,
            }
            if exporter is not None
            else None
        ),
        "resource_limits": {
            "timeout_seconds": int(setting(values, "resource.timeout_seconds", "ELK_EL_TIMEOUT_SECONDS", "600")),
            "memory": setting(values, "resource.memory", "ELK_EL_MEMORY", "24GiB"),
            "threads": int(setting(values, "resource.threads", "ELK_EL_THREADS", "1")),
        },
        "answer_key_policy": "not_permitted",
    }


def require_complete_exporter(boundary: dict[str, Any]) -> Path:
    exporter = boundary.get("exporter")
    if not isinstance(exporter, dict):
        raise BoundaryError(
            "not_available",
            "ELK 0.6.0 classification/taxonomy output is not a complete OWL2Bench query graph; no finite RDF exporter is pinned",
            NOT_AVAILABLE,
        )
    if exporter.get("boundary") != "finite_query_complete_rdf_graph":
        raise BoundaryError(
            "not_available",
            "ELK exporter boundary must be finite_query_complete_rdf_graph",
            NOT_AVAILABLE,
        )
    for field in ("version", "revision"):
        if not isinstance(exporter.get(field), str) or not exporter[field].strip():
            raise BoundaryError(
                "not_available", f"ELK finite RDF exporter {field} pin is missing", NOT_AVAILABLE
            )
    return Path(str(exporter["path"]))


def write_provenance(boundary: dict[str, Any], status: str, reason: str = "") -> None:
    record = dict(boundary)
    record["status"] = status
    if reason:
        record["reason"] = reason
    output = os.environ.get("ELK_EL_PROVENANCE_OUT", "")
    if output:
        Path(output).expanduser().resolve().write_text(
            json.dumps(record, ensure_ascii=False, indent=2, sort_keys=True) + "\n",
            encoding="utf-8",
        )
    print(json.dumps(record, ensure_ascii=False, sort_keys=True), file=sys.stderr)


def run_exporter(boundary: dict[str, Any], exporter: Path, source: Path) -> int:
    env = dict(os.environ)
    env.update(
        {
            "ELK_EL_PROFILE": PROFILE,
            "ELK_EL_ENGINE_VERSION": ENGINE_VERSION,
            "ELK_EL_OWLAPI_JAR": boundary["engine"]["owlapi_jar"]["path"],
            "ELK_EL_OWLAPI_SHA256": boundary["engine"]["owlapi_jar"]["sha256"],
        }
    )
    cli = boundary["engine"].get("cli_jar")
    if isinstance(cli, dict):
        env["ELK_EL_CLI_JAR"] = cli["path"]
        env["ELK_EL_CLI_SHA256"] = cli["sha256"]
    timeout_seconds = int(boundary["resource_limits"]["timeout_seconds"])
    try:
        result = subprocess.run(
            [str(exporter), "export", str(source)],
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            timeout=timeout_seconds,
            check=False,
            env=env,
        )
    except subprocess.TimeoutExpired:
        write_provenance(boundary, "timeout", "ELK finite RDF exporter exceeded timeout")
        return 124
    if result.stderr:
        sys.stderr.buffer.write(result.stderr)
    if result.returncode == 0:
        sys.stdout.buffer.write(result.stdout)
        write_provenance(boundary, "complete")
    elif result.returncode == NOT_AVAILABLE:
        write_provenance(boundary, "not_available", "ELK finite RDF exporter declared runtime unavailable")
    elif result.returncode == UNSUPPORTED:
        write_provenance(boundary, "unsupported", "ELK finite RDF exporter declared capability unsupported")
    else:
        write_provenance(boundary, "failed", f"ELK finite RDF exporter failed with status {result.returncode}")
    return result.returncode


def command_version(boundary: dict[str, Any]) -> int:
    print(json.dumps(boundary, ensure_ascii=False, sort_keys=True))
    return 0


def command_export(boundary: dict[str, Any], source_value: str) -> int:
    source = require_file(source_value, "source graph")
    boundary["source_graph"] = {"path": str(source), "sha256": digest(source)}
    exporter = require_complete_exporter(boundary)
    return run_exporter(boundary, exporter, source)


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("command", choices=("version", "export"))
    parser.add_argument("source", nargs="?")
    parser.add_argument(
        "--config",
        type=Path,
        default=(
            Path(os.environ["ELK_EL_EXPORT_BOUNDARY_CONFIG"]).expanduser().resolve()
            if os.environ.get("ELK_EL_EXPORT_BOUNDARY_CONFIG")
            else None
        ),
        help="Pinned ELK/export boundary properties file",
    )
    args = parser.parse_args(argv)
    try:
        config = args.config
        if config is None and os.environ.get("ELK_EL_CONFIG"):
            config = Path(os.environ["ELK_EL_CONFIG"]).expanduser().resolve()
        if config is not None:
            config = require_file(str(config), "ELK boundary config")
        boundary = load_boundary(config)
        try:
            if args.command == "version":
                return command_version(boundary)
            if args.source is None:
                parser.error("export requires <source-nquads>")
            return command_export(boundary, args.source)
        except BoundaryError as error:
            write_provenance(boundary, error.status, error.reason)
            return error.exit_code
    except BoundaryError as error:
        boundary: dict[str, Any] = {
            "schema_version": 1,
            "adapter": "elk-el-export-boundary",
            "profile": PROFILE,
            "engine": {"name": ENGINE, "version": ENGINE_VERSION},
            "status": error.status,
            "reason": error.reason,
            "answer_key_policy": "not_permitted",
        }
        print(json.dumps(boundary, ensure_ascii=False, sort_keys=True), file=sys.stderr)
        return error.exit_code


if __name__ == "__main__":
    raise SystemExit(main())
