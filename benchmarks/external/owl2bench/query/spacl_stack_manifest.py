#!/usr/bin/env python3
"""Build the SPACL split-stack manifests from pinned provenance.

The SPACL stack verdict must declare the split stack the campaign's
owl2bench-query contract validates (ADR 0027).  This builder never invents a
pin: every identity it emits is read from provenance that already exists —
the SPACL adapter state under ``<data-dir>/<profile>/.spacl-<profile>/``
(``provenance.json`` pins the adapter, the reasoner binary digest, the source
ontology, and the Oxigraph image digest; ``handoff.json`` pins the
materialized graph), the dataset anchor metadata, and the pinned capabilities
file.  Any drift between those artifacts and the files on disk fails the
build instead of being silently absorbed.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import re
import sys
from pathlib import Path
from typing import Any


PROFILES = ("DL", "EL", "QL", "RL")
EXPECTED_COUNTS = {"DL": 22, "EL": 10, "QL": 10, "RL": 18}
EXPECTED_QUERY_COUNT = sum(EXPECTED_COUNTS.values())
ROOT_DIR = Path(__file__).resolve().parents[3]


class BuildError(ValueError):
    """The SPACL split-stack manifests cannot be built without new pins."""


def digest(path: Path) -> str:
    hasher = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            hasher.update(chunk)
    return "sha256:" + hasher.hexdigest()


def digest_value(value: Any) -> str:
    return str(value or "").strip().removeprefix("sha256:").lower()


def valid_digest(value: Any) -> bool:
    normalized = digest_value(value)
    return len(normalized) == 64 and all(
        character in "0123456789abcdefABCDEF" for character in normalized
    )


def load_json(path: Path, label: str) -> dict[str, Any]:
    if not path.is_file():
        raise BuildError(f"{label} is missing: {path}")
    try:
        value = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, UnicodeDecodeError, json.JSONDecodeError) as error:
        raise BuildError(f"{label} is malformed: {path}: {error}") from error
    if not isinstance(value, dict):
        raise BuildError(f"{label} root must be an object: {path}")
    return value


def nonempty(value: Any) -> bool:
    return isinstance(value, str) and bool(value.strip())


def required_str(document: dict[str, Any], field: str, label: str) -> str:
    value = document.get(field)
    if not nonempty(value):
        raise BuildError(f"{label}.{field} is missing")
    return str(value)


def write_json(path: Path, value: dict[str, Any]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(value, ensure_ascii=False, indent=2, sort_keys=True) + "\n", encoding="utf-8")


def default_reasoner_version() -> str:
    cargo = ROOT_DIR / "Cargo.toml"
    if not cargo.is_file():
        return "unknown"
    match = re.search(r'^version\s*=\s*"([^"]+)"', cargo.read_text(encoding="utf-8"), re.MULTILINE)
    return match.group(1) if match else "unknown"


def generator_seed(dataset_id: str) -> int:
    match = re.search(r"seed(\d+)", dataset_id)
    if not match:
        raise BuildError(f"dataset id does not carry a generator seed: {dataset_id}")
    return int(match.group(1))


def profile_state(
    data_dir: Path,
    profile: str,
    capabilities: dict[str, Any],
) -> tuple[Path, dict[str, Any], dict[str, Any]]:
    """Load and pin-check one profile's adapter state."""

    relative_paths = capabilities.get("provenance_relative_paths")
    adapters = capabilities.get("provenance_adapters")
    if not isinstance(relative_paths, dict) or not isinstance(adapters, dict):
        raise BuildError("capabilities file must pin provenance_relative_paths and provenance_adapters")
    relative_path = relative_paths.get(profile)
    adapter = adapters.get(profile)
    if not isinstance(relative_path, str) or not relative_path \
            or relative_path.startswith("/") or ".." in relative_path:
        raise BuildError(f"capabilities provenance path for {profile} is invalid: {relative_path!r}")
    if not nonempty(adapter):
        raise BuildError(f"capabilities provenance adapter for {profile} is missing")

    context_dir = data_dir / profile
    if not context_dir.is_dir():
        raise BuildError(f"profile context directory is missing: {context_dir}")
    context_dir = context_dir.resolve()
    provenance_path = (context_dir / relative_path).resolve()
    state_dir = provenance_path.parent
    provenance = load_json(provenance_path, f"{profile} adapter provenance")

    if required_str(provenance, "adapter", f"{profile} provenance") != adapter:
        raise BuildError(f"{profile} provenance adapter does not match the pinned capability")
    if provenance.get("profile") != profile:
        raise BuildError(f"{profile} provenance profile identity mismatch")
    context_recorded = str(provenance.get("context_dir") or "")
    if not nonempty(context_recorded) or Path(context_recorded).resolve() != context_dir:
        raise BuildError(f"{profile} provenance context_dir does not match {context_dir}")

    oxigraph = provenance.get("oxigraph")
    if not isinstance(oxigraph, dict):
        raise BuildError(f"{profile} provenance oxigraph pin is missing")
    image = required_str(capabilities, "engine_image", "capabilities")
    image_digest = required_str(capabilities, "engine_image_digest", "capabilities")
    if not valid_digest(image_digest):
        raise BuildError(f"capabilities engine_image_digest is invalid: {image_digest}")
    if oxigraph.get("image") != image or digest_value(oxigraph.get("image_digest")) != digest_value(image_digest):
        raise BuildError(f"{profile} provenance oxigraph pin does not match the pinned capability")

    reasoner_bin = str(provenance.get("reasoner_bin") or "")
    reasoner_sha = provenance.get("reasoner_bin_sha256")
    if not nonempty(reasoner_bin) or not Path(reasoner_bin).is_file():
        raise BuildError(f"{profile} provenance reasoner binary is missing: {reasoner_bin}")
    if not valid_digest(reasoner_sha):
        raise BuildError(f"{profile} provenance reasoner digest is invalid")
    if digest_value(digest(Path(reasoner_bin))) != digest_value(reasoner_sha):
        raise BuildError(
            f"{profile} reasoner binary drifted from the pinned load digest: {reasoner_bin}"
        )

    ontology = provenance.get("ontology")
    if not isinstance(ontology, dict):
        raise BuildError(f"{profile} provenance ontology pin is missing")
    ontology_path = str(ontology.get("path") or "")
    if not nonempty(ontology_path) or not Path(ontology_path).is_file():
        raise BuildError(f"{profile} source ontology is missing: {ontology_path}")
    if not valid_digest(ontology.get("sha256")):
        raise BuildError(f"{profile} source ontology digest is invalid")
    if digest_value(digest(Path(ontology_path))) != digest_value(ontology.get("sha256")):
        raise BuildError(f"{profile} source ontology drifted from the pinned digest: {ontology_path}")

    handoff = load_json(state_dir / "handoff.json", f"{profile} materialization handoff")
    if handoff.get("status") != "ready":
        raise BuildError(f"{profile} materialization handoff is not ready")
    graph_path = str(handoff.get("graph_path") or "")
    if not nonempty(graph_path) or not Path(graph_path).is_file():
        raise BuildError(f"{profile} materialized graph is missing: {graph_path}")
    if not valid_digest(handoff.get("graph_sha256")):
        raise BuildError(f"{profile} handoff graph digest is invalid")
    if digest_value(digest(Path(graph_path))) != digest_value(handoff.get("graph_sha256")):
        raise BuildError(f"{profile} materialized graph drifted from its handoff: {graph_path}")

    return context_dir, provenance, handoff


