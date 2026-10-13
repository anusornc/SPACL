"""Finite source-domain witness export for Openllet QL."""

from __future__ import annotations

import os
import re
import subprocess
import sys
import uuid
from pathlib import Path
from typing import Any, Iterable

from rdf_canonicalization import Quad, Term, parse_nquads

from openllet_ql_contract import (
    OWL_EQUIVALENT_CLASS,
    OWL_EQUIVALENT_PROPERTY,
    OWL_HAS_VALUE,
    OWL_MIN_CARDINALITY,
    OWL_MIN_QUALIFIED_CARDINALITY,
    OWL_ON_PROPERTY,
    OWL_QUALIFIED_CARDINALITY,
    OWL_RESTRICTION,
    OWL_SOME_VALUES_FROM,
    RDFS,
    RDFS_SUBCLASS,
    RDFS_SUBPROPERTY,
    RDF_TYPE,
    OpenlletQlError,
    OpenlletQlTimeout,
    OpenlletQlUnavailable,
    accepted_runtime_attestation,
    require_file,
    resolve_executable,
)


def predicate_values(quads: Iterable[Quad], subject: Term, predicate: str) -> list[Term]:
    return [
        quad.object
        for quad in quads
        if quad.subject == subject and quad.predicate.value == predicate
    ]


def transitive_targets(edges: dict[str, set[str]], source: str) -> set[str]:
    reached = {source}
    pending = [source]
    while pending:
        current = pending.pop()
        for target in edges.get(current, set()):
            if target not in reached:
                reached.add(target)
                pending.append(target)
    return reached


def positive_cardinality(value: Term) -> int:
    if value.kind != "literal":
        return 0
    try:
        return int(value.value)
    except ValueError:
        return 0


