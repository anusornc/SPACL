# OWL2Bench External Validation

This folder adds an **external benchmark workflow** on top of the existing
`benchmarks/competitors/scripts/run_benchmarks.sh` harness.

Goal: validate Tableauxx with an external ontology set while preserving the same
status model (`success/failed/timeout/not_available`) and report format used in
the paper pipeline.

## Layout

- `prepare.sh`: stage `.owl` files for benchmark execution
- `run.sh`: execute the competitor harness against staged OWL2Bench ontologies
- `report.sh`: regenerate report/csv/tables for an existing `RUN_ID`

## Quick Start

```bash
# 1) Prepare staged ontologies from a local OWL2Bench source tree
OWL2BENCH_SOURCE_DIR=/path/to/owl2bench \
benchmarks/external/owl2bench/prepare.sh

# Optional: auto-clone source when missing
OWL2BENCH_AUTO_CLONE=1 benchmarks/external/owl2bench/prepare.sh

# 2) Run benchmark (uses existing competitor harness)
RUN_ID=owl2bench_smoke \
TIMEOUT_SECONDS=900 \
SKIP_BUILD=1 \
benchmarks/external/owl2bench/run.sh all

# 3) Rebuild report later
benchmarks/external/owl2bench/report.sh owl2bench_smoke
```

## Important Environment Variables

- `OWL2BENCH_SOURCE_DIR`: source tree that contains `.owl` files (recursive scan)
- `OWL2BENCH_AUTO_CLONE=1`: clone OWL2Bench source when `SOURCE_DIR` missing
- `OWL2BENCH_CLONE_URL`: clone URL (default: `https://github.com/kracr/owl2bench.git`)
- `OWL2BENCH_MAX_FILES`: cap number of staged files (`0` = all)
- `OWL2BENCH_LINK_MODE=link|copy`: stage via hardlink/symlink fallback or copy
- `OWL2BENCH_STAGED_DIR`: staged directory used by `run.sh`

Harness-side variables (forwarded to `run_benchmarks.sh`):

- `RUN_ID`, `TIMEOUT_SECONDS`, `SKIP_BUILD`, `REASONERS_OVERRIDE`, `ONTOLOGY_REGEX`

Default `ONTOLOGY_REGEX` in `run.sh` is `^...._.*\.owl$` so only staged
OWL2Bench files are included (avoids accidental inclusion of `tests/data/*.owl`).

## Reproducibility Notes

- `run.sh` forces `ONTOLOGY_SUITE=standard` and sets `ONTOLOGIES_DIR_OVERRIDE` to
  the staged OWL2Bench directory, so no change is required in the core harness.
- `run_metadata.json` records both ontology directory paths (`ontologies_dir` and
  `large_ontologies_dir`) for traceability.
- If ontology file names collide across source folders, `prepare.sh` prefixes
  staged filenames with an index to keep them unique.
