#!/usr/bin/env python3
"""GraphDB 11.5.0 OWL2Bench QL materialization/export adapter.

The executable boundary is compatible with profile_materialization.py:

    export <source.nq>

It also provides helper commands to generate the profile identity and an
optional component manifest. The adapter never creates or approves answer keys.
When Docker, the pinned image, the runtime license, or the RDF4J export path is
absent, it returns the existing profile-materializer not_available status 125.
Deadline exhaustion is reported as timeout status 124.
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
from typing import Any
from urllib.parse import quote

from graphdb_graph_filter import merge_graphs
from resource_variant import resource_variant
from runtime_license import license_identity


SCRIPT_DIR = Path(__file__).resolve().parent
PROFILE = "QL"
GENERIC_COMPONENT_NAME = "QL-ruleset-engine"
ADAPTER_NAME = "graphdb-ql"
GRAPHDB_VERSION = "11.5.0"
GRAPHDB_TAG = "ontotext/graphdb:11.5.0"
GRAPHDB_PLATFORM = "linux/amd64"
GRAPHDB_IMAGE_DIGEST = (
    "sha256:d688879c4e5751a0c323f996d86bf1b2189191290711b1936e211d4510b53485"
)
GRAPHDB_IMAGE = f"ontotext/graphdb@{GRAPHDB_IMAGE_DIGEST}"
RULESET_ID = "owl2-ql"
NOT_AVAILABLE_EXIT_CODE = 125
UNSUPPORTED_EXIT_CODE = 123
TIMEOUT_EXIT_CODE = 124
EXPECTED_QL_QUERY_COUNT = 10
BOUNDARY_TEXT = (
    "GraphDB 11.5 owl2-ql predefined ruleset; GraphDB documentation records "
    "OWL2-QL non-conformance, so materialized output is differential execution "
    "evidence only and oracle_unavailable."
)
DOC_URLS = {
    "reasoning": "https://graphdb.ontotext.com/documentation/11.5/reasoning.html",
    "repository_api": "https://graphdb.ontotext.com/documentation/11.5/using-the-graphdb-rest-api.html",
    "rdf4j_api": "https://graphdb.ontotext.com/documentation/11.5/rdf4j-rest-api.html",
    "docker_hub": "https://hub.docker.com/r/ontotext/graphdb/tags/",
}


class GraphDBQLUnavailable(RuntimeError):
    """Runtime or product boundary is absent in the current environment."""


class GraphDBQLTimeout(RuntimeError):
    """A GraphDB operation exceeded its declared deadline."""


class GraphDBQLError(RuntimeError):
    """The GraphDB adapter failed after runtime availability was established."""


def sha256_bytes(data: bytes) -> str:
    return "sha256:" + hashlib.sha256(data).hexdigest()


def file_digest(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return "sha256:" + digest.hexdigest()


def write_json(path: Path, value: dict[str, Any]) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(
        json.dumps(value, ensure_ascii=False, indent=2, sort_keys=True) + "\n",
        encoding="utf-8",
    )


def nonempty(value: Any) -> bool:
    return isinstance(value, str) and bool(value.strip())


def require_file(path: Path, label: str) -> Path:
    resolved = path.expanduser().resolve()
    if not resolved.is_file():
        raise GraphDBQLUnavailable(f"{label} is missing: {resolved}")
    return resolved


def current_git_revision() -> str:
    try:
        return subprocess.check_output(
            ["git", "-C", str(SCRIPT_DIR), "rev-parse", "HEAD"],
            text=True,
            stderr=subprocess.DEVNULL,
        ).strip()
    except (OSError, subprocess.CalledProcessError):
        return "unavailable"


def repository_id_for(source: Path) -> str:
    return "owl2bench-ql-" + file_digest(source).removeprefix("sha256:")[:16]


def render_repository_config(repository_id: str) -> str:
    template = require_file(SCRIPT_DIR / "graphdb_ql_repository.ttl", "GraphDB repository template")
    return template.read_text(encoding="utf-8").replace("{{REPOSITORY_ID}}", repository_id)


def write_repository_config(output_dir: Path, repository_id: str) -> Path:
    config_path = output_dir / "graphdb_ql_repository.ttl"
    config_path.parent.mkdir(parents=True, exist_ok=True)
    config_path.write_text(render_repository_config(repository_id), encoding="utf-8")
    return config_path


def load_dataset_root(value: str | None) -> Path:
    if value:
        return Path(value).expanduser().resolve()
    raise GraphDBQLUnavailable("dataset root is not supplied; pass --dataset-root or set OWL2BENCH_PROFILE_DATASET_ROOT")


def ql_paths(dataset_root: Path) -> dict[str, Path]:
    profile_dir = dataset_root / "profiles" / PROFILE
    generator_dir = dataset_root / "generator"
    return {
        "profile_dir": profile_dir,
        "source": profile_dir / "source.nq",
        "dataset": profile_dir / "dataset.owl",
        "tbox": profile_dir / "profile-tbox.owl",
        "abox": profile_dir / "abox.nq",
        "profile_identity": profile_dir / "profile-identity.json",
        "generator": generator_dir / "OWL2Bench.jar",
        "generator_config": generator_dir / "config.properties",
    }


def load_profile_identity(path: Path) -> dict[str, Any]:
    if not path.is_file():
        raise GraphDBQLUnavailable(f"frozen QL profile identity is missing: {path}")
    try:
        value = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, UnicodeDecodeError, json.JSONDecodeError) as error:
        raise GraphDBQLUnavailable(f"frozen QL profile identity is malformed: {error}") from error
    if not isinstance(value, dict):
        raise GraphDBQLUnavailable("frozen QL profile identity root must be an object")
    if value.get("profile") != PROFILE or value.get("status") != "complete":
        raise GraphDBQLUnavailable("frozen QL profile identity must be a complete QL record")
    dataset = value.get("dataset")
    if not isinstance(dataset, dict):
        raise GraphDBQLUnavailable("frozen QL profile identity dataset is missing")
    for field in (
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
    ):
        if not nonempty(dataset.get(field)):
            raise GraphDBQLUnavailable(f"frozen QL dataset.{field} is missing")
    if dataset.get("profile") != PROFILE:
        raise GraphDBQLUnavailable("frozen QL dataset profile does not match QL")
    generator = value.get("generator")
    if not isinstance(generator, dict):
        raise GraphDBQLUnavailable("frozen QL generator identity is missing")
    for field in ("id", "revision", "path", "sha256"):
        if not nonempty(generator.get(field)):
            raise GraphDBQLUnavailable(f"frozen QL generator.{field} is missing")
    generator_configuration = value.get("generator_configuration")
    if not isinstance(generator_configuration, dict):
        raise GraphDBQLUnavailable("frozen QL generator configuration identity is missing")
    for field in ("id", "path", "revision", "sha256"):
        if not nonempty(generator_configuration.get(field)):
            raise GraphDBQLUnavailable(f"frozen QL generator_configuration.{field} is missing")
    context = value.get("profile_context")
    if not isinstance(context, dict) or not nonempty(context.get("id")) or not nonempty(context.get("directory")):
        raise GraphDBQLUnavailable("frozen QL profile context identity is missing")
    return value


def collect_ql_queries(query_manifest: Path) -> list[dict[str, Any]]:
    manifest = require_file(query_manifest, "query manifest")
    lines = manifest.read_text(encoding="utf-8").splitlines()
    if not lines:
        raise GraphDBQLError("query manifest is empty")
    header = lines[0].split("\t")
    try:
        path_index = header.index("staged_query")
        query_id_index = header.index("query_id")
        profile_index = header.index("profile")
    except ValueError as error:
        raise GraphDBQLError("query manifest is missing required columns") from error
    queries: list[dict[str, Any]] = []
    for line_number, line in enumerate(lines[1:], start=2):
        if not line:
            continue
        fields = line.split("\t")
        if len(fields) != len(header):
            raise GraphDBQLError(f"query manifest row {line_number} has the wrong field count")
        profiles = {part.strip().upper() for part in fields[profile_index].split(",")}
        if PROFILE not in profiles:
            continue
        query_path = require_file(Path(fields[path_index]), f"QL query row {line_number}")
        queries.append(
            {
                "id": fields[query_id_index],
                "path": str(query_path),
                "sha256": file_digest(query_path),
                "manifest_row": line_number,
            }
        )
    if len(queries) != EXPECTED_QL_QUERY_COUNT:
        raise GraphDBQLError(
            f"query manifest must provide {EXPECTED_QL_QUERY_COUNT} QL rows, found {len(queries)}"
        )
    return queries


def build_identity(args: argparse.Namespace) -> dict[str, Any]:
    dataset_root = load_dataset_root(args.dataset_root)
    paths = ql_paths(dataset_root)
    for label, path in paths.items():
        if label in {"profile_dir"}:
            if not path.is_dir():
                raise GraphDBQLUnavailable(f"QL profile directory is missing: {path}")
            continue
        require_file(path, label)
    output_dir = args.output.parent.resolve()
    repository_id = args.repository_id or repository_id_for(paths["source"])
    config_path = write_repository_config(output_dir, repository_id)
    exporter = require_file(Path(args.exporter), "GraphDB QL exporter")
    query_backend = exporter
    profile_identity = load_profile_identity(paths["profile_identity"])
    git_revision = current_git_revision()
    source_hash = file_digest(paths["source"])
    generated_dataset_hash = file_digest(paths["dataset"])
    tbox_hash = file_digest(paths["tbox"])
    abox_hash = file_digest(paths["abox"])
    generator_hash = file_digest(paths["generator"])
    generator_config_hash = file_digest(paths["generator_config"])
    config_hash = file_digest(config_path)
    exporter_hash = file_digest(exporter)
    queries = collect_ql_queries(args.query_manifest)
    dataset_record = profile_identity["dataset"]
    generator_record = profile_identity["generator"]
    generator_configuration_record = profile_identity["generator_configuration"]
    expected_source_path = Path(dataset_record["artifact_path"]).expanduser().resolve()
    if expected_source_path != paths["source"]:
        raise GraphDBQLUnavailable("frozen QL conversion artifact path does not match source.nq")
    if dataset_record["sha256"] != source_hash:
        raise GraphDBQLUnavailable("frozen QL dataset hash does not match source.nq")
    if dataset_record["source_ontology_sha256"] != generated_dataset_hash:
        raise GraphDBQLUnavailable("frozen QL generated dataset hash does not match dataset.owl")
    if dataset_record["tbox_sha256"] != tbox_hash:
        raise GraphDBQLUnavailable("frozen QL dataset TBox hash does not match profile-tbox.owl")
    if dataset_record["abox_sha256"] != abox_hash:
        raise GraphDBQLUnavailable("frozen QL dataset ABox hash does not match abox.owl")
    if generator_record["sha256"] != generator_hash:
        raise GraphDBQLUnavailable("frozen QL generator hash does not match OWL2Bench.jar")
    if generator_configuration_record["sha256"] != generator_config_hash:
        raise GraphDBQLUnavailable("frozen QL generator configuration hash does not match config.properties")
    frozen_context = profile_identity["profile_context"]
    if Path(str(frozen_context["directory"])).expanduser().resolve() != paths["profile_dir"]:
        raise GraphDBQLUnavailable("frozen QL profile context directory does not match the dataset root")
    seed = dataset_record.get("seed")
    if isinstance(seed, bool) or not isinstance(seed, int) or seed < 0:
        raise GraphDBQLUnavailable("frozen QL generator seed is invalid")
    resource_limits = {
        "timeout_seconds": args.timeout_seconds,
        "memory": args.memory,
        "threads": args.threads,
        "cpu": args.cpu,
        "disk": args.disk,
        "network": "local-only",
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
    identity = {
        "profile": PROFILE,
        "component": {
            "name": GENERIC_COMPONENT_NAME,
            "version": f"graphdb-{GRAPHDB_VERSION}",
            "revision": git_revision,
            "artifact_digest": exporter_hash,
            "path": str(exporter),
            "engine": {
                "name": "GraphDB",
                "version": GRAPHDB_VERSION,
                "image": GRAPHDB_TAG,
                "image_reference": GRAPHDB_IMAGE,
                "image_digest": GRAPHDB_IMAGE_DIGEST,
                "image_platform": GRAPHDB_PLATFORM,
            },
        },
        "exporter": {
            "name": "graphdb-ql-rdf4j-exporter",
            "version": GRAPHDB_VERSION,
            "revision": git_revision,
            "artifact_digest": exporter_hash,
            "path": str(exporter),
            "contract": "export <source.nq> writes inferred RDF4J statements as N-Quads to stdout",
        },
        "query_backend": {
            "name": "graphdb-rdf4j-statements-export",
            "version": GRAPHDB_VERSION,
            "revision": git_revision,
            "artifact_digest": file_digest(query_backend),
            "path": str(query_backend),
            "read_only": True,
            "contract": "offline importrdf load from the pinned repository config, then RDF4J GET /repositories/{id}/statements?includeInferred=true with Accept: application/n-quads",
        },
        "configuration": {
            "id": f"graphdb-ql-repository:{repository_id}",
            "path": str(config_path),
            "sha256": config_hash,
            "digest": config_hash,
            "repository_id": repository_id,
            "repository_template": str((SCRIPT_DIR / "graphdb_ql_repository.ttl").resolve()),
            "ruleset": RULESET_ID,
        },
        "ruleset": {
            "id": RULESET_ID,
            "version": f"GraphDB {GRAPHDB_VERSION}",
            "revision": "GraphDB 11.5 predefined rulesets",
            "artifact_digest": GRAPHDB_IMAGE_DIGEST,
            "conformance_boundary": BOUNDARY_TEXT,
            "product_limitation": "GraphDB documentation records OWL2-QL non-conformance",
        },
        "derivation": {
            "version": "graphdb-ql-materialization/1",
            "revision": git_revision,
            "source_included": True,
            "anonymous_witness_policy": "not_exported",
            "oracle_policy": "oracle_unavailable",
        },
        "runtime": {
            "id": f"docker:{GRAPHDB_IMAGE}:{GRAPHDB_PLATFORM}",
            "dependencies": [
                {
                    "name": "GraphDB Docker image",
                    "version": GRAPHDB_VERSION,
                    "digest": GRAPHDB_IMAGE_DIGEST,
                    "image": GRAPHDB_TAG,
                    "reference": GRAPHDB_IMAGE,
                    "platform": GRAPHDB_PLATFORM,
                }
            ],
            "docker": {
                "image": GRAPHDB_TAG,
                "image_reference": GRAPHDB_IMAGE,
                "image_digest": GRAPHDB_IMAGE_DIGEST,
                "platform": GRAPHDB_PLATFORM,
                "pull_policy": "never unless GRAPHDB_QL_ALLOW_PULL=1",
            },
        },
        "resource_limits": resource_limits,
        "profile_context": {
            "id": str(frozen_context["id"]),
            "directory": str(frozen_context["directory"]),
            "profile": PROFILE,
        },
        "source_document": {
            "path": str(paths["source"]),
            "sha256": source_hash,
            "revision": "721cb0c dataset freezer output",
        },
        "dataset": {
            "profile": PROFILE,
            "id": str(dataset_record.get("id") or f"owl2bench-ql:{generated_dataset_hash}"),
            "artifact_id": str(dataset_record.get("artifact_id") or f"dataset:{generated_dataset_hash}"),
            "artifact_path": str(dataset_record["artifact_path"]),
            "artifact_revision": str(dataset_record["artifact_revision"]),
            "sha256": source_hash,
            "source_ontology_sha256": dataset_record["source_ontology_sha256"],
            "tbox_id": str(dataset_record.get("tbox_id") or f"tbox:{tbox_hash}"),
            "tbox_sha256": tbox_hash,
            "abox_id": str(dataset_record.get("abox_id") or f"abox:{abox_hash}"),
            "abox_sha256": abox_hash,
        },
        "dataset_artifact": {
            "id": str(dataset_record["artifact_id"]),
            "path": str(paths["source"]),
            "sha256": source_hash,
            "revision": str(dataset_record["artifact_revision"]),
        },
        "generated_dataset": {
            "id": f"generated-dataset:{generated_dataset_hash}",
            "path": str(paths["dataset"]),
            "sha256": generated_dataset_hash,
            "revision": "721cb0c dataset freezer output",
        },
        "profile_tbox": {
            "path": str(paths["tbox"]),
            "sha256": tbox_hash,
            "revision": "721cb0c dataset freezer output",
        },
        "profile_abox": {
            "path": str(paths["abox"]),
            "sha256": abox_hash,
            "revision": "721cb0c dataset freezer output",
        },
        "generator": {
            "id": str(generator_record.get("id") or "OWL2Bench"),
            "revision": str(generator_record.get("revision") or "721cb0c dataset freezer output"),
            "seed": seed,
            "artifact_path": str(paths["generator"]),
            "artifact_digest": generator_hash,
            "configuration_id": "OWL2Bench config.properties",
            "configuration_sha256": generator_config_hash,
        },
        "generator_configuration": {
            "path": str(paths["generator_config"]),
            "sha256": generator_config_hash,
            "revision": "721cb0c dataset freezer output",
        },
        "implementation": {
            "path": str(Path(__file__).resolve()),
            "sha256": file_digest(Path(__file__).resolve()),
            "revision": git_revision,
        },
        "query_workload": {
            "profile": PROFILE,
            "query_count": EXPECTED_QL_QUERY_COUNT,
            "queries": queries,
            "source_manifest": str(require_file(args.query_manifest, "query manifest")),
            "oracle_policy": "no_answer_key_oracle",
        },
        "credentials": {"status": "not_required"},
        "license": license_identity(),
        "profile_capability_boundary": BOUNDARY_TEXT,
        "ruleset_required": True,
        "two_run_determinism_required": True,
        "graphdb": {
            "image": GRAPHDB_TAG,
            "image_reference": GRAPHDB_IMAGE,
            "image_digest": GRAPHDB_IMAGE_DIGEST,
            "image_platform": GRAPHDB_PLATFORM,
            "repository_id": repository_id,
            "ruleset": RULESET_ID,
            "documentation": DOC_URLS,
            "historical_graphdb_claim": "not_used",
        },
    }
    return identity


def component_manifest_from_identity(identity: dict[str, Any], query_manifest: Path) -> dict[str, Any]:
    materializer = {
        key: identity["component"][key]
        for key in ("name", "version", "revision", "artifact_digest")
    }
    exporter = {
        key: identity["exporter"][key]
        for key in ("name", "version", "revision", "artifact_digest")
    }
    backend = {
        key: identity["query_backend"][key]
        for key in ("name", "version", "revision", "artifact_digest")
    }
    return {
        "schema_version": 1,
        "manifest_id": "graphdb-ql-component-manifest",
        "matrix": {
            "expansion": "query_profile",
            "expected_query_count": 60,
            "expected_profile_case_counts": {"DL": 22, "EL": 10, "QL": 10, "RL": 18},
        },
        "profiles": {
            PROFILE: {
                "components": {
                    "engine": exporter,
                    "materializer": materializer,
                    "query": backend,
                },
                "ruleset": {
                    key: identity["ruleset"][key]
                    for key in ("id", "version", "revision", "artifact_digest", "conformance_boundary")
                },
                "configuration": {
                    "id": identity["configuration"]["id"],
                    "path": identity["configuration"]["path"],
                    "digest": identity["configuration"]["sha256"],
                },
                "runtime": identity["runtime"],
                "credentials": {"status": "not_required"},
                "license": identity["license"],
                "capability": {
                    "status": "supported",
                    "supports_query": True,
                    "boundary": BOUNDARY_TEXT,
                },
                "resource_limits": identity["resource_limits"],
                "dataset": {
                    "id": identity["dataset"]["id"],
                    "sha256": identity["dataset"]["sha256"],
                    "tbox_id": identity["dataset"]["tbox_id"],
                    "tbox_sha256": identity["dataset"]["tbox_sha256"],
                    "abox_id": identity["dataset"]["abox_id"],
                    "abox_sha256": identity["dataset"]["abox_sha256"],
                },
                "generator": {
                    "id": identity["generator"]["id"],
                    "revision": identity["generator"]["revision"],
                    "seed": identity["generator"]["seed"],
                    "configuration_id": identity["generator"]["configuration_id"],
                    "configuration_sha256": identity["generator"]["configuration_sha256"],
                },
                "profile_context": identity["profile_context"],
                "query_manifest": {
                    "path": str(query_manifest.resolve()),
                    "sha256": file_digest(query_manifest.resolve()),
                },
            }
        },
    }


def docker_available(docker_bin: str) -> str:
    resolved = shutil.which(docker_bin) if os.path.basename(docker_bin) == docker_bin else docker_bin
    if not resolved or not Path(resolved).is_file():
        raise GraphDBQLUnavailable(f"docker executable is missing: {docker_bin}")
    return str(Path(resolved).resolve())


def run_checked(command: list[str], timeout_seconds: int) -> subprocess.CompletedProcess[str]:
    try:
        return subprocess.run(
            command,
            text=True,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            check=False,
            timeout=timeout_seconds,
        )
    except subprocess.TimeoutExpired as error:
        raise GraphDBQLTimeout(f"command timed out: {' '.join(command)}") from error


def ensure_image(docker_bin: str, timeout_seconds: int) -> None:
    inspect = run_checked(
        [docker_bin, "image", "inspect", GRAPHDB_IMAGE],
        timeout_seconds,
    )
    if inspect.returncode != 0 and os.environ.get("GRAPHDB_QL_ALLOW_PULL") == "1":
        pull = run_checked(
            [
                docker_bin,
                "pull",
                "--platform",
                GRAPHDB_PLATFORM,
                GRAPHDB_IMAGE,
            ],
            timeout_seconds,
        )
        if pull.returncode != 0:
            raise GraphDBQLUnavailable("pinned GraphDB image pull failed: " + pull.stderr.strip())
        inspect = run_checked([docker_bin, "image", "inspect", GRAPHDB_IMAGE], timeout_seconds)
    if inspect.returncode != 0:
        raise GraphDBQLUnavailable(
            f"pinned GraphDB image is not available locally: {GRAPHDB_IMAGE}"
        )


def offline_import_export(
    docker_bin: str,
    source: Path,
    config_text: str,
    repository_id: str,
    timeout_seconds: int,
    memory: str,
    cpus: float,
) -> bytes:
    """One container, the route the free runtime actually serves.

    GraphDB 11.5.0's free mode gates the repository-creation REST
    endpoint behind a license ("No license was set"), while the
    offline ``importrdf load`` tool and the statements export are
    served openly — the route the RL variant proved end to end. The
    repository is built offline from the pinned QL config, the server
    boots against that data directory, and the full graph (explicit
    plus inferred) streams out as N-Quads.
    """
    tmp_dir = Path(tempfile.mkdtemp(prefix="tableauxx-graphdb-ql."))
    quoted_repository_id = quote(repository_id, safe="")
    inner = f"""
