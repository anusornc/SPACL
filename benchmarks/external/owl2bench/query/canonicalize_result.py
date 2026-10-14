#!/usr/bin/env python3
"""Canonicalize SPARQL result documents for the OWL2Bench query lane."""

from __future__ import annotations

import argparse
import hashlib
import json
import re
import sys
import xml.etree.ElementTree as ElementTree
from pathlib import Path
from typing import Any, Dict, Iterable, List, Optional, Sequence, Tuple


CANONICALIZER_VERSION = "owl2bench-query-canonicalizer/2"
CANONICALIZER_NAME = "owl2bench-query-canonicalizer"
XML_NAMESPACE = "http://www.w3.org/2005/sparql-results#"
XML_LANGUAGE_ATTRIBUTE = "{http://www.w3.org/XML/1998/namespace}lang"
XSD_STRING = "http://www.w3.org/2001/XMLSchema#string"
VARIABLE_PATTERN = re.compile(r"(?<![A-Za-z0-9_])[?$]([A-Za-z_][A-Za-z0-9_]*)")


class CanonicalizationError(Exception):
    """A result cannot be represented by the canonical result contract."""


def local_name(tag: str) -> str:
    return tag.rsplit("}", 1)[-1].lower()


def normalize_variable(name: str) -> str:
    normalized = name.strip()
    if normalized.startswith(("?", "$")):
        normalized = normalized[1:]
    if not re.fullmatch(r"[A-Za-z_][A-Za-z0-9_]*", normalized):
        raise CanonicalizationError(f"invalid projection variable: {name!r}")
    return normalized


def parse_select_projection(projection_text: str) -> List[str]:
    if "*" in projection_text:
        return []

    variables: List[str] = []
    depth = 0
    index = 0
    while index < len(projection_text):
        character = projection_text[index]
        if character == "(":
            depth += 1
            index += 1
            continue
        if character == ")":
            depth -= 1
            if depth < 0:
                raise CanonicalizationError("unbalanced SELECT projection parentheses")
            index += 1
            continue

        if depth == 0:
            variable_match = VARIABLE_PATTERN.match(projection_text, index)
            if variable_match:
                variables.append(normalize_variable(variable_match.group(1)))
                index = variable_match.end()
                continue
        else:
            alias_match = re.match(
                r"AS\s+([?$][A-Za-z_][A-Za-z0-9_]*)", projection_text[index:], re.IGNORECASE
            )
            if alias_match:
                variables.append(normalize_variable(alias_match.group(1)))
                index += alias_match.end()
                continue
        index += 1

    if depth != 0:
        raise CanonicalizationError("unbalanced SELECT projection parentheses")
    if not variables:
        raise CanonicalizationError("SELECT projection has no variables or wildcard")
    if len(set(variables)) != len(variables):
        raise CanonicalizationError("duplicate projection variable in SELECT query")
    return variables


def parse_query_metadata(query_text: str) -> Dict[str, Any]:
    select_match = re.search(r"\bSELECT\b", query_text, re.IGNORECASE)
    if select_match is None:
        raise CanonicalizationError("only SELECT query projection metadata is supported")

    after_select = query_text[select_match.end() :]
    modifier_match = re.match(r"\s*(DISTINCT|REDUCED)\b", after_select, re.IGNORECASE)
    modifier = modifier_match.group(1).lower() if modifier_match else "none"
    if modifier_match:
        after_select = after_select[modifier_match.end() :]

    projection_end = re.search(r"\bWHERE\b|\{", after_select, re.IGNORECASE)
    projection_text = after_select if projection_end is None else after_select[: projection_end.start()]
    projection = parse_select_projection(projection_text)

    return {
        "projection": projection,
        "modifier": modifier,
        "distinct": modifier == "distinct",
        "ordered": re.search(r"\bORDER\s+BY\b", query_text, re.IGNORECASE) is not None,
    }


def unbound_term() -> Dict[str, Any]:
    return {"type": "unbound"}


def iri_term(value: str) -> Dict[str, Any]:
    if value == "":
        raise CanonicalizationError("empty IRI binding")
    return {"type": "iri", "value": value}


