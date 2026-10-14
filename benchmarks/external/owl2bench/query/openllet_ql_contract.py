"""Accepted identity, runtime, and manifest contracts for Openllet QL."""

from __future__ import annotations

import copy
import hashlib
import json
import os
import re
import shutil
import subprocess
import sys
from pathlib import Path
from typing import Any, Iterable

from materialization_manifest import file_digest
from resource_variant import resource_variant


SCRIPT_DIR = Path(__file__).resolve().parent
PROFILE = "QL"
EXECUTION_VARIANT = "openllet-ql"
EXPECTED_QUERY_COUNT = 10
NOT_AVAILABLE_EXIT_CODE = 125
TIMEOUT_EXIT_CODE = 124
EXPECTED_IMAGE_DIGEST = "sha256:548e5ecefa8b252f8baec95bfb4e9528363257b653dd79a189aa0f351df937bd"
EXPECTED_RUNTIME_REVISION = "openllet-ql-runtime-v1"
EXPECTED_CLASSPATH_JARS = 78
EXPECTED_OPENLLET_VERSION = "2.6.5"
EXPECTED_OWLAPI_VERSION = "5.5.1"
EXPECTED_MATERIALIZATION_REVISION = "openllet-ql-materialization-v1"
OXIGRAPH_VERSION = "0.5.9"
OXIGRAPH_IMAGE_DIGEST = "sha256:e68b3625743db4a4b18129a907ae36766f89bb6deeb8ab50d35685dbabe00b0e"
CONFORMANCE_BOUNDARY = "Openllet Direct Semantics over OWL 2 QL input"
RULESET_ID = "openllet-direct-semantics-on-ql-input"
DEFAULT_QUERY_NAMESPACE = "https://kracr.iiitd.edu.in/OWL2Bench#"

RDF = "http://www.w3.org/1999/02/22-rdf-syntax-ns#"
RDFS = "http://www.w3.org/2000/01/rdf-schema#"
OWL = "http://www.w3.org/2002/07/owl#"
XSD = "http://www.w3.org/2001/XMLSchema#"
RDF_TYPE = RDF + "type"
RDFS_SUBCLASS = RDFS + "subClassOf"
RDFS_SUBPROPERTY = RDFS + "subPropertyOf"
OWL_EQUIVALENT_CLASS = OWL + "equivalentClass"
OWL_EQUIVALENT_PROPERTY = OWL + "equivalentProperty"
OWL_RESTRICTION = OWL + "Restriction"
OWL_ON_PROPERTY = OWL + "onProperty"
OWL_SOME_VALUES_FROM = OWL + "someValuesFrom"
OWL_HAS_VALUE = OWL + "hasValue"
OWL_MIN_CARDINALITY = OWL + "minCardinality"
OWL_MIN_QUALIFIED_CARDINALITY = OWL + "minQualifiedCardinality"
OWL_QUALIFIED_CARDINALITY = OWL + "qualifiedCardinality"

PREFIX_DECLARATION = re.compile(
    r"(?im)^\s*PREFIX\s+([A-Za-z][A-Za-z0-9_-]*)?\s*:\s*<([^>]+)>"
)
PREFIXED_NAME = re.compile(
    r"(?<![A-Za-z0-9_])([A-Za-z][A-Za-z0-9_-]*)?:([A-Za-z_][A-Za-z0-9_.-]*)"
)
FULL_IRI = re.compile(r"<([^>]+)>")
IMPLEMENTATION_MODULES = (
    "openllet_ql_materialization.py",
    "openllet_ql_contract.py",
    "openllet_ql_exporter.py",
    "openllet_ql_diagnostic.py",
)


class OpenlletQlError(ValueError):
    """The requested materialization cannot produce accepted evidence."""


class OpenlletQlUnavailable(OpenlletQlError):
    """A required input, runtime, or finite witness is unavailable."""


class OpenlletQlTimeout(OpenlletQlError):
    """The bounded runtime exceeded its deadline."""


def digest(path: Path) -> str:
    value = file_digest(str(path))
    if value is None:
        raise OpenlletQlUnavailable(f"artifact digest is unavailable: {path}")
    return value


