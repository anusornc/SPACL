# OWL2 Reasoner Competitor Benchmarks

This directory contains Docker-based benchmarking infrastructure for comparing **SPACL (Tableauxx runtime)** with major OWL reasoners using a single, status-checked harness.

## Prerequisites

- Docker Engine (`docker` CLI must be usable by your account)
- `jq`
- GNU `timeout` (from `coreutils`)

Quick check:

```bash
docker --version
jq --version
timeout --version | head -n 1
```

## Competitors

| Reasoner | Type | Language | Benchmark Status |
|----------|------|----------|------------------|
| **Tableauxx** | Tableau + Speculative Parallelism | Rust | ✅ Direct |
| **HermiT** | Hypertableau | Java | ✅ Direct |
| **Konclude** | Saturation/Tableau hybrid | C++ | ✅ Direct |
| **Openllet** | Tableau | Java | ✅ Direct |
| **ELK** | Consequence-based (EL profile) | Java | ✅ Direct |
| **JFact** | Tableau | Java | ✅ Direct |
| **Pellet** | Tableau | Java | ✅ Direct (legacy Pellet 2.3.3 artifacts) |
| **FaCT++** | Tableau | C++ | ⚠️ Optional (`INCLUDE_FACTPP=1`, may be unavailable by environment) |

## Quick Start

```bash
# Run complete benchmark suite
cd benchmarks/competitors
./scripts/run_benchmarks.sh

# Recommended first run: one-ontology smoke (fast validation)
RUN_ID=smoke_$(date +%Y%m%d_%H%M%S) \
ONTOLOGY_SUITE=standard \
ONTOLOGY_REGEX='^disjunctive_simple\.owl$' \
REASONERS_OVERRIDE=tableauxx \
TIMEOUT_SECONDS=60 \
SKIP_BUILD=0 \
./scripts/run_benchmarks.sh all

# Or step by step:
./scripts/run_benchmarks.sh prepare   # Stage suite ontologies into an isolated run dir
./scripts/run_benchmarks.sh build     # Build Docker images
./scripts/run_benchmarks.sh run       # Run benchmarks
./scripts/run_benchmarks.sh report    # Generate report

# Optional: large real-world suite (PATO/DOID/UBERON/GO/ChEBI)
ONTOLOGY_SUITE=large SKIP_BUILD=1 TIMEOUT_SECONDS=900 ./scripts/run_benchmarks.sh all

# Optional: override reasoners or timeout
REASONERS_OVERRIDE=tableauxx,hermit,konclude,openllet,elk,jfact,pellet TIMEOUT_SECONDS=600 ./scripts/run_benchmarks.sh all
INCLUDE_FACTPP=1 ./scripts/run_benchmarks.sh all

# Optional: override ontology directories (for external benchmark sets)
ONTOLOGIES_DIR_OVERRIDE=/path/to/custom_owl_dir ONTOLOGY_SUITE=standard ./scripts/run_benchmarks.sh all

# Stage split for fair analysis (Tableauxx parse-only vs reason-stage timing)
./scripts/run_stage_benchmark.sh go-basic.owl
TIMEOUT_SECONDS=1800 ./scripts/run_stage_benchmark.sh chebi.owl

# Optional legacy owlbin diagnostic (not part of the primary stage contract)
./scripts/profile_bin_cache.sh go-basic.owl
REPEAT_WARM=5 TIMEOUT_SECONDS=1800 ./scripts/profile_bin_cache.sh chebi.owl

# Batch stage split across large suite
./scripts/run_stage_suite.sh
CHEBI_TIMEOUT_SECONDS=2400 ./scripts/run_stage_suite.sh doid.owl go-basic.owl uberon.owl
```

If you hit Docker socket permission errors, run:

```bash
sudo usermod -aG docker "$USER"
# then log out/login (or reboot), and retry
```

## Individual Reasoner Testing

```bash
# Build a specific reasoner
docker build -f docker/Dockerfile.hermit -t owl-reasoner-hermit ../..

# Run benchmark
docker run --rm \
  -v ./ontologies:/ontologies:ro \
  -v ./results:/results \
  owl-reasoner-hermit \
  /ontologies/univ-bench.owl consistency
```

## Directory Structure

```
benchmarks/competitors/
├── docker/                  # Dockerfiles for each reasoner
│   ├── Dockerfile.hermit
│   ├── Dockerfile.konclude
│   ├── Dockerfile.openllet
│   ├── Dockerfile.elk
│   ├── Dockerfile.jfact
│   ├── Dockerfile.pellet
│   ├── Dockerfile.factpp
│   └── Dockerfile.tableauxx
├── scripts/
│   ├── run_benchmarks.sh   # Main benchmark orchestration
│   ├── run_stage_benchmark.sh
│   ├── profile_bin_cache.sh
│   └── run_stage_suite.sh
├── ontologies/             # Baseline/synthetic ontologies
├── results/                # Benchmark output (run-scoped under results/history/<run_id>)
└── docker-compose.yml      # Compose setup (optional)
```

## Test Ontologies

Suites:

