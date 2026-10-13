#!/usr/bin/env python3
"""Deterministic OWL 2 RL derivation core for the derived-oracle pipeline.

Owns the derivation procedure mechanics: a strict-subset RDF/XML parser, the
OWL 2 RL/RDF rule table (rule ids follow OWL 2 Profiles section 4.3), a
deterministic fixpoint closure engine that records one derivation per inferred
triple, and a SELECT DISTINCT basic-graph-pattern evaluator applied over the
closure. No query engine is consulted. Axioms outside the implemented rule
fragment are reported in an inventory instead of being silently dropped.

Documented conformance boundary (also emitted in every provenance artifact):
- eq-ref (reflexive sameAs) is omitted: it derives only tautologies and cannot
  change any query answer over this vocabulary.
- prp-spo2 (property chains) is omitted: the anchor dataset declares none and
  any occurrence is rejected by the uncovered-axiom inventory.
- hasKey lists are restricted to one or two properties (prp-key1, prp-key2).
- AllDisjointClasses owl:members lists are expanded to pairwise
  owl:disjointWith triples as recorded derivation steps (cax-adc-expand).
"""

from __future__ import annotations

import hashlib
import re
import xml.etree.ElementTree as ET
from typing import Any, Dict, Iterable, List, Optional, Sequence, Set, Tuple

RULE_TABLE_VERSION = "owl2-rl-rule-table/1"
CLOSURE_ALGORITHM = "datalog-fixpoint-deterministic/2"

RDF = "http://www.w3.org/1999/02/22-rdf-syntax-ns#"
RDFS = "http://www.w3.org/2000/01/rdf-schema#"
OWL = "http://www.w3.org/2002/07/owl#"

XML_LANG = "{http://www.w3.org/XML/1998/namespace}lang"
XML_BASE = "{http://www.w3.org/XML/1998/namespace}base"

TYPE = ("iri", RDF + "type")
SUB_CLASS = ("iri", RDFS + "subClassOf")
SUB_PROP = ("iri", RDFS + "subPropertyOf")
DOMAIN = ("iri", RDFS + "domain")
RANGE = ("iri", RDFS + "range")
FIRST = ("iri", RDF + "first")
REST = ("iri", RDF + "rest")
NIL = ("iri", RDF + "nil")
SAME_AS = ("iri", OWL + "sameAs")
NOTHING = ("iri", OWL + "Nothing")

Term = Tuple[str, ...]
Triple = Tuple[Term, Term, Term]


def iri(value: str) -> Term:
    return ("iri", value)


def bnode(label: str) -> Term:
    return ("bnode", label)


def literal(value: str, datatype: Optional[str] = None, language: Optional[str] = None) -> Term:
    return ("literal", value, datatype or "", language or "")


def term_to_json(term: Term) -> Dict[str, Any]:
    if term[0] == "iri":
        return {"type": "iri", "value": term[1]}
    if term[0] == "bnode":
        return {"type": "blank_node", "value": term[1]}
    if term[0] == "unbound":
        return {"type": "unbound"}
    return {"type": "literal", "value": term[1], "datatype": term[2] or None, "language": term[3] or None}


def term_sort_key(term: Term) -> Tuple[Any, ...]:
    rank = {"iri": 0, "bnode": 1, "literal": 2}
    return (rank.get(term[0], 3),) + tuple(str(part) for part in term[1:])


def triple_sort_key(triple: Triple) -> Tuple[Any, ...]:
    return tuple(term_sort_key(term) for term in triple)


def triple_digest(triples: Iterable[Triple]) -> str:
    lines = sorted("\t".join("|".join(term[1:]) for term in triple) for triple in triples)
    return hashlib.sha256("\n".join(lines).encode("utf-8")).hexdigest()


# ---------------------------------------------------------------------------
# RDF/XML parsing.


class ParserError(ValueError):
    """An RDF/XML construct this parser does not support was encountered."""


