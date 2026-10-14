#!/usr/bin/env python3
"""Build a GraphDB RL profile identity manifest for profile_materialization.py."""

from __future__ import annotations

import argparse
import csv
import hashlib
import json
import sys
from pathlib import Path
from typing import Any

from resource_variant import resource_variant
from runtime_license import license_identity

SCRIPT_DIR = Path(__file__).resolve().parent
RUNTIME_PIN = SCRIPT_DIR / "graphdb_rl_runtime_pin.json"
REPOSITORY_CONFIG = SCRIPT_DIR / "graphdb_rl_repository_config.ttl"
EXPORTER = SCRIPT_DIR / "graphdb_rl_materializer.sh"
CAPABILITIES = SCRIPT_DIR / "graphdb-rl-capabilities.json"
GRAPHDB_IMAGE_DIGEST = "sha256:d688879c4e5751a0c323f996d86bf1b2189191290711b1936e211d4510b53485"
GRAPHDB_IMAGE_REF = "ontotext/graphdb@sha256:d688879c4e5751a0c323f996d86bf1b2189191290711b1936e211d4510b53485"


class IdentityError(ValueError):
    pass


def digest(path: Path) -> str:
    hasher = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            hasher.update(chunk)
    return "sha256:" + hasher.hexdigest()


def load_json(path: Path) -> dict[str, Any]:
    try:
        value = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, UnicodeDecodeError, json.JSONDecodeError) as error:
        raise IdentityError(f"JSON input is malformed: {path}: {error}") from error
    if not isinstance(value, dict):
        raise IdentityError(f"JSON root must be an object: {path}")
    return value


def require_file(path: Path, label: str) -> Path:
    resolved = path.expanduser().resolve()
    if not resolved.is_file():
        raise IdentityError(f"{label} is missing: {resolved}")
    return resolved


def require_dir(path: Path, label: str) -> Path:
    resolved = path.expanduser().resolve()
    if not resolved.is_dir():
        raise IdentityError(f"{label} is missing: {resolved}")
    return resolved


def query_records_from_directory(query_dir: Path) -> list[dict[str, str]]:
    queries = sorted(
        path for path in query_dir.iterdir()
        if path.is_file() and path.suffix.lower() in {".rq", ".sparql", ".spql"}
    )
    if len(queries) != 18:
        raise IdentityError(f"RL query workload must contain exactly 18 query files, found {len(queries)}")
    return [{"path": str(path), "sha256": digest(path)} for path in queries]


def query_records_from_manifest(query_manifest: Path) -> list[dict[str, str]]:
    manifest = require_file(query_manifest, "RL query manifest")
    records: list[dict[str, str]] = []
    with manifest.open("r", encoding="utf-8", newline="") as handle:
        reader = csv.DictReader(handle, delimiter="\t")
        for line_number, row in enumerate(reader, start=2):
            profiles = {
                part.strip().upper()
                for part in str(row.get("profile", "")).split(",")
                if part.strip()
            }
            if "RL" not in profiles:
                continue
            query_path = require_file(Path(str(row.get("staged_query", ""))), f"RL query row {line_number}")
            records.append(
                {
                    "id": str(row.get("query_id", "")),
                    "path": str(query_path),
                    "sha256": digest(query_path),
                    "source_query": str(row.get("source_query", "")),
                    "source_corpus": str(row.get("source_corpus", "")),
                    "source_revision": str(row.get("source_revision", "")),
                    "manifest_row": str(line_number),
                }
            )
    if len(records) != 18:
        raise IdentityError(f"RL query manifest must contain exactly 18 RL rows, found {len(records)}")
    return records


def query_records(query_dir: Path | None, query_manifest: Path | None) -> list[dict[str, str]]:
    if query_manifest is not None:
        return query_records_from_manifest(query_manifest)
    if query_dir is not None:
        return query_records_from_directory(query_dir)
    raise IdentityError("an RL query manifest or query directory is required")


def copy_section(source: dict[str, Any], name: str) -> dict[str, Any]:
    value = source.get(name)
    if not isinstance(value, dict):
        raise IdentityError(f"profile identity is missing object section: {name}")
    return dict(value)


