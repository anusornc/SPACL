# Benchmark Runbook

This document tracks how benchmarks are executed in this repository and which
run artifacts are currently used for paper-level reporting.

## 0) Environment Prerequisites

- Docker Engine (`docker` CLI accessible by your user)
- `jq`
- GNU `timeout`

Quick check:

```bash
docker --version
jq --version
timeout --version | head -n 1
```

If Docker socket permission fails (`permission denied ... /var/run/docker.sock`),
add your user to Docker group and re-login:

```bash
sudo usermod -aG docker "$USER"
```

## 1) Current Primary Benchmark Flow

Core harness:

- `benchmarks/competitors/scripts/run_benchmarks.sh`

Data suites:

- `standard`: `benchmarks/competitors/ontologies/*.owl`
- `large`: `benchmarks/ontologies/other/*.owl` (controlled by `INCLUDE_CHEBI`)

Primary operation:

- `consistency` (wall-clock is the primary comparison metric)

Status model:

- `success`, `failed`, `timeout`, `not_available`

## 2) Current Paper-Grade Artifacts

Primary artifacts currently referenced for paper-level summaries:

- Head-to-head aggregate (standard + large):
  - `headtohead_repeated_aggregate_20260226`
  - Path: `benchmarks/competitors/results/history/headtohead_repeated_aggregate_20260226`
- OWL2Bench clean 3-run aggregate:
  - `owl2bench_univ_core_clean_aggregate_20260223`
  - Path: `benchmarks/competitors/results/history/owl2bench_univ_core_clean_aggregate_20260223`
- Stage-level parser A/B references:
  - `stages_doid_20260219_130432`
  - `stages_go_basic_20260219_130710`
  - `stages_uberon_20260219_130942`

Timeout policy used in these flows:

- Standard / OWL2Bench core: `300s`
- Large biomedical panel: `900s`

## 3) Re-run Commands (Canonical)

Recommended smoke run first:

```bash
RUN_ID=smoke_$(date +%Y%m%d_%H%M%S) \
ONTOLOGY_SUITE=standard \
ONTOLOGY_REGEX='^disjunctive_simple\.owl$' \
REASONERS_OVERRIDE=tableauxx \
TIMEOUT_SECONDS=60 \
SKIP_BUILD=0 \
benchmarks/competitors/scripts/run_benchmarks.sh all
```

Small suite:

```bash
RUN_ID=small_workload_suite_real_YYYYMMDD \
ONTOLOGY_SUITE=standard \
ONTOLOGY_REGEX='^(disjunctive_simple|disjunctive_test|hierarchy_100|hierarchy_1000|hierarchy_10000|univ-bench)\.owl$' \
REASONERS_OVERRIDE=tableauxx,hermit,konclude,openllet,elk,jfact,pellet \
TIMEOUT_SECONDS=180 \
SKIP_BUILD=0 \
benchmarks/competitors/scripts/run_benchmarks.sh all
```

Large suite:

```bash
RUN_ID=large_sanity_real_YYYYMMDD \
ONTOLOGY_SUITE=large \
INCLUDE_CHEBI=0 \
ONTOLOGY_REGEX='^(doid|go-basic|uberon)\.owl$' \
REASONERS_OVERRIDE=tableauxx,hermit,konclude,openllet,elk,jfact,pellet \
TIMEOUT_SECONDS=900 \
SKIP_BUILD=1 \
OWL2_REASONER_STAGE_TIMING=1 \
benchmarks/competitors/scripts/run_benchmarks.sh all
```

## 4) External Validation (OWL2Bench)

New wrapper entry points:

- `benchmarks/external/owl2bench/prepare.sh`
- `benchmarks/external/owl2bench/run.sh`
- `benchmarks/external/owl2bench/report.sh`

Purpose:

- Run external ontology sets through the **same competitor harness** (same status
  handling and report format), reducing “custom benchmark only” risk.

