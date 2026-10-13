#!/usr/bin/env python3
"""Independently validate one profile materialization artifact.

Validation is structural and provenance-oriented. It never evaluates a query,
creates an answer key, or treats a materialized graph as a semantic oracle.
"""

from __future__ import annotations

import argparse
import hashlib
import json
import re
import sys
from pathlib import Path
from typing import Any, Iterable

from materialization_manifest import file_digest
from rdf_canonicalization import Quad, Term, canonicalize_file, load_nquads
from split_stack_manifest import forbidden_field_paths


PROFILES = {"DL", "EL", "QL", "RL"}
REQUIRED_ASSERTION_FAMILIES = (
    "class",
    "object_property",
    "data_property",
    "tbox",
    "rbox",
)


class ManifestMissingError(ValueError):
    """The validator input manifest is not present."""


RDF_TYPE = "http://www.w3.org/1999/02/22-rdf-syntax-ns#type"
RDFS_SUBCLASS = "http://www.w3.org/2000/01/rdf-schema#subClassOf"
RDFS_SUBPROPERTY = "http://www.w3.org/2000/01/rdf-schema#subPropertyOf"
RDFS_DOMAIN = "http://www.w3.org/2000/01/rdf-schema#domain"
RDFS_RANGE = "http://www.w3.org/2000/01/rdf-schema#range"
OWL = "http://www.w3.org/2002/07/owl#"
RDFS = "http://www.w3.org/2000/01/rdf-schema#"
RDF = "http://www.w3.org/1999/02/22-rdf-syntax-ns#"
OWL_EQUIVALENT_CLASS = OWL + "equivalentClass"
OWL_EQUIVALENT_PROPERTY = OWL + "equivalentProperty"
OWL_INVERSE_OF = OWL + "inverseOf"
OWL_OBJECT_PROPERTY = OWL + "ObjectProperty"
OWL_DATA_PROPERTY = OWL + "DatatypeProperty"
OWL_CLASS = OWL + "Class"
DEFAULT_QUERY_NAMESPACE = "https://kracr.iiitd.edu.in/OWL2Bench#"
PREFIX_DECLARATION = re.compile(
    r"(?im)^\s*PREFIX\s+([A-Za-z][A-Za-z0-9_-]*)?\s*:\s*<([^>]+)>"
)
PREFIXED_NAME = re.compile(
    r"(?<![A-Za-z0-9_])([A-Za-z][A-Za-z0-9_-]*)?:([A-Za-z_][A-Za-z0-9_.-]*)"
)
FULL_IRI = re.compile(r"<([^>]+)>")

TBOX_PREDICATES = {
    RDFS_SUBCLASS,
    RDFS_DOMAIN,
    RDFS_RANGE,
    OWL + "equivalentClass",
    OWL + "disjointWith",
    OWL + "complementOf",
    OWL + "unionOf",
    OWL + "intersectionOf",
    OWL + "oneOf",
    OWL + "onClass",
    OWL + "onDataRange",
    OWL + "onProperty",
    OWL + "someValuesFrom",
    OWL + "allValuesFrom",
    OWL + "hasValue",
    OWL + "qualifiedCardinality",
    OWL + "minQualifiedCardinality",
    OWL + "maxQualifiedCardinality",
}
RBOX_PREDICATES = {
    RDFS_SUBPROPERTY,
    OWL + "inverseOf",
    OWL + "equivalentProperty",
    OWL + "propertyDisjointWith",
    OWL + "TransitiveProperty",
    OWL + "SymmetricProperty",
    OWL + "AsymmetricProperty",
    OWL + "ReflexiveProperty",
    OWL + "IrreflexiveProperty",
    OWL + "FunctionalProperty",
    OWL + "InverseFunctionalProperty",
}
SCHEMA_PREDICATES = TBOX_PREDICATES | RBOX_PREDICATES | {
    RDF_TYPE,
    RDF + "first",
    RDF + "rest",
    RDFS + "label",
    RDFS + "comment",
}


def digest_value(value: Any) -> str:
    return str(value).strip().removeprefix("sha256:").lower()


def valid_digest(value: Any) -> bool:
    normalized = digest_value(value)
    return len(normalized) == 64 and all(character in "0123456789abcdef" for character in normalized)


def nonempty(value: Any) -> bool:
    return isinstance(value, str) and bool(value.strip())


def load_json(path: Path) -> dict[str, Any]:
    if not path.is_file():
        raise ManifestMissingError(f"manifest missing: {path.expanduser().resolve()}")
    try:
        document = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, UnicodeDecodeError, json.JSONDecodeError) as error:
        raise ValueError(f"manifest is malformed: {error}") from error
    if not isinstance(document, dict):
        raise ValueError("manifest root must be an object")
    return document


def term_domain(quads: Iterable[Quad]) -> set[Term]:
    return {term for quad in quads for term in quad.terms()}


def assertion_counts(quads: Iterable[Quad]) -> dict[str, int]:
    counts = {family: 0 for family in REQUIRED_ASSERTION_FAMILIES}
    for quad in quads:
        predicate = quad.predicate.value
        if predicate == RDF_TYPE:
            counts["class"] += 1
        if predicate in TBOX_PREDICATES:
            counts["tbox"] += 1
        if predicate in RBOX_PREDICATES:
            counts["rbox"] += 1
        if predicate not in SCHEMA_PREDICATES:
            if quad.object.kind == "literal":
                counts["data_property"] += 1
            elif quad.object.kind in {"iri", "blank_node"}:
                counts["object_property"] += 1
    return counts


def base_result(profile: str, status: str, reasons: list[str]) -> dict[str, Any]:
    return {
        "schema_version": 1,
        "validator": "owl2bench-materialization-validator/1",
        "profile": profile,
        "status": status,
        "reasons": reasons,
        "assertion_counts": {family: 0 for family in REQUIRED_ASSERTION_FAMILIES},
        "query_handoff": {
            "status": "ready" if status == "complete" else "not_ready",
            "profile": profile,
            "read_only": True,
        },
    }


def _require_identity(
    record: dict[str, Any], section: str, fields: tuple[str, ...], reasons: list[str]
) -> None:
    value = record.get(section)
    if not isinstance(value, dict):
        reasons.append(f"identity_section_missing:{section}")
        return
    for field in fields:
        field_value = value.get(field)
        if field in {"artifact_digest", "sha256"}:
            if not valid_digest(field_value):
                reasons.append(f"identity_field_invalid:{section}.{field}")
        elif not nonempty(field_value):
                reasons.append(f"identity_field_missing:{section}.{field}")