def implementation_identity() -> dict[str, Any]:
    modules: list[dict[str, str]] = []
    hasher = hashlib.sha256()
    for filename in IMPLEMENTATION_MODULES:
        path = require_file(SCRIPT_DIR / filename, f"Openllet implementation module {filename}")
        module_digest = digest(path)
        modules.append({"path": str(path), "sha256": module_digest})
        hasher.update(filename.encode("utf-8"))
        hasher.update(b"\0")
        hasher.update(path.read_bytes())
        hasher.update(b"\0")
    entrypoint = Path(modules[0]["path"])
    return {
        "path": str(entrypoint),
        "sha256": modules[0]["sha256"],
        "revision": "sha256:" + hasher.hexdigest(),
        "modules": modules,
    }


def load_json(path: Path, label: str) -> dict[str, Any]:
    if not path.is_file():
        raise OpenlletQlUnavailable(f"{label} is missing: {path}")
    try:
        value = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, UnicodeDecodeError, json.JSONDecodeError) as error:
        raise OpenlletQlError(f"{label} is malformed: {error}") from error
    if not isinstance(value, dict):
        raise OpenlletQlError(f"{label} root must be an object")
    return value


def require_file(path: Path, label: str, *, executable: bool = False) -> Path:
    resolved = path.expanduser().resolve()
    if not resolved.is_file() or not os.access(resolved, os.R_OK):
        raise OpenlletQlUnavailable(f"{label} is missing or unreadable: {resolved}")
    if executable and not os.access(resolved, os.X_OK):
        raise OpenlletQlUnavailable(f"{label} is not executable: {resolved}")
    return resolved


def require_directory(path: Path, label: str) -> Path:
    resolved = path.expanduser().resolve()
    if not resolved.is_dir() or not os.access(resolved, os.R_OK):
        raise OpenlletQlUnavailable(f"{label} is missing or unreadable: {resolved}")
    return resolved


def resolve_executable(value: str, label: str) -> Path:
    candidate = shutil.which(value) if os.path.basename(value) == value else value
    if not candidate:
        raise OpenlletQlUnavailable(f"{label} executable is missing: {value}")
    return require_file(Path(candidate), label, executable=True)


def nonempty(value: Any) -> bool:
    return isinstance(value, str) and bool(value.strip())


def ql_dataset_paths(dataset_root: Path) -> dict[str, Path]:
    root = require_directory(dataset_root, "frozen profile dataset root")
    profile_dir = root / "profiles" / PROFILE
    generator_dir = root / "generator"
    return {
        "root": root,
        "profile_dir": require_directory(profile_dir, "frozen QL profile directory"),
        "source": require_file(profile_dir / "source.nq", "frozen QL source graph"),
        "dataset": require_file(profile_dir / "dataset.owl", "frozen QL generated dataset"),
        "tbox": require_file(profile_dir / "profile-tbox.owl", "frozen QL profile TBox"),
        "abox": require_file(profile_dir / "abox.nq", "frozen QL profile ABox"),
        "profile_identity": require_file(
            profile_dir / "profile-identity.json", "frozen QL profile identity"
        ),
        "generator": require_file(generator_dir / "OWL2Bench.jar", "OWL2Bench generator"),
        "generator_config": require_file(
            generator_dir / "config.properties", "OWL2Bench generator configuration"
        ),
    }