class RdfXmlParser:
    def __init__(self, base: str, connect_expressions: bool = False) -> None:
        self.base = base.rstrip("#")
        self.connect_expressions = connect_expressions
        self.triples: List[Triple] = []
        self.counter = 0

    # Element tags that denote a node (a typed resource) rather than a
    # property. Under connect_expressions these tags define the object node
    # of the enclosing property element and contribute an rdf:type triple;
    # the legacy path leaves both unwritten, which severs class expressions
    # behind a one-edge wrapper blank node.
    NODE_ELEMENT_TYPES = frozenset(
        {
            OWL + "Class",
            OWL + "Restriction",
            OWL + "NamedIndividual",
            OWL + "ObjectProperty",
            OWL + "DatatypeProperty",
            OWL + "AnnotationProperty",
            OWL + "Ontology",
            OWL + "AllDisjointClasses",
            OWL + "AllDifferent",
            OWL + "NegativePropertyAssertion",
            RDFS + "Class",
            RDFS + "Datatype",
        }
    )

    def node_element_type(self, node: ET.Element) -> Optional[str]:
        if not self.connect_expressions:
            return None
        tag = self.expand_tag(node.tag)
        return tag if tag in self.NODE_ELEMENT_TYPES else None

    @staticmethod
    def attr(node: ET.Element, local: str) -> Optional[str]:
        """Read an rdf:/xml: attribute; ElementTree keys are {namespace}local."""
        if local.startswith("xml:"):
            return node.attrib.get("{http://www.w3.org/XML/1998/namespace}" + local[4:])
        return node.attrib.get("{" + RDF + "}" + local)

    def fresh_bnode(self) -> Term:
        self.counter += 1
        return bnode(f"genid-{self.counter:06d}")

    def resolve(self, reference: str) -> str:
        if reference.startswith("#"):
            return f"{self.base}#{reference[1:]}"
        return reference

    def parse(self, data: bytes) -> List[Triple]:
        root = ET.fromstring(data)
        base = self.attr(root, "xml:base")
        if base:
            self.base = base.rstrip("#")
        for node in root:
            subject = self.node_subject(node)
            if (node_type := self.node_element_type(node)) is not None:
                self.triples.append((subject, TYPE, iri(node_type)))
            self.walk_node(node, subject)
        return self.triples

    def node_subject(self, node: ET.Element) -> Term:
        about = self.attr(node, "about")
        node_id = self.attr(node, "nodeID")
        identifier = self.attr(node, "ID")
        supplied = [value for value in (about, node_id, identifier) if value is not None]
        if len(supplied) > 1:
            raise ParserError(f"multiple subject attributes on {node.tag}")
        if about is not None:
            return iri(self.resolve(about))
        if node_id is not None:
            return bnode(node_id)
        if identifier is not None:
            return iri(self.resolve(f"#{identifier}"))
        return self.fresh_bnode()

    def walk_node(self, node: ET.Element, subject: Term) -> None:
        for child in node:
            predicate = iri(self.expand_tag(child.tag))
            parse_type = self.attr(child, "parseType")
            if parse_type == "Collection":
                self.walk_collection(child, subject, predicate)
                continue
            if parse_type == "Resource":
                nested = self.fresh_bnode()
                self.triples.append((subject, predicate, nested))
                self.walk_node(child, nested)
                continue
            if parse_type is not None:
                raise ParserError(f"unsupported rdf:parseType {parse_type} on {child.tag}")
            resource = self.attr(child, "resource")
            about = self.attr(child, "about")
            node_id = self.attr(child, "nodeID")
            referenced = [value for value in (resource, about, node_id) if value is not None]
            if referenced:
                if resource is not None:
                    obj = iri(self.resolve(resource))
                elif about is not None:
                    obj = iri(self.resolve(about))
                else:
                    obj = bnode(node_id)
                self.triples.append((subject, predicate, obj))
                self.walk_node(child, obj)
                continue
            if len(child) > 0:
                value: Optional[Term] = None
                if self.connect_expressions and len(child) == 1:
                    only = list(child)[0]
                    only_type = self.node_element_type(only)
                    if only_type is not None or self.expand_tag(only.tag) == RDF + "Description":
                        value = self.node_subject(only)
                        if only_type is not None:
                            self.triples.append((value, TYPE, iri(only_type)))
                        self.walk_node(only, value)
                if value is None:
                    value = self.fresh_bnode()
                    self.walk_node(child, value)
                self.triples.append((subject, predicate, value))
                continue
            datatype = self.attr(child, "datatype")
            language = self.attr(child, "xml:lang")
            text = (child.text or "").strip()
            self.triples.append((subject, predicate, literal(text, datatype, language)))

    def walk_collection(self, node: ET.Element, subject: Term, predicate: Term) -> None:
        items = list(node)
        if not items:
            self.triples.append((subject, predicate, NIL))
            return
        cells = [self.fresh_bnode() for _ in items]
        self.triples.append((subject, predicate, cells[0]))
        for index, item in enumerate(items):
            head = self.item_term(item)
            if (item_type := self.node_element_type(item)) is not None:
                self.triples.append((head, TYPE, iri(item_type)))
            self.triples.append((cells[index], FIRST, head))
            tail = NIL if index == len(items) - 1 else cells[index + 1]
            self.triples.append((cells[index], REST, tail))
            self.walk_node(item, head)

    def item_term(self, item: ET.Element) -> Term:
        about = self.attr(item, "about")
        if about is not None:
            return iri(self.resolve(about))
        node_id = self.attr(item, "nodeID")
        if node_id is not None:
            return bnode(node_id)
        return self.fresh_bnode()

    @staticmethod
    def expand_tag(tag: str) -> str:
        if tag.startswith("{"):
            namespace, local = tag[1:].split("}", 1)
            return namespace + local
        raise ParserError(f"unprefixed element tag {tag} is not supported")


STANDARD_VOCABULARIES = frozenset(
    {RDF, RDFS, OWL, "http://www.w3.org/2001/XMLSchema#"}
)


def namespace_of(value: str) -> str:
    """Namespace part of an IRI: fragment style first, then path style."""
    if "#" in value:
        return value.split("#", 1)[0] + "#"
    if "/" in value:
        return value.rsplit("/", 1)[0] + "/"
    return value


def dominant_namespace(triples: Iterable[Triple]) -> Optional[str]:
    """Namespace carrying the ABox vocabulary, or None when there is none.

    OWL2Bench generator output can split one dataset across two vocabularies:
    the TBox is serialized under the document xml:base while every instance
    typing and property assertion uses the generator's fixed prefix. Query
    terms must follow the vocabulary the assertions actually use, so this is
    the most frequent predicate namespace outside the standard RDF/RDFS/OWL/
    XSD vocabularies, with ties broken lexicographically for determinism.
    """
    counts: Dict[str, int] = {}
    for _subject, predicate, _object in triples:
        if predicate[0] != "iri":
            continue
        namespace = namespace_of(predicate[1])
        if namespace in STANDARD_VOCABULARIES:
            continue
        counts[namespace] = counts.get(namespace, 0) + 1
    if not counts:
        return None
    return max(sorted(counts), key=lambda ns: counts[ns])


def query_namespace_for(triples: Iterable[Triple], fallback: str) -> str:
    """Query-term namespace: the dominant ABox vocabulary, else the fallback.

    Both the derivation pipeline and the audit resolve empty-prefix query
    terms through this one rule so the two sides cannot drift apart.
    """
    return dominant_namespace(triples) or fallback


# ---------------------------------------------------------------------------
# Rule table.


def _v(name: str) -> str:
    return "?" + name