def validate_obligations(
    record: dict[str, Any],
    profile: str,
    source_path: Path,
    source_quads: Iterable[Quad],
    candidate_quads: Iterable[Quad],
    result: dict[str, Any],
) -> set[Quad] | None:
    """Check the exporter-provided finite entailment obligation graph."""

    obligations = record.get("obligations")
    if obligations is None:
        contract = record.get("graph_contract")
        if isinstance(contract, dict) and contract.get(
            "independent_obligation_artifacts_required"
        ) is True:
            result["status"] = "not_available"
            result["reasons"].append("artifact_missing:obligations_record")
        return None
    if not isinstance(obligations, dict):
        result["status"] = "failed"
        result["reasons"].append("obligations_record_invalid")
        return None
    manifest_value = obligations.get("manifest_path")
    graph_value = obligations.get("graph_path")
    if not nonempty(manifest_value) or not nonempty(graph_value):
        result["status"] = "failed"
        result["reasons"].append("obligations_paths_missing")
        return None
    manifest_path = Path(str(manifest_value)).expanduser().resolve()
    graph_path = Path(str(graph_value)).expanduser().resolve()
    if not manifest_path.is_file() or not graph_path.is_file():
        result["status"] = "not_available"
        if not manifest_path.is_file():
            result["reasons"].append(f"artifact_missing:obligations_manifest:{manifest_path}")
        if not graph_path.is_file():
            result["reasons"].append(f"artifact_missing:obligations_graph:{graph_path}")
        return None
    try:
        obligation_manifest = load_json(manifest_path)
    except ManifestMissingError as error:
        result["status"] = "not_available"
        result["reasons"].append(f"artifact_missing:obligations_manifest:{error}")
        return None
    except ValueError as error:
        result["status"] = "failed"
        result["reasons"].append(f"obligations_manifest_malformed:{error}")
        return None
    forbidden = forbidden_field_paths(obligation_manifest)
    if forbidden:
        result["status"] = "failed"
        result["reasons"].extend(f"forbidden_field:obligations.{path}" for path in forbidden)
        return None
    if obligation_manifest.get("profile") != profile:
        result["status"] = "failed"
        result["reasons"].append("obligations_profile_mismatch")
    if obligation_manifest.get("status") != "complete":
        declared = obligation_manifest.get("status")
        result["status"] = declared if declared in {"not_available", "unsupported", "timeout"} else "failed"
        result["reasons"].append("obligations_declared_status_not_complete")
        return None
    if obligation_manifest.get("closure_scope") != "finite_query_complete":
        result["status"] = "failed"
        result["reasons"].append("obligations_closure_scope_invalid")
    if obligation_manifest.get("anonymous_witness_policy") != "not_exported":
        result["status"] = "failed"
        result["reasons"].append("obligations_witness_policy_invalid")
    if obligation_manifest.get("required_assertion_families") != list(REQUIRED_ASSERTION_FAMILIES):
        result["status"] = "failed"
        result["reasons"].append("obligations_assertion_family_contract_invalid")

    coverage = obligation_manifest.get("coverage")
    if not isinstance(coverage, dict):
        result["status"] = "failed"
        result["reasons"].append("obligations_coverage_missing")
    else:
        if coverage.get("strategy") != "finite_source_domain_query_scope":
            result["status"] = "failed"
            result["reasons"].append("obligations_coverage_strategy_invalid")
        if coverage.get("exhaustive") is not True:
            result["status"] = "failed"
            result["reasons"].append("obligations_coverage_not_exhaustive")
        source_terms = term_domain(source_quads)
        source_resources = {
            term for term in source_terms if term.kind in {"iri", "blank_node"}
        }
        if coverage.get("source_term_count") != len(source_terms):
            result["status"] = "failed"
            result["reasons"].append("obligations_source_term_count_mismatch")
        if coverage.get("source_resource_count") != len(source_resources):
            result["status"] = "failed"
            result["reasons"].append("obligations_source_resource_count_mismatch")
        if not isinstance(coverage.get("input_blank_node_count"), int) or coverage.get(
            "input_blank_node_count"
        ) < 0:
            result["status"] = "failed"
            result["reasons"].append("obligations_blank_node_count_invalid")
        query_scope = obligation_manifest.get("query_scope")
        if not isinstance(query_scope, dict) or not isinstance(
            query_scope.get("query_iris"), list
        ):
            result["status"] = "failed"
            result["reasons"].append("obligations_query_scope_missing")
        elif coverage.get("query_iri_count") != len(query_scope["query_iris"]):
            result["status"] = "failed"
            result["reasons"].append("obligations_query_iri_count_mismatch")
        if isinstance(query_scope, dict) and isinstance(query_scope.get("query_iris"), list):
            result["obligation_query_iris"] = list(query_scope["query_iris"])
        result["obligation_coverage"] = {
            key: coverage.get(key)
            for key in (
                "strategy",
                "exhaustive",
                "source_term_count",
                "source_resource_count",
                "input_blank_node_count",
                "query_iri_count",
            )
        }

    source_record = obligation_manifest.get("source_graph")
    graph_record = obligation_manifest.get("obligation_graph")
    if not isinstance(source_record, dict) or not isinstance(graph_record, dict):
        result["status"] = "failed"
        result["reasons"].append("obligations_graph_identity_missing")
        return None
    if Path(str(source_record.get("path", ""))).expanduser().resolve() != source_path:
        result["status"] = "failed"
        result["reasons"].append("obligations_source_path_mismatch")
    if Path(str(graph_record.get("path", ""))).expanduser().resolve() != graph_path:
        result["status"] = "failed"
        result["reasons"].append("obligations_graph_path_mismatch")
    actual_source_hash = file_digest(str(source_path))
    actual_obligation_hash = file_digest(str(graph_path))
    if digest_value(source_record.get("sha256")) != digest_value(actual_source_hash):
        result["status"] = "failed"
        result["reasons"].append("obligations_source_hash_mismatch")
    if digest_value(graph_record.get("sha256")) != digest_value(actual_obligation_hash):
        result["status"] = "failed"
        result["reasons"].append("obligations_graph_hash_mismatch")
    try:
        obligation_quads = load_nquads(graph_path)
    except (OSError, UnicodeDecodeError, ValueError) as error:
        result["status"] = "failed"
        result["reasons"].append(f"obligations_graph_malformed:{error}")
        return None
    declared_count = graph_record.get("count")
    if declared_count != len(obligation_quads):
        result["status"] = "failed"
        result["reasons"].append("obligations_count_mismatch")
    source_terms = term_domain(source_quads)
    if not term_domain(obligation_quads).issubset(source_terms):
        result["status"] = "failed"
        result["reasons"].append("obligations_term_outside_source_domain")
    if not set(obligation_quads).issubset(set(candidate_quads)):
        result["status"] = "failed"
        result["reasons"].append("obligation_missing_from_candidate")
    candidate_inferences = set(candidate_quads) - set(source_quads)
    if set(obligation_quads) & set(source_quads):
        result["status"] = "failed"
        result["reasons"].append("obligation_contains_source_quad")
    if not candidate_inferences.issubset(set(obligation_quads)):
        result["status"] = "failed"
        result["reasons"].append("candidate_inference_missing_from_obligations")
    if not set(obligation_quads).issubset(candidate_inferences):
        result["status"] = "failed"
        result["reasons"].append("obligation_contains_non_inference_quad")
    result["obligations"] = {
        "manifest": str(manifest_path),
        "graph": str(graph_path),
        "sha256": actual_obligation_hash,
        "count": len(obligation_quads),
    }
    return set(obligation_quads)


def transitive_closure(edges: set[tuple[str, str]]) -> dict[str, set[str]]:
    nodes = {node for edge in edges for node in edge}
    closure = {node: {node} for node in nodes}
    for source, target in edges:
        closure[source].add(target)
    changed = True
    while changed:
        changed = False
        for source in tuple(closure):
            expanded = set().union(*(closure.get(target, {target}) for target in closure[source]))
            if not expanded.issubset(closure[source]):
                closure[source].update(expanded)
                changed = True
    return closure


def equivalence_closure(edges: set[tuple[str, str]]) -> dict[str, set[str]]:
    undirected = {(left, right) for left, right in edges}
    undirected.update((right, left) for left, right in edges)
    return transitive_closure(undirected)