def blank_node_term(value: str) -> Dict[str, Any]:
    if value == "":
        raise CanonicalizationError("empty blank-node binding")
    return {"type": "blank_node", "value": value}


def literal_term(value: str, datatype: Optional[str] = None, language: Optional[str] = None) -> Dict[str, Any]:
    if datatype and language:
        raise CanonicalizationError("literal cannot have both datatype and language")
    if datatype == XSD_STRING and not language:
        # RDF 1.1 defines a simple literal as one with datatype xsd:string,
        # so the two serializations are the same term and canonicalize alike.
        datatype = None
    return {
        "type": "literal",
        "value": value,
        "datatype": datatype,
        "language": language.lower() if language else None,
    }


def sparql_tag(name: str) -> str:
    return "{" + XML_NAMESPACE + "}" + name


def reconcile_projection(query_projection: Sequence[str], result_projection: Sequence[str]) -> List[str]:
    if query_projection:
        if result_projection and set(query_projection) != set(result_projection):
            raise CanonicalizationError(
                "result projection does not match the SELECT projection: "
                f"query={list(query_projection)!r}, result={list(result_projection)!r}"
            )
        return list(query_projection)
    if not result_projection:
        raise CanonicalizationError("result does not declare a projection")
    return list(result_projection)


def parse_xml_term(element: ElementTree.Element) -> Dict[str, Any]:
    if element.tag not in {
        sparql_tag("uri"),
        sparql_tag("bnode"),
        sparql_tag("literal"),
        sparql_tag("unbound"),
    }:
        raise CanonicalizationError(f"unsupported SPARQL XML binding element: {element.tag}")
    kind = local_name(element.tag)
    if list(element):
        raise CanonicalizationError(f"nested XML elements are not valid in {kind} binding")
    value = element.text or ""
    if kind == "uri":
        return iri_term(value)
    if kind == "bnode":
        return blank_node_term(value)
    if kind == "unbound":
        if value.strip():
            raise CanonicalizationError("unbound XML binding must be empty")
        return unbound_term()
    if kind == "literal":
        datatype = element.attrib.get("datatype") or None
        language = element.attrib.get(XML_LANGUAGE_ATTRIBUTE) or element.attrib.get("xml:lang")
        return literal_term(value, datatype=datatype, language=language)
    raise CanonicalizationError(f"unsupported SPARQL XML binding type: {kind}")


def parse_xml_result(data: bytes, query_metadata: Dict[str, Any]) -> Tuple[str, List[str], List[List[Dict[str, Any]]]]:
    if b"<!DOCTYPE" in data.upper() or b"<!ENTITY" in data.upper():
        raise CanonicalizationError("DOCTYPE and ENTITY declarations are not supported")
    try:
        root = ElementTree.fromstring(data)
    except ElementTree.ParseError as error:
        raise CanonicalizationError(f"malformed SPARQL Results XML: {error}") from error
    if root.tag != sparql_tag("sparql"):
        raise CanonicalizationError("SPARQL XML result root must use the SPARQL Results namespace")

    head_nodes = [child for child in root if child.tag == sparql_tag("head")]
    results_nodes = [child for child in root if child.tag == sparql_tag("results")]
    if len(head_nodes) != 1 or len(results_nodes) != 1:
        raise CanonicalizationError("SPARQL XML result must contain exactly one <head> and <results>")
    if any(child.tag not in {sparql_tag("head"), sparql_tag("results")} for child in root):
        raise CanonicalizationError("SPARQL XML result contains an unexpected root child")
    head = head_nodes[0]
    results = results_nodes[0]
    result_projection: List[str] = []
    for child in head:
        if child.tag == sparql_tag("variable"):
            if list(child) or "name" not in child.attrib:
                raise CanonicalizationError("SPARQL XML variable declaration is malformed")
            result_projection.append(normalize_variable(child.attrib["name"]))
        elif child.tag == sparql_tag("link"):
            if list(child) or not child.attrib.get("href"):
                raise CanonicalizationError("SPARQL XML link declaration is malformed")
        else:
            raise CanonicalizationError("SPARQL XML head contains an unexpected child")
    if len(set(result_projection)) != len(result_projection):
        raise CanonicalizationError("duplicate variable in SPARQL XML result head")
    projection = reconcile_projection(query_metadata["projection"], result_projection)

    rows: List[List[Dict[str, Any]]] = []
    for result in results:
        if result.tag != sparql_tag("result"):
            raise CanonicalizationError("SPARQL XML results contains an unexpected child")
        bindings: Dict[str, Dict[str, Any]] = {}
        for binding in result:
            if binding.tag != sparql_tag("binding") or "name" not in binding.attrib:
                raise CanonicalizationError("SPARQL XML result contains a malformed binding")
            name = normalize_variable(binding.attrib["name"])
            if name in bindings:
                raise CanonicalizationError(f"duplicate XML binding for variable: {name}")
            children = list(binding)
            if len(children) != 1:
                raise CanonicalizationError(f"XML binding for {name} must contain exactly one value")
            bindings[name] = parse_xml_term(children[0])
        unknown_bindings = set(bindings) - set(projection)
        if unknown_bindings:
            raise CanonicalizationError(
                "result contains bindings outside the SELECT projection: "
                + ", ".join(sorted(unknown_bindings))
            )
        rows.append([bindings.get(name, unbound_term()) for name in projection])
    return "sparql-results-xml", projection, rows