- `ONTOLOGY_SUITE=standard` (default): `benchmarks/competitors/ontologies/*.owl` + `tests/data/*.owl`
- `ONTOLOGY_SUITE=large`: `benchmarks/ontologies/other/*.owl` (set `INCLUDE_CHEBI=0` to skip ChEBI)
- `ONTOLOGY_SUITE=all`: union of standard + large

Examples in the large suite:

- `pato.owl`
- `doid.owl`
- `uberon.owl`
- `go-basic.owl`
- `chebi.owl` (very large)

## Metrics

Each reasoner is tested on one operation per run:

1. **Consistency Checking** - Determine if ontology is consistent (`OPERATION=consistency`)

Measured:
- Wall-clock time around container run (ms) - **primary comparison metric**
- Success/failure status
- Error messages (if any)
- Engine-reported duration when available (diagnostic only)

For `Tableauxx`, stage timing (`parse_time_ms`, `reason_time_ms`) is also emitted.
Use stage scripts to avoid mixing parse and reason effects when comparing optimization work.
The primary stage split now uses `parse_only` and `reason_only_stage` from the same cold text run.
Use `./scripts/profile_bin_cache.sh` only for `.owlbin` diagnostics; it is not part of the primary KPI.

## Latest Parser Snapshot (2026-02-19)

Structural parser mode (`OWL2_REASONER_STRUCTURAL_XML_PARSER=1`) with stage harness showed:

- `doid.owl` parse-only: `27307 ms -> 830 ms` (`-96.96%`)
  - `benchmarks/competitors/results/history/stages_doid_20260218_183604/stage_summary.csv`
  - `benchmarks/competitors/results/history/stages_doid_20260219_130432/stage_summary.csv`
- `go-basic.owl` parse-only: `158306 ms -> 3472 ms` (`-97.81%`)
  - `benchmarks/competitors/results/history/stages_go_basic_20260218_185126/stage_summary.csv`
  - `benchmarks/competitors/results/history/stages_go_basic_20260219_130710/stage_summary.csv`
- `uberon.owl` parse-only: `145843 ms -> 2952 ms` (`-97.98%`)
  - `benchmarks/competitors/results/history/stages_uberon_20260218_185933/stage_summary.csv`
  - `benchmarks/competitors/results/history/stages_uberon_20260219_130942/stage_summary.csv`

Mean parse-only improvement across these large ontologies: `-97.58%`.

## Status Semantics

Each result JSON is classified as one of:

- `success`: Reasoner executed and returned a completed status
- `failed`: Runtime or command failure
- `timeout`: Exceeded `TIMEOUT_SECONDS`
- `not_available`: Reasoner intentionally not benchmarkable in the current setup

This prevents false-positive "success" rows when a reasoner fails to start.

## Notes

- **Pellet vs Openllet**: Both are benchmarked directly. Pellet uses legacy `2.3.3` artifacts (`pellet-owlapiv3`) from `maven.aksw.org`.
- **Pellet availability caveat**: If `maven.aksw.org` is unreachable in a given environment, Pellet image build may fail and benchmark rows will become `not_available`.
- **Konclude input format**: The current corpus is RDF/XML `.owl`; this Konclude CLI path reports parser errors (`OWL2/XML` expectation) and is therefore marked `failed` in result JSONs.
- **`univ-bench.owl` in this repo**: The bundled file is intentionally lightweight (`62` lines in current tree) and should be treated as a smoke/sanity input, not a full-fidelity performance target.
- **FaCT++**: Standalone CLI availability is environment-dependent; use `INCLUDE_FACTPP=1` to attempt it.
- **Timeouts**: Default timeout is 5 minutes per reasoner/ontology run.
- **Hardware**: Results depend on host CPU/RAM and Docker limits.

## Output Artifacts

For each run:

- `benchmarks/competitors/results/history/<run_id>/run_metadata.json`
- `benchmarks/competitors/results/history/<run_id>/*.json` (one per reasoner+ontology)
- `benchmarks/competitors/results/history/<run_id>/benchmark_report.md`
- `benchmarks/competitors/results/history/<run_id>/results.csv`
- `benchmarks/competitors/results/history/<run_id>/paper_table.md`
- `benchmarks/competitors/results/history/<run_id>/paper_table.tex`

`benchmarks/competitors/results/latest` points to the most recent completed run.

## External Validation (OWL2Bench)

External benchmark wrappers are provided under:

- `benchmarks/external/owl2bench/prepare.sh`
- `benchmarks/external/owl2bench/run.sh`
- `benchmarks/external/owl2bench/report.sh`

These wrappers stage external `.owl` files and execute the same core harness for
status-compatible reports (`success/failed/timeout/not_available`).

## References

- [HermiT](http://www.hermit-reasoner.com/) - Oxford University
- [Konclude](https://www.derivo.de/fileadmin/externe_websites/ext_derivo/Konclude/) - University of Ulm
- [Pellet](https://github.com/stardog-union/pellet) - Stardog
- [FaCT++](https://bitbucket.org/dtsarkov/factplusplus/) - Manchester University
