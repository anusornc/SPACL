#!/usr/bin/env python3
"""Canonicalize RDF N-Quads with the URDNA2015 dataset algorithm.

The module deliberately implements only the N-Quads surface needed by the
materialization contract. It has no network, store, or oracle behavior.
"""

from __future__ import annotations

import argparse
import hashlib
import itertools
import re
from dataclasses import dataclass
from pathlib import Path
from typing import Iterable, Iterator


class CanonicalizationError(ValueError):
    """An input document is not a supported RDF N-Quads document."""


@dataclass(frozen=True)
class Term:
    """One RDF term in an N-Quads position."""

    kind: str
    value: str
    datatype: str | None = None
    language: str | None = None

    @property
    def is_blank(self) -> bool:
        return self.kind == "blank_node"


@dataclass(frozen=True)
class Quad:
    """One RDF quad; ``graph`` is None for the default graph."""

    subject: Term
    predicate: Term
    object: Term
    graph: Term | None = None

    def terms(self) -> tuple[Term, ...]:
        if self.graph is None:
            return (self.subject, self.predicate, self.object)
        return (self.subject, self.predicate, self.object, self.graph)


_LANGUAGE_RE = re.compile(r"[A-Za-z]+(?:-[A-Za-z0-9]+)*")
_BLANK_RE = re.compile(r"_:[^\s]+")


def _decode_escapes(value: str, *, allow_echar: bool = True) -> str:
    """Decode N-Quads ECHAR and UCHAR escapes into RDF term values."""

    result: list[str] = []
    index = 0
    escapes = {
        "t": "\t",
        "b": "\b",
        "n": "\n",
        "r": "\r",
        "f": "\f",
        '"': '"',
        "'": "'",
        "\\": "\\",
    }
    while index < len(value):
        character = value[index]
        if character != "\\":
            result.append(character)
            index += 1
            continue
        index += 1
        if index >= len(value):
            raise CanonicalizationError("unterminated escape sequence")
        escaped = value[index]
        if allow_echar and escaped in escapes:
            result.append(escapes[escaped])
            index += 1
            continue
        if escaped in {"u", "U"}:
            width = 4 if escaped == "u" else 8
            digit_start = index + 1
            digit_end = digit_start + width
            digits = value[digit_start:digit_end]
            if len(digits) != width or not re.fullmatch(r"[0-9A-Fa-f]+", digits):
                raise CanonicalizationError(f"invalid Unicode escape: \\{escaped}{digits}")
            codepoint = int(digits, 16)
            if codepoint > 0x10FFFF or 0xD800 <= codepoint <= 0xDFFF:
                raise CanonicalizationError(f"invalid Unicode code point: U+{codepoint:04X}")
            result.append(chr(codepoint))
            index = digit_end
            continue
        raise CanonicalizationError(f"unsupported escape sequence: \\{escaped}")
    return "".join(result)


def _escape_literal(value: str) -> str:
    escapes = {
        "\t": r"\t",
        "\b": r"\b",
        "\n": r"\n",
        "\r": r"\r",
        "\f": r"\f",
        '"': r'\"',
        "\\": r"\\",
    }
    result: list[str] = []
    for character in value:
        if character in escapes:
            result.append(escapes[character])
        elif ord(character) < 0x20 or ord(character) == 0x7F:
            result.append(f"\\u{ord(character):04X}")
        else:
            result.append(character)
    return "".join(result)


def _escape_iri(value: str) -> str:
    """Escape only characters that are not legal directly inside an IRIREF."""

    forbidden = set('<>"{}|^`\\')
    result: list[str] = []
    for character in value:
        codepoint = ord(character)
        if codepoint <= 0x20 or character in forbidden:
            if codepoint <= 0xFFFF:
                result.append(f"\\u{codepoint:04X}")
            else:
                result.append(f"\\U{codepoint:08X}")
        else:
            result.append(character)
    return "".join(result)