def parse_tsv_literal(token: str) -> Dict[str, Any]:
    decoder = json.JSONDecoder()
    try:
        value, end = decoder.raw_decode(token)
    except json.JSONDecodeError as error:
        raise CanonicalizationError(f"malformed quoted TSV literal: {token!r}") from error
    if not isinstance(value, str) or end == 0:
        raise CanonicalizationError(f"malformed quoted TSV literal: {token!r}")
    suffix = token[end:]
    if suffix == "":
        return literal_term(value)
    if suffix.startswith("@") and re.fullmatch(r"@[A-Za-z]+(?:-[A-Za-z0-9]+)*", suffix):
        return literal_term(value, language=suffix[1:])
    if suffix.startswith("^^<") and suffix.endswith(">") and len(suffix) > 4:
        return literal_term(value, datatype=suffix[3:-1])
    raise CanonicalizationError(f"unsupported TSV literal suffix: {token!r}")


def parse_tsv_term(token: str) -> Dict[str, Any]:
    if token == "":
        return unbound_term()
    if token.startswith("<") and token.endswith(">"):
        return iri_term(token[1:-1])
    if token.startswith("_:"):
        return blank_node_term(token[2:])
    if token.startswith('"'):
        return parse_tsv_literal(token)
    raise CanonicalizationError(f"unsupported TSV RDF term: {token!r}")


def parse_tsv_result(data: bytes, query_metadata: Dict[str, Any]) -> Tuple[str, List[str], List[List[Dict[str, Any]]]]:
    try:
        text = data.decode("utf-8-sig")
    except UnicodeDecodeError as error:
        raise CanonicalizationError(f"result is not valid UTF-8 TSV: {error}") from error
    lines = text.splitlines()
    if not lines or lines[0].rstrip("\r") == "":
        raise CanonicalizationError("TSV result is missing its projection header")
    headers = [normalize_variable(value) for value in lines[0].rstrip("\r").split("\t")]
    if len(set(headers)) != len(headers):
        raise CanonicalizationError("duplicate variable in TSV result header")
    projection = reconcile_projection(query_metadata["projection"], headers)
    header_indexes = {name: index for index, name in enumerate(headers)}
    rows: List[List[Dict[str, Any]]] = []
    for line in lines[1:]:
        fields = line.rstrip("\r").split("\t")
        if len(fields) != len(headers):
            raise CanonicalizationError(
                f"TSV row has {len(fields)} fields; expected {len(headers)}"
            )
        parsed = [parse_tsv_term(field) for field in fields]
        rows.append([parsed[header_indexes[name]] for name in projection])
    return "tsv", projection, rows


def parse_result(data: bytes, query_metadata: Dict[str, Any]) -> Tuple[str, List[str], List[List[Dict[str, Any]]]]:
    stripped = data.lstrip(b"\xef\xbb\xbf \t\r\n")
    if not stripped:
        raise CanonicalizationError("result document is empty")
    if stripped.startswith(b"<?xml") or re.match(rb"<\s*sparql(?:\s|>)", stripped, re.IGNORECASE):
        return parse_xml_result(data, query_metadata)
    return parse_tsv_result(data, query_metadata)


