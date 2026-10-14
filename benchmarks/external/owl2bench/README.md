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
- `stage_report.sh`: add OWL2Bench profile/stage metadata to harness CSV output

## Quick Start

```bash
# 1) Prepare staged ontologies from a local OWL2Bench source tree
OWL2BENCH_SOURCE_DIR=/path/to/owl2bench \
benchmarks/external/owl2bench/prepare.sh

# Optional deterministic smoke staging across DL/EL/QL/RL.
# When scalability assets have multiple ABox or university sizes, this keeps
# the first two sorted sizes per profile.
OWL2BENCH_SMOKE=1 \
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

## Freeze profile datasets for the query lane

The consistency wrapper above stages existing OWL files. For the reasoning-aware
query lane, freeze one official generated dataset per profile first. The freezer
uses the OWL2Bench generator with an explicit university count and seed, extracts
the explicit ABox, converts the generated dataset and ABox to canonical N-Quads,
and writes a profile-isolated `dataset-manifest.json`.

```bash
python3 benchmarks/external/owl2bench/profile_dataset.py prepare \
  --source-dir /path/to/owl2bench \
  --output-dir /path/to/owl2bench-profile-datasets \
  --profiles DL,EL,QL,RL \
  --universities 1 \
  --seed 1 \
  --query-manifest benchmarks/ontologies/external/owl2bench/query-staged/manifest.tsv

python3 benchmarks/external/owl2bench/profile_dataset.py validate \
  --manifest /path/to/owl2bench-profile-datasets/dataset-manifest.json

python3 benchmarks/external/owl2bench/profile_dataset.py compare \
  --first /path/to/first/dataset-manifest.json \
  --second /path/to/second/dataset-manifest.json \
  --output /path/to/reproducibility.json
```

The package records official source and generator revisions, every input and
conversion hash, isolated profile contexts, and the declared resource envelope.
It does not create query answers or an oracle. `complete` here means that the
input package is reproducible; materializer completeness and query correctness
remain separate gates.

## Generate the scalability ladder datasets (ADR 0026)

The consistency performance lane runs over the official low scalability
ladder (universities 1, 2, 5, 10, 20, 50; seed 1; one dataset per profile
DL, EL, QL, RL). `ladder_dataset.py` generates the 20 missing points from
the pinned official generator (ADR 0015/0017 preserved clone; jar hash and
revision are verified fail-closed), runs every generation twice in isolated
workspaces, and stages a dataset only after the two outputs are
byte-identical. The one-university correctness anchors are reused by their
pinned sha256 from the staged anchor manifest and are never regenerated.

Datasets land in gitignored storage under
`benchmarks/ontologies/external/owl2bench/scalability/`; only
`ladder-manifest.json` (all 24 ladder points: generator identity,
invocation, size, profile, output sha256, and per-run wall time and peak
RSS actuals) is committed.

```bash
python3 benchmarks/external/owl2bench/ladder_dataset.py generate \
  --source-dir /path/to/preserved-owl2bench-clone

python3 benchmarks/external/owl2bench/ladder_dataset.py validate \
  --manifest benchmarks/ontologies/external/owl2bench/scalability/ladder-manifest.json \
  --shape-only
```

`--timeout-seconds` (or `OWL2BENCH_LADDER_TIMEOUT_SECONDS`) bounds each
single generator run; the default budget is generous for universities=50.
`validate --shape-only` checks the committed manifest without requiring the
local datasets.

## Important Environment Variables

- `OWL2BENCH_SOURCE_DIR`: source tree that contains `.owl` files (recursive scan)
- `OWL2BENCH_AUTO_CLONE=1`: clone OWL2Bench source when `SOURCE_DIR` missing
- `OWL2BENCH_CLONE_URL`: clone URL (default: `https://github.com/kracr/owl2bench.git`)
- `OWL2BENCH_MAX_FILES`: cap number of staged files (`0` = all)
- `OWL2BENCH_LINK_MODE=link|copy`: stage via hardlink/symlink fallback or copy
- `OWL2BENCH_STAGED_DIR`: staged directory used by `run.sh`
- `OWL2BENCH_PROFILES`: comma-separated profile labels to stage (`DL,EL,QL,RL`)
- `OWL2BENCH_STAGE`: `correctness`, `scalability`, or `all`
- `OWL2BENCH_SMOKE=1`: deterministic smoke subset, one correctness case per
  profile and up to two scalability sizes per profile when available

Harness-side variables (forwarded to `run_benchmarks.sh`):

- `RUN_ID`, `TIMEOUT_SECONDS`, `SKIP_BUILD`, `REASONERS_OVERRIDE`, `ONTOLOGY_REGEX`

Default `ONTOLOGY_REGEX` in `run.sh` is `^...._.*\.owl$` so only staged
OWL2Bench files are included (avoids accidental inclusion of `tests/data/*.owl`).

## Reproducibility Notes

- `run.sh` forces `ONTOLOGY_SUITE=standard` and sets `ONTOLOGIES_DIR_OVERRIDE` to
  the staged OWL2Bench directory, so no change is required in the core harness.
- `prepare.sh` writes `manifest.tsv` with `profile`, `seed`, `abox_size`,
  `university_count`, and `stage`. Profile labels are OWL2Bench family labels:
  `DL`, `EL`, `QL`, and `RL`; `DL` is not treated as a W3C profile claim.
- Correctness and scalability are reported separately in
  `owl2bench_stage_report.md` and `owl2bench_stage_results.csv` inside each run
  directory. The stage suite is an execution-informational performance lane
  (ADR 0024/0027): scalability averages count completed executions with
  recorded wall time and the lane adjudicates no correctness.
- Query execution and query oracles are intentionally out of scope for this
  wrapper; issue #7 owns that lane.
- `run_metadata.json` records both ontology directory paths (`ontologies_dir` and
  `large_ontologies_dir`) for traceability.
- If ontology file names collide across source folders, `prepare.sh` prefixes
  staged filenames with an index to keep them unique.