set -euo pipefail
export GRAPHDB_HOME_DATA=/work/data
mkdir -p /work/data /work/tmp
/opt/graphdb/dist/bin/importrdf \
  -Dgraphdb.home.data=/work/data \
  -Djava.io.tmpdir=/work/tmp \
  load -c /work/repository-config.ttl -m parallel /work/source.nq \
  >/work/importrdf.log 2>&1
/opt/graphdb/dist/bin/graphdb \
  -Dgraphdb.home.data=/work/data \
  -Dgraphdb.home=/work/home \
  -Dgraphdb.workbench.importDirectory=/work/import \
  >/work/graphdb.log 2>&1 &
graphdb_pid="$!"
trap "kill \"$graphdb_pid\" >/dev/null 2>&1 || true" EXIT
for _ in $(seq 1 120); do
  if curl -fsS "http://127.0.0.1:7200/rest/repositories" >/dev/null 2>&1; then
    break
  fi
  sleep 1
done
curl -fsS \
  -H "Accept: application/n-quads" \
  "http://127.0.0.1:7200/repositories/{quoted_repository_id}/statements?includeInferred=true"
"""
    try:
        (tmp_dir / "source.nq").write_bytes(source.read_bytes())
        (tmp_dir / "repository-config.ttl").write_text(config_text, encoding="utf-8")
        command = [
            docker_bin,
            "run",
            "--rm",
            "--platform",
            GRAPHDB_PLATFORM,
            "--memory",
            memory,
            "--cpus",
            str(cpus),
            "--entrypoint",
            "/bin/bash",
            "--volume",
            f"{tmp_dir}:/work:rw",
            GRAPHDB_IMAGE,
            "-lc",
            inner,
        ]
        try:
            finished = subprocess.run(
                command,
                capture_output=True,
                timeout=timeout_seconds,
                check=False,
                text=True,
            )
        except subprocess.TimeoutExpired as error:
            raise GraphDBQLTimeout(
                f"GraphDB offline import/export exceeded {timeout_seconds}s"
            ) from error
        if finished.returncode != 0:
            details = (finished.stderr or "") + "\n"
            for log in ("importrdf.log", "graphdb.log"):
                details += (tmp_dir / log).read_text(encoding="utf-8", errors="replace")[:2000]
            # The import and server output land in the redirected logs,
            # not on stderr — the license scan must read both (the RL
            # variant's contains_license_error mirror).
            if any(word in details.lower() for word in ("license", "licence", "not authorized", "authorization")):
                raise GraphDBQLUnavailable(
                    "GraphDB runtime license or authorization is unavailable: "
                    + details[:1000]
                )
            raise GraphDBQLError(
                "GraphDB offline import/export failed: " + details[:2000]
            )
        payload = finished.stdout.encode("utf-8") if isinstance(finished.stdout, str) else finished.stdout
        if not payload.strip():
            raise GraphDBQLError("GraphDB statement export returned an empty graph")
        return payload
    finally:
        shutil.rmtree(tmp_dir, ignore_errors=True)


def export_graph(args: argparse.Namespace) -> int:
    docker_bin = docker_available(args.docker_bin)
    ensure_image(docker_bin, args.timeout_seconds)
    source = require_file(args.source, "source graph")
    repository_id = args.repository_id or repository_id_for(source)
    config_text = render_repository_config(repository_id)
    exported = offline_import_export(
        docker_bin,
        source,
        config_text,
        repository_id,
        args.timeout_seconds,
        args.memory,
        args.cpus,
    )
    sys.stdout.buffer.write(merge_graphs(source, exported.decode("utf-8")))
    return 0


def command_identity(args: argparse.Namespace) -> int:
    identity = build_identity(args)
    write_json(args.output.resolve(), identity)
    if args.component_manifest_output:
        write_json(
            args.component_manifest_output.resolve(),
            component_manifest_from_identity(identity, require_file(args.query_manifest, "query manifest")),
        )
    print(json.dumps({"status": "ready", "identity": str(args.output.resolve())}, sort_keys=True))
    return 0


def command_component_manifest(args: argparse.Namespace) -> int:
    identity = json.loads(require_file(args.identity, "GraphDB QL identity").read_text(encoding="utf-8"))
    write_json(
        args.output.resolve(),
        component_manifest_from_identity(identity, require_file(args.query_manifest, "query manifest")),
    )
    print(json.dumps({"status": "ready", "component_manifest": str(args.output.resolve())}, sort_keys=True))
    return 0


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    subparsers = parser.add_subparsers(dest="command", required=True)

    export_parser = subparsers.add_parser("export", help="export GraphDB QL materialization as N-Quads")
    export_parser.add_argument("source", type=Path)
    export_parser.add_argument("--docker-bin", default=os.environ.get("GRAPHDB_QL_DOCKER_BIN", "docker"))
    export_parser.add_argument("--repository-id", default=os.environ.get("GRAPHDB_QL_REPOSITORY_ID", ""))
    export_parser.add_argument("--timeout-seconds", type=int, default=int(os.environ.get("GRAPHDB_QL_TIMEOUT_SECONDS", "600")))
    export_parser.add_argument("--memory", default=os.environ.get("GRAPHDB_QL_MEMORY", "24GiB"))
    export_parser.add_argument("--cpus", type=float, default=float(os.environ.get("GRAPHDB_QL_CPUS", "1")))

    identity_parser = subparsers.add_parser("identity", help="write a profile_materialization.py-compatible identity")
    identity_parser.add_argument("--dataset-root", default=os.environ.get("OWL2BENCH_PROFILE_DATASET_ROOT", ""))
    identity_parser.add_argument("--query-manifest", required=True, type=Path)
    identity_parser.add_argument("--exporter", default=str(SCRIPT_DIR / "graphdb_ql_materializer.sh"))
    identity_parser.add_argument("--output", required=True, type=Path)
    identity_parser.add_argument("--component-manifest-output", type=Path)
    identity_parser.add_argument("--repository-id", default=os.environ.get("GRAPHDB_QL_REPOSITORY_ID", ""))
    identity_parser.add_argument("--timeout-seconds", type=int, default=int(os.environ.get("GRAPHDB_QL_TIMEOUT_SECONDS", "600")))
    identity_parser.add_argument("--memory", default=os.environ.get("GRAPHDB_QL_MEMORY", "24GiB"))
    identity_parser.add_argument("--threads", type=int, default=int(os.environ.get("GRAPHDB_QL_THREADS", "1")))
    identity_parser.add_argument("--cpu", default=os.environ.get("GRAPHDB_QL_CPU", "1"))
    identity_parser.add_argument("--disk", default=os.environ.get("GRAPHDB_QL_DISK", "10GiB"))
    identity_parser.add_argument("--generator-seed", type=int, default=1)

    component_parser = subparsers.add_parser("component-manifest", help="write a mergeable QL component manifest")
    component_parser.add_argument("--identity", required=True, type=Path)
    component_parser.add_argument("--query-manifest", required=True, type=Path)
    component_parser.add_argument("--output", required=True, type=Path)

    args = parser.parse_args(argv)
    try:
        if args.command == "export":
            return export_graph(args)
        if args.command == "identity":
            return command_identity(args)
        if args.command == "component-manifest":
            return command_component_manifest(args)
    except GraphDBQLTimeout as error:
        print(f"[graphdb-ql][timeout] {error}", file=sys.stderr)
        return TIMEOUT_EXIT_CODE
    except GraphDBQLUnavailable as error:
        print(f"[graphdb-ql][not_available] {error}", file=sys.stderr)
        return NOT_AVAILABLE_EXIT_CODE
    except GraphDBQLError as error:
        print(f"[graphdb-ql][failed] {error}", file=sys.stderr)
        return 1
    except (OSError, json.JSONDecodeError, ValueError) as error:
        print(f"[graphdb-ql][failed] {error}", file=sys.stderr)
        return 1
    return 2


if __name__ == "__main__":
    raise SystemExit(main())