def compact_json(value: Any) -> str:
    return json.dumps(value, ensure_ascii=False, sort_keys=True, separators=(",", ":"))


def term_skeleton(term: Dict[str, Any]) -> Dict[str, Any]:
    if term["type"] == "blank_node":
        return {"type": "blank_node"}
    return term


def canonicalize_blank_nodes(rows: Iterable[List[Dict[str, Any]]]) -> List[List[Dict[str, Any]]]:
    materialized_rows = [list(row) for row in rows]
    labels = {
        term["value"]
        for row in materialized_rows
        for term in row
        if term["type"] == "blank_node"
    }
    signatures: Dict[str, str] = {}
    for label in labels:
        contexts: List[Tuple[int, str]] = []
        for row in materialized_rows:
            skeleton = [term_skeleton(term) for term in row]
            for index, term in enumerate(row):
                if term["type"] == "blank_node" and term["value"] == label:
                    contexts.append((index, compact_json(skeleton)))
        signatures[label] = compact_json(sorted(contexts))
    ordered_labels = sorted(labels, key=lambda label: (signatures[label], label))
    mapping = {label: f"b{index}" for index, label in enumerate(ordered_labels)}

    canonical_rows: List[List[Dict[str, Any]]] = []
    for row in materialized_rows:
        canonical_row: List[Dict[str, Any]] = []
        for term in row:
            canonical_term = dict(term)
            if canonical_term["type"] == "blank_node":
                canonical_term["value"] = mapping[canonical_term["value"]]
            canonical_row.append(canonical_term)
        canonical_rows.append(canonical_row)
    return canonical_rows


def term_sort_key(term: Dict[str, Any]) -> Tuple[Any, ...]:
    type_rank = {"iri": 0, "blank_node": 1, "literal": 2, "unbound": 3}
    return (
        type_rank[term["type"]],
        term["type"],
        term.get("value", ""),
        term.get("datatype") or "",
        term.get("language") or "",
    )


def canonicalize(query_path: Path, result_path: Path) -> Dict[str, Any]:
    try:
        query_text = query_path.read_text(encoding="utf-8-sig")
        result_data = result_path.read_bytes()
    except OSError as error:
        raise CanonicalizationError(str(error)) from error
    query_metadata = parse_query_metadata(query_text)
    result_format, projection, rows = parse_result(result_data, query_metadata)
    canonical_rows = canonicalize_blank_nodes(rows)
    if query_metadata["ordered"]:
        row_order = "query"
    else:
        row_order = "typed_tuple"
        canonical_rows.sort(key=lambda row: tuple(term_sort_key(term) for term in row))

    payload = {
        "canonicalizer_version": CANONICALIZER_VERSION,
        "projection": projection,
        "select_modifier": query_metadata["modifier"],
        "distinct": query_metadata["distinct"],
        "ordered": query_metadata["ordered"],
        "row_order": row_order,
        "rows": canonical_rows,
    }
    canonical_bytes = compact_json(payload).encode("utf-8")
    return {
        **payload,
        "format": result_format,
        "row_count": len(canonical_rows),
        "result_hash": "sha256:" + hashlib.sha256(canonical_bytes).hexdigest(),
    }


def parse_arguments() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--query", type=Path)
    parser.add_argument("--input", type=Path)
    parser.add_argument("--output", type=Path)
    parser.add_argument("--version", action="store_true")
    arguments = parser.parse_args()
    if not arguments.version and (arguments.query is None or arguments.input is None or arguments.output is None):
        parser.error("--query, --input, and --output are required unless --version is used")
    return arguments


def main() -> int:
    arguments = parse_arguments()
    if arguments.version:
        print(CANONICALIZER_VERSION)
        return 0
    try:
        result = canonicalize(arguments.query, arguments.input)
        arguments.output.write_text(json.dumps(result, ensure_ascii=False, indent=2, sort_keys=True) + "\n", encoding="utf-8")
    except (CanonicalizationError, OSError, ValueError) as error:
        print(f"canonicalization failed: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