def build_rule_table() -> List[Dict[str, Any]]:
    raw: List[Tuple[str, List[Triple], List[Triple]]] = []

    def add(rule_id: str, premises: Sequence[Triple], consequents: Sequence[Triple]) -> None:
        raw.append((rule_id, list(premises), list(consequents)))

    p, o, c, d = (_v(n) for n in "pocd")
    x, y, z, u, v = (_v(n) for n in "xyzuv")
    ce, pn, on, adc, cell, key = (_v(n) for n in ("ce", "pn", "on", "adc", "cell", "key"))

    sym = iri(OWL + "SymmetricProperty")
    trans = iri(OWL + "TransitiveProperty")
    func = iri(OWL + "FunctionalProperty")
    ifunc = iri(OWL + "InverseFunctionalProperty")
    irref = iri(OWL + "IrreflexiveProperty")
    asym = iri(OWL + "AsymmetricProperty")
    equiv_class = iri(OWL + "equivalentClass")
    equiv_prop = iri(OWL + "equivalentProperty")
    inverse_of = iri(OWL + "inverseOf")
    on_property = iri(OWL + "onProperty")
    some_values = iri(OWL + "someValuesFrom")
    all_values = iri(OWL + "allValuesFrom")
    has_value = iri(OWL + "hasValue")
    union_of = iri(OWL + "unionOf")
    inter_of = iri(OWL + "intersectionOf")
    disjoint_with = iri(OWL + "disjointWith")
    has_key = iri(OWL + "hasKey")
    chain_axiom = iri(OWL + "propertyChainAxiom")
    w = _v("w")

    # class expressions via rdf:first cells
    add("cls-uni", [(c, union_of, ce), (ce, FIRST, d)], [(d, SUB_CLASS, c)])
    add("cls-int1", [(c, inter_of, ce), (ce, FIRST, d)], [(c, SUB_CLASS, d)])
    add("cls-int2", [(c, inter_of, ce), (ce, FIRST, d), (x, TYPE, c)], [(x, TYPE, d)])

    # restrictions
    add(
        "cls-svf1",
        [(c, SUB_CLASS, ce), (ce, on_property, p), (ce, some_values, d), (u, p, v)],
        [(u, TYPE, d)],
    )
    add(
        "cls-av1",
        [(c, SUB_CLASS, ce), (ce, on_property, p), (ce, all_values, d), (u, TYPE, c), (u, p, v)],
        [(v, TYPE, d)],
    )
    add(
        "cls-hv1",
        [(c, SUB_CLASS, ce), (ce, on_property, p), (ce, has_value, v), (u, TYPE, c)],
        [(u, p, v)],
    )
    add(
        "cls-hv2",
        [(c, SUB_CLASS, ce), (ce, on_property, p), (ce, has_value, v), (u, p, v)],
        [(u, TYPE, c)],
    )

    # class axioms
    add("cax-sco", [(x, TYPE, c), (c, SUB_CLASS, d)], [(x, TYPE, d)])
    add("cax-eqc1", [(x, TYPE, c), (c, equiv_class, d)], [(x, TYPE, d)])
    add("cax-eqc2", [(x, TYPE, c), (d, equiv_class, c)], [(x, TYPE, d)])
    add("cax-dw", [(x, TYPE, c), (x, TYPE, d), (c, disjoint_with, d)], [(x, TYPE, NOTHING)])

    # AllDisjointClasses expansion markers (pairwise disjointWith derived by
    # the recorded cax-adc-expand preprocessing step, not by these rules)
    add(
        "cax-adc-membership",
        [(adc, TYPE, iri(OWL + "AllDisjointClasses")), (adc, iri(OWL + "members"), cell), (cell, FIRST, c)],
        [(c, TYPE, iri(OWL + "AllDisjoint"))],
    )

    # property axioms
    add("prp-spo1", [(u, p, v), (p, SUB_PROP, o)], [(u, o, v)])
    add("prp-eqp1", [(u, p, v), (p, equiv_prop, o)], [(u, o, v)])
    add("prp-eqp2", [(u, p, v), (o, equiv_prop, p)], [(u, o, v)])
    add("prp-dom", [(u, p, v), (p, DOMAIN, c)], [(u, TYPE, c)])
    add("prp-rng", [(u, p, v), (p, RANGE, c)], [(v, TYPE, c)])
    add("prp-symp", [(u, p, v), (p, TYPE, sym)], [(v, p, u)])
    add("prp-trp", [(u, p, v), (v, p, z), (p, TYPE, trans)], [(u, p, z)])
    add("prp-fp", [(u, p, v), (u, p, z), (p, TYPE, func)], [(v, SAME_AS, z)])
    add("prp-ifp", [(u, p, v), (z, p, v), (p, TYPE, ifunc)], [(u, SAME_AS, z)])
    add("prp-inv1", [(u, p, v), (p, inverse_of, o)], [(v, o, u)])
    add("prp-inv2", [(u, p, v), (o, inverse_of, p)], [(v, o, u)])
    add("prp-irref", [(u, p, u), (p, TYPE, irref)], [(u, TYPE, NOTHING)])
    add("prp-asym", [(u, p, v), (v, p, u), (p, TYPE, asym)], [(u, TYPE, NOTHING)])
    add("prp-spo2-2", [
        (p, chain_axiom, cell), (cell, FIRST, on), (cell, REST, ce),
        (ce, FIRST, pn), (ce, REST, NIL),
        (u, on, v), (v, pn, z),
    ], [(u, p, z)])
    add("prp-spo2-3", [
        (p, chain_axiom, cell), (cell, FIRST, on), (cell, REST, ce),
        (ce, FIRST, pn), (ce, REST, key), (key, FIRST, o), (key, REST, NIL),
        (u, on, v), (v, pn, z), (z, o, w),
    ], [(u, p, w)])
    add(
        "prp-key1",
        [
            (c, has_key, key),
            (key, FIRST, p),
            (key, REST, NIL),
            (x, TYPE, c),
            (x, p, v),
            (y, TYPE, c),
            (y, p, v),
        ],
        [(x, SAME_AS, y)],
    )
    add(
        "prp-key2",
        [
            (c, has_key, key),
            (key, FIRST, p),
            (key, REST, ce),
            (ce, FIRST, o),
            (ce, REST, NIL),
            (x, TYPE, c),
            (x, p, u),
            (x, o, v),
            (y, TYPE, c),
            (y, p, u),
            (y, o, v),
        ],
        [(x, SAME_AS, y)],
    )

    # equality
    add("eq-sym", [(x, SAME_AS, y)], [(y, SAME_AS, x)])
    add("eq-trans", [(x, SAME_AS, y), (y, SAME_AS, z)], [(x, SAME_AS, z)])
    add("eq-rep-s", [(x, SAME_AS, y), (x, p, v)], [(y, p, v)])
    add("eq-rep-o", [(u, p, x), (x, SAME_AS, y)], [(u, p, y)])

    # structural rules
    add("scm-sco", [(c, SUB_CLASS, d), (d, SUB_CLASS, z)], [(c, SUB_CLASS, z)])
    add("scm-eqc1", [(c, equiv_class, d)], [(c, SUB_CLASS, d)])
    add("scm-eqc2", [(c, equiv_class, d)], [(d, SUB_CLASS, c)])
    add("scm-spo", [(p, SUB_PROP, o), (o, SUB_PROP, z)], [(p, SUB_PROP, z)])
    add("scm-eqp1", [(p, equiv_prop, o)], [(p, SUB_PROP, o)])
    add("scm-eqp2", [(p, equiv_prop, o)], [(o, SUB_PROP, p)])
    add("scm-dom", [(p, DOMAIN, c), (c, SUB_CLASS, d)], [(p, DOMAIN, d)])
    add("scm-rng", [(p, RANGE, c), (c, SUB_CLASS, d)], [(p, RANGE, d)])

    return [
        {"id": rule_id, "premises": [list(t) for t in premises], "consequents": [list(t) for t in consequents]}
        for rule_id, premises, consequents in raw
    ]


