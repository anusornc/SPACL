# SPACL (Tableauxx)

SPACL is an OWL2 reasoner library and CLI toolkit implemented in Rust.
This public repository focuses on runnable code, examples, tests, and lightweight docs for trying the system quickly.

## Quick start

```bash
# Build
cargo build --release

# Run tests
cargo test

# Check ontology consistency
cargo run --bin owl2-reasoner -- check tests/data/univ-bench.owl

# Auto-select reasoner by ontology profile
cargo run --bin owl2-reasoner -- check-auto tests/data/univ-bench.owl

# Convert ontology to binary cache format
cargo run --bin owl2-reasoner -- convert tests/data/univ-bench.owl /tmp/univ-bench.owlbin
```

## Main CLIs

- `owl2-reasoner`: primary CLI for loading, checking, profiling, and converting ontologies
- `owl2_validation`: lightweight validation/check/stats CLI
- `epcis-reasoner`: EPCIS/traceability-oriented demo CLI

Run help:

```bash
cargo run --bin owl2-reasoner -- help
cargo run --bin owl2_validation -- help
cargo run --bin epcis-reasoner -- help
```

## Benchmarking

This repository includes benchmark runner scripts only:
- `benchmarks/competitors/scripts/run_benchmarks.sh`
- `benchmarks/competitors/scripts/run_stage_benchmark.sh`
- `benchmarks/competitors/scripts/run_stage_suite.sh`

Large benchmark datasets/results are intentionally not bundled in this public package.

## Documentation map

Start here:

- `docs/README.md`
- `docs/QUICK_START.md`
- `docs/PROJECT_STRUCTURE.md`
- `docs/SPACL_ALGORITHM.md`

## License

- Code: MIT (`LICENSE`)