def validate_frozen_profile(paths: dict[str, Path]) -> dict[str, Any]:
    identity = load_json(paths["profile_identity"], "frozen QL profile identity")
    if identity.get("profile") != PROFILE or identity.get("status") != "complete":
        raise OpenlletQlUnavailable("frozen profile identity must be a complete QL record")
    dataset = identity.get("dataset")
    generator = identity.get("generator")
    configuration = identity.get("generator_configuration")
    context = identity.get("profile_context")
    if not isinstance(dataset, dict):
        raise OpenlletQlUnavailable("frozen QL dataset identity is missing")
    if not isinstance(generator, dict) or not isinstance(configuration, dict):
        raise OpenlletQlUnavailable("frozen QL generator identity is incomplete")
    if not isinstance(context, dict):
        raise OpenlletQlUnavailable("frozen QL profile context identity is missing")

    required_dataset_fields = (
        "id",
        "profile",
        "sha256",
        "source_ontology_sha256",
        "artifact_id",
        "artifact_path",
        "artifact_revision",
        "tbox_id",
        "tbox_sha256",
        "abox_id",
        "abox_sha256",
    )
    missing = [field for field in required_dataset_fields if not nonempty(dataset.get(field))]
    if missing:
        raise OpenlletQlUnavailable(
            "frozen QL dataset identity fields are missing: " + ", ".join(missing)
        )
    if dataset.get("profile") != PROFILE:
        raise OpenlletQlUnavailable("frozen dataset profile does not match QL")
    checks = (
        (
            Path(str(dataset["artifact_path"])).expanduser().resolve(),
            paths["source"],
            "source path",
        ),
        (dataset["sha256"], digest(paths["source"]), "source hash"),
        (dataset["source_ontology_sha256"], digest(paths["dataset"]), "generated dataset hash"),
        (dataset["tbox_sha256"], digest(paths["tbox"]), "profile TBox hash"),
        (dataset["abox_sha256"], digest(paths["abox"]), "profile ABox hash"),
        (
            Path(str(generator.get("path", ""))).expanduser().resolve(),
            paths["generator"],
            "generator path",
        ),
        (generator.get("sha256"), digest(paths["generator"]), "generator hash"),
        (
            Path(str(configuration.get("path", ""))).expanduser().resolve(),
            paths["generator_config"],
            "generator configuration path",
        ),
        (
            configuration.get("sha256"),
            digest(paths["generator_config"]),
            "generator configuration hash",
        ),
        (
            Path(str(context.get("directory", ""))).expanduser().resolve(),
            paths["profile_dir"],
            "profile context directory",
        ),
    )
    for actual, expected, label in checks:
        if actual != expected:
            raise OpenlletQlUnavailable(f"frozen QL {label} does not match its artifact")
    seed = dataset.get("seed")
    if isinstance(seed, bool) or not isinstance(seed, int) or seed < 0:
        raise OpenlletQlUnavailable("frozen QL generator seed is invalid")
    return identity


def validate_acceptance_lock(
    paths: dict[str, Path],
    frozen: dict[str, Any],
    query_manifest: Path,
    queries: list[dict[str, Any]],
) -> dict[str, Any]:
    lock_path = require_file(
        SCRIPT_DIR / "openllet_ql_acceptance_lock.json",
        "Openllet QL acceptance lock",
    )
    lock = load_json(lock_path, "Openllet QL acceptance lock")
    if (
        lock.get("status") != "accepted"
        or lock.get("profile") != PROFILE
        or lock.get("execution_variant") != EXECUTION_VARIANT
        or lock.get("materialization_revision")
        != EXPECTED_MATERIALIZATION_REVISION
    ):
        raise OpenlletQlError("Openllet QL acceptance lock identity is invalid")
    accepted = validate_acceptance_contract(
        lock, paths, frozen, query_manifest, queries
    )
    return {
        "path": str(lock_path),
        "sha256": digest(lock_path),
        "materialization_revision": lock["materialization_revision"],
        **accepted,
    }