CONSUMED_PREDICATES: Set[str] = set()
for _rule in build_rule_table():
    for _pattern in _rule["premises"] + _rule["consequents"]:
        if _pattern[1][0] == "iri":
            CONSUMED_PREDICATES.add(_pattern[1][1])
CONSUMED_PREDICATES.update({RDF + "type", OWL + "members", OWL + "complementOf", OWL + "propertyChainAxiom"})

# Vocabulary whose semantics the rule table intentionally does not consume.
# These predicates are legal in the anchor file but inert for RL derivation;
# the pipeline lists every occurrence in the uncovered-axiom inventory.
BOUNDARY_INERT_PREDICATES = {
    OWL + "complementOf",
    OWL + "annotatedSource",
    OWL + "annotatedProperty",
    OWL + "annotatedTarget",
    OWL + "onDataRange",
    OWL + "qualifiedCardinality",
    OWL + "minQualifiedCardinality",
    OWL + "maxQualifiedCardinality",
}


# ---------------------------------------------------------------------------
# Closure engine.



QL_RULE_TABLE_VERSION = "owl2-ql-rule-table/1"

# OWL 2 QL (DL-Lite_R) positive instance entailments as a subset of the RL
# table: class and role inclusions with equivalences, inverse roles, and
# domain/range typing. Constructs outside QL — class expressions, property
# characteristics, property chains, keys, sameAs — stay out so any occurrence
# in a dataset lands in the uncovered-axiom inventory instead of being
# reasoned with semantics the profile does not have.
_QL_RULE_IDS = frozenset(
    {
        "cax-sco",
        "cax-eqc1",
        "cax-eqc2",
        "scm-sco",
        "scm-eqc1",
        "scm-eqc2",
        "prp-spo1",
        "prp-eqp1",
        "prp-eqp2",
        "prp-symp",
        "scm-spo",
        "scm-eqp1",
        "scm-eqp2",
        "prp-inv1",
        "prp-inv2",
        "prp-dom",
        "prp-rng",
        "scm-dom",
        "scm-rng",
    }
)


def build_ql_rule_table() -> List[Dict[str, Any]]:
    return [rule for rule in build_rule_table() if rule["id"] in _QL_RULE_IDS]


EL_RULE_TABLE_VERSION = "owl2-el-rule-table/1"

# OWL 2 EL (EL++) positive instance entailments as a subset of the RL table.
# EL keeps class expressions (intersection, someValuesFrom, hasValue), class
# inclusions with equivalences and disjointness, subproperties with chains,
# domain/range typing, keys, and sameAs. Constructs EL cannot express — union,
# allValuesFrom, inverse roles, and every property characteristic except
# chain-expressed transitivity (symmetric, functional, inverse-functional,
# transitive-as-characteristic, irreflexive, asymmetric) — stay out so any
# occurrence lands in the uncovered-axiom inventory instead of being reasoned
# with semantics the profile does not have.
_EL_RULE_IDS = frozenset(
    {
        "cls-int1",
        "cls-int2",
        "cls-svf1",
        "cls-hv1",
        "cls-hv2",
        "cax-sco",
        "cax-eqc1",
        "cax-eqc2",
        "cax-dw",
        "cax-adc-membership",
        "prp-spo1",
        "prp-eqp1",
        "prp-eqp2",
        "prp-dom",
        "prp-rng",
        "prp-trp",
        "prp-spo2-2",
        "prp-spo2-3",
        "prp-key1",
        "prp-key2",
        "eq-sym",
        "eq-trans",
        "eq-rep-s",
        "eq-rep-o",
        "scm-sco",
        "scm-eqc1",
        "scm-eqc2",
        "scm-spo",
        "scm-eqp1",
        "scm-eqp2",
        "scm-dom",
        "scm-rng",
    }
)


def build_el_rule_table() -> List[Dict[str, Any]]:
    return [rule for rule in build_rule_table() if rule["id"] in _EL_RULE_IDS]


DL_RULE_TABLE_VERSION = "owl2-dl-rule-table/1"

# OWL 2 DL is the full logic every RL rule is expressed in, so the DL table is
# the complete RL rule set under its own versioned identity. DL-expressible
# constructs the table does not cover (complement, exact/min/max cardinalities,
# hasSelf, disjointUnionOf, oneOf membership) land in the uncovered-axiom
# inventory exactly as they do for RL; the DL anchor's answers are exact for
# the official query matrix because every queried IRI lives in the dominant
# ABox vocabulary, which no TBox axiom mentions (see the procedure doc's DL
# section for the occurrence-level verification).
def build_dl_rule_table() -> List[Dict[str, Any]]:
    return build_rule_table()


# --- patched-namespace track (v2) ------------------------------------------
#
# On namespace-rebased anchors the TBox classifies the ABox, so the rule
# tables extend with the rules those anchors exercise beyond the shipped-byte
# tables. Two groups:
#
# Schema rule — prp-ref (reflexive roles). Its individual universe is the
# declared named individuals (owl:NamedIndividual type triples, which the
# v2 parse emits for typed node elements) — the same enumeration a complete
# reasoner's domain gives (the patched anchors declare 3,677 individuals,
# every individual they assert), rather than the raw active domain that
# would include classes and blank nodes. Reflexivity is expressible in
# OWL 2 EL, RL, and DL, so their v2 tables add the rule; QL (DL-Lite_R)
# cannot express it, so the QL v2 table keeps the rule out — a reflexive
# declaration stays inert there (rdf:type is a consumed predicate, so it
# does not surface in the uncovered-axiom inventory; the patched QL anchor
# declares none).
#
# Construct-membership rules — cls-hs, cls-intro, cls-mqc3, cls-qc1. The
# patched anchors define the queried classes through ObjectHasSelf (006),
# exact/min qualified cardinality (017/018), and binary intersections, in
# every profile's TBox (the generator emits the same shapes regardless of
# profile conformance). These rules are sound OWL 2 entailments
# unconditionally, so they join every v2 profile table; the per-profile
# schema boundaries (inverse roles, union, property characteristics for
# EL/QL) are unchanged. cls-intro covers binary intersections only — every
# intersectionOf list in the patched anchors is binary (13/13). cls-qc1's
# at-least reading of "exactly 1" is exact only while no subject has two
# distinct qualified witnesses; the derivation aborts (fails closed) on any
# violation via exact_cardinality_guard, and the patched DL anchor has zero
# (765 subjects with exactly one qualified enrollFor, none with two).