def _parse_iri(text: str, index: int) -> tuple[Term, int]:
    if index >= len(text) or text[index] != "<":
        raise CanonicalizationError("IRI must start with '<'")
    cursor = index + 1
    raw: list[str] = []
    while cursor < len(text):
        character = text[cursor]
        if character == ">":
            return Term("iri", _decode_escapes("".join(raw), allow_echar=False)), cursor + 1
        if character == "\\":
            if cursor + 1 >= len(text):
                raise CanonicalizationError("unterminated IRI escape")
            raw.extend((character, text[cursor + 1]))
            cursor += 2
        else:
            raw.append(character)
            cursor += 1
    raise CanonicalizationError("unterminated IRI")


def _parse_literal(text: str, index: int) -> tuple[Term, int]:
    if index >= len(text) or text[index] != '"':
        raise CanonicalizationError("literal must start with '\"'")
    cursor = index + 1
    raw: list[str] = []
    while cursor < len(text):
        character = text[cursor]
        if character == '"':
            value = _decode_escapes("".join(raw))
            cursor += 1
            if text[cursor : cursor + 1] == "@":
                match = _LANGUAGE_RE.match(text, cursor + 1)
                if match is None:
                    raise CanonicalizationError("invalid language tag")
                return Term("literal", value, language=match.group(0)), match.end()
            if text[cursor : cursor + 2] == "^^":
                datatype, end = _parse_iri(text, cursor + 2)
                return Term("literal", value, datatype=datatype.value), end
            return Term("literal", value), cursor
        if character == "\\":
            if cursor + 1 >= len(text):
                raise CanonicalizationError("unterminated literal escape")
            raw.extend((character, text[cursor + 1]))
            cursor += 2
        else:
            if character in "\r\n":
                raise CanonicalizationError("literal contains an unescaped line break")
            raw.append(character)
            cursor += 1
    raise CanonicalizationError("unterminated literal")


def _parse_term(text: str, index: int, position: str) -> tuple[Term, int]:
    while index < len(text) and text[index].isspace():
        index += 1
    if index >= len(text):
        raise CanonicalizationError(f"missing {position} term")
    if text[index] == "<":
        return _parse_iri(text, index)
    if text[index] == '"':
        if position not in {"object"}:
            raise CanonicalizationError(f"literal is not valid in {position} position")
        return _parse_literal(text, index)
    if text.startswith("_:", index):
        match = _BLANK_RE.match(text, index)
        if match is None:
            raise CanonicalizationError("invalid blank-node label")
        if position == "predicate":
            raise CanonicalizationError("blank node is not valid as an RDF predicate")
        return Term("blank_node", match.group(0)[2:]), match.end()
    raise CanonicalizationError(f"unsupported {position} term")


def parse_quad_line(line: str, line_number: int = 1) -> Quad | None:
    """Parse one N-Quads line, returning None for blank/comment lines."""

    stripped = line.strip()
    if not stripped or stripped.startswith("#"):
        return None
    try:
        subject, cursor = _parse_term(stripped, 0, "subject")
        predicate, cursor = _parse_term(stripped, cursor, "predicate")
        object_term, cursor = _parse_term(stripped, cursor, "object")
        cursor_before_graph = cursor
        while cursor < len(stripped) and stripped[cursor].isspace():
            cursor += 1
        graph = None
        if cursor < len(stripped) and stripped[cursor] != ".":
            graph, cursor = _parse_term(stripped, cursor_before_graph, "graph")
            while cursor < len(stripped) and stripped[cursor].isspace():
                cursor += 1
        if cursor >= len(stripped) or stripped[cursor] != ".":
            raise CanonicalizationError("quad must end with '.'")
        cursor += 1
        if stripped[cursor:].strip():
            raise CanonicalizationError("unexpected content after quad")
        return Quad(subject, predicate, object_term, graph)
    except CanonicalizationError as error:
        raise CanonicalizationError(f"line {line_number}: {error}") from error


def parse_nquads(text: str) -> list[Quad]:
    """Parse an N-Quads document into a duplicate-free quad list."""

    quads: set[Quad] = set()
    for line_number, line in enumerate(text.splitlines(), start=1):
        quad = parse_quad_line(line, line_number)
        if quad is not None:
            quads.add(quad)
    return sorted(quads, key=serialize_quad)


def load_nquads(path: Path) -> list[Quad]:
    return parse_nquads(path.read_text(encoding="utf-8"))


