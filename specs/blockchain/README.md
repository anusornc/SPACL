# Blockchain Integration Specs

This directory contains implementation-ready specifications for blockchain
transaction interoperability with Tableauxx.

## Structure

- `schemas/transaction-envelope.schema.json`
  - Canonical transaction envelope schema for participant submissions.
- `schemas/ontology-pack-manifest.schema.json`
  - Canonical ontology pack manifest schema for governance and deployment.
- `examples/transaction-envelope.example.json`
  - Example transaction envelope payload.
- `examples/ontology-pack-manifest.example.json`
  - Example ontology pack manifest.

## Design Intent

- Keep blockchain integration artifacts separate from core Tableauxx docs/code.
- Make consortium-level contracts explicit and machine-validated.
- Support strict versioning and hash pinning for deterministic execution.