def normalize_generator(frozen: dict[str, Any]) -> dict[str, Any]:
    source = copy_section(frozen, "generator")
    dataset = frozen.get("dataset") if isinstance(frozen.get("dataset"), dict) else {}
    path = source.get("artifact_path") or source.get("path")
    digest_value = source.get("artifact_digest") or source.get("sha256")
    if not isinstance(path, str) or not path:
        raise IdentityError("frozen generator artifact path is missing")
    generator_path = require_file(Path(path), "OWL2Bench generator")
    actual = digest(generator_path)
    if not isinstance(digest_value, str) or digest_value != actual:
        raise IdentityError("frozen generator artifact hash does not match its path")
    seed = source.get("seed", dataset.get("seed", 1))
    if isinstance(seed, bool) or not isinstance(seed, int) or seed < 0:
        raise IdentityError("frozen generator seed is invalid")
    return {
        "id": str(source.get("id") or "OWL2Bench-generator"),
        "revision": str(source.get("revision") or frozen.get("source_revision") or "unknown"),
        "seed": seed,
        "artifact_path": str(generator_path),
        "artifact_digest": actual,
        "configuration_id": str(source.get("configuration_id") or "OWL2Bench-generator-config"),
        "configuration_sha256": str(source.get("configuration_sha256") or ""),
    }


def normalize_generator_configuration(frozen: dict[str, Any]) -> dict[str, Any]:
    source = copy_section(frozen, "generator_configuration")
    path = require_file(Path(str(source.get("path", ""))), "OWL2Bench generator configuration")
    actual = digest(path)
    declared = source.get("sha256")
    if declared != actual:
        raise IdentityError("frozen generator configuration hash does not match its path")
    return {
        "id": str(source.get("id") or "OWL2Bench-generator-config"),
        "path": str(path),
        "revision": str(source.get("revision") or frozen.get("source_revision") or "unknown"),
        "sha256": actual,
    }