def build(args: argparse.Namespace) -> int:
    data_dir = args.data_dir.expanduser().resolve()
    capabilities_path = args.capabilities.expanduser().resolve()
    dataset_metadata_path = args.dataset_metadata.expanduser().resolve()
    capabilities = load_json(capabilities_path, "capabilities file")
    dataset_metadata = load_json(dataset_metadata_path, "dataset anchor metadata")
    if capabilities.get("supports_query") is not True:
        raise BuildError("capabilities file does not support query execution")
    if args.timeout_seconds <= 0 or args.threads <= 0:
        raise BuildError("timeout_seconds and threads must be positive integers")

    image = required_str(capabilities, "engine_image", "capabilities")
    image_digest = required_str(capabilities, "engine_image_digest", "capabilities")
    image_name, _, image_tag = image.rpartition(":")
    capabilities_digest = digest(capabilities_path)

    component_profiles: dict[str, dict[str, Any]] = {}
    materialization_profiles: dict[str, dict[str, Any]] = {}
    stack_inputs: dict[str, dict[str, Any]] = {}

    for profile in PROFILES:
        context_dir, provenance, handoff = profile_state(data_dir, profile, capabilities)
        adapter = str(capabilities["provenance_adapters"][profile])
        anchor = dataset_metadata.get(profile)
        if not isinstance(anchor, dict):
            raise BuildError(f"dataset anchor metadata for {profile} is missing")
        dataset_id = anchor.get("dataset_id")
        dataset_sha = anchor.get("dataset_sha256")
        if not nonempty(dataset_id) or not valid_digest(dataset_sha):
            raise BuildError(f"dataset anchor for {profile} is malformed")
        ontology_path = Path(str(provenance["ontology"]["path"])).resolve()
        if digest_value(digest(ontology_path)) != digest_value(dataset_sha):
            raise BuildError(
                f"{profile} dataset anchor does not match the source ontology on disk: {ontology_path}"
            )

        reasoner_bin = Path(str(provenance["reasoner_bin"])).resolve()
        if args.reasoner_bin is not None \
                and args.reasoner_bin.expanduser().resolve() != reasoner_bin:
            raise BuildError(
                f"{profile} pinned reasoner {reasoner_bin} is not the requested binary {args.reasoner_bin}"
            )
        reasoner_digest = str(provenance["reasoner_bin_sha256"])
        graph_path = Path(str(handoff["graph_path"])).resolve()
        graph_digest = str(handoff["graph_sha256"])
        anchor_digest = str(dataset_sha)

        engine_component = {
            "name": "owl2-reasoner",
            "version": args.reasoner_version,
            "revision": reasoner_digest,
            "artifact_digest": reasoner_digest,
        }
        materializer_component = {
            "name": "owl2-reasoner-materialize",
            "version": args.reasoner_version,
            "revision": reasoner_digest,
            "artifact_digest": reasoner_digest,
        }
        query_component = {
            "name": image_name or image,
            "version": image_tag or image,
            "revision": image_digest,
            "artifact_digest": image_digest,
        }
        resource_limits = {
            "timeout_seconds": args.timeout_seconds,
            "memory": args.memory,
            "threads": args.threads,
            "cpu": args.cpu,
            "disk": args.disk,
            "network": args.network,
            "variant": adapter,
        }
        profile_context = {
            "id": f"profile={profile};directory={context_dir}",
            "directory": str(context_dir),
        }
        dataset_record = {
            "id": dataset_id,
            "sha256": anchor_digest,
            "tbox_id": f"owl2bench-{profile.lower()}-tbox:{digest_value(anchor_digest)}",
            "tbox_sha256": anchor_digest,
            "abox_id": f"owl2bench-{profile.lower()}-abox:{digest_value(anchor_digest)}",
            "abox_sha256": anchor_digest,
        }
        generator = {
            "id": "owl2bench-generator",
            "revision": args.generator_revision,
            "seed": generator_seed(dataset_id),
        }
        ruleset = {
            "id": f"owl2-{profile.lower()}-profile-ruleset",
            "version": args.reasoner_version,
            "revision": reasoner_digest,
            "artifact_digest": reasoner_digest,
            "conformance_boundary": f"OWL 2 {profile} profile materialization (owl2-reasoner --profile {profile})",
        }

        component_profiles[profile] = {
            "components": {
                "engine": engine_component,
                "materializer": materializer_component,
                "query": query_component,
            },
            "configuration": {
                "id": "spacl-stack-capabilities",
                "digest": capabilities_digest,
            },
            "runtime": {
                "id": "spacl-stack-host",
                "dependencies": [
                    {
                        "name": image_name or image,
                        "version": image_tag or image,
                        "digest": image_digest,
                    }
                ],
            },
            "credentials": {"status": "not_required"},
            "license": {"status": "not_required"},
            "capability": {"status": "supported", "supports_query": True},
            "resource_limits": resource_limits,
            "dataset": dataset_record,
            "generator": generator,
            "profile_context": profile_context,
        }
        if profile in {"QL", "RL"}:
            component_profiles[profile]["ruleset"] = ruleset

        materialization_profiles[profile] = {
            "profile": profile,
            "status": "complete",
            "source_ontology": {
                "id": dataset_id,
                "path": str(ontology_path),
                "sha256": anchor_digest,
            },
            "component": dict(materializer_component),
            "configuration": {
                "id": "spacl-stack-capabilities",
                "path": str(capabilities_path),
                "sha256": capabilities_digest,
            },
            "output_graph": {
                "id": f"spacl-{profile.lower()}-materialized:{digest_value(graph_digest)}",
                "path": str(graph_path),
                "sha256": graph_digest,
            },
            "derivation": {
                "version": args.reasoner_version,
                "revision": reasoner_digest,
                "semantics": f"owl2-reasoner materialize --profile {profile}",
            },
            "resource_limits": dict(resource_limits),
            "input_sha256": anchor_digest,
            "output_sha256": graph_digest,
            "profile_context": dict(profile_context),
        }
        if profile in {"QL", "RL"}:
            materialization_profiles[profile]["ruleset"] = dict(ruleset)

        stack_inputs[profile] = {
            "adapter": adapter,
            "context_dir": str(context_dir),
            "source_ontology": str(ontology_path),
            "source_ontology_sha256": anchor_digest,
            "materialized_graph": str(graph_path),
            "materialized_graph_sha256": graph_digest,
            "reasoner_bin": str(reasoner_bin),
            "reasoner_bin_sha256": reasoner_digest,
        }

    output_dir = args.output_dir.expanduser().resolve()
    component_manifest = {
        "schema_version": 1,
        "manifest_id": "spacl-stack-components",
        "matrix": {
            "expansion": "query_profile",
            "expected_query_count": EXPECTED_QUERY_COUNT,
            "expected_profile_case_counts": EXPECTED_COUNTS,
        },
        "profiles": component_profiles,
    }
    materialization_manifest = {
        "schema_version": 1,
        "manifest_id": "spacl-stack-materialization",
        "profiles": materialization_profiles,
    }
    stack_metadata = {
        "schema_version": 1,
        "stack": "spacl-split",
        "data_dir": str(data_dir),
        "capabilities": {
            "path": str(capabilities_path),
            "sha256": capabilities_digest,
            "engine_image": image,
            "engine_image_digest": image_digest,
        },
        "dataset_metadata": {
            "path": str(dataset_metadata_path),
            "sha256": digest(dataset_metadata_path),
        },
        "reasoner_version": args.reasoner_version,
        "generator_revision": args.generator_revision,
        "profiles": stack_inputs,
        "answer_key_policy": "not_permitted",
    }
    write_json(output_dir / "component.json", component_manifest)
    write_json(output_dir / "materialization.json", materialization_manifest)
    write_json(output_dir / "spacl-stack.json", stack_metadata)
    print(f"spacl-stack manifests: {output_dir / 'component.json'}")
    print(f"spacl-stack manifests: {output_dir / 'materialization.json'}")
    return 0


