# Quick Start

## 1. Build and test

```bash
cargo build --release
cargo test
```

## 2. Run the main reasoner CLI

```bash
# Consistency check
cargo run --bin owl2-reasoner -- check tests/data/univ-bench.owl

# Auto profile-aware reasoner selection
cargo run --bin owl2-reasoner -- check-auto tests/data/univ-bench.owl

# Stats
cargo run --bin owl2-reasoner -- stats tests/data/univ-bench.owl

# Convert to binary format
cargo run --bin owl2-reasoner -- convert tests/data/univ-bench.owl /tmp/univ-bench.owlbin
```

## 3. Alternative CLIs

```bash
cargo run --bin owl2_validation -- help
cargo run --bin epcis-reasoner -- help
```

## 4. Large ontology loading options

Environment controls used by shared loader:

- `OWL2_REASONER_LARGE_PARSE=1` - enable large parse mode
- `OWL2_REASONER_AUTO_CACHE=1` - write `.owlbin` cache after text parse
- `OWL2_REASONER_FORCE_TEXT=1` - force text parsing even if `.owlbin` exists
- `OWL2_REASONER_BIN_ONLY=1` - require `.owlbin`
- `OWL2_REASONER_MAX_FILE_SIZE=<bytes>` - override file-size threshold
- `OWL2_REASONER_STAGE_TIMING=1` - emit parse/reason stage timing in benchmark runs

Example:

```bash
OWL2_REASONER_LARGE_PARSE=1 OWL2_REASONER_AUTO_CACHE=1 \
cargo run --bin owl2-reasoner -- check tests/data/hierarchy_10000.owl
```

## 5. Run benchmark harness

The public package includes benchmark harness scripts under:

- `benchmarks/competitors/scripts/run_benchmarks.sh`
- `benchmarks/competitors/scripts/run_stage_benchmark.sh`
- `benchmarks/competitors/scripts/run_stage_suite.sh`

These scripts require additional benchmark assets (for example competitor Dockerfiles and external ontology sets) that are not bundled in this public repository.

Minimal example:

```bash
RUN_ID=public_smoke_YYYYMMDD \
ONTOLOGY_SUITE=standard \
REASONERS_OVERRIDE=tableauxx,hermit,konclude,openllet,elk,jfact,pellet \
TIMEOUT_SECONDS=180 \
SKIP_BUILD=0 \
benchmarks/competitors/scripts/run_benchmarks.sh all
```