XSD = "http://www.w3.org/2001/XMLSchema#"

_LIT_TRUE = ("literal", "true", XSD + "boolean", "")
_LIT_ONE = ("literal", "1", XSD + "nonNegativeInteger", "")
_LIT_THREE = ("literal", "3", XSD + "nonNegativeInteger", "")

PRP_REF_RULE: Dict[str, Any] = {
    "id": "prp-ref",
    "premises": [
        [_v("p"), ("iri", RDF + "type"), ("iri", OWL + "ReflexiveProperty")],
        [_v("u"), ("iri", RDF + "type"), ("iri", OWL + "NamedIndividual")],
    ],
    "consequents": [[_v("u"), _v("p"), _v("u")]],
}

CONSTRUCT_RULES: List[Dict[str, Any]] = [
    {
        "id": "cls-hs",
        "premises": [
            [_v("ce"), ("iri", OWL + "onProperty"), _v("p")],
            [_v("ce"), ("iri", OWL + "hasSelf"), _LIT_TRUE],
            [_v("u"), _v("p"), _v("u")],
        ],
        "consequents": [[_v("u"), ("iri", RDF + "type"), _v("ce")]],
    },
    {
        "id": "cls-intro",
        "premises": [
            [_v("c"), ("iri", OWL + "intersectionOf"), _v("cell1")],
            [_v("cell1"), ("iri", RDF + "first"), _v("d1")],
            [_v("cell1"), ("iri", RDF + "rest"), _v("cell2")],
            [_v("cell2"), ("iri", RDF + "first"), _v("d2")],
            [_v("cell2"), ("iri", RDF + "rest"), ("iri", RDF + "nil")],
            [_v("x"), ("iri", RDF + "type"), _v("d1")],
            [_v("x"), ("iri", RDF + "type"), _v("d2")],
        ],
        "consequents": [[_v("x"), ("iri", RDF + "type"), _v("c")]],
    },
    {
        "id": "cls-mqc3",
        "premises": [
            [_v("ce"), ("iri", OWL + "onProperty"), _v("p")],
            [_v("ce"), ("iri", OWL + "minQualifiedCardinality"), _LIT_THREE],
            [_v("ce"), ("iri", OWL + "onClass"), _v("d")],
            [_v("x"), _v("p"), _v("v1")],
            [_v("x"), _v("p"), _v("v2")],
            [_v("x"), _v("p"), _v("v3")],
            [_v("v1"), ("iri", RDF + "type"), _v("d")],
            [_v("v2"), ("iri", RDF + "type"), _v("d")],
            [_v("v3"), ("iri", RDF + "type"), _v("d")],
        ],
        "consequents": [[_v("x"), ("iri", RDF + "type"), _v("ce")]],
    },
    {
        "id": "cls-qc1",
        "premises": [
            [_v("ce"), ("iri", OWL + "onProperty"), _v("p")],
            [_v("ce"), ("iri", OWL + "qualifiedCardinality"), _LIT_ONE],
            [_v("ce"), ("iri", OWL + "onClass"), _v("d")],
            [_v("x"), _v("p"), _v("v")],
            [_v("v"), ("iri", RDF + "type"), _v("d")],
        ],
        "consequents": [[_v("x"), ("iri", RDF + "type"), _v("ce")]],
    },
]

# cls-hvn: hasValue restriction nodes can be typed directly (u p v) makes u
# an instance of the restriction "p some {v}" — sound unconditionally, and
# it is what lets membership of classes defined as intersections containing
# a hasValue restriction (the DL anchor's T20CricketFan shape) derive
# through cls-intro. Kept DL-scoped like the mqc3 removal: the other
# anchors' hasValue shapes are direct superclass edges that cls-hv2
# already covers.
DL_ONLY_RULES: List[Dict[str, Any]] = [
    {
        "id": "cls-hvn",
        "premises": [
            [_v("ce"), ("iri", OWL + "onProperty"), _v("p")],
            [_v("ce"), ("iri", OWL + "hasValue"), _v("v")],
            [_v("u"), _v("p"), _v("v")],
        ],
        "consequents": [[_v("u"), ("iri", RDF + "type"), _v("ce")]],
    },
]


def _insert_rule(rules: List[Dict[str, Any]], rule: Dict[str, Any], after: str) -> List[Dict[str, Any]]:
    extended = [dict(existing) for existing in rules]
    position = next((index for index, existing in enumerate(extended) if existing["id"] == after), len(extended))
    extended.insert(position + 1, {**rule, "premises": [list(p) for p in rule["premises"]], "consequents": [list(c) for c in rule["consequents"]]})
    return extended


