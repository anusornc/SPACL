# Source Architecture (`src/`)

This document describes the current Rust module layout for SPACL in this public repository snapshot.

## High-level module tree

```text
src/
├── lib.rs
├── lib_original.rs            # Legacy library facade kept for reference
├── app/           # App/domain helpers (including EPCIS demo support)
├── bin/           # CLI binaries (owl2-reasoner, owl2_validation, epcis-reasoner)
├── core/          # Fundamental data structures and error types
├── logic/         # OWL axioms, expressions, and datatype logic
├── parser/        # Ontology parsers and parser utilities
├── reasoner/      # Consistency/classification engines and strategies
├── serializer/    # Binary serializer and related IO helpers
├── storage/       # Storage abstractions
├── strategy/      # Profile-aware and adaptive strategy selection
└── util/          # Shared utilities (config/cache/loader/helpers)
```

## Notable submodules

- `src/logic/axioms/` and `src/logic/datatypes/`
- `src/parser/json_ld/`, `src/parser/manchester/`, `src/parser/owl_functional/`
- `src/reasoner/tableaux/` and `src/reasoner/query/`
- `src/strategy/profiles/`
- `src/util/profiling/`

## Responsibilities

- `core/`
  - ontology container, entities, IRI handling, common errors

- `logic/`
  - logical model used by parsers and reasoners

- `parser/`
  - RDF/XML, OWL/XML, OWL Functional, Turtle, N-Triples, JSON-LD, Manchester support (via `ParserFactory`)
  - parser factory and shared parser configuration
  - note: `src/parser/streaming/` exists, but `pub mod streaming` is currently disabled in `src/parser/mod.rs`

- `reasoner/`
  - reasoning implementations and execution paths
  - includes `simple`, `speculative` (SPACL), `tableaux`, classification/hierarchy variants, and query helpers

- `strategy/`
  - profile detection and strategy routing logic

- `serializer/`
  - `.owlbin` serialization/deserialization

- `util/`
  - shared loader (`load_ontology_with_env`) and runtime configuration glue

## Binary entry points

- `src/bin/owl2-reasoner.rs`
- `src/bin/owl2_validation.rs`
- `src/bin/epcis-reasoner.rs`
- `src/bin/main.rs`

## Maintenance guidance

When module layout changes:

1. place code in the correct domain folder
2. expose stable public API from `src/lib.rs` only when intended
3. update this document (tree + responsibilities + WIP notes) in the same change