Quick usage:

```bash
OWL2BENCH_SOURCE_DIR=/path/to/owl2bench \
benchmarks/external/owl2bench/prepare.sh

RUN_ID=owl2bench_smoke \
TIMEOUT_SECONDS=900 \
SKIP_BUILD=1 \
benchmarks/external/owl2bench/run.sh all
```

Current smoke artifact (wrapper validation):

- `owl2bench_external_smoke_20260221_v3`
- Path: `benchmarks/competitors/results/history/owl2bench_external_smoke_20260221_v3`
- Note: this smoke used locally staged sample ontologies to validate wrapper flow.

Current external comparison artifact (OWL2Bench core profiles):

- `owl2bench_univ_core_20260223`
- Path: `benchmarks/competitors/results/history/owl2bench_univ_core_20260223`
- Scope: `UNIV-BENCH-OWL2DL`, `UNIV-BENCH-OWL2EL`, `UNIV-BENCH-OWL2QL`, `UNIV-BENCH-OWL2RL`
- Timeout policy: `300s`
- Result snapshot:
  - `tableauxx` success `4/4`, fastest on `EL`, `QL`, `RL`
  - `elk` success `4/4`, fastest on `DL`
  - `konclude` failed `4/4` in this environment (see per-run logs)
- `jfact` unstable on `HasKey` in this dataset (`1` success, `2` failed, `1` timeout)

Clean rerun artifact (no manual intervention during run):

- `owl2bench_univ_core_clean_20260223`
- Path: `benchmarks/competitors/results/history/owl2bench_univ_core_clean_20260223`
- Scope: `UNIV-BENCH-OWL2DL`, `UNIV-BENCH-OWL2EL`, `UNIV-BENCH-OWL2QL`, `UNIV-BENCH-OWL2RL`
- Timeout policy: `300s`
- Result snapshot:
  - `tableauxx` success `4/4`, fastest `4/4`
  - `elk`, `hermit`, `openllet`, `pellet` success `4/4`
  - `konclude` failed `4/4` in this environment
  - `jfact` success `1/4`, timeout `3/4` (HasKey-related limitations)

Three-run clean baseline (for robust statistics):

- `owl2bench_univ_core_clean_20260223`
- `owl2bench_univ_core_clean_20260223_r2`
- `owl2bench_univ_core_clean_20260223_r3`
- Aggregate: `benchmarks/competitors/results/history/owl2bench_univ_core_clean_aggregate_20260223`
- Aggregate snapshot (success-only timing):
  - `tableauxx`: `12/12` success, median `2107.5 ms`, P95 `2707 ms`
  - `pellet`: `12/12` success, median `2886.0 ms`, P95 `3135 ms`
  - `elk`: `12/12` success, median `2999.0 ms`, P95 `3370 ms`
  - `openllet`: `12/12` success, median `3204.5 ms`, P95 `3836 ms`
  - `hermit`: `12/12` success, median `3573.0 ms`, P95 `4505 ms`
  - `jfact`: `3/12` success (QL only), median `3507 ms`, P95 `3762 ms`
  - `konclude`: `0/12` success in this environment

Filtering note:

- Running `prepare.sh` on OWL2Bench repository root may include many non-core test files.
- For paper-facing OWL2Bench profile claims, use `OWL2BENCH_SOURCE_DIR=/tmp/owl2bench/OWL2Bench`.

## 5) Governance Notes

- Do not publish runs that include synthetic/fake timing rows.
- Keep raw run artifacts in `benchmarks/competitors/results/history/<run_id>`.
- For paper claims, reference explicit run IDs and timeout policy.
- If a reasoner is environment-dependent (e.g., legacy artifact repository), keep
  that caveat in method/limitations text.
- Always validate run statuses from result JSONs; a completed harness run can still
  contain only `not_available` rows when dependencies (e.g., Docker permissions/image build) fail.