def validate_acceptance_contract(
    lock: dict[str, Any],
    paths: dict[str, Path],
    frozen: dict[str, Any],
    query_manifest: Path,
    queries: list[dict[str, Any]],
) -> dict[str, str]:
    """Compare supplied artifacts with an already selected acceptance lock."""

    profile_lock = lock.get("profile_identity")
    artifacts = lock.get("artifacts")
    workload = lock.get("query_workload")
    if (
        not isinstance(profile_lock, dict)
        or not isinstance(artifacts, dict)
        or not isinstance(workload, dict)
    ):
        raise OpenlletQlError("Openllet QL acceptance lock is incomplete")

    dataset = frozen["dataset"]
    profile_checks = {
        "id": dataset.get("id"),
        "source_revision": frozen.get("source_revision"),
        "identity_sha256": frozen.get("identity_sha256"),
    }
    for field, actual in profile_checks.items():
        if profile_lock.get(field) != actual:
            raise OpenlletQlUnavailable(
                f"frozen QL profile identity does not match acceptance lock: {field}"
            )
    artifact_checks = {
        "source": digest(paths["source"]),
        "generated_dataset": digest(paths["dataset"]),
        "profile_tbox": digest(paths["tbox"]),
        "profile_abox": digest(paths["abox"]),
        "generator": digest(paths["generator"]),
        "generator_configuration": digest(paths["generator_config"]),
    }
    for field, actual in artifact_checks.items():
        if artifacts.get(field) != actual:
            raise OpenlletQlUnavailable(
                f"frozen QL artifact does not match acceptance lock: {field}"
            )
    if workload.get("manifest_sha256") != digest(query_manifest):
        raise OpenlletQlUnavailable(
            "official QL query manifest does not match acceptance lock"
        )
    expected_queries = workload.get("queries")
    if not isinstance(expected_queries, list) or workload.get("query_count") != len(queries):
        raise OpenlletQlError("accepted QL query list is incomplete")
    expected_identity = [
        {"id": item.get("id"), "sha256": item.get("sha256")}
        for item in expected_queries
        if isinstance(item, dict)
    ]
    actual_identity = [
        {"id": query.get("id"), "sha256": query.get("sha256")}
        for query in queries
    ]
    if expected_identity != actual_identity:
        raise OpenlletQlUnavailable(
            "official QL query files do not match acceptance lock"
        )
    for query in queries:
        if (
            query.get("source_corpus") != workload.get("source_corpus")
            or query.get("source_revision") != workload.get("source_revision")
        ):
            raise OpenlletQlUnavailable(
                "official QL query provenance does not match acceptance lock"
            )
    return {
        "profile_identity_sha256": profile_lock["identity_sha256"],
        "query_manifest_sha256": workload["manifest_sha256"],
    }


def collect_queries(query_manifest: Path, expected_count: int) -> list[dict[str, Any]]:
    manifest = require_file(query_manifest, "query manifest")
    lines = manifest.read_text(encoding="utf-8").splitlines()
    if not lines:
        raise OpenlletQlError("query manifest is empty")
    header = lines[0].split("\t")
    try:
        path_index = header.index("staged_query")
        source_query_index = header.index("source_query")
        query_id_index = header.index("query_id")
        profile_index = header.index("profile")
        workload_index = header.index("workload")
        source_corpus_index = header.index("source_corpus")
        source_revision_index = header.index("source_revision")
    except ValueError as error:
        raise OpenlletQlError("query manifest is missing required columns") from error
    queries: list[dict[str, Any]] = []
    for line_number, line in enumerate(lines[1:], start=2):
        if not line:
            continue
        fields = line.split("\t")
        if len(fields) != len(header):
            raise OpenlletQlError(
                f"query manifest row {line_number} has the wrong field count"
            )
        profiles = {part.strip().upper() for part in fields[profile_index].split(",")}
        if PROFILE not in profiles:
            continue
        query_path = require_file(Path(fields[path_index]), f"QL query row {line_number}")
        queries.append(
            {
                "id": fields[query_id_index],
                "path": str(query_path),
                "sha256": digest(query_path),
                "manifest_row": line_number,
                "source_query": fields[source_query_index],
                "workload": fields[workload_index],
                "source_corpus": fields[source_corpus_index],
                "source_revision": fields[source_revision_index],
            }
        )
    if len(queries) != expected_count:
        raise OpenlletQlError(
            f"query manifest must provide {expected_count} QL rows, found {len(queries)}"
        )
    return queries


def query_iris(queries: Iterable[dict[str, Any]]) -> set[str]:
    iris: set[str] = set()
    defaults = {
        "": DEFAULT_QUERY_NAMESPACE,
        "rdf": RDF,
        "rdfs": RDFS,
        "owl": OWL,
        "xsd": XSD,
    }
    for query in queries:
        path = require_file(Path(str(query["path"])), "query workload entry")
        if digest(path) != query.get("sha256"):
            raise OpenlletQlError(f"query workload hash changed: {path}")
        text = path.read_text(encoding="utf-8")
        prefixes = dict(defaults)
        for declaration in PREFIX_DECLARATION.finditer(text):
            prefixes[declaration.group(1) or ""] = declaration.group(2)
        body = PREFIX_DECLARATION.sub("", text)
        iris.update(match.group(1) for match in FULL_IRI.finditer(body))
        without_full_iris = FULL_IRI.sub(" ", body)
        for prefixed in PREFIXED_NAME.finditer(without_full_iris):
            prefix = prefixed.group(1) or ""
            if prefix in prefixes:
                local_name = prefixed.group(2).rstrip(".")
                if local_name:
                    iris.add(prefixes[prefix] + local_name)
        if re.search(r"(?<![?A-Za-z0-9_:])a(?![A-Za-z0-9_:])", without_full_iris):
            iris.add(RDF_TYPE)
    if not iris:
        raise OpenlletQlError("QL query workload contains no materialization IRIs")
    return iris