def query_iris_from_record(
    record: dict[str, Any], result: dict[str, Any]
) -> set[str] | None:
    workload = record.get("query_workload")
    if not isinstance(workload, dict) or not isinstance(workload.get("queries"), list):
        result["status"] = "not_available"
        result["reasons"].append("artifact_missing:query_workload")
        return None
    namespace = DEFAULT_QUERY_NAMESPACE
    configuration = record.get("configuration")
    if isinstance(configuration, dict):
        configuration_path = Path(str(configuration.get("path", ""))).expanduser().resolve()
        if configuration_path.is_file():
            for line in configuration_path.read_text(encoding="utf-8").splitlines():
                if line.startswith("query.namespace="):
                    namespace = line.split("=", 1)[1].strip() or namespace
                    break
    query_iris: set[str] = set()
    for query in workload["queries"]:
        if not isinstance(query, dict) or not nonempty(query.get("path")):
            result["status"] = "failed"
            result["reasons"].append("query_workload_identity_invalid")
            return None
        path = Path(str(query["path"])).expanduser().resolve()
        if not path.is_file():
            result["status"] = "not_available"
            result["reasons"].append(f"artifact_missing:query:{path}")
            return None
        actual_hash = file_digest(str(path))
        if not valid_digest(query.get("sha256")) or digest_value(query["sha256"]) != digest_value(actual_hash):
            result["status"] = "failed"
            result["reasons"].append(f"query_hash_mismatch:{path}")
            return None
        text = path.read_text(encoding="utf-8")
        prefixes = {"": namespace, "rdf": RDF, "rdfs": RDFS, "owl": OWL}
        for declaration in PREFIX_DECLARATION.finditer(text):
            prefixes[declaration.group(1) or ""] = declaration.group(2)
        for prefixed in PREFIXED_NAME.finditer(text):
            base = prefixes.get(prefixed.group(1) or "")
            if base is not None:
                query_iris.add(base + prefixed.group(2))
        query_iris.update(match.group(1) for match in FULL_IRI.finditer(text))
    if not query_iris:
        result["status"] = "failed"
        result["reasons"].append("query_workload_contains_no_iris")
        return None
    return query_iris


def independent_obligation_quads(
    record: dict[str, Any],
    source_quads: Iterable[Quad],
    query_iris: set[str],
) -> set[Quad]:
    """Derive finite source-domain RDF/RDFS/OWL structural obligations."""

    source = set(source_quads)
    expected: set[Quad] = set()
    subclass_edges = {
        (quad.subject.value, quad.object.value)
        for quad in source
        if quad.predicate.value == RDFS_SUBCLASS
        and quad.subject.kind == "iri"
        and quad.object.kind == "iri"
    }
    class_equivalence_edges = {
        (quad.subject.value, quad.object.value)
        for quad in source
        if quad.predicate.value == OWL_EQUIVALENT_CLASS
        and quad.subject.kind == "iri"
        and quad.object.kind == "iri"
    }
    subclass = transitive_closure(subclass_edges)
    class_equivalence = equivalence_closure(class_equivalence_edges)

    def class_targets(class_name: str) -> set[str]:
        targets = set(subclass.get(class_name, {class_name}))
        for equivalent in class_equivalence.get(class_name, {class_name}):
            targets.update(subclass.get(equivalent, {equivalent}))
        return targets

    subproperty_edges = {
        (quad.subject.value, quad.object.value)
        for quad in source
        if quad.predicate.value == RDFS_SUBPROPERTY
        and quad.subject.kind == "iri"
        and quad.object.kind == "iri"
    }
    property_equivalence_edges = {
        (quad.subject.value, quad.object.value)
        for quad in source
        if quad.predicate.value == OWL_EQUIVALENT_PROPERTY
        and quad.subject.kind == "iri"
        and quad.object.kind == "iri"
    }
    inverse_edges = {
        (quad.subject.value, quad.object.value)
        for quad in source
        if quad.predicate.value == OWL_INVERSE_OF
        and quad.subject.kind == "iri"
        and quad.object.kind == "iri"
    }
    subproperty = transitive_closure(subproperty_edges)
    property_equivalence = equivalence_closure(property_equivalence_edges)
    inverse: dict[str, set[str]] = {}
    for left, right in inverse_edges:
        inverse.setdefault(left, set()).add(right)
        inverse.setdefault(right, set()).add(left)

    def property_targets(property_name: str) -> set[str]:
        targets = set(subproperty.get(property_name, {property_name}))
        for equivalent in property_equivalence.get(property_name, {property_name}):
            targets.update(subproperty.get(equivalent, {equivalent}))
        return targets

    domains: dict[str, set[str]] = {}
    ranges: dict[str, set[str]] = {}
    for quad in source:
        if quad.subject.kind != "iri" or quad.object.kind != "iri":
            continue
        if quad.predicate.value == RDFS_DOMAIN:
            domains.setdefault(quad.subject.value, set()).add(quad.object.value)
        elif quad.predicate.value == RDFS_RANGE:
            ranges.setdefault(quad.subject.value, set()).add(quad.object.value)

    class_nodes = {
        quad.subject.value
        for quad in source
        if quad.predicate.value == RDF_TYPE
        and quad.object.kind == "iri"
        and quad.object.value == OWL_CLASS
    }
    class_nodes.update(left for left, _ in subclass_edges)
    class_nodes.update(right for _, right in subclass_edges)
    class_nodes.update(left for left, _ in class_equivalence_edges)
    class_nodes.update(right for _, right in class_equivalence_edges)
    object_properties = {
        quad.subject.value
        for quad in source
        if quad.predicate.value == RDF_TYPE
        and quad.object.kind == "iri"
        and quad.object.value == OWL_OBJECT_PROPERTY
    }
    data_properties = {
        quad.subject.value
        for quad in source
        if quad.predicate.value == RDF_TYPE
        and quad.object.kind == "iri"
        and quad.object.value == OWL_DATA_PROPERTY
    }
    property_nodes = object_properties | data_properties | set(domains) | set(ranges)
    property_nodes.update(quad.predicate.value for quad in source if quad.predicate.value not in SCHEMA_PREDICATES)
    property_nodes.update(left for left, _ in subproperty_edges)
    property_nodes.update(right for _, right in subproperty_edges)
    property_nodes.update(left for left, _ in property_equivalence_edges)
    property_nodes.update(right for _, right in property_equivalence_edges)

    for quad in source:
        if quad.predicate.value == RDF_TYPE and quad.object.kind == "iri":
            for target in class_targets(quad.object.value):
                if target in query_iris:
                    expected.add(Quad(quad.subject, Term("iri", RDF_TYPE), Term("iri", target), quad.graph))
            continue
        predicate = quad.predicate.value
        if predicate in SCHEMA_PREDICATES or quad.predicate.kind != "iri":
            continue
        if quad.object.kind == "literal":
            for target in property_targets(predicate):
                if target in query_iris:
                    expected.add(Quad(quad.subject, Term("iri", target), quad.object, quad.graph))
            continue
        if quad.object.kind not in {"iri", "blank_node"}:
            continue
        for target in property_targets(predicate):
            if target in query_iris:
                expected.add(Quad(quad.subject, Term("iri", target), quad.object, quad.graph))
        for inverse_property in inverse.get(predicate, {predicate}):
            if inverse_property == predicate:
                continue
            for target in property_targets(inverse_property):
                if target in query_iris:
                    expected.add(Quad(quad.object, Term("iri", target), quad.subject, quad.graph))
        for derived_property in property_targets(predicate):
            for domain in domains.get(derived_property, set()):
                for target in class_targets(domain):
                    if target in query_iris:
                        expected.add(
                            Quad(quad.subject, Term("iri", RDF_TYPE), Term("iri", target), quad.graph)
                        )
            for range_name in ranges.get(derived_property, set()):
                for target in class_targets(range_name):
                    if target in query_iris and quad.object.kind in {"iri", "blank_node"}:
                        expected.add(
                            Quad(quad.object, Term("iri", RDF_TYPE), Term("iri", target), quad.graph)
                        )

    class_query_predicates = {
        RDFS_SUBCLASS,
        OWL_EQUIVALENT_CLASS,
    }
    class_subjects = (
        class_nodes if query_iris & class_query_predicates else query_iris & class_nodes
    )
    for class_name in class_subjects:
        for target in class_targets(class_name) - {class_name}:
            if target in class_nodes:
                expected.add(
                    Quad(Term("iri", class_name), Term("iri", RDFS_SUBCLASS), Term("iri", target))
                )
        for target in class_equivalence.get(class_name, {class_name}) - {class_name}:
            expected.add(
                Quad(
                    Term("iri", class_name),
                    Term("iri", OWL_EQUIVALENT_CLASS),
                    Term("iri", target),
                )
            )

    property_query_predicates = {
        RDFS_SUBPROPERTY,
        OWL_EQUIVALENT_PROPERTY,
        OWL_INVERSE_OF,
        RDFS_DOMAIN,
        RDFS_RANGE,
    }
    property_subjects = (
        property_nodes
        if query_iris & property_query_predicates
        else query_iris & property_nodes
    )
    for property_name in property_subjects:
        for target in subproperty.get(property_name, {property_name}) - {property_name}:
            expected.add(
                Quad(Term("iri", property_name), Term("iri", RDFS_SUBPROPERTY), Term("iri", target))
            )
        for target in property_equivalence.get(property_name, {property_name}) - {property_name}:
            expected.add(
                Quad(
                    Term("iri", property_name),
                    Term("iri", OWL_EQUIVALENT_PROPERTY),
                    Term("iri", target),
                )
            )
        for target in inverse.get(property_name, {property_name}) - {property_name}:
            expected.add(
                Quad(Term("iri", property_name), Term("iri", OWL_INVERSE_OF), Term("iri", target))
            )
        for derived_property in property_targets(property_name):
            for domain in domains.get(derived_property, set()):
                for target in class_targets(domain):
                    expected.add(
                        Quad(Term("iri", property_name), Term("iri", RDFS_DOMAIN), Term("iri", target))
                    )
            for range_name in ranges.get(derived_property, set()):
                expected.add(
                    Quad(Term("iri", property_name), Term("iri", RDFS_RANGE), Term("iri", range_name))
                )
    return expected


