#!/usr/bin/env python3
"""Attest the pinned Openllet QL runtime identity."""

from __future__ import annotations

import argparse
import hashlib
import json
import re
import sys
from pathlib import Path
from typing import Any

from openllet_ql_variant_control import evaluate_variant_control


EXPECTED_BASE_DIGEST = "sha256:e2532c643e3fbc52e339b24b6a47196b46ec8c269e08da3cd49365db41d9d252"
EXPECTED_IMAGE_DIGEST = "sha256:548e5ecefa8b252f8baec95bfb4e9528363257b653dd79a189aa0f351df937bd"
EXPECTED_VARIANT_REVISION = "openllet-ql-runtime-v1"
EXPECTED_OPENLLET_VERSION = "2.6.5"
EXPECTED_OWLAPI_VERSION = "5.5.1"
EXPECTED_JAR_COUNT = 78
SCRIPT_DIR = Path(__file__).resolve().parent
REPO_ROOT = SCRIPT_DIR.parents[3]
SHA256_PATTERN = re.compile(r"^sha256:[0-9a-f]{64}$")


class AttestationError(ValueError):
    def __init__(self, status: str, reasons: list[str]) -> None:
        super().__init__(", ".join(reasons))
        self.status = status
        self.reasons = reasons


def digest(path: Path) -> str:
    hasher = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            hasher.update(chunk)
    return "sha256:" + hasher.hexdigest()


def load_json(path: Path) -> Any:
    return json.loads(path.read_text(encoding="utf-8"))


def write_json(path: Path | None, document: dict[str, Any]) -> None:
    payload = json.dumps(document, indent=2, sort_keys=True) + "\n"
    if path is None:
        sys.stdout.write(payload)
        return
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(payload, encoding="utf-8")


def parse_classpath(path: Path) -> list[dict[str, str]]:
    jars: list[dict[str, str]] = []
    for line_number, line in enumerate(path.read_text(encoding="utf-8").splitlines(), start=1):
        if not line.strip():
            continue
        parts = line.split()
        if len(parts) != 2:
            raise AttestationError("failed", [f"classpath.line_{line_number}_invalid"])
        sha, name = parts
        if not re.fullmatch(r"[0-9a-f]{64}", sha):
            raise AttestationError("failed", [f"classpath.line_{line_number}_digest_invalid"])
        jars.append({"name": name, "sha256": "sha256:" + sha})
    return jars


def require_object(value: Any, reason: str) -> dict[str, Any]:
    if not isinstance(value, dict):
        raise AttestationError("not_available", [reason])
    return value


def require_digest(value: Any, reason: str) -> str:
    if not isinstance(value, str) or SHA256_PATTERN.fullmatch(value) is None:
        raise AttestationError("not_available", [reason])
    return value


def reject_mutable_image_ref(reference: str) -> None:
    if "@sha256:" not in reference:
        raise AttestationError("failed", ["image.reference_mutable_tag"])


def build_input_summary(
    dockerfile: Path,
    pom: Path,
    classpath_lock: Path,
    configuration: Path,
    exporter: Path,
) -> dict[str, Any]:
    for path, label in (
        (dockerfile, "dockerfile"),
        (pom, "pom"),
        (classpath_lock, "classpath"),
        (configuration, "configuration"),
        (exporter, "exporter"),
    ):
        if not path.is_file():
            raise AttestationError("not_available", [f"{label}.missing"])

    dockerfile_text = dockerfile.read_text(encoding="utf-8")
    pom_text = pom.read_text(encoding="utf-8")
    jars = parse_classpath(classpath_lock)
    jar_by_name = {jar["name"]: jar["sha256"] for jar in jars}
    return {
        "schema_version": 1,
        "base_image": {
            "reference": "maven:3.9-eclipse-temurin-17",
            "digest": EXPECTED_BASE_DIGEST,
        },
        "openllet_version": EXPECTED_OPENLLET_VERSION,
        "owlapi_version": EXPECTED_OWLAPI_VERSION,
        "dockerfile": {
            "path": str(dockerfile),
            "sha256": digest(dockerfile),
            "base_digest_pinned": EXPECTED_BASE_DIGEST in dockerfile_text,
            "classpath_drift_check": "diff -u /opt/reasoner/expected-classpath.sha256 /opt/reasoner/classpath.sha256" in dockerfile_text,
            "build_network_contract": "host-maven-resolution-only",
            "exposes_service": bool(re.search(r"(?m)^\\s*EXPOSE\\b", dockerfile_text)),
        },
        "pom": {
            "path": str(pom),
            "sha256": digest(pom),
            "openllet_2_6_5_pinned": "<version>2.6.5</version>" in pom_text,
        },
        "classpath": {
            "path": str(classpath_lock),
            "sha256": digest(classpath_lock),
            "jar_count": len(jars),
            "openllet_owlapi_2_6_5": jar_by_name.get("openllet-owlapi-2.6.5.jar"),
            "owlapi_distribution_5_5_1": jar_by_name.get("owlapi-distribution-5.5.1.jar"),
            "jars": jars,
        },
        "configuration": {"path": str(configuration), "sha256": digest(configuration)},
        "exporter": {"path": str(exporter), "sha256": digest(exporter)},
    }