def write_query_iris(path: Path, queries: Iterable[dict[str, Any]]) -> set[str]:
    values = query_iris(queries)
    path.write_text("\n".join(sorted(values)) + "\n", encoding="utf-8")
    return values


def attest_runtime(
    docker_bin: str, runtime_identity_path: Path, output_dir: Path
) -> dict[str, Any]:
    docker = resolve_executable(docker_bin, "Docker")
    identity_path = require_file(runtime_identity_path, "Openllet runtime identity")
    identity = load_json(identity_path, "Openllet runtime identity")
    runtime = identity.get("runtime")
    if not isinstance(runtime, dict):
        raise OpenlletQlUnavailable("Openllet runtime identity is missing runtime")
    image = runtime.get("image")
    if not isinstance(image, dict):
        raise OpenlletQlUnavailable("Openllet runtime image identity is missing")
    image_reference = str(image.get("reference") or "")
    if "@sha256:" not in image_reference:
        raise OpenlletQlError("Openllet runtime image reference must be digest pinned")

    output_dir.mkdir(parents=True, exist_ok=True)
    inspect_path = output_dir / "image-inspect.json"
    attestation_path = output_dir / "attestation.json"
    inspected = subprocess.run(
        [str(docker), "image", "inspect", image_reference],
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        check=False,
    )
    if inspected.returncode != 0 or not inspected.stdout.strip():
        raise OpenlletQlUnavailable(
            f"pinned Openllet image is not available locally: {image_reference}"
        )
    inspect_path.write_bytes(inspected.stdout)
    attestor = require_file(
        SCRIPT_DIR / "openllet_ql_runtime_identity.py", "Openllet runtime attestor"
    )
    attested = subprocess.run(
        [
            sys.executable,
            str(attestor),
            "attest",
            "--identity",
            str(identity_path),
            "--image-ref",
            image_reference,
            "--image-inspect",
            str(inspect_path),
            "--output",
            str(attestation_path),
        ],
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        check=False,
    )
    result = load_json(attestation_path, "Openllet runtime attestation")
    if attested.returncode == NOT_AVAILABLE_EXIT_CODE or result.get("status") == "not_available":
        raise OpenlletQlUnavailable(
            "Openllet runtime attestation is unavailable: "
            + ", ".join(str(reason) for reason in result.get("reasons", []))
        )
    if attested.returncode != 0 or result.get("status") != "available":
        raise OpenlletQlError(
            "Openllet runtime attestation failed: "
            + ", ".join(str(reason) for reason in result.get("reasons", []))
        )
    attested_runtime = result.get("runtime")
    classpath = result.get("classpath")
    if not isinstance(attested_runtime, dict) or not isinstance(classpath, dict):
        raise OpenlletQlError("Openllet runtime attestation is incomplete")
    attested_image = attested_runtime.get("image")
    if (
        not isinstance(attested_image, dict)
        or attested_image.get("digest") != EXPECTED_IMAGE_DIGEST
        or attested_image.get("local_attestation") != "matched"
        or attested_runtime.get("variant_revision") != EXPECTED_RUNTIME_REVISION
        or classpath.get("jar_count") != EXPECTED_CLASSPATH_JARS
    ):
        raise OpenlletQlError("Openllet runtime attestation does not match the accepted variant")
    result["attestation_artifact"] = {
        "path": str(attestation_path),
        "sha256": digest(attestation_path),
    }
    result["image_inspect_artifact"] = {
        "path": str(inspect_path),
        "sha256": digest(inspect_path),
    }
    return result