def serialize_term(term: Term, blank_node_labels: dict[str, str] | None = None) -> str:
    if term.kind == "iri":
        return f"<{_escape_iri(term.value)}>"
    if term.kind == "blank_node":
        label = (blank_node_labels or {}).get(term.value, term.value)
        return f"_:{label}"
    if term.kind == "literal":
        value = f'"{_escape_literal(term.value)}"'
        if term.language:
            return f"{value}@{term.language}"
        if term.datatype:
            return f"{value}^^<{term.datatype}>"
        return value
    raise CanonicalizationError(f"unsupported RDF term kind: {term.kind}")


def serialize_quad(quad: Quad, blank_node_labels: dict[str, str] | None = None) -> str:
    terms = [
        serialize_term(term, blank_node_labels)
        for term in quad.terms()
    ]
    return " ".join(terms) + " .\n"


class IdentifierIssuer:
    """The identifier issuer used by the URDNA2015 algorithm."""

    def __init__(self, prefix: str) -> None:
        self.prefix = prefix
        self.counter = 0
        self.issued: dict[str, str] = {}

    def has(self, identifier: str) -> bool:
        return identifier in self.issued

    def get(self, identifier: str) -> str:
        return self.issued[identifier]

    def issue(self, identifier: str) -> str:
        if identifier not in self.issued:
            self.issued[identifier] = f"{self.prefix}{self.counter}"
            self.counter += 1
        return self.issued[identifier]

    def copy(self) -> "IdentifierIssuer":
        copied = IdentifierIssuer(self.prefix)
        copied.counter = self.counter
        copied.issued = dict(self.issued)
        return copied