def inspect_image(path: Path) -> dict[str, Any]:
    value = load_json(path)
    if isinstance(value, list) and value:
        first = value[0]
    else:
        first = value
    if not isinstance(first, dict):
        raise AttestationError("failed", ["image.inspect_malformed"])
    return first


def declared_path(value: Any, fallback: Path) -> Path:
    if not isinstance(value, str) or not value:
        return fallback
    path = Path(value).expanduser()
    if path.is_absolute():
        return path
    for root in (Path.cwd(), REPO_ROOT, SCRIPT_DIR):
        candidate = root / path
        if candidate.exists():
            return candidate
    return REPO_ROOT / path


def attest(args: argparse.Namespace) -> int:
    identity_path = args.identity.expanduser().resolve()
    output = args.output.expanduser().resolve() if args.output else None
    if not identity_path.is_file():
        write_json(output, {"schema_version": 1, "status": "not_available", "reasons": ["identity.missing"]})
        return 125

    try:
        identity = require_object(load_json(identity_path), "identity.malformed")
        runtime = require_object(identity.get("runtime"), "runtime.missing")
        image = require_object(runtime.get("image"), "runtime.image_missing")
        base = require_object(runtime.get("base_image"), "runtime.base_image_missing")
        variant_revision = runtime.get("variant_revision")
        if not isinstance(variant_revision, str) or not variant_revision:
            raise AttestationError("not_available", ["runtime.variant_revision_missing"])
        classpath = require_object(identity.get("classpath"), "classpath.missing")
        inputs = require_object(identity.get("inputs"), "inputs.missing")

        image_ref = str(args.image_ref or image.get("reference") or "")
        reject_mutable_image_ref(image_ref)
        image_digest = require_digest(image.get("digest"), "runtime.image_digest_missing")
        base_digest = require_digest(base.get("digest"), "runtime.base_digest_missing")
        reasons: list[str] = []
        if base_digest != EXPECTED_BASE_DIGEST:
            reasons.append("base_image.digest_mismatch")
        if image_digest != EXPECTED_IMAGE_DIGEST:
            reasons.append("image.digest_mismatch")
        if variant_revision != EXPECTED_VARIANT_REVISION:
            reasons.append("runtime.variant_revision_mismatch")
        if image_digest not in image_ref:
            reasons.append("image.reference_digest_mismatch")

        config_path = declared_path(
            inputs.get("configuration", {}).get("path"),
            SCRIPT_DIR / "openllet_ql_runtime_config.json",
        )
        exporter_path = declared_path(
            inputs.get("exporter", {}).get("path"),
            SCRIPT_DIR / "OpenlletQlMaterializer.java",
        )
        lock_path = (
            args.classpath_lock.expanduser().resolve()
            if args.classpath_lock
            else declared_path(classpath.get("path"), SCRIPT_DIR / "openllet_ql_classpath.sha256")
        )
        for current_path, section, field in (
            (config_path, "configuration", "inputs.configuration_sha256_mismatch"),
            (exporter_path, "exporter", "inputs.exporter_sha256_mismatch"),
            (lock_path, None, "classpath.lock_sha256_mismatch"),
        ):
            if not current_path.is_file():
                raise AttestationError("not_available", [f"{current_path.name}.missing"])
            declared = classpath.get("lock_sha256") if section is None else inputs.get(section, {}).get("sha256")
            if declared != digest(current_path):
                reasons.append(field)

        jars = parse_classpath(lock_path)
        if len(jars) != EXPECTED_JAR_COUNT:
            reasons.append("classpath.jar_count_mismatch")
        jar_by_name = {jar["name"]: jar["sha256"] for jar in jars}
        if jar_by_name.get("openllet-owlapi-2.6.5.jar") != "sha256:c9f9376f59b0103ac8fe19d715a1f205dc2b4a1b677e8a4289659346c72da588":
            reasons.append("classpath.openllet_owlapi_hash_mismatch")
        if jar_by_name.get("owlapi-distribution-5.5.1.jar") != "sha256:747b1a5269fee2992487dcde946f16dfbc14aa458d50854994a0485cf263ce07":
            reasons.append("classpath.owlapi_distribution_hash_mismatch")

        local_attestation = "not_checked"
        if args.image_inspect:
            inspected = inspect_image(args.image_inspect.expanduser().resolve())
            repo_digests = inspected.get("RepoDigests")
            if not isinstance(repo_digests, list):
                repo_digests = []
            if inspected.get("Id") != image_digest or image_ref not in repo_digests:
                reasons.append("image.local_identity_mismatch")
            if inspected.get("Architecture") != "amd64" or inspected.get("Os") != "linux":
                reasons.append("image.platform_mismatch")
            config = inspected.get("Config")
            labels = config.get("Labels") if isinstance(config, dict) else None
            inspected_revision = labels.get("org.opencontainers.image.revision") if isinstance(labels, dict) else None
            if inspected_revision != variant_revision:
                reasons.append("image.variant_revision_mismatch")
            local_attestation = "matched" if not reasons else "mismatched"
        elif not reasons:
            raise AttestationError("not_available", ["image.inspect_missing"])

        if reasons:
            write_json(output, {"schema_version": 1, "status": "failed", "reasons": reasons})
            return 1

        result = {
            "schema_version": 1,
            "status": "available",
            "component": identity["component"],
            "runtime": {
                **runtime,
                "image": {**image, "reference": image_ref, "local_attestation": local_attestation},
            },
            "inputs": inputs,
            "classpath": {**classpath, "jar_count": len(jars), "jars": jars},
            "resource_limits": identity["resource_limits"],
            "credentials": identity["credentials"],
            "license": identity["license"],
        }
        write_json(output, result)
        return 0
    except AttestationError as error:
        write_json(output, {"schema_version": 1, "status": error.status, "reasons": error.reasons})
        return 125 if error.status == "not_available" else 1