def validate_independent_obligations(
    record: dict[str, Any],
    source_quads: Iterable[Quad],
    candidate_quads: Iterable[Quad],
    obligation_quads: set[Quad] | None,
    result: dict[str, Any],
) -> None:
    """Require a validator-derived structural closure, not only exporter claims."""

    contract = record.get("graph_contract")
    if record.get("status") != "complete":
        return
    if isinstance(contract, dict) and contract.get("independent_obligation_check_required") is False:
        return
    query_iris = query_iris_from_record(record, result)
    if query_iris is None:
        return
    source = set(source_quads)
    candidate = set(candidate_quads)
    expected_inferred = independent_obligation_quads(record, source, query_iris) - source
    recorded_query_iris = result.get("obligation_query_iris")
    if obligation_quads is not None and (
        not isinstance(recorded_query_iris, list) or set(recorded_query_iris) != query_iris
    ):
        result["status"] = "failed"
        result["reasons"].append("obligations_query_scope_not_independently_confirmed")
    missing_candidate = expected_inferred - candidate
    missing_obligations = expected_inferred - obligation_quads if obligation_quads is not None else set()
    if missing_candidate:
        result["status"] = "failed"
        result["reasons"].append(
            f"independent_obligation_missing_from_candidate:{len(missing_candidate)}"
        )
    if missing_obligations:
        result["status"] = "failed"
        result["reasons"].append(
            f"independent_obligation_missing_from_manifest:{len(missing_obligations)}"
        )
    result["independent_obligations"] = {
        "query_iri_count": len(query_iris),
        "expected_inferred_count": len(expected_inferred),
        "expected_inferred_assertion_counts": assertion_counts(expected_inferred),
        "candidate_checked": True,
        "manifest_checked": obligation_quads is not None,
    }


def validate_runtime_classpath(record: dict[str, Any], result: dict[str, Any]) -> None:
    """Verify every explicitly recorded runtime classpath artifact."""

    classpath = record.get("runtime_classpath")
    if classpath is None:
        return
    if not isinstance(classpath, dict) or not nonempty(classpath.get("requested")):
        result["status"] = "failed"
        result["reasons"].append("runtime_classpath_identity_invalid")
        return
    artifacts = classpath.get("artifacts")
    if not isinstance(artifacts, list) or not artifacts:
        result["status"] = "failed"
        result["reasons"].append("runtime_classpath_artifacts_missing")
        return
    for index, artifact in enumerate(artifacts):
        if not isinstance(artifact, dict):
            result["status"] = "failed"
            result["reasons"].append(f"runtime_classpath_artifact_invalid:{index}")
            continue
        for field in ("name", "version", "revision", "path"):
            if not nonempty(artifact.get(field)):
                result["status"] = "failed"
                result["reasons"].append(f"runtime_classpath_field_missing:{index}.{field}")
        if not valid_digest(artifact.get("sha256")):
            result["status"] = "failed"
            result["reasons"].append(f"runtime_classpath_hash_invalid:{index}")
            continue
        path = Path(str(artifact.get("path", ""))).expanduser().resolve()
        if not path.is_file():
            result["status"] = "not_available"
            result["reasons"].append(f"artifact_missing:runtime_classpath:{path}")
        elif digest_value(artifact["sha256"]) != digest_value(file_digest(str(path))):
            result["status"] = "failed"
            result["reasons"].append(f"runtime_classpath_hash_mismatch:{index}")


def directory_digest(path: Path) -> str:
    hasher = hashlib.sha256()
    for child in sorted(item for item in path.rglob("*") if item.is_file()):
        hasher.update(child.relative_to(path).as_posix().encode("utf-8"))
        hasher.update(b"\0")
        hasher.update(child.read_bytes())
        hasher.update(b"\0")
    return "sha256:" + hasher.hexdigest()


