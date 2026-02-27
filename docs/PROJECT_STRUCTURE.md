# Source Architecture (`src/`)

This document describes the current Rust module layout for Tableauxx.

## High-level module tree

```text
src/
├── lib.rs
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

## Responsibilities

- `core/`
  - ontology container, entities, IRI handling, common errors

- `logic/`
  - logical model used by parsers and reasoners

- `parser/`
  - RDF/XML, OWL/XML, OWL Functional, Turtle, N-Triples, JSON-LD, Manchester support
  - parser factory and shared parser configuration

- `reasoner/`
  - reasoning implementations and execution paths
  - profile-aware selection hooks used by CLIs

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

When adding new modules:

1. place code in the correct domain folder
2. expose stable public API from `lib.rs` only when intended
3. update this document if top-level module responsibilities change