def backend_identity(path: Path) -> dict[str, Any]:
    backend = require_file(path, "materialized-graph query backend", executable=True)
    identity = {
        "name": "materialized-graph-oxigraph",
        "version": OXIGRAPH_VERSION,
        "revision": f"oxigraph/oxigraph@{OXIGRAPH_IMAGE_DIGEST}",
        "artifact_digest": digest(backend),
        "path": str(backend),
        "read_only": True,
        "image_digest": OXIGRAPH_IMAGE_DIGEST,
        "contract": "query-only access to the exact validated read-only graph handoff",
    }
    production_wrapper = (SCRIPT_DIR / "openllet_ql_query_engine.sh").resolve()
    if backend == production_wrapper:
        delegate = require_file(
            SCRIPT_DIR / "materialized_graph_oxigraph_engine.sh",
            "Oxigraph materialized-graph delegate",
            executable=True,
        )
        identity["delegate"] = {
            "path": str(delegate),
            "sha256": digest(delegate),
            "image_digest": OXIGRAPH_IMAGE_DIGEST,
        }
    return identity


def build_identity(
    paths: dict[str, Path],
    frozen: dict[str, Any],
    queries: list[dict[str, Any]],
    query_manifest: Path,
    attestation: dict[str, Any],
    exporter: Path,
    query_backend: Path,
    acceptance_lock: dict[str, Any],
) -> dict[str, Any]:
    dataset = frozen["dataset"]
    generator = frozen["generator"]
    generator_configuration = frozen["generator_configuration"]
    frozen_context = frozen["profile_context"]
    runtime = copy.deepcopy(attestation["runtime"])
    runtime["id"] = f"docker:{runtime['image']['reference']}"
    runtime["attestation"] = attestation["attestation_artifact"]
    runtime["image_inspect"] = attestation["image_inspect_artifact"]
    runtime["classpath"] = {
        "lock_sha256": attestation["classpath"]["lock_sha256"],
        "jar_count": attestation["classpath"]["jar_count"],
    }
    resource_limits = copy.deepcopy(attestation["resource_limits"])
    resource_limits["network"] = "disabled"
    resource_limits["variant"] = resource_variant(
        resource_limits,
        {
            "timeout_seconds": 600,
            "memory": "24GiB",
            "threads": 1,
            "cpu": "1",
            "disk": "10GiB",
            "network": "disabled",
        },
    )
    wrapper = require_file(exporter, "Openllet QL exporter", executable=True)
    implementation = implementation_identity()
    config = require_file(
        SCRIPT_DIR / "openllet_ql_runtime_config.json", "Openllet runtime configuration"
    )
    java_exporter = require_file(
        SCRIPT_DIR / "OpenlletQlMaterializer.java", "Openllet Java exporter"
    )
    license_path = require_file(
        SCRIPT_DIR / "openllet_ql_license_identity.json",
        "Openllet license identity",
    )
    license_document = load_json(license_path, "Openllet license identity")
    if (
        license_document.get("id") != "AGPL-3.0-only"
        or license_document.get("open_source") is not True
        or license_document.get("runtime_license_required") is not False
        or license_document.get("distribution_review_status") != "pending"
    ):
        raise OpenlletQlError("Openllet license identity is invalid")
    source_hash = digest(paths["source"])
    dataset_hash = digest(paths["dataset"])
    tbox_hash = digest(paths["tbox"])
    abox_hash = digest(paths["abox"])
    generator_hash = digest(paths["generator"])
    generator_config_hash = digest(paths["generator_config"])
    seed = dataset["seed"]
    identity = {
        "profile": PROFILE,
        "execution_variant": EXECUTION_VARIANT,
        "acceptance_lock": copy.deepcopy(acceptance_lock),
        "component": {
            "name": "QL-ruleset-engine",
            "version": f"openllet-{EXPECTED_OPENLLET_VERSION}",
            "revision": implementation["revision"],
            "artifact_digest": digest(wrapper),
            "path": str(wrapper),
        },
        "exporter": {
            "name": "Openllet",
            "version": EXPECTED_OPENLLET_VERSION,
            "revision": implementation["revision"],
            "artifact_digest": digest(wrapper),
            "path": str(wrapper),
            "implementation": {
                "path": str(java_exporter),
                "sha256": digest(java_exporter),
            },
            "contract": "export <source.nq> emits source-inclusive finite named entailments",
        },
        "query_backend": backend_identity(query_backend),
        "configuration": {
            "id": "openllet-ql-runtime-config",
            "path": str(config),
            "sha256": digest(config),
            "digest": digest(config),
        },
        "ruleset": {
            "id": RULESET_ID,
            "version": EXPECTED_OPENLLET_VERSION,
            "revision": EXPECTED_RUNTIME_REVISION,
            "artifact_digest": EXPECTED_IMAGE_DIGEST,
            "conformance_boundary": CONFORMANCE_BOUNDARY,
        },
        "derivation": {
            "version": "openllet-ql-materialization/1",
            "revision": EXPECTED_MATERIALIZATION_REVISION,
            "implementation_sha256": implementation["revision"],
            "semantics": CONFORMANCE_BOUNDARY,
            "source_included": True,
            "anonymous_witness_policy": "not_exported",
        },
        "runtime": runtime,
        "resource_limits": resource_limits,
        "profile_context": {
            "id": str(frozen_context["id"]),
            "directory": str(paths["profile_dir"]),
            "profile": PROFILE,
        },
        "source_document": {
            "path": str(paths["source"]),
            "sha256": source_hash,
            "revision": str(dataset["artifact_revision"]),
        },
        "dataset": {
            "profile": PROFILE,
            "id": str(dataset["id"]),
            "artifact_id": str(dataset["artifact_id"]),
            "artifact_path": str(paths["source"]),
            "artifact_revision": str(dataset["artifact_revision"]),
            "sha256": source_hash,
            "source_ontology_sha256": dataset_hash,
            "tbox_id": str(dataset["tbox_id"]),
            "tbox_sha256": tbox_hash,
            "abox_id": str(dataset["abox_id"]),
            "abox_sha256": abox_hash,
            "seed": seed,
        },
        "dataset_artifact": {
            "id": str(dataset["artifact_id"]),
            "path": str(paths["source"]),
            "sha256": source_hash,
            "revision": str(dataset["artifact_revision"]),
        },
        "generated_dataset": {
            "id": f"generated-dataset:{dataset_hash}",
            "path": str(paths["dataset"]),
            "sha256": dataset_hash,
            "revision": str(dataset["artifact_revision"]),
        },
        "profile_tbox": {
            "id": str(dataset["tbox_id"]),
            "path": str(paths["tbox"]),
            "sha256": tbox_hash,
            "revision": str(dataset["artifact_revision"]),
        },
        "profile_abox": {
            "id": str(dataset["abox_id"]),
            "path": str(paths["abox"]),
            "sha256": abox_hash,
            "revision": str(dataset["artifact_revision"]),
        },
        "generator": {
            "id": str(generator["id"]),
            "revision": str(generator["revision"]),
            "seed": seed,
            "artifact_path": str(paths["generator"]),
            "artifact_digest": generator_hash,
            "configuration_id": str(generator_configuration["id"]),
            "configuration_sha256": generator_config_hash,
        },
        "generator_configuration": {
            "id": str(generator_configuration["id"]),
            "path": str(paths["generator_config"]),
            "sha256": generator_config_hash,
            "revision": str(generator_configuration["revision"]),
        },
        "implementation": implementation,
        "query_workload": {
            "id": "owl2bench-query-workload:QL",
            "profile": PROFILE,
            "query_count": len(queries),
            "queries": queries,
            "source_manifest": str(query_manifest),
            "source_manifest_sha256": digest(query_manifest),
        },
        "credentials": copy.deepcopy(attestation["credentials"]),
        "license": {
            "status": "available",
            "id": license_document["id"],
            "digest": digest(license_path),
            "path": str(license_path),
            "runtime_license_required": license_document["runtime_license_required"],
            "distribution_review_status": license_document[
                "distribution_review_status"
            ],
            "provenance": license_document["provenance"],
            "open_source": license_document["open_source"],
        },
        "profile_capability_boundary": CONFORMANCE_BOUNDARY,
        "ruleset_required": True,
        "two_run_determinism_required": True,
    }
    return identity


