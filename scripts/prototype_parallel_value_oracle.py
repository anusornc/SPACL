#!/usr/bin/env python3
"""Throwaway independent graph-colouring oracle for the SPACL parallel-value kernel.

This module intentionally does not import production reasoner code.  The locked
cases model the OWL star-role construction where each target is a graph vertex:
all targets are members of C, graph edges are DifferentIndividuals constraints,
and the owner has max-k R.C.  Consistency is therefore exactly k-colourability.

Vertex identities are stable by index: vertex 0 is v00, vertex 1 is v01, and so
on through v10 in the locked corpus.  Prefixes are branch assignments in original
vertex order.  Equal fixed prefix colours merge vertices in the quotient graph;
different fixed colours are kept distinct and must remain unequal.
"""

from __future__ import annotations

from functools import lru_cache
from typing import Iterable


def _canonical_edges(edges: Iterable[Iterable[int]], vertices: int) -> tuple[tuple[int, int], ...]:
    canonical = []
    seen = set()
    for edge in edges:
        if (
            not isinstance(edge, (list, tuple))
            or len(edge) != 2
            or type(edge[0]) is not int
            or type(edge[1]) is not int
        ):
            raise ValueError("edges must be integer pairs")
        source, target = edge
        if source == target or source < 0 or target < 0 or source >= vertices or target >= vertices:
            raise ValueError("edges must be in range and non-looping")
        low, high = sorted((source, target))
        if (low, high) in seen:
            raise ValueError("edges must be unique")
        seen.add((low, high))
        canonical.append((low, high))
    ordered = tuple(sorted(canonical))
    if list(edges) != [[source, target] for source, target in ordered]:
        raise ValueError("edges must be sorted canonical pairs")
    return ordered


def _cycle_edges(size: int) -> list[list[int]]:
    return _edges((index, (index + 1) % size) for index in range(size))


def _clique_edges(size: int) -> list[list[int]]:
    return _edges((source, target) for source in range(size) for target in range(source + 1, size))


def _mycielski_c5_edges() -> list[list[int]]:
    cycle = [(index, (index + 1) % 5) for index in range(5)]
    edges = list(cycle)
    for source, target in cycle:
        edges.append((source + 5, target))
        edges.append((target + 5, source))
    edges.extend((10, shadow) for shadow in range(5, 10))
    return _edges(edges)


def _edges(edges: Iterable[tuple[int, int]]) -> list[list[int]]:
    return [[low, high] for low, high in sorted({tuple(sorted(edge)) for edge in edges})]


def cases() -> list[dict]:
    cycle5 = _cycle_edges(5)
    mycielski_c5 = _mycielski_c5_edges()
    clique7 = _clique_edges(7)
    clique9 = _clique_edges(9)
    return [
        {"id": "cycle5_2", "vertices": 5, "edges": cycle5, "colors": 2, "role": "control"},
        {"id": "cycle5_3", "vertices": 5, "edges": cycle5, "colors": 3, "role": "control"},
        {"id": "mycielski_c5_3", "vertices": 11, "edges": mycielski_c5, "colors": 3, "role": "primary"},
        {"id": "mycielski_c5_4", "vertices": 11, "edges": mycielski_c5, "colors": 4, "role": "control"},
        {"id": "clique7_6", "vertices": 7, "edges": clique7, "colors": 6, "role": "control"},
        {"id": "clique7_7", "vertices": 7, "edges": clique7, "colors": 7, "role": "control"},
        {"id": "clique9_8", "vertices": 9, "edges": clique9, "colors": 8, "role": "primary"},
        {"id": "clique9_9", "vertices": 9, "edges": clique9, "colors": 9, "role": "control"},
    ]