def reject_unbounded_witness_dependencies(source: Path, query_iris_path: Path) -> None:
    """Fail closed for direct query-relevant existential witness requirements.

    The check deliberately handles the named-owner QL form at the adapter
    boundary. Complex anonymous class expressions remain the reasoner's input,
    but no generated term is ever exported; the independent validator still
    rejects every output term outside the declared source domain.
    """

    try:
        quads = parse_nquads(source.read_text(encoding="utf-8"))
    except (OSError, UnicodeDecodeError, ValueError) as error:
        raise OpenlletQlError(f"source graph is malformed: {error}") from error
    query_values = {
        line.strip()
        for line in query_iris_path.read_text(encoding="utf-8").splitlines()
        if line.strip() and not line.lstrip().startswith("#")
    }
    subclass_edges: dict[str, set[str]] = {}
    subproperty_edges: dict[str, set[str]] = {}
    types: dict[Term, set[str]] = {}
    assertions: dict[tuple[Term, str], list[Term]] = {}
    schema_predicates = {
        RDF_TYPE,
        RDFS_SUBCLASS,
        RDFS_SUBPROPERTY,
        OWL_EQUIVALENT_CLASS,
        OWL_EQUIVALENT_PROPERTY,
        OWL_ON_PROPERTY,
        OWL_SOME_VALUES_FROM,
        OWL_HAS_VALUE,
        OWL_MIN_CARDINALITY,
        OWL_MIN_QUALIFIED_CARDINALITY,
        OWL_QUALIFIED_CARDINALITY,
    }
    for quad in quads:
        if (
            quad.predicate.value == RDFS_SUBCLASS
            and quad.subject.kind == "iri"
            and quad.object.kind == "iri"
        ):
            subclass_edges.setdefault(quad.subject.value, set()).add(quad.object.value)
        elif (
            quad.predicate.value == OWL_EQUIVALENT_CLASS
            and quad.subject.kind == "iri"
            and quad.object.kind == "iri"
        ):
            subclass_edges.setdefault(quad.subject.value, set()).add(quad.object.value)
            subclass_edges.setdefault(quad.object.value, set()).add(quad.subject.value)
        elif (
            quad.predicate.value == RDFS_SUBPROPERTY
            and quad.subject.kind == "iri"
            and quad.object.kind == "iri"
        ):
            subproperty_edges.setdefault(quad.subject.value, set()).add(quad.object.value)
        elif (
            quad.predicate.value == OWL_EQUIVALENT_PROPERTY
            and quad.subject.kind == "iri"
            and quad.object.kind == "iri"
        ):
            subproperty_edges.setdefault(quad.subject.value, set()).add(quad.object.value)
            subproperty_edges.setdefault(quad.object.value, set()).add(quad.subject.value)
        elif quad.predicate.value == RDF_TYPE and quad.object.kind == "iri":
            types.setdefault(quad.subject, set()).add(quad.object.value)
        elif quad.predicate.value not in schema_predicates:
            assertions.setdefault((quad.subject, quad.predicate.value), []).append(quad.object)

    restriction_nodes = {
        quad.subject
        for quad in quads
        if quad.predicate.value == RDF_TYPE
        and quad.object.kind == "iri"
        and quad.object.value == OWL_RESTRICTION
    }
    for restriction in restriction_nodes:
        properties = predicate_values(quads, restriction, OWL_ON_PROPERTY)
        if len(properties) != 1 or properties[0].kind != "iri":
            continue
        property_iri = properties[0].value
        fillers = predicate_values(quads, restriction, OWL_SOME_VALUES_FROM)
        minimum = 1 if fillers else 0
        for cardinality_predicate in (
            OWL_MIN_CARDINALITY,
            OWL_MIN_QUALIFIED_CARDINALITY,
            OWL_QUALIFIED_CARDINALITY,
        ):
            for value in predicate_values(quads, restriction, cardinality_predicate):
                minimum = max(minimum, positive_cardinality(value))
        if minimum <= 0:
            continue
        filler = fillers[0] if fillers else None
        filler_queried = (
            filler is not None
            and filler.kind == "iri"
            and filler.value in query_values
        )
        if property_iri not in query_values and not filler_queried:
            continue
        owners = {
            quad.subject.value
            for quad in quads
            if quad.subject.kind == "iri"
            and quad.object == restriction
            and quad.predicate.value in {RDFS_SUBCLASS, OWL_EQUIVALENT_CLASS}
        }
        if not owners:
            continue
        subjects = {
            subject
            for subject, asserted_types in types.items()
            if any(
                owner in transitive_targets(subclass_edges, asserted)
                for asserted in asserted_types
                for owner in owners
            )
        }
        source_properties = {
            candidate
            for candidate in {predicate for _, predicate in assertions}
            if property_iri in transitive_targets(subproperty_edges, candidate)
        }
        source_properties.add(property_iri)
        for subject in subjects:
            values = {
                value
                for candidate_property in source_properties
                for value in assertions.get((subject, candidate_property), [])
            }
            if filler is not None and filler.kind == "iri":
                if any(value.kind == "literal" for value in values):
                    values = {
                        value
                        for value in values
                        if value.kind == "literal"
                        and (
                            filler.value == RDFS + "Literal"
                            or value.datatype == filler.value
                        )
                    }
                else:
                    values = {
                        value
                        for value in values
                        if any(
                            filler.value in transitive_targets(subclass_edges, asserted)
                            for asserted in types.get(value, set())
                        )
                    }
            if len(values) < minimum:
                raise OpenlletQlUnavailable(
                    "query dependency requires a generated anonymous witness for "
                    + property_iri
                )