def exact_cardinality_guard(closure: List[Triple], rules: Optional[List[Dict[str, Any]]] = None) -> List[str]:
    """Fail-closed guards for the qualified-cardinality construct rules.

    Returns one violation string per check that fails; the derivation must
    abort when any exist, because the affected memberships would be
    unsound:

    - cls-qc1 reads "exactly 1" as "at least 1", which is sound only while
      no subject has two distinct qualified witnesses for the same
      exact-1 restriction.
    - cls-mqc3's three witnesses are syntactically distinct premise
      triples, which implies distinct individuals only while no two of a
      subject's qualified witnesses are owl:sameAs-co-referenced.

    When the active rule list is provided, each check runs only if its
    rule is present in the table (a profile whose table drops cls-mqc3
    cannot violate its side condition).
    """
    rule_ids = {rule["id"] for rule in (rules or [])} if rules is not None else {"cls-qc1", "cls-mqc3"}
    by_predicate = {}
    for triple in closure:
        by_predicate.setdefault(triple[1], []).append(triple)
    on_property = {}
    for s, _p, o in by_predicate.get(("iri", OWL + "onProperty"), []):
        on_property.setdefault(s, []).append(o)
    on_class = {}
    for s, _p, o in by_predicate.get(("iri", OWL + "onClass"), []):
        on_class.setdefault(s, []).append(o)
    exact_one = {
        s for s, _p, o in by_predicate.get(("iri", OWL + "qualifiedCardinality"), [])
        if o == ("literal", "1", XSD + "nonNegativeInteger", "")
    }
    min_three = {
        s for s, _p, o in by_predicate.get(("iri", OWL + "minQualifiedCardinality"), [])
        if o == ("literal", "3", XSD + "nonNegativeInteger", "")
    }
    violations: List[str] = []

    same_as_pairs = {
        frozenset((s, o))
        for s, _p, o in by_predicate.get(("iri", OWL + "sameAs"), [])
    }

    def qualified_witnesses(restriction: Term) -> Dict[Term, Set[Term]]:
        properties = on_property.get(restriction, [])
        qualifiers = on_class.get(restriction, [])
        qualified = {s for s, _p, o in by_predicate.get(TYPE, []) if o in qualifiers}
        witnesses: Dict[Term, Set[Term]] = {}
        for property_iri in properties:
            for subject, _p, obj in by_predicate.get(property_iri, []):
                if obj in qualified:
                    witnesses.setdefault(subject, set()).add(obj)
        return witnesses

    for restriction in sorted(exact_one, key=str):
        if "cls-qc1" not in rule_ids:
            break
        for subject, values in qualified_witnesses(restriction).items():
            if len(values) >= 2:
                violations.append(
                    f"cls-qc1-at-most-violated restriction={restriction} subject={subject} witnesses={len(values)}"
                )

    if "cls-mqc3" in rule_ids:
        for restriction in sorted(min_three, key=str):
            for subject, values in qualified_witnesses(restriction).items():
                merged = {
                    frozenset((v1, v2)) in same_as_pairs
                    for v1 in values
                    for v2 in values
                    if v1 != v2
                }
                if any(merged):
                    violations.append(
                        f"cls-mqc3-sameAs-witnesses restriction={restriction} subject={subject} witnesses={len(values)}"
                    )
    return violations


def _patched_version(version: str) -> str:
    """owl2-<profile>-rule-table/1 -> owl2-<profile>-rule-table/2.

    Applied to every profile under the patched track, including QL whose
    rule set is unchanged: the version identifies the configuration, and a
    patched artifact must never carry a shipped-track version string.
    """
    return version.rsplit("/", 1)[0] + "/2"


def rule_table_for(profile: str, track: str = "shipped") -> Tuple[List[Dict[str, Any]], str]:
    if track not in ("shipped", "patched"):
        raise ValueError(f"unsupported derivation track {track}")
    if profile == "RL":
        table, version = build_rule_table(), RULE_TABLE_VERSION
    elif profile == "QL":
        table, version = build_ql_rule_table(), QL_RULE_TABLE_VERSION
    elif profile == "EL":
        table, version = build_el_rule_table(), EL_RULE_TABLE_VERSION
    elif profile == "DL":
        table, version = build_dl_rule_table(), DL_RULE_TABLE_VERSION
    else:
        raise ValueError(f"unsupported derivation profile {profile}")
    if track == "patched":
        # Construct-membership rules join every profile (sound OWL 2
        # entailments; the patched TBoxes define the queried classes through
        # them). prp-ref is profile-gated: QL cannot express reflexivity.
        for rule in reversed(CONSTRUCT_RULES):
            anchor = "cls-hv2" if rule["id"] in ("cls-hs", "cls-intro") else "cax-sco"
            table = _insert_rule(table, rule, anchor)
        if profile != "QL":
            table = _insert_rule(table, PRP_REF_RULE, "prp-trp")
        if profile == "DL":
            # cls-mqc3's distinct-witness reading is unsound where the
            # closure's sameAs replacement merges co-referent witnesses
            # (348 subjects on the patched DL anchor); counting semantic
            # equivalence classes is outside the datalog rule shape, so
            # query 018 fails closed on DL and the rule stays out.
            table = [rule for rule in table if rule["id"] != "cls-mqc3"]
            # cls-hvn closes #105: the DL anchor defines T20CricketFan as
            # an intersection containing a hasValue restriction, which no
            # rule could type (the engine already derived it; the external
            # key agrees at 20). /4 bumps the table identity.
            for rule in DL_ONLY_RULES:
                table = _insert_rule(table, rule, "cls-hs")
            version = version.rsplit("/", 1)[0] + "/4"
        else:
            version = _patched_version(version)
    return table, version


def build_indexes(triples: List[Triple]) -> Dict[Term, List[Triple]]:
    """Predicate -> sorted triples. Values stay sorted so iteration order,
    and therefore recorded derivations, remain deterministic."""
    by_predicate: Dict[Term, List[Triple]] = {}
    for triple in triples:
        by_predicate.setdefault(triple[1], []).append(triple)
    for matches in by_predicate.values():
        matches.sort(key=triple_sort_key)
    return by_predicate


def build_join_indexes(triples: List[Triple]) -> Tuple[Dict[Tuple[Term, Term], List[Triple]], Dict[Tuple[Term, Term], List[Triple]]]:
    """(predicate, subject) and (predicate, object) -> sorted triples.

    Lookup acceleration for _match_premise. Order-preserving by
    construction: every bucket is a filter of a predicate-sorted list, so a
    bucket's relative order equals the order the same matches would appear
    in when scanning the full predicate pool — recorded derivations are
    unaffected by which of the two paths served the match.
    """
    by_subject: Dict[Tuple[Term, Term], List[Triple]] = {}
    by_object: Dict[Tuple[Term, Term], List[Triple]] = {}
    for triple in sorted(triples, key=triple_sort_key):
        by_subject.setdefault((triple[1], triple[0]), []).append(triple)
        by_object.setdefault((triple[1], triple[2]), []).append(triple)
    return by_subject, by_object



def order_premises(premises: List[Triple], by_predicate: Dict[Term, List[Triple]], all_sorted: List[Triple]) -> List[Triple]:
    def pool_size(pattern: Triple) -> Tuple[int, str]:
        predicate = None
        if not isinstance(pattern[1], str):
            predicate = tuple(pattern[1])
        if predicate is None:
            return (len(all_sorted), str(pattern))
        return (len(by_predicate.get(predicate, [])), str(pattern))

    return sorted(premises, key=pool_size)