class URDNA2015:
    """A small, dependency-free implementation of RDF dataset normalization."""

    def __init__(self, quads: Iterable[Quad]) -> None:
        self.quads = tuple(set(quads))
        self.quads_by_blank: dict[str, list[Quad]] = {}
        for quad in self.quads:
            for term in quad.terms():
                if term.is_blank:
                    self.quads_by_blank.setdefault(term.value, []).append(quad)
        self.first_degree_hashes: dict[str, str] = {}
        self.canonical_issuer = IdentifierIssuer("c14n")

    @property
    def blank_nodes(self) -> tuple[str, ...]:
        return tuple(sorted(self.quads_by_blank))

    def hash_first_degree_quads(self, identifier: str) -> str:
        serialized: list[str] = []
        for quad in self.quads_by_blank.get(identifier, []):
            labels = {
                term.value: ("a" if term.value == identifier else "z")
                for term in quad.terms()
                if term.is_blank
            }
            serialized.append(serialize_quad(quad, labels))
        payload = "".join(sorted(serialized)).encode("utf-8")
        return hashlib.sha256(payload).hexdigest()

    def _related_blank_nodes(
        self, quad: Quad, identifier: str
    ) -> Iterator[tuple[str, str]]:
        positions = (
            ("s", quad.subject),
            ("p", quad.predicate),
            ("o", quad.object),
        )
        if quad.graph is not None:
            positions += (("g", quad.graph),)
        for position, term in positions:
            if term.is_blank and term.value != identifier:
                yield term.value, position

    def hash_related_blank_node(
        self, related: str, quad: Quad, position: str, issuer: IdentifierIssuer
    ) -> str:
        input_value = position
        if position != "g":
            input_value += serialize_term(quad.predicate)
        if self.canonical_issuer.has(related):
            input_value += "_:" + self.canonical_issuer.get(related)
        elif issuer.has(related):
            input_value += "_:" + issuer.get(related)
        else:
            input_value += self.first_degree_hashes[related]
        return hashlib.sha256(input_value.encode("utf-8")).hexdigest()

    def hash_n_degree_quads(
        self, identifier: str, issuer: IdentifierIssuer
    ) -> tuple[str, IdentifierIssuer]:
        hash_to_related: dict[str, list[str]] = {}
        for quad in self.quads_by_blank.get(identifier, []):
            for related, position in self._related_blank_nodes(quad, identifier):
                related_hash = self.hash_related_blank_node(related, quad, position, issuer)
                hash_to_related.setdefault(related_hash, []).append(related)

        data_to_hash: list[str] = []
        for related_hash in sorted(hash_to_related):
            related_nodes = hash_to_related[related_hash]
            chosen_path: str | None = None
            chosen_issuer: IdentifierIssuer | None = None
            for permutation in itertools.permutations(related_nodes):
                issuer_copy = issuer.copy()
                path = related_hash
                recursive_nodes: list[str] = []
                for related in permutation:
                    if self.canonical_issuer.has(related):
                        path += "_:" + self.canonical_issuer.get(related)
                    elif issuer_copy.has(related):
                        path += "_:" + issuer_copy.get(related)
                    else:
                        recursive_nodes.append(related)
                        path += "_:" + issuer_copy.issue(related)

                if recursive_nodes:
                    for related in recursive_nodes:
                        recursive_hash, issuer_copy = self.hash_n_degree_quads(
                            related, issuer_copy
                        )
                        path += "_:" + issuer_copy.get(related) + "<" + recursive_hash + ">"

                if chosen_path is None or path < chosen_path:
                    chosen_path = path
                    chosen_issuer = issuer_copy
            if chosen_path is None or chosen_issuer is None:
                raise CanonicalizationError("unable to issue a canonical blank-node path")
            data_to_hash.append(chosen_path)
            issuer = chosen_issuer

        return (
            hashlib.sha256("".join(data_to_hash).encode("utf-8")).hexdigest(),
            issuer,
        )

    def canonicalize(self) -> bytes:
        for identifier in self.blank_nodes:
            self.first_degree_hashes[identifier] = self.hash_first_degree_quads(identifier)

        # URDNA2015 repeatedly issues identifiers for uniquely identified
        # nodes. Issuing one node can make another node unique in the next
        # first-degree pass, so a single pass is not sufficient.
        non_normalized = set(self.blank_nodes)
        hash_to_blank_nodes: dict[str, list[str]] = {}
        simple = True
        while simple:
            simple = False
            hash_to_blank_nodes = {}
            for identifier in sorted(non_normalized):
                digest = self.first_degree_hashes[identifier]
                hash_to_blank_nodes.setdefault(digest, []).append(identifier)
            for digest in sorted(tuple(hash_to_blank_nodes)):
                identifiers = hash_to_blank_nodes[digest]
                if len(identifiers) != 1:
                    continue
                identifier = identifiers[0]
                self.canonical_issuer.issue(identifier)
                non_normalized.remove(identifier)
                del hash_to_blank_nodes[digest]
                simple = True

        for digest in sorted(hash_to_blank_nodes):
            results: list[tuple[str, IdentifierIssuer]] = []
            for identifier in sorted(hash_to_blank_nodes[digest]):
                if self.canonical_issuer.has(identifier):
                    continue
                issuer = IdentifierIssuer("b")
                issuer.issue(identifier)
                n_degree_digest, issued = self.hash_n_degree_quads(identifier, issuer)
                results.append((n_degree_digest, issued))
            results.sort(key=lambda item: item[0])
            for _, issuer in results:
                for identifier in issuer.issued:
                    self.canonical_issuer.issue(identifier)

        labels = {
            identifier: self.canonical_issuer.get(identifier)
            for identifier in self.blank_nodes
        }
        lines = sorted(serialize_quad(quad, labels) for quad in self.quads)
        return "".join(lines).encode("utf-8")


def canonicalize_quads(quads: Iterable[Quad]) -> bytes:
    return URDNA2015(quads).canonicalize()


def canonicalize_text(text: str) -> bytes:
    return canonicalize_quads(parse_nquads(text))


def canonicalize_file(input_path: Path) -> bytes:
    return canonicalize_quads(load_nquads(input_path))


def write_canonical(path: Path, canonical_bytes: bytes) -> str:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(canonical_bytes)
    return "sha256:" + hashlib.sha256(canonical_bytes).hexdigest()


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    subparsers = parser.add_subparsers(dest="command", required=True)
    canonicalize_parser = subparsers.add_parser("canonicalize")
    canonicalize_parser.add_argument("--input", required=True, type=Path)
    canonicalize_parser.add_argument("--output", required=True, type=Path)
    args = parser.parse_args(argv)
    if args.command == "canonicalize":
        canonical_bytes = canonicalize_file(args.input)
        digest = write_canonical(args.output, canonical_bytes)
        print(digest)
        return 0
    return 2


if __name__ == "__main__":
    raise SystemExit(main())