def evaluate_case(case: dict, prefix: list[int | None] | None = None) -> dict:
    try:
        vertices, edges, colors = _validate_case(case)
    except ValueError as error:
        return _not_sat(case, "invalid_graph", str(error), 0)
    try:
        normalized_prefix = _validate_prefix(prefix, vertices, colors)
    except ValueError as error:
        return _not_sat(case, "invalid_prefix", str(error), 0)

    if normalized_prefix is None:
        chromatic_number, witness = _solve_unprefixed(vertices, edges)
        expected = chromatic_number <= colors
        return {
            "id": case.get("id"),
            "expected": expected,
            "chromatic_number": chromatic_number,
            "witness": witness if expected else None,
            "status": "sat" if expected else "not_sat",
            "role": case.get("role"),
        }

    quotient = _quotient(vertices, edges, normalized_prefix)
    if quotient["contradiction"]:
        return _not_sat(case, "contradictory_prefix", "fixed equal-colour vertices are adjacent", 0)

    chromatic_number, quotient_witness = _solve_with_fixed(
        quotient["vertices"],
        quotient["edges"],
        quotient["fixed"],
        colors,
    )
    expected = quotient_witness is not None and chromatic_number <= colors
    witness = None
    if expected:
        witness = [quotient_witness[quotient["vertex_to_group"][vertex]] for vertex in range(vertices)]
    return {
        "id": case.get("id"),
        "expected": expected,
        "chromatic_number": chromatic_number,
        "witness": witness,
        "status": "sat" if expected else "not_sat",
        "role": case.get("role"),
    }


def validate_witness(case: dict, witness: list[int], prefix: list[int | None] | None = None) -> bool:
    try:
        vertices, edges, colors = _validate_case(case)
        normalized_prefix = _validate_prefix(prefix, vertices, colors)
    except ValueError:
        return False
    if not isinstance(witness, list) or len(witness) != vertices:
        return False
    for colour in witness:
        if type(colour) is not int or colour < 0 or colour >= colors:
            return False
    if normalized_prefix is not None:
        for vertex, fixed in enumerate(normalized_prefix):
            if fixed is not None and witness[vertex] != fixed:
                return False
    return all(witness[source] != witness[target] for source, target in edges)


def _validate_case(case: dict) -> tuple[int, tuple[tuple[int, int], ...], int]:
    vertices = case.get("vertices")
    colors = case.get("colors")
    if type(vertices) is not int or vertices < 0 or vertices > 13:
        raise ValueError("vertices must be an integer in range 0..13")
    if type(colors) is not int or colors < 0:
        raise ValueError("colors must be a non-negative integer")
    edges = _canonical_edges(case.get("edges", []), vertices)
    if case.get("role") not in ("control", "primary"):
        raise ValueError("role must be control or primary")
    return vertices, edges, colors


def _validate_prefix(prefix: list[int | None] | None, vertices: int, colors: int) -> tuple[int | None, ...] | None:
    if prefix is None:
        return None
    if not isinstance(prefix, list) or len(prefix) != vertices:
        raise ValueError("prefix must be a list in original vertex order")
    normalized = []
    for fixed in prefix:
        if fixed is None:
            normalized.append(None)
        elif type(fixed) is int and 0 <= fixed < colors:
            normalized.append(fixed)
        else:
            raise ValueError("prefix entries must be None or palette colour integers")
    return tuple(normalized)


def _not_sat(case: dict, status: str, reason: str, chromatic_number: int) -> dict:
    return {
        "id": case.get("id"),
        "expected": False,
        "chromatic_number": chromatic_number,
        "witness": None,
        "status": status,
        "reason": reason,
        "role": case.get("role"),
    }


@lru_cache(maxsize=None)
def _solve_unprefixed(vertices: int, edges: tuple[tuple[int, int], ...]) -> tuple[int, list[int] | None]:
    if vertices == 0:
        return 0, []
    independent = _independent_masks(vertices, edges)
    full = (1 << vertices) - 1
    dp = [vertices + 1] * (1 << vertices)
    parent = [0] * (1 << vertices)
    dp[0] = 0
    for mask in range(1, full + 1):
        submask = mask
        while submask:
            if independent[submask] and dp[mask ^ submask] + 1 < dp[mask]:
                dp[mask] = dp[mask ^ submask] + 1
                parent[mask] = submask
            submask = (submask - 1) & mask
    witness = [None] * vertices
    mask = full
    colour = 0
    while mask:
        colour_class = parent[mask]
        for vertex in range(vertices):
            if colour_class & (1 << vertex):
                witness[vertex] = colour
        mask ^= colour_class
        colour += 1
    return dp[full], witness