def merge_source_and_inferences(source: Path, inferred: bytes) -> bytes:
    try:
        source_text = source.read_text(encoding="utf-8")
        inferred_text = inferred.decode("utf-8")
        source_quads = parse_nquads(source_text)
        inferred_quads = parse_nquads(inferred_text)
    except (OSError, UnicodeDecodeError, ValueError) as error:
        raise OpenlletQlError(f"Openllet materialization graph is malformed: {error}") from error
    source_terms = {current for quad in source_quads for current in quad.terms()}
    inferred_terms = {current for quad in inferred_quads for current in quad.terms()}
    extra_terms = inferred_terms - source_terms
    if extra_terms:
        if any(current.kind == "blank_node" for current in extra_terms):
            raise OpenlletQlUnavailable(
                "Openllet generated an anonymous witness outside the source domain"
            )
        raise OpenlletQlUnavailable(
            "Openllet emitted a witness term outside the declared source domain"
        )
    lines = {
        line.strip()
        for line in (source_text + "\n" + inferred_text).splitlines()
        if line.strip()
    }
    return ("\n".join(sorted(lines)) + "\n").encode("utf-8")


def docker_memory(value: Any) -> str:
    text = str(value)
    match = re.fullmatch(r"([0-9]+)(?:GiB|GB|g)", text, re.IGNORECASE)
    return f"{match.group(1)}g" if match else text


def export_graph(args: argparse.Namespace) -> int:
    source = require_file(args.source, "Openllet QL source graph")
    query_iris_path = Path(
        args.query_iris or os.environ.get("OPENLLET_QL_QUERY_IRIS", "")
    )
    if not str(query_iris_path):
        raise OpenlletQlUnavailable("Openllet query IRI scope is not configured")
    query_path = require_file(query_iris_path, "Openllet query IRI scope")
    attestation = accepted_runtime_attestation(args.runtime_attestation)
    runtime = attestation["runtime"]
    image = runtime["image"]
    image_reference = str(image["reference"])
    reject_unbounded_witness_dependencies(source, query_path)

    docker = resolve_executable(args.docker_bin, "Docker")
    resources = attestation.get("resource_limits")
    if not isinstance(resources, dict):
        raise OpenlletQlError("Openllet runtime resource limits are missing")
    timeout_seconds = resources.get("timeout_seconds")
    if (
        isinstance(timeout_seconds, bool)
        or not isinstance(timeout_seconds, int)
        or timeout_seconds <= 0
    ):
        raise OpenlletQlError("Openllet runtime timeout is invalid")
    container_name = f"tableauxx-openllet-ql-{uuid.uuid4().hex[:12]}"
    command = [
        str(docker),
        "run",
        "--rm",
        "--name",
        container_name,
        "--platform",
        str(image.get("platform") or "linux/amd64"),
        "--user",
        f"{os.getuid()}:{os.getgid()}",
        "--cpus",
        str(resources.get("cpu") or "1"),
        "--memory",
        docker_memory(resources.get("memory") or "24GiB"),
        "--network",
        "none",
        "--read-only",
        "--tmpfs",
        "/tmp:rw,noexec,nosuid,size=64m",
        "--env",
        "JAVA_TOOL_OPTIONS=-Xmx20g -XX:ActiveProcessorCount=1",
        "--volume",
        f"{source}:/input/source.nq:ro",
        "--volume",
        f"{query_path}:/input/query-iris.txt:ro",
        image_reference,
        "/input/source.nq",
        "/input/query-iris.txt",
    ]
    try:
        result = subprocess.run(
            command,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            timeout=timeout_seconds,
            check=False,
        )
    except subprocess.TimeoutExpired as error:
        subprocess.run(
            [str(docker), "rm", "--force", container_name],
            stdout=subprocess.DEVNULL,
            stderr=subprocess.DEVNULL,
            check=False,
        )
        raise OpenlletQlTimeout("Openllet QL materialization timed out") from error
    if result.returncode != 0:
        diagnostic = result.stderr.decode("utf-8", errors="replace").strip()
        raise OpenlletQlError(
            f"Openllet QL materializer failed with status {result.returncode}: {diagnostic}"
        )
    sys.stdout.buffer.write(merge_source_and_inferences(source, result.stdout))
    if result.stderr:
        sys.stderr.buffer.write(result.stderr)
    return 0
