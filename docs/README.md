# Documentation Hub

This directory contains the active technical documentation for Tableauxx.

## Start here

- `docs/QUICK_START.md` - first commands to build, test, run, benchmark
- `docs/PROJECT_STRUCTURE.md` - source-level architecture (`src/*`)
- `docs/DIRECTORY_STRUCTURE.md` - repository-level layout and ownership
- `docs/benchmarking/BENCHMARK_RUNBOOK.md` - benchmark protocol, run IDs, reproducibility

## Core technical docs

- `docs/SPACL_ALGORITHM.md` - SPACL reasoning algorithm notes
- `docs/HIERARCHICAL_CLASSIFICATION_IMPLEMENTATION.md` - hierarchy-path implementation details
- `docs/LARGE_ONTOLOGY_OPTIMIZATION_PLAN.md` - large ontology optimization strategy
- `docs/experiments/TABLEAUXX_PARSER_ALGORITHM_REFERENCE.md` - parser pipeline reference
- `docs/experiments/PARSER_SPEED_DECISION_LOG.md` - parser optimization decision log

## Deployment and integration docs

- `docs/BLOCKCHAIN_TRANSACTION_PROFILE_GUIDE.md` - ontology transaction integration guidance

## Reports and research

- `docs/reports/` - report artifacts and summaries
- `docs/research/` - research notes and findings

## Archive policy

Legacy or superseded planning/status docs are moved to:

- `docs/archive/legacy-status-2026q1/`

Rule:
- keep active docs concise and current
- move stale planning/status snapshots into archive instead of deleting historical context

## Updating docs

When code or benchmark behavior changes:

1. update `docs/QUICK_START.md` if command usage changed
2. update `docs/benchmarking/BENCHMARK_RUNBOOK.md` if benchmark protocol/results changed
3. update `docs/PROJECT_STRUCTURE.md` or `docs/DIRECTORY_STRUCTURE.md` if layout changed