def _solve_with_fixed(
    vertices: int,
    edges: tuple[tuple[int, int], ...],
    fixed: dict[int, int],
    limit: int,
) -> tuple[int, list[int] | None]:
    cache_key = (vertices, edges, tuple(sorted(fixed.items())), limit)
    return _solve_with_fixed_cached(cache_key)


@lru_cache(maxsize=None)
def _solve_with_fixed_cached(cache_key: tuple) -> tuple[int, list[int] | None]:
    vertices, edges, fixed_items, limit = cache_key
    fixed = dict(fixed_items)
    minimum = max(fixed.values(), default=-1) + 1
    maximum = max(vertices, minimum, limit)
    independent = _independent_masks(vertices, edges)
    all_mask = (1 << vertices) - 1
    fixed_mask = 0
    for vertex in fixed:
        fixed_mask |= 1 << vertex
    free_mask = all_mask ^ fixed_mask
    adjacency = _adjacency_masks(vertices, edges)

    for palette_size in range(minimum, maximum + 1):
        if palette_size > limit:
            break
        allowed_by_colour = []
        valid = True
        for colour in range(palette_size):
            seed = 0
            blocked = 0
            for vertex, fixed_colour in fixed.items():
                if fixed_colour == colour:
                    seed |= 1 << vertex
                    blocked |= adjacency[vertex]
            if not independent[seed]:
                valid = False
                break
            allowed_by_colour.append(free_mask & ~blocked)
        if not valid:
            continue
        path = _partition_free_vertices(free_mask, independent, allowed_by_colour)
        if path is None:
            continue
        witness = [None] * vertices
        for vertex, colour in fixed.items():
            witness[vertex] = colour
        for colour, colour_class in enumerate(path):
            for vertex in range(vertices):
                if colour_class & (1 << vertex):
                    witness[vertex] = colour
        return palette_size, witness
    return maximum + 1, None


def _partition_free_vertices(
    free_mask: int,
    independent: tuple[bool, ...],
    allowed_by_colour: list[int],
) -> list[int] | None:
    reachable = {0: []}
    for allowed in allowed_by_colour:
        next_reachable = {}
        for assigned, path in reachable.items():
            remaining = free_mask ^ assigned
            submask = remaining & allowed
            candidate = submask
            while True:
                if independent[candidate]:
                    merged = assigned | candidate
                    next_reachable.setdefault(merged, path + [candidate])
                if candidate == 0:
                    break
                candidate = (candidate - 1) & submask
        reachable = next_reachable
        if not reachable:
            return None
    return reachable.get(free_mask)


@lru_cache(maxsize=None)
def _independent_masks(vertices: int, edges: tuple[tuple[int, int], ...]) -> tuple[bool, ...]:
    independent = [True] * (1 << vertices)
    edge_masks = [(1 << source) | (1 << target) for source, target in edges]
    for mask in range(1 << vertices):
        independent[mask] = all(mask & edge_mask != edge_mask for edge_mask in edge_masks)
    return tuple(independent)


def _adjacency_masks(vertices: int, edges: tuple[tuple[int, int], ...]) -> list[int]:
    adjacency = [0] * vertices
    for source, target in edges:
        adjacency[source] |= 1 << target
        adjacency[target] |= 1 << source
    return adjacency


def _quotient(vertices: int, edges: tuple[tuple[int, int], ...], prefix: tuple[int | None, ...]) -> dict:
    colour_to_group = {}
    vertex_to_group = {}
    next_group = 0
    fixed = {}
    for vertex, colour in enumerate(prefix):
        if colour is None:
            vertex_to_group[vertex] = next_group
            next_group += 1
            continue
        if colour not in colour_to_group:
            colour_to_group[colour] = next_group
            fixed[next_group] = colour
            next_group += 1
        vertex_to_group[vertex] = colour_to_group[colour]

    quotient_edges = set()
    for source, target in edges:
        source_group = vertex_to_group[source]
        target_group = vertex_to_group[target]
        if source_group == target_group:
            return {"contradiction": True}
        quotient_edges.add(tuple(sorted((source_group, target_group))))
    return {
        "contradiction": False,
        "vertices": next_group,
        "edges": tuple(sorted(quotient_edges)),
        "fixed": fixed,
        "vertex_to_group": vertex_to_group,
    }


if __name__ == "__main__":
    for locked_case in cases():
        print(evaluate_case(locked_case))