def parse_arguments(argv: list[str] | None = None) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    subparsers = parser.add_subparsers(dest="command", required=True)
    build_parser = subparsers.add_parser("build", help="build the SPACL split-stack manifests")
    build_parser.add_argument("--data-dir", required=True, type=Path)
    build_parser.add_argument("--capabilities", required=True, type=Path)
    build_parser.add_argument("--dataset-metadata", required=True, type=Path)
    build_parser.add_argument("--reasoner-bin", type=Path, default=None)
    build_parser.add_argument("--reasoner-version", default=default_reasoner_version())
    build_parser.add_argument("--generator-revision", required=True)
    build_parser.add_argument("--timeout-seconds", type=int, default=3600)
    build_parser.add_argument("--memory", default="host")
    build_parser.add_argument("--threads", type=int, default=1)
    build_parser.add_argument("--cpu", default="host")
    build_parser.add_argument("--disk", default="host")
    build_parser.add_argument("--network", default="local-only")
    build_parser.add_argument("--output-dir", required=True, type=Path)
    build_parser.set_defaults(handler=build)
    args = parser.parse_args(argv)
    return args


def main(argv: list[str] | None = None) -> int:
    args = parse_arguments(argv)
    try:
        return args.handler(args)
    except BuildError as error:
        print(f"[spacl-stack-manifest][error] {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