def validate_hermit_identity(record: dict[str, Any], result: dict[str, Any]) -> None:
    """Validate the separate DL TBox, ABox, generator, and context identities."""

    component = record.get("component")
    if not isinstance(component, dict) or component.get("name") != "HermiT":
        return
    backend = record.get("query_backend")
    backend_manifest = backend.get("identity_manifest") if isinstance(backend, dict) else None
    if not isinstance(backend_manifest, dict):
        result["reasons"].append("query_backend_identity_manifest_missing")
    else:
        backend_manifest_path = Path(str(backend_manifest.get("path", ""))).expanduser().resolve()
        if not backend_manifest_path.is_file():
            result["status"] = "not_available"
            result["reasons"].append(
                f"artifact_missing:query_backend_identity_manifest:{backend_manifest_path}"
            )
        elif digest_value(backend_manifest.get("sha256")) != digest_value(
            file_digest(str(backend_manifest_path))
        ):
            result["reasons"].append("query_backend_identity_manifest_hash_mismatch")
        else:
            try:
                backend_document = load_json(backend_manifest_path)
            except (ManifestMissingError, ValueError) as error:
                result["reasons"].append(
                    f"query_backend_identity_manifest_malformed:{error}"
                )
            else:
                if forbidden_field_paths(backend_document):
                    result["reasons"].append("query_backend_identity_manifest_forbidden_field")
                for field in ("name", "version", "revision", "path", "sha256"):
                    if not nonempty(backend_document.get(field)):
                        result["reasons"].append(
                            f"query_backend_identity_manifest_missing:{field}"
                        )
                if isinstance(backend, dict):
                    if backend_document.get("name") != backend.get("name"):
                        result["reasons"].append("query_backend_manifest_name_mismatch")
                    if backend_document.get("version") != backend.get("version"):
                        result["reasons"].append("query_backend_manifest_version_mismatch")
                    if backend_document.get("revision") != backend.get("revision"):
                        result["reasons"].append("query_backend_manifest_revision_mismatch")
                    if digest_value(backend_document.get("sha256")) != digest_value(
                        backend.get("artifact_digest")
                    ):
                        result["reasons"].append("query_backend_manifest_artifact_mismatch")
                    if backend_document.get("configuration") != backend.get("configuration"):
                        result["reasons"].append("query_backend_manifest_configuration_mismatch")
                    if backend_document.get("dependencies") != backend.get("dependencies"):
                        result["reasons"].append("query_backend_manifest_dependencies_mismatch")
    for section, fields in (
        ("profile_tbox", ("id", "path", "revision", "sha256")),
        ("profile_abox", ("id", "path", "revision", "sha256")),
        ("dataset_artifact", ("id", "path", "revision", "sha256")),
        ("generated_dataset", ("id", "path", "revision", "sha256")),
        ("generator_configuration", ("id", "path", "revision", "sha256")),
    ):
        _require_identity(record, section, fields, result["reasons"])
        value = record.get(section)
        if not isinstance(value, dict):
            continue
        path = Path(str(value.get("path", ""))).expanduser().resolve()
        if not path.is_file():
            result["status"] = "not_available"
            result["reasons"].append(f"artifact_missing:{section}:{path}")
        elif digest_value(value.get("sha256")) != digest_value(file_digest(str(path))):
            result["status"] = "failed"
            result["reasons"].append(f"artifact_hash_mismatch:{section}")

    dataset = record.get("dataset")
    if not isinstance(dataset, dict):
        result["status"] = "failed"
        result["reasons"].append("dataset_identity_missing")
    else:
        for field in (
            "id",
            "profile",
            "artifact_id",
            "artifact_path",
            "artifact_revision",
            "tbox_id",
            "abox_id",
        ):
            if not nonempty(dataset.get(field)):
                result["reasons"].append(f"dataset_identity_missing:{field}")
        for field in (
            "sha256",
            "source_ontology_sha256",
            "tbox_sha256",
            "abox_sha256",
        ):
            if not valid_digest(dataset.get(field)):
                result["reasons"].append(f"dataset_identity_invalid:{field}")
        seed = dataset.get("seed")
        university_count = dataset.get("university_count")
        if isinstance(seed, bool) or not isinstance(seed, int) or seed < 0:
            result["reasons"].append("dataset_identity_invalid:seed")
        if isinstance(university_count, bool) or not isinstance(university_count, int) or university_count <= 0:
            result["reasons"].append("dataset_identity_invalid:university_count")
        dataset_path = Path(str(dataset.get("artifact_path", ""))).expanduser().resolve()
        if not dataset_path.is_file():
            result["status"] = "not_available"
            result["reasons"].append(f"artifact_missing:dataset:{dataset_path}")
        elif digest_value(dataset.get("sha256")) != digest_value(file_digest(str(dataset_path))):
            result["reasons"].append("dataset_artifact_hash_mismatch")
        source_document = record.get("source_document")
        if isinstance(source_document, dict):
            if component.get("name") == "HermiT":
                generated_dataset = record.get("generated_dataset")
                if not isinstance(generated_dataset, dict):
                    result["reasons"].append("generated_dataset_identity_missing")
                else:
                    generated_path = Path(str(generated_dataset.get("path", ""))).expanduser().resolve()
                    if Path(str(source_document.get("path", ""))).expanduser().resolve() != generated_path:
                        result["reasons"].append("source_generated_dataset_path_mismatch")
                    if digest_value(source_document.get("sha256")) != digest_value(generated_dataset.get("sha256")):
                        result["reasons"].append("source_generated_dataset_hash_mismatch")
            else:
                if Path(str(source_document.get("path", ""))).expanduser().resolve() != dataset_path:
                    result["reasons"].append("source_dataset_path_mismatch")
                if digest_value(source_document.get("sha256")) != digest_value(dataset.get("sha256")):
                    result["reasons"].append("source_dataset_hash_mismatch")
        for section, id_field, hash_field in (
            ("profile_tbox", "tbox_id", "tbox_sha256"),
            ("profile_abox", "abox_id", "abox_sha256"),
            ("dataset_artifact", "artifact_id", "sha256"),
        ):
            artifact = record.get(section)
            if isinstance(artifact, dict):
                if dataset.get(id_field) != artifact.get("id"):
                    result["reasons"].append(f"dataset_{section}_id_mismatch")
                if digest_value(dataset.get(hash_field)) != digest_value(artifact.get("sha256")):
                    result["reasons"].append(f"dataset_{section}_hash_mismatch")

    generator = record.get("generator")
    if not isinstance(generator, dict):
        result["reasons"].append("generator_identity_missing")
    else:
        configuration_hash = generator.get("configuration_sha256")
        configuration = record.get("generator_configuration")
        if not valid_digest(configuration_hash) or not isinstance(configuration, dict) or digest_value(configuration_hash) != digest_value(configuration.get("sha256")):
            result["reasons"].append("generator_configuration_identity_mismatch")
        artifact_path = Path(str(generator.get("artifact_path", ""))).expanduser().resolve()
        if not artifact_path.is_file():
            result["status"] = "not_available"
            result["reasons"].append(f"artifact_missing:generator:{artifact_path}")
        elif digest_value(generator.get("artifact_digest")) != digest_value(file_digest(str(artifact_path))):
            result["reasons"].append("artifact_hash_mismatch:generator")

    context = record.get("profile_context")
    if isinstance(context, dict):
        context_dir = Path(str(context.get("directory", ""))).expanduser().resolve()
        if context_dir.is_dir() and valid_digest(context.get("sha256")):
            if digest_value(context["sha256"]) != digest_value(directory_digest(context_dir)):
                result["reasons"].append("profile_context_hash_mismatch")
        else:
            result["status"] = "not_available"
            result["reasons"].append(f"artifact_missing:profile_context:{context_dir}")
    else:
        result["reasons"].append("profile_context_identity_missing")


def validate_simple_determinism(
    record: dict[str, Any], graph_path: Path, result: dict[str, Any]
) -> None:
    """Verify the two canonical graph outputs used by non-HermiT profiles."""

    determinism = record.get("determinism")
    if determinism is None:
        result["status"] = "not_available"
        result["reasons"].append("artifact_missing:determinism_record")
        return
    if not isinstance(determinism, dict):
        result["status"] = "failed"
        result["reasons"].append("determinism_record_invalid")
        return
    declared = determinism.get("status")
    if declared != "complete":
        result["status"] = declared if declared in {"not_available", "timeout"} else "failed"
        result["reasons"].append(f"determinism_status:{declared}")
        return
    if determinism.get("runs") != 2:
        result["status"] = "failed"
        result["reasons"].append("determinism_run_count_invalid")
        return
    first = determinism.get("first_graph")
    second = determinism.get("second_graph")
    if not isinstance(first, dict) or not isinstance(second, dict):
        result["status"] = "failed"
        result["reasons"].append("determinism_graph_identity_missing")
        return
    first_path = Path(str(first.get("path", ""))).expanduser().resolve()
    second_path = Path(str(second.get("path", ""))).expanduser().resolve()
    if first_path != graph_path:
        result["status"] = "failed"
        result["reasons"].append("determinism_first_graph_path_mismatch")
    if not second_path.is_file():
        result["status"] = "not_available"
        result["reasons"].append(f"artifact_missing:determinism_graph:{second_path}")
        return
    actual_first_hash = file_digest(str(graph_path))
    actual_second_hash = file_digest(str(second_path))
    if not valid_digest(first.get("sha256")) or digest_value(first["sha256"]) != digest_value(actual_first_hash):
        result["status"] = "failed"
        result["reasons"].append("determinism_first_graph_hash_mismatch")
    if not valid_digest(second.get("sha256")) or digest_value(second["sha256"]) != digest_value(actual_second_hash):
        result["status"] = "failed"
        result["reasons"].append("determinism_second_graph_hash_mismatch")
    try:
        first_bytes = graph_path.read_bytes()
        second_bytes = canonicalize_file(second_path)
    except (OSError, UnicodeDecodeError, ValueError) as error:
        result["status"] = "failed"
        result["reasons"].append(f"determinism_graph_malformed:{error}")
        return
    if first_bytes != second_bytes:
        result["status"] = "failed"
        result["reasons"].append("determinism_graph_hash_mismatch")
    result["determinism"] = {
        "runs": 2,
        "first_graph": {"path": str(graph_path), "sha256": actual_first_hash},
        "second_graph": {"path": str(second_path), "sha256": actual_second_hash},
    }