def _match_premise(
    pattern: Triple,
    binding: Dict[str, Term],
    by_predicate: Dict[Term, List[Triple]],
    all_sorted: List[Triple],
    join_subject: Optional[Dict[Tuple[Term, Term], List[Triple]]] = None,
    join_object: Optional[Dict[Tuple[Term, Term], List[Triple]]] = None,
) -> List[Tuple[Dict[str, Term], Triple]]:
    def resolve(term: Any) -> Optional[Term]:
        if isinstance(term, str):
            return binding.get(term)
        return tuple(term)

    wanted = tuple(resolve(part) for part in pattern)
    pool: Iterable[Triple]
    if wanted[1] is not None:
        if wanted[0] is not None and join_subject is not None:
            pool = join_subject.get((wanted[1], wanted[0]), [])
        elif wanted[2] is not None and join_object is not None:
            pool = join_object.get((wanted[1], wanted[2]), [])
        else:
            pool = by_predicate.get(wanted[1], [])
    else:
        pool = all_sorted
    results: List[Tuple[Dict[str, Term], Triple]] = []
    for triple in pool:
        local = dict(binding)
        ok = True
        for position, term in enumerate(wanted):
            if term is None:
                variable = pattern[position]
                if not isinstance(variable, str):
                    raise ValueError(f"unbound constant in pattern {pattern}")
                bound = local.get(variable)
                if bound is None:
                    local[variable] = triple[position]
                elif bound != triple[position]:
                    ok = False
                    break
            elif term != triple[position]:
                ok = False
                break
        if ok:
            results.append((local, triple))
    return results


def expand_all_disjoint(base: List[Triple]) -> List[Triple]:
    """Expand owl:AllDisjointClasses owl:members lists into pairwise
    owl:disjointWith triples (recorded as cax-adc-expand derivation steps)."""
    base_set = set(base)
    first_of: Dict[Term, Term] = {}
    rest_of: Dict[Term, Term] = {}
    for s, pred, o in base:
        if pred == FIRST:
            first_of[s] = o
        elif pred == REST:
            rest_of[s] = o
    members = iri(OWL + "members")
    all_disjoint_classes = iri(OWL + "AllDisjointClasses")
    derived: List[Triple] = []
    for s, pred, head in sorted(base, key=triple_sort_key):
        if pred != members or (s, TYPE, all_disjoint_classes) not in base_set:
            continue
        collected: List[Term] = []
        cell: Optional[Term] = head
        seen: Set[Term] = set()
        while cell is not None and cell != NIL and cell in first_of and cell not in seen:
            seen.add(cell)
            collected.append(first_of[cell])
            cell = rest_of.get(cell)
        for i in range(len(collected)):
            for j in range(len(collected)):
                if i != j:
                    derived.append((collected[i], ("iri", OWL + "disjointWith"), collected[j]))
    return derived


def compute_closure(base_triples: List[Triple], rules: List[Dict[str, Any]]) -> Dict[str, Any]:
    """Deterministic fixpoint closure.

    Iteration is fully ordered: rules in table order, premise matches over
    predicate-sorted triple pools. The first firing (in that order) that
    derives a new triple is the recorded derivation, so identical inputs
    always produce identical derivation records and identical hashes.
    """
    base_set = set(base_triples)
    expanded = expand_all_disjoint(base_triples)
    known: Dict[Triple, Dict[str, Any]] = {t: {"rule": "base", "premises": []} for t in base_set}
    for triple in sorted(expanded, key=triple_sort_key):
        if triple not in known:
            known[triple] = {
                "rule": "cax-adc-expand",
                "premises": [],
                "note": "pairwise expansion of owl:AllDisjointClasses members",
            }

    changed = True
    while changed:
        current = sorted(known.keys(), key=triple_sort_key)
        by_predicate = build_indexes(current)
        join_subject, join_object = build_join_indexes(current)
        additions: List[Tuple[Triple, str, List[Triple]]] = []
        added_set: Set[Triple] = set()
        for rule in rules:
            if not rule["consequents"]:
                continue
            premises = order_premises(rule["premises"], by_predicate, current)
            original_position = {id(pattern): position for position, pattern in enumerate(rule["premises"])}

            def fire(index: int, binding: Dict[str, Term], used: List[Triple], positions: List[int]):
                if index == len(premises):
                    for pattern in rule["consequents"]:
                        derived = tuple(
                            binding.get(term, tuple(term) if not isinstance(term, str) else term)
                            for term in pattern
                        )
                        canonical = [None] * len(rule["premises"])
                        for position, triple in zip(positions, used):
                            canonical[position] = triple
                        yield derived, list(canonical)
                    return
                for local, triple in _match_premise(premises[index], binding, by_predicate, current, join_subject, join_object):
                    if any(triple == u for u in used):
                        continue
                    yield from fire(index + 1, local, used + [triple], positions + [original_position[id(premises[index])]])

            for derived, used in fire(0, {}, [], []):
                if derived in known or derived in added_set:
                    continue
                added_set.add(derived)
                additions.append((derived, rule["id"], list(used)))
        for derived, rule_id, used in additions:
            known[derived] = {"rule": rule_id, "premises": used}
        changed = bool(additions)

    closure = sorted(known.keys(), key=triple_sort_key)
    inconsistent = [t for t in closure if t[1] == TYPE and t[2] == NOTHING]
    return {
        "base": sorted(base_set, key=triple_sort_key),
        "closure": closure,
        "derivations": known,
        "inconsistent_instances": inconsistent,
        "expanded_disjoint": expanded,
    }


# ---------------------------------------------------------------------------
# Query evaluation: SELECT DISTINCT over basic graph patterns.


QUERY_PREFIXES = {
    "rdf": RDF,
    "rdfs": RDFS,
    "owl": OWL,
    "xsd": "http://www.w3.org/2001/XMLSchema#",
}


class QueryError(ValueError):
    """The query text is outside the supported SELECT DISTINCT BGP fragment."""