def build_identity(args: argparse.Namespace) -> dict[str, Any]:
    profile_identity_path = require_file(args.profile_identity, "frozen RL profile identity")
    frozen = load_json(profile_identity_path)
    if frozen.get("profile") != "RL" or frozen.get("status") != "complete":
        raise IdentityError("frozen profile identity must be a complete RL record")

    conversion_artifact = copy_section(frozen, "conversion_artifact")
    source = require_file(args.source or Path(conversion_artifact["path"]), "RL source N-Quads")
    if source != Path(str(conversion_artifact["path"])).expanduser().resolve():
        raise IdentityError("RL source path does not match the frozen conversion artifact")
    if digest(source) != conversion_artifact.get("sha256"):
        raise IdentityError("RL source hash does not match the frozen conversion artifact")
    query_dir = require_dir(args.query_dir, "RL query workload directory") if args.query_dir else None
    query_manifest = require_file(args.query_manifest, "RL query manifest") if args.query_manifest else None
    profile_context = copy_section(frozen, "profile_context")
    context_dir = require_dir(Path(str(profile_context.get("directory", ""))), "RL profile context")
    if context_dir.name != "RL":
        raise IdentityError("profile context directory must be named RL")

    source_hash = digest(source)
    dataset = copy_section(frozen, "dataset")
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
        if not isinstance(dataset.get(field), str) or not dataset[field].strip():
            raise IdentityError(f"frozen RL dataset.{field} is missing")
    if dataset.get("profile") != "RL":
        raise IdentityError("frozen RL dataset profile does not match RL")
    if Path(dataset["artifact_path"]).expanduser().resolve() != source:
        raise IdentityError("RL dataset artifact path does not match the frozen conversion artifact")
    if dataset.get("sha256") != source_hash:
        raise IdentityError("RL dataset hash does not match the frozen conversion artifact")

    config_hash = digest(REPOSITORY_CONFIG)
    runtime_pin_hash = digest(RUNTIME_PIN)
    exporter_hash = digest(EXPORTER)
    capabilities_hash = digest(CAPABILITIES)

    identity = {
        "schema_version": 1,
        "profile": "RL",
        "component": {
            "name": "RL-ruleset-engine",
            "implementation": "GraphDB",
            "version": "11.5.0",
            "revision": GRAPHDB_IMAGE_REF,
            "artifact_digest": runtime_pin_hash,
            "path": str(RUNTIME_PIN),
            "container_image": {
                "reference": GRAPHDB_IMAGE_REF,
                "tag": "ontotext/graphdb:11.5.0",
                "digest": GRAPHDB_IMAGE_DIGEST,
                "platform": "linux/amd64",
            },
        },
        "exporter": {
            "name": "graphdb-rl-exporter",
            "version": "1",
            "revision": "graphdb-rl-export-contract/1",
            "artifact_digest": exporter_hash,
            "path": str(EXPORTER),
            "command": "export <source.nq>",
        },
        "query_backend": {
            "name": "graphdb-rdf4j-read-only-sparql-boundary",
            "version": "11.5.0",
            "revision": GRAPHDB_IMAGE_REF,
            "artifact_digest": exporter_hash,
            "path": str(EXPORTER),
            "read_only": True,
        },
        "configuration": {
            "id": "graphdb-rl-repository-config:owl2bench-graphdb-rl:owl2-rl",
            "path": str(REPOSITORY_CONFIG),
            "sha256": config_hash,
            "repository_id": "owl2bench-graphdb-rl",
            "ruleset": "owl2-rl",
        },
        "ruleset": {
            "id": "graphdb-11.5.0-built-in-owl2-rl",
            "version": "11.5.0",
            "revision": GRAPHDB_IMAGE_REF,
            "artifact_digest": GRAPHDB_IMAGE_DIGEST,
            "conformance_boundary": "GraphDB 11.5 built-in owl2-rl ruleset; forward-chaining total materialization at load time; GraphDB output is differential execution evidence only",
        },
        "derivation": {
            "version": "owl2bench-graphdb-rl-materialization/1",
            "revision": "graphdb-rl-export-boundary/1",
        },
        "runtime": {
            "id": f"docker:{GRAPHDB_IMAGE_REF}:linux/amd64",
            "dependencies": [
                {
                    "name": "ontotext/graphdb",
                    "version": "11.5.0",
                    "digest": GRAPHDB_IMAGE_DIGEST,
                    "platform": "linux/amd64",
                },
                {
                    "name": "graphdb-rl-runtime-pin",
                    "version": "1",
                    "digest": runtime_pin_hash,
                    "path": str(RUNTIME_PIN),
                },
            ],
        },
        "resource_limits": {
            "timeout_seconds": args.timeout_seconds,
            "memory": args.memory,
            "threads": args.threads,
            "cpu": args.cpu,
            "disk": args.disk,
            "network": "local-only",
        },
        "profile_context": profile_context,
        "source_document": {
            "id": f"frozen-rl-source-nquads:{source_hash}",
            "path": str(source),
            "sha256": source_hash,
            "source": "dataset freezer output from commit 721cb0c",
        },
        "query_workload": {
            "id": "owl2bench-rl-query-workload:18",
            "query_count": 18,
            "queries": query_records(query_dir, query_manifest),
            "source_manifest": {
                "path": str(query_manifest),
                "sha256": digest(query_manifest),
            } if query_manifest else None,
        },
        "dataset": dataset,
        "generator": normalize_generator(frozen),
        "generator_configuration": normalize_generator_configuration(frozen),
        "profile_tbox": copy_section(frozen, "profile_tbox"),
        "profile_abox": copy_section(frozen, "profile_abox"),
        "dataset_artifact": {
            **copy_section(frozen, "dataset_artifact"),
        },
        "implementation": {
            "name": "graphdb-rl-materialization-boundary",
            "path": str(EXPORTER),
            "sha256": exporter_hash,
            "version": "1",
        },
        "capabilities": {
            "path": str(CAPABILITIES),
            "sha256": capabilities_hash,
        },
        "original_source_ontology": copy_section(frozen, "source_ontology"),
        "oracle_policy": "oracle_unavailable; GraphDB output and engine agreement are differential evidence only",
        "license": {
            **license_identity(),
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
    return identity


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("build", choices=("build",))
    parser.add_argument("--profile-identity", required=True, type=Path)
    parser.add_argument("--source", type=Path)
    parser.add_argument("--query-dir", type=Path)
    parser.add_argument("--query-manifest", type=Path)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--timeout-seconds", type=int, default=600)
    parser.add_argument("--memory", default="24GiB")
    parser.add_argument("--threads", type=int, default=1)
    parser.add_argument("--cpu", default="1")
    parser.add_argument("--disk", default="10GiB")
    args = parser.parse_args(argv)
    try:
        if args.timeout_seconds <= 0 or args.threads <= 0:
            raise IdentityError("timeout and threads must be positive")
        if args.query_dir is None and args.query_manifest is None:
            raise IdentityError("--query-dir or --query-manifest is required")
        identity = build_identity(args)
        args.output.parent.mkdir(parents=True, exist_ok=True)
        args.output.write_text(json.dumps(identity, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    except IdentityError as error:
        print(f"[graphdb-rl][failed] {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