def validate_determinism(
    record: dict[str, Any], source_path: Path, graph_path: Path, result: dict[str, Any]
) -> None:
    """Verify the second canonical exporter run required by HermiT."""

    contract = record.get("graph_contract")
    if record.get("status") != "complete":
        return
    if isinstance(contract, dict) and contract.get("two_run_determinism_required") is False:
        return
    if not isinstance(record.get("obligations"), dict):
        validate_simple_determinism(record, graph_path, result)
        return
    determinism = record.get("determinism")
    if determinism is None:
        result["status"] = "not_available"
        result["reasons"].append("artifact_missing:determinism_record")
        return
    if not isinstance(determinism, dict):
        result["status"] = "failed"
        result["reasons"].append("determinism_record_invalid")
        return
    declared = determinism.get("status")
    if declared != "complete":
        result["status"] = declared if declared in {"not_available", "timeout"} else "failed"
        result["reasons"].append(f"determinism_status:{declared}")
        return
    if determinism.get("runs") != 2:
        result["status"] = "failed"
        result["reasons"].append("determinism_run_count_invalid")
        return
    first = determinism.get("first_graph")
    second = determinism.get("second_graph")
    first_source = determinism.get("first_source_graph")
    second_source = determinism.get("second_source_graph")
    second_obligation = determinism.get("second_obligation_graph")
    second_obligation_manifest = determinism.get("second_obligation_manifest")
    if (
        not isinstance(first, dict)
        or not isinstance(second, dict)
        or not isinstance(first_source, dict)
        or not isinstance(second_source, dict)
        or not isinstance(second_obligation, dict)
        or not isinstance(second_obligation_manifest, dict)
    ):
        result["status"] = "failed"
        result["reasons"].append("determinism_graph_identity_missing")
        return
    first_path = Path(str(first.get("path", ""))).expanduser().resolve()
    second_path = Path(str(second.get("path", ""))).expanduser().resolve()
    first_source_path = Path(str(first_source.get("path", ""))).expanduser().resolve()
    second_source_path = Path(str(second_source.get("path", ""))).expanduser().resolve()
    second_obligation_path = Path(str(second_obligation.get("path", ""))).expanduser().resolve()
    second_obligation_manifest_path = Path(
        str(second_obligation_manifest.get("path", ""))
    ).expanduser().resolve()
    if first_path != graph_path:
        result["status"] = "failed"
        result["reasons"].append("determinism_first_graph_path_mismatch")
    if first_source_path != source_path:
        result["status"] = "failed"
        result["reasons"].append("determinism_first_source_path_mismatch")
    if not second_source_path.is_file():
        result["status"] = "not_available"
        result["reasons"].append(f"artifact_missing:determinism_source_graph:{second_source_path}")
        return
    if not second_path.is_file():
        result["status"] = "not_available"
        result["reasons"].append(f"artifact_missing:determinism_graph:{second_path}")
        return
    if not second_obligation_path.is_file():
        result["status"] = "not_available"
        result["reasons"].append(
            f"artifact_missing:determinism_obligation_graph:{second_obligation_path}"
        )
        return
    if not second_obligation_manifest_path.is_file():
        result["status"] = "not_available"
        result["reasons"].append(
            f"artifact_missing:determinism_obligation_manifest:{second_obligation_manifest_path}"
        )
        return
    actual_first_hash = file_digest(str(graph_path))
    actual_second_hash = file_digest(str(second_path))
    actual_first_source_hash = file_digest(str(source_path))
    actual_second_source_hash = file_digest(str(second_source_path))
    actual_second_obligation_hash = file_digest(str(second_obligation_path))
    actual_second_obligation_manifest_hash = file_digest(str(second_obligation_manifest_path))
    if not valid_digest(first.get("sha256")) or digest_value(first["sha256"]) != digest_value(actual_first_hash):
        result["status"] = "failed"
        result["reasons"].append("determinism_first_graph_hash_mismatch")
    if not valid_digest(second.get("sha256")) or digest_value(second["sha256"]) != digest_value(actual_second_hash):
        result["status"] = "failed"
        result["reasons"].append("determinism_second_graph_hash_mismatch")
    if not valid_digest(first_source.get("sha256")) or digest_value(first_source["sha256"]) != digest_value(actual_first_source_hash):
        result["status"] = "failed"
        result["reasons"].append("determinism_first_source_hash_mismatch")
    if not valid_digest(second_source.get("sha256")) or digest_value(second_source["sha256"]) != digest_value(actual_second_source_hash):
        result["status"] = "failed"
        result["reasons"].append("determinism_second_source_hash_mismatch")
    if not valid_digest(second_obligation.get("sha256")) or digest_value(second_obligation["sha256"]) != digest_value(actual_second_obligation_hash):
        result["status"] = "failed"
        result["reasons"].append("determinism_second_obligation_hash_mismatch")
    if not valid_digest(second_obligation_manifest.get("sha256")) or digest_value(second_obligation_manifest["sha256"]) != digest_value(actual_second_obligation_manifest_hash):
        result["status"] = "failed"
        result["reasons"].append("determinism_second_obligation_manifest_hash_mismatch")
    try:
        first_bytes = graph_path.read_bytes()
        second_bytes = canonicalize_file(second_path)
        first_source_bytes = canonicalize_file(source_path)
        second_source_bytes = canonicalize_file(second_source_path)
        first_obligation_path = Path(
            str(record["obligations"].get("graph_path", ""))
        ).expanduser().resolve()
        first_obligation_bytes = canonicalize_file(first_obligation_path)
        second_obligation_bytes = canonicalize_file(second_obligation_path)
        second_obligation_document = load_json(second_obligation_manifest_path)
    except (OSError, UnicodeDecodeError, ValueError) as error:
        result["status"] = "failed"
        result["reasons"].append(f"determinism_graph_malformed:{error}")
        return
    if first_bytes != second_bytes or digest_value(actual_first_hash) != digest_value(actual_second_hash):
        result["status"] = "failed"
        result["reasons"].append("determinism_graph_hash_mismatch")
    if first_source_bytes != second_source_bytes:
        result["status"] = "failed"
        result["reasons"].append("determinism_source_graph_mismatch")
    if first_obligation_bytes != second_obligation_bytes:
        result["status"] = "failed"
        result["reasons"].append("determinism_obligation_graph_mismatch")
    second_obligation_record = second_obligation_document.get("obligation_graph")
    if not isinstance(second_obligation_record, dict):
        result["status"] = "failed"
        result["reasons"].append("determinism_obligation_manifest_identity_missing")
    else:
        if Path(str(second_obligation_record.get("path", ""))).expanduser().resolve() != second_obligation_path:
            result["status"] = "failed"
            result["reasons"].append("determinism_obligation_manifest_path_mismatch")
        if digest_value(second_obligation_record.get("sha256")) != digest_value(actual_second_obligation_hash):
            result["status"] = "failed"
            result["reasons"].append("determinism_obligation_manifest_graph_hash_mismatch")
    result["determinism"] = {
        "runs": 2,
        "first_graph": {"path": str(graph_path), "sha256": actual_first_hash},
        "second_graph": {"path": str(second_path), "sha256": actual_second_hash},
        "first_source_graph": {"path": str(source_path), "sha256": actual_first_source_hash},
        "second_source_graph": {"path": str(second_source_path), "sha256": actual_second_source_hash},
        "second_obligation_graph": {
            "path": str(second_obligation_path),
            "sha256": actual_second_obligation_hash,
        },
        "second_obligation_manifest": {
            "path": str(second_obligation_manifest_path),
            "sha256": actual_second_obligation_manifest_hash,
        },
    }