def component_manifest(identity: dict[str, Any], query_manifest: Path) -> dict[str, Any]:
    components = {
        "engine": {
            key: identity["exporter"][key]
            for key in ("name", "version", "revision", "artifact_digest")
        },
        "materializer": {
            key: identity["component"][key]
            for key in ("name", "version", "revision", "artifact_digest")
        },
        "query": {
            key: identity["query_backend"][key]
            for key in ("name", "version", "revision", "artifact_digest")
        },
    }
    return {
        "schema_version": 1,
        "manifest_id": "openllet-ql-component-manifest",
        "matrix": {
            "expansion": "query_profile",
            "expected_query_count": 60,
            "expected_profile_case_counts": {"DL": 22, "EL": 10, "QL": 10, "RL": 18},
        },
        "profiles": {
            PROFILE: {
                "execution_variant": EXECUTION_VARIANT,
                "acceptance_lock": copy.deepcopy(identity["acceptance_lock"]),
                "materialization_implementation": copy.deepcopy(
                    identity["implementation"]
                ),
                "components": components,
                "ruleset": copy.deepcopy(identity["ruleset"]),
                "configuration": {
                    "id": identity["configuration"]["id"],
                    "path": identity["configuration"]["path"],
                    "digest": identity["configuration"]["sha256"],
                },
                "runtime": copy.deepcopy(identity["runtime"]),
                "credentials": copy.deepcopy(identity["credentials"]),
                "license": copy.deepcopy(identity["license"]),
                "capability": {
                    "status": "supported",
                    "supports_query": True,
                    "boundary": CONFORMANCE_BOUNDARY,
                },
                "resource_limits": copy.deepcopy(identity["resource_limits"]),
                "dataset": {
                    key: identity["dataset"][key]
                    for key in (
                        "id",
                        "sha256",
                        "tbox_id",
                        "tbox_sha256",
                        "abox_id",
                        "abox_sha256",
                    )
                },
                "generator": {
                    key: identity["generator"][key]
                    for key in (
                        "id",
                        "revision",
                        "seed",
                        "configuration_id",
                        "configuration_sha256",
                    )
                },
                "profile_context": copy.deepcopy(identity["profile_context"]),
                "query_manifest": {
                    "path": str(query_manifest),
                    "sha256": digest(query_manifest),
                },
            }
        },
    }


