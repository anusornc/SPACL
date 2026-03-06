# SPACL (Tableauxx Repository)

This repository contains the SPACL OWL reasoner implementation and benchmarking toolkit in Rust.
Repository/package names still use `tableauxx` in some paths and scripts for historical compatibility.
It supports multi-format ontology loading, profile-aware reasoning, and reproducible head-to-head benchmarking.

## Prerequisites

- Rust toolchain (stable; tested with Rust `1.84+`)
- Docker Engine (`docker` CLI must be usable by your user)
- `jq`
- GNU `timeout` (usually from `coreutils`)

Quick check:

```bash
cargo --version
docker --version
jq --version
timeout --version | head -n 1
```

## Current benchmark position (latest)

From the latest clean OWL2Bench core 3-run aggregate in this repository:

- Run set: `owl2bench_univ_core_clean_20260223`, `_r2`, `_r3`
- Aggregate: `benchmarks/competitors/results/history/owl2bench_univ_core_clean_aggregate_20260223`
- SPACL/Tableauxx: `12/12` success, median wall time `2107.5 ms`, P95 `2707 ms`
- In this harness, SPACL/Tableauxx is median winner on all 4 core profiles (`DL`, `EL`, `QL`, `RL`)

See details in `docs/benchmarking/BENCHMARK_RUNBOOK.md`.

## Quick start

```bash
# Build
cargo build --release

# Run tests
cargo test

# Check consistency
cargo run --bin owl2-reasoner -- check tests/data/univ-bench.owl

# Auto-select reasoner by ontology profile
cargo run --bin owl2-reasoner -- check-auto tests/data/univ-bench.owl

# Convert ontology to binary cache format
cargo run --bin owl2-reasoner -- convert tests/data/univ-bench.owl /tmp/univ-bench.owlbin
```

## Benchmark smoke test (recommended first)

Before full competitor runs, verify your environment with a one-ontology smoke run:

```bash
RUN_ID=smoke_$(date +%Y%m%d_%H%M%S) \
ONTOLOGY_SUITE=standard \
ONTOLOGY_REGEX='^disjunctive_simple\.owl$' \
REASONERS_OVERRIDE=tableauxx \
TIMEOUT_SECONDS=60 \
SKIP_BUILD=0 \
benchmarks/competitors/scripts/run_benchmarks.sh all
```

If Docker permission fails (e.g., `permission denied while trying to connect to the docker API socket`), add your user to Docker group and re-login:

```bash
sudo usermod -aG docker "$USER"
# then log out/login (or reboot) before retrying
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

Primary benchmark harness:

- `benchmarks/competitors/scripts/run_benchmarks.sh`

External OWL2Bench wrapper:

- `benchmarks/external/owl2bench/prepare.sh`
- `benchmarks/external/owl2bench/run.sh`
- `benchmarks/external/owl2bench/report.sh`

Operational benchmark guide:

- `docs/benchmarking/BENCHMARK_RUNBOOK.md`

## Documentation map

Start here:

- `docs/README.md`
- `docs/QUICK_START.md`
- `docs/PROJECT_STRUCTURE.md`
- `docs/DIRECTORY_STRUCTURE.md`

Domain and deployment docs:

- `docs/BLOCKCHAIN_TRANSACTION_PROFILE_GUIDE.md`

## Paper

Submission workspace:

- `paper/submission/manuscript.tex`
- `paper/submission/compile.sh`

Build PDF:

```bash
cd paper/submission
./compile.sh
```

## License

- Code: MIT (`LICENSE`)