def validate(
    profile: str,
    source_path: Path,
    candidate_path: Path,
    graph_path: Path,
    manifest_path: Path,
) -> dict[str, Any]:
    result = base_result(profile, "failed", [])
    try:
        manifest = load_json(manifest_path)
    except ManifestMissingError as error:
        result["status"] = "not_available"
        result["reasons"] = [str(error)]
        return result
    except ValueError as error:
        result["reasons"] = ["manifest_malformed", str(error)]
        return result

    forbidden = forbidden_field_paths(manifest)
    if forbidden:
        result["reasons"] = [f"forbidden_field:{path}" for path in forbidden]
        return result

    profiles = manifest.get("profiles")
    record = profiles.get(profile) if isinstance(profiles, dict) else None
    if not isinstance(record, dict):
        result["status"] = "not_available"
        result["reasons"] = ["profile_record_missing"]
        result["query_handoff"]["status"] = "not_ready"
        return result
    if record.get("profile") != profile:
        result["reasons"] = ["profile_identity_mismatch"]
        return result
    if record.get("status") != "complete":
        declared = record.get("status")
        result["status"] = declared if declared in {"partial", "failed", "not_available", "unsupported", "timeout"} else "failed"
        result["reasons"] = ["declared_status_not_complete"]
        result["query_handoff"]["status"] = "not_ready"
        return result

    source_path = source_path.expanduser().resolve()
    candidate_path = candidate_path.expanduser().resolve()
    graph_path = graph_path.expanduser().resolve()
    source_record = record.get("source_ontology")
    candidate_record = record.get("candidate_graph")
    output_record = record.get("output_graph")
    if not isinstance(candidate_record, dict) or candidate_record.get("immutable") is not True:
        result["reasons"].append("candidate_graph_not_immutable")
    if not isinstance(output_record, dict) or output_record.get("immutable") is not True:
        result["reasons"].append("output_graph_not_immutable")
    for section, expected_path in (
        ("source_ontology", source_path),
        ("candidate_graph", candidate_path),
        ("output_graph", graph_path),
    ):
        section_record = record.get(section)
        if not isinstance(section_record, dict) or not nonempty(section_record.get("path")):
            result["reasons"].append(f"path_missing:{section}")
        elif Path(str(section_record["path"])).expanduser().resolve() != expected_path:
            result["reasons"].append(f"path_identity_mismatch:{section}")

    for section, fields in (
        ("source_ontology", ("id", "path", "sha256")),
        ("candidate_graph", ("id", "path", "sha256")),
        ("output_graph", ("id", "path", "sha256")),
        ("component", ("name", "version", "revision", "artifact_digest", "path")),
        ("exporter", ("name", "version", "revision", "artifact_digest", "path")),
        ("query_backend", ("name", "version", "revision", "artifact_digest", "path")),
        ("configuration", ("id", "path", "sha256")),
        ("derivation", ("version", "revision")),
        ("profile_context", ("id", "directory")),
    ):
        _require_identity(record, section, fields, result["reasons"])

    validate_hermit_identity(record, result)
    runtime = record.get("runtime")
    if not isinstance(runtime, dict) or not nonempty(runtime.get("id")):
        result["reasons"].append("identity_field_missing:runtime.id")
    elif not isinstance(runtime.get("dependencies"), list) or not runtime["dependencies"]:
        result["reasons"].append("identity_field_missing:runtime.dependencies")
    else:
        for index, dependency in enumerate(runtime["dependencies"]):
            if not isinstance(dependency, dict):
                result["reasons"].append(f"runtime_dependency_invalid:{index}")
                continue
            for field in ("name", "version"):
                if not nonempty(dependency.get(field)):
                    result["reasons"].append(f"runtime_dependency_missing:{index}.{field}")
            if not valid_digest(dependency.get("digest")):
                result["reasons"].append(f"runtime_dependency_invalid:{index}.digest")

    validate_runtime_classpath(record, result)
    query_backend = record.get("query_backend")
    if not isinstance(query_backend, dict) or query_backend.get("read_only") is not True:
        result["reasons"].append("query_backend_not_read_only")
    for section in ("component", "exporter", "query_backend"):
        artifact = record.get(section)
        artifact_path = artifact.get("path") if isinstance(artifact, dict) else None
        if not nonempty(artifact_path):
            continue
        resolved_artifact = Path(str(artifact_path)).expanduser().resolve()
        if not resolved_artifact.is_file():
            result["status"] = "not_available"
            result["reasons"].append(f"artifact_missing:{section}:{resolved_artifact}")
        elif digest_value(artifact.get("artifact_digest")) != digest_value(file_digest(str(resolved_artifact))):
            result["reasons"].append(f"artifact_hash_mismatch:{section}")
    resources = record.get("resource_limits")
    if not isinstance(resources, dict):
        result["reasons"].append("identity_section_missing:resource_limits")
    else:
        timeout = resources.get("timeout_seconds")
        threads = resources.get("threads")
        if isinstance(timeout, bool) or not isinstance(timeout, int) or timeout <= 0:
            result["reasons"].append("resource_limit_invalid:timeout_seconds")
        if isinstance(threads, bool) or not isinstance(threads, int) or threads <= 0:
            result["reasons"].append("resource_limit_invalid:threads")
        if not nonempty(resources.get("memory")):
            result["reasons"].append("resource_limit_missing:memory")

    contract = record.get("graph_contract")
    if not isinstance(contract, dict):
        result["reasons"].append("graph_contract_missing")
    else:
        canonicalizer = contract.get("canonicalizer")
        if not isinstance(canonicalizer, dict) or canonicalizer.get("name") != "URDNA2015" or canonicalizer.get("version") != "2015" or canonicalizer.get("serialization") != "N-Quads":
            result["reasons"].append("canonicalizer_contract_invalid")
        if contract.get("closure_scope") != "finite_query_complete":
            result["reasons"].append("closure_scope_invalid")
        if contract.get("source_included") is not True:
            result["reasons"].append("source_inclusion_not_declared")
        families = contract.get("required_assertion_families")
        if families != list(REQUIRED_ASSERTION_FAMILIES):
            result["reasons"].append("assertion_family_contract_invalid")
        if contract.get("anonymous_witness_policy") != "not_exported":
            result["reasons"].append("anonymous_witness_policy_invalid")

    missing_artifacts = [path for path in (source_path, candidate_path, graph_path) if not path.is_file()]
    if missing_artifacts:
        result["status"] = "not_available" if not result["reasons"] else "failed"
        result["reasons"].extend(f"artifact_missing:{path}" for path in missing_artifacts)
        result["query_handoff"]["status"] = "not_ready"
        return result

    try:
        source_quads = load_nquads(source_path)
        candidate_quads = load_nquads(candidate_path)
        graph_quads = load_nquads(graph_path)
    except (OSError, UnicodeDecodeError, ValueError) as error:
        result["status"] = "failed"
        result["reasons"].append(f"graph_malformed:{error}")
        return result

    result["assertion_counts"] = assertion_counts(candidate_quads)
    source_terms = term_domain(source_quads)
    candidate_terms = term_domain(candidate_quads)
    obligation_quads = validate_obligations(
        record,
        profile,
        source_path,
        source_quads,
        candidate_quads,
        result,
    )
    validate_independent_obligations(
        record,
        source_quads,
        candidate_quads,
        obligation_quads,
        result,
    )
    validate_determinism(record, source_path, graph_path, result)
    extra_terms = candidate_terms - source_terms
    if extra_terms:
        result["status"] = "failed"
        if any(term.kind == "blank_node" for term in extra_terms):
            result["reasons"].append("generated_anonymous_witness_not_allowed")
        else:
            result["reasons"].append("output_term_outside_source_domain")

    if not set(source_quads).issubset(set(candidate_quads)):
        result["status"] = "failed"
        result["reasons"].append("source_inclusion_missing")

    missing_families = [
        family for family, count in result["assertion_counts"].items() if count == 0
    ]
    if missing_families:
        contract = record.get("graph_contract")
        if isinstance(contract, dict) and contract.get(
            "unavailable_assertion_families_status"
        ) == "not_available":
            result["status"] = "not_available"
            result["reasons"].extend(
                f"assertion_family_unavailable:{family}" for family in missing_families
            )
        else:
            result["status"] = "failed"
            result["reasons"].extend(
                f"assertion_family_missing:{family}" for family in missing_families
            )

    try:
        expected_canonical = canonicalize_file(candidate_path)
        actual_canonical = graph_path.read_bytes()
        round_trip = canonicalize_file(graph_path)
    except (OSError, UnicodeDecodeError, ValueError) as error:
        result["status"] = "failed"
        result["reasons"].append(f"canonicalization_failed:{error}")
        return result

    if actual_canonical != expected_canonical:
        result["reasons"].append("canonical_graph_mismatch")
    if round_trip != actual_canonical:
        result["reasons"].append("canonical_round_trip_mismatch")

    actual_source_hash = file_digest(str(source_path))
    actual_candidate_hash = file_digest(str(candidate_path))
    actual_graph_hash = file_digest(str(graph_path))
    if not isinstance(source_record, dict) or digest_value(source_record.get("sha256")) != digest_value(actual_source_hash):
        result["reasons"].append("source_hash_mismatch")
    if not isinstance(candidate_record, dict) or digest_value(candidate_record.get("sha256")) != digest_value(actual_candidate_hash):
        result["reasons"].append("candidate_graph_hash_mismatch")
    if not isinstance(output_record, dict) or digest_value(output_record.get("sha256")) != digest_value(actual_graph_hash):
        result["reasons"].append("output_graph_hash_mismatch")
    if record.get("input_sha256") is None or digest_value(record.get("input_sha256")) != digest_value(actual_source_hash):
        result["reasons"].append("input_hash_mismatch")
    if record.get("output_sha256") is None or digest_value(record.get("output_sha256")) != digest_value(actual_graph_hash):
        result["reasons"].append("output_hash_mismatch")

    configuration = record.get("configuration")
    if isinstance(configuration, dict):
        config_path = Path(str(configuration.get("path", ""))).expanduser().resolve()
        if not config_path.is_file():
            result["status"] = "not_available"
            result["reasons"].append(f"configuration_missing:{config_path}")
        elif not valid_digest(configuration.get("sha256")) or digest_value(configuration["sha256"]) != digest_value(file_digest(str(config_path))):
            result["reasons"].append("configuration_hash_mismatch")

    profile_context = record.get("profile_context")
    if isinstance(profile_context, dict):
        context_dir = Path(str(profile_context.get("directory", ""))).expanduser().resolve()
        if not context_dir.is_dir():
            result["status"] = "not_available"
            result["reasons"].append(f"profile_context_missing:{context_dir}")

    if not result["reasons"]:
        result["status"] = "complete"
    elif result["status"] == "not_available" and not all(
        reason.startswith(
            (
                "artifact_missing:",
                "configuration_missing:",
                "profile_context_missing:",
                "determinism_status:",
                "assertion_family_unavailable:",
            )
        )
        for reason in result["reasons"]
    ):
        result["status"] = "failed"
    if result["status"] == "complete":
        result["query_handoff"].update(
            {
                "status": "ready",
                "graph_path": str(graph_path),
                "graph_sha256": actual_graph_hash,
                "validator_status": "complete",
            }
        )
    else:
        result["query_handoff"]["status"] = "not_ready"
    result["source"] = {"path": str(source_path), "sha256": actual_source_hash}
    result["candidate_graph"] = {"path": str(candidate_path), "sha256": actual_candidate_hash}
    result["graph"] = {"path": str(graph_path), "sha256": actual_graph_hash}
    return result