def accepted_runtime_attestation(configured_path: str) -> dict[str, Any]:
    attestation_path = Path(
        configured_path
        or os.environ.get("OPENLLET_QL_RUNTIME_ATTESTATION", "")
    )
    if not str(attestation_path):
        raise OpenlletQlUnavailable("Openllet runtime attestation is not configured")
    attestation = load_json(
        require_file(attestation_path, "Openllet runtime attestation"),
        "Openllet runtime attestation",
    )
    if attestation.get("status") != "available":
        raise OpenlletQlUnavailable("Openllet runtime attestation is not available")
    runtime = attestation.get("runtime")
    classpath = attestation.get("classpath")
    if not isinstance(runtime, dict) or not isinstance(classpath, dict):
        raise OpenlletQlError("Openllet runtime attestation is incomplete")
    image = runtime.get("image")
    if (
        not isinstance(image, dict)
        or image.get("digest") != EXPECTED_IMAGE_DIGEST
        or image.get("local_attestation") != "matched"
        or runtime.get("variant_revision") != EXPECTED_RUNTIME_REVISION
        or classpath.get("jar_count") != EXPECTED_CLASSPATH_JARS
    ):
        raise OpenlletQlError("Openllet runtime attestation does not match the accepted variant")
    image_reference = str(image.get("reference") or "")
    if "@sha256:" not in image_reference or EXPECTED_IMAGE_DIGEST not in image_reference:
        raise OpenlletQlError(
            "Openllet runtime image reference is not pinned to the accepted digest"
        )
    return attestation


def runtime_version(args: argparse.Namespace) -> int:
    attestation = accepted_runtime_attestation(args.runtime_attestation)
    component = attestation.get("component")
    if (
        not isinstance(component, dict)
        or component.get("openllet_version") != EXPECTED_OPENLLET_VERSION
        or component.get("owlapi_version") != EXPECTED_OWLAPI_VERSION
    ):
        raise OpenlletQlError("Openllet component version identity is invalid")
    print(
        f"Openllet {EXPECTED_OPENLLET_VERSION} "
        f"(OWLAPI {EXPECTED_OWLAPI_VERSION}; {EXPECTED_RUNTIME_REVISION})"
    )
    return 0