def summarize_build_inputs(args: argparse.Namespace) -> int:
    try:
        summary = build_input_summary(
            args.dockerfile.expanduser().resolve(),
            args.pom.expanduser().resolve(),
            args.classpath_lock.expanduser().resolve(),
            args.configuration.expanduser().resolve(),
            args.exporter.expanduser().resolve(),
        )
        write_json(args.output.expanduser().resolve() if args.output else None, summary)
        return 0
    except AttestationError as error:
        write_json(args.output.expanduser().resolve() if args.output else None, {"schema_version": 1, "status": error.status, "reasons": error.reasons})
        return 125 if error.status == "not_available" else 1


def load_control_json(path: Path, label: str) -> dict[str, Any]:
    resolved = path.expanduser().resolve()
    if not resolved.is_file():
        raise AttestationError("not_available", [f"{label}.missing"])
    try:
        value = load_json(resolved)
    except (OSError, UnicodeDecodeError, json.JSONDecodeError) as error:
        raise AttestationError("not_available", [f"{label}.malformed"]) from error
    if not isinstance(value, dict):
        raise AttestationError("not_available", [f"{label}.malformed"])
    return value


def validate_variant(args: argparse.Namespace) -> int:
    output = args.output.expanduser().resolve() if args.output else None
    try:
        identity = load_control_json(args.identity, "identity")
        second_host_manifest = (
            args.second_host_evidence_manifest.expanduser().resolve()
            if args.second_host_evidence_manifest
            else None
        )
        agpl_review = (
            load_control_json(args.agpl_review_evidence, "agpl_review_evidence")
            if args.agpl_review_evidence
            else None
        )
        evidence_manifest = args.evidence_manifest.expanduser().resolve() if args.evidence_manifest else None
        result = evaluate_variant_control(
            identity,
            evidence_manifest,
            second_host_manifest,
            agpl_review,
        )
        write_json(output, result)
        return 0 if result.get("status") == "complete" else 1
    except AttestationError as error:
        write_json(output, {"schema_version": 1, "status": error.status, "reasons": error.reasons})
        return 125 if error.status == "not_available" else 1


def parser() -> argparse.ArgumentParser:
    main = argparse.ArgumentParser(description=__doc__)
    subparsers = main.add_subparsers(dest="command", required=True)

    attest_parser = subparsers.add_parser("attest")
    attest_parser.add_argument("--identity", type=Path, required=True)
    attest_parser.add_argument("--image-ref")
    attest_parser.add_argument("--image-inspect", type=Path)
    attest_parser.add_argument("--classpath-lock", type=Path)
    attest_parser.add_argument("--output", type=Path)
    attest_parser.set_defaults(func=attest)

    summary_parser = subparsers.add_parser("summarize-build-inputs")
    summary_parser.add_argument("--dockerfile", type=Path, required=True)
    summary_parser.add_argument("--pom", type=Path, required=True)
    summary_parser.add_argument("--classpath-lock", type=Path, required=True)
    summary_parser.add_argument("--configuration", type=Path, required=True)
    summary_parser.add_argument("--exporter", type=Path, required=True)
    summary_parser.add_argument("--output", type=Path)
    summary_parser.set_defaults(func=summarize_build_inputs)

    variant_parser = subparsers.add_parser("validate-variant")
    variant_parser.add_argument("--identity", type=Path, required=True)
    variant_parser.add_argument("--evidence-manifest", type=Path)
    variant_parser.add_argument("--second-host-evidence-manifest", type=Path)
    variant_parser.add_argument("--agpl-review-evidence", type=Path)
    variant_parser.add_argument("--output", type=Path)
    variant_parser.set_defaults(func=validate_variant)
    return main


def main() -> int:
    args = parser().parse_args()
    return args.func(args)


if __name__ == "__main__":
    raise SystemExit(main())