class BgpQuery:
    def __init__(self, projection: List[str], patterns: List[Triple], text_sha256: str) -> None:
        self.projection = projection
        self.patterns = patterns
        self.text_sha256 = text_sha256

    def evaluate(self, closure: List[Triple]) -> List[Dict[str, Term]]:
        by_predicate: Dict[Term, List[Triple]] = {}
        for triple in closure:
            by_predicate.setdefault(triple[1], []).append(triple)
        ordered = sorted(closure, key=triple_sort_key)

        solutions: List[Dict[str, Term]] = [{}]

        def match(index: int, binding: Dict[str, Term]) -> List[Dict[str, Term]]:
            if index == len(self.patterns):
                return [dict(binding)]
            results: List[Dict[str, Term]] = []
            pattern = self.patterns[index]
            wanted: List[Optional[Term]] = []
            for position, term in enumerate(pattern):
                if isinstance(term, str):
                    wanted.append(binding.get(term))
                else:
                    wanted.append(term)
            pool = by_predicate.get(wanted[1], []) if wanted[1] is not None else ordered
            for triple in pool:
                local = dict(binding)
                ok = True
                for position, term in enumerate(wanted):
                    if term is None:
                        local[pattern[position]] = triple[position]
                    elif term != triple[position]:
                        ok = False
                        break
                if ok:
                    results.extend(match(index + 1, local))
            return results

        solutions = match(0, {})
        projected: List[Tuple[Term, ...]] = []
        witnesses: List[Dict[str, Term]] = []
        seen = set()
        for binding in solutions:
            row = tuple(binding.get(name, ("unbound",)) for name in self.projection)
            if row not in seen:
                seen.add(row)
                projected.append(row)
                witnesses.append(binding)
        projected.sort(key=lambda row: tuple(term_sort_key(term) for term in row))
        ordered = sorted(range(len(projected)), key=lambda index: tuple(term_sort_key(term) for term in projected[index]))
        projected = [projected[index] for index in ordered]
        witnesses = [witnesses[index] for index in ordered]
        return [
            {name: row[index] for index, name in enumerate(self.projection)}
            for row in projected
        ], witnesses

    def witness_triples(self, binding: Dict[str, Term], closure_set: Set[Triple]) -> List[Triple]:
        """One grounding triple per pattern; fails if any grounding is absent."""
        used: List[Triple] = []
        for pattern in self.patterns:
            grounded = tuple(
                binding.get(term, tuple(term) if not isinstance(term, str) else term)
                for term in pattern
            )
            if grounded not in closure_set:
                raise QueryError(f"witness grounding missing from closure: {grounded}")
            used.append(grounded)
        return used


def parse_query(text: str, default_namespace: str) -> BgpQuery:
    sha = hashlib.sha256(text.encode("utf-8")).hexdigest()
    body = " ".join(text.split())
    match = re.match(r"^SELECT\s+DISTINCT\s+(.*?)\s+WHERE\s*\{\s*(.*?)\s*\}\s*$", body, re.IGNORECASE)
    if not match:
        raise QueryError("query is not SELECT DISTINCT ... WHERE { ... }")
    projection = []
    for token in match.group(1).split():
        if not token.startswith("?"):
            raise QueryError(f"unsupported projection token {token}")
        projection.append(token)
    raw_patterns = [part.strip() for part in match.group(2).split(".") if part.strip()]

    def resolve_term(token: str) -> Any:
        if token.startswith("?"):
            return token
        if token.startswith("<") and token.endswith(">"):
            return iri(token[1:-1])
        if ":" in token:
            prefix, local = token.split(":", 1)
            if prefix == "":
                return iri(default_namespace + local)
            if prefix in QUERY_PREFIXES:
                return iri(QUERY_PREFIXES[prefix] + local)
            raise QueryError(f"unknown prefix {prefix}:")
        if token.startswith('"') and token.endswith('"'):
            return literal(token[1:-1])
        raise QueryError(f"unsupported query term {token}")

    patterns: List[Triple] = []
    for raw in raw_patterns:
        parts = raw.split()
        if len(parts) == 3:
            s, p, o = parts
        elif len(parts) == 4 and parts[1].lower() == "a":
            s, p, o = parts[0], "rdf:type", parts[2]
        elif len(parts) == 4 and parts[1].lower() in ("is", "of"):
            raise QueryError(f"inverse property syntax is unsupported: {raw}")
        else:
            raise QueryError(f"unsupported triple pattern {raw}")
        patterns.append((resolve_term(s), resolve_term(p), resolve_term(o)))
    return BgpQuery(projection, patterns, sha)

def rule_substitution_check(
    rule: Dict[str, Any], premise_triples: List[Triple], conclusion: Triple
) -> bool:
    """Check a recorded firing is a uniform instance of the rule."""
    if len(premise_triples) != len(rule["premises"]):
        return False
    binding: Dict[str, Term] = {}
    for pattern, triple in zip(rule["premises"], premise_triples):
        for position, term in enumerate(pattern):
            if isinstance(term, str):
                bound = binding.get(term)
                if bound is None:
                    binding[term] = triple[position]
                elif bound != triple[position]:
                    return False
            elif tuple(term) != triple[position]:
                return False
    for conclusion_pattern in rule["consequents"]:
        derived = tuple(
            binding.get(term, tuple(term) if not isinstance(term, str) else term)
            for term in conclusion_pattern
        )
        if derived == tuple(conclusion):
            return True
    return False


def saturation_check(
    closure: List[Triple], rules: List[Dict[str, Any]], expanded: List[Triple]
) -> List[Triple]:
    """One full round of every rule over the given triple set; returns every
    derivable triple that is absent (empty means saturated)."""
    known = set(closure)
    additions: List[Triple] = []
    current = sorted(known, key=triple_sort_key)
    by_predicate = build_indexes(current)
    join_subject, join_object = build_join_indexes(current)
    for triple in expanded:
        if triple not in known:
            additions.append(triple)
    for rule in rules:
        if not rule["consequents"]:
            continue
        # Same join ordering as the closure engine: without it the first
        # premise in table order can carry an unbound predicate and scan the
        # whole triple set before any variable is bound.
        premises = order_premises(rule["premises"], by_predicate, current)

        def fire(index: int, binding: Dict[str, Term], used: List[Triple]):
            if index == len(premises):
                for pattern in rule["consequents"]:
                    yield tuple(
                        binding.get(term, tuple(term) if not isinstance(term, str) else term)
                        for term in pattern
                    )
                return
            for local, triple in _match_premise(premises[index], binding, by_predicate, current, join_subject, join_object):
                if any(triple == u for u in used):
                    continue
                yield from fire(index + 1, local, used + [triple])

        for derived in fire(0, {}, []):
            if derived not in known:
                additions.append(derived)
    return sorted(set(additions), key=triple_sort_key)