def handoff(validation_path: Path, graph_path: Path) -> dict[str, Any]:
    try:
        validation = load_json(validation_path)
    except ManifestMissingError as error:
        return {
            "schema_version": 1,
            "status": "not_available",
            "reasons": [str(error)],
            "read_only": True,
        }
    except ValueError as error:
        return {"schema_version": 1, "status": "failed", "reasons": [str(error)]}
    if validation.get("status") != "complete":
        return {
            "schema_version": 1,
            "status": validation.get("status", "failed"),
            "reasons": ["validation_not_complete"],
            "read_only": True,
        }
    graph = graph_path.expanduser().resolve()
    if not graph.is_file():
        return {
            "schema_version": 1,
            "status": "not_available",
            "reasons": ["graph_missing"],
            "read_only": True,
        }
    try:
        canonicalize_file(graph)
    except (OSError, UnicodeDecodeError, ValueError) as error:
        return {
            "schema_version": 1,
            "status": "failed",
            "reasons": [f"graph_malformed:{error}"],
            "read_only": True,
        }
    actual_hash = file_digest(str(graph))
    expected = validation.get("query_handoff", {}).get("graph_sha256")
    if digest_value(expected) != digest_value(actual_hash) or validation.get("query_handoff", {}).get("graph_path") != str(graph):
        return {
            "schema_version": 1,
            "status": "failed",
            "reasons": ["validated_graph_identity_mismatch"],
            "read_only": True,
        }
    return {
        "schema_version": 1,
        "status": "ready",
        "profile": validation.get("profile"),
        "graph_path": str(graph),
        "graph_sha256": actual_hash,
        "read_only": True,
        "validator": validation.get("validator"),
    }


def write_json(path: Path | None, document: dict[str, Any]) -> None:
    payload = json.dumps(document, ensure_ascii=False, indent=2, sort_keys=True) + "\n"
    if path is None:
        sys.stdout.write(payload)
    else:
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(payload, encoding="utf-8")


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    subparsers = parser.add_subparsers(dest="command", required=True)
    validate_parser = subparsers.add_parser("validate")
    validate_parser.add_argument("--profile", required=True, choices=sorted(PROFILES))
    validate_parser.add_argument("--source", required=True, type=Path)
    validate_parser.add_argument("--candidate-graph", required=True, type=Path)
    validate_parser.add_argument("--graph", required=True, type=Path)
    validate_parser.add_argument("--manifest", required=True, type=Path)
    validate_parser.add_argument("--output", type=Path)
    handoff_parser = subparsers.add_parser("handoff")
    handoff_parser.add_argument("--validation", required=True, type=Path)
    handoff_parser.add_argument("--graph", required=True, type=Path)
    handoff_parser.add_argument("--output", type=Path)
    args = parser.parse_args(argv)
    if args.command == "validate":
        result = validate(
            args.profile,
            args.source,
            args.candidate_graph,
            args.graph,
            args.manifest,
        )
        write_json(args.output, result)
        return 0 if result["status"] == "complete" else 1
    if args.command == "handoff":
        result = handoff(args.validation, args.graph)
        write_json(args.output, result)
        return 0 if result["status"] == "ready" else 1
    return 2


if __name__ == "__main__":
    raise SystemExit(main())
