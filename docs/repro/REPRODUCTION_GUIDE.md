# Reproduction guide (Tier 2): regenerating the evidence from official pins

Date: 2026-10-13. Status: active (issue [#204](https://github.com/anusornc/tableauxx/issues/204),
spec [#202](https://github.com/anusornc/tableauxx/issues/202), [ADR 0028](../adr/0028-publishable-reproduction-via-spacl-public-repo.md)).

## Scope

This guide regenerates the four evidence lanes end to end from committed
scripts and official sources: corpus fetch, dataset ladder generation,
derived-oracle regeneration, engine image rebuilds from the committed
Dockerfiles, the comparative sweep invocations, aggregation and ladder
rendering, and the export step. Every step names its script path, its
environment variables, and its expected artifact outputs; no step is
prose-only. Checking an existing bundle instead of rerunning is Tier 1
([verification guide](VERIFICATION_GUIDE.md)).

Redistribution boundary, verbatim from ADR 0028:

> Redistribution is pointers, not blobs: no engine binaries, no corpora, and no
> generated datasets are redistributed; manifests record identities and hashes,
> sources are fetched from their official pins, and engine images are rebuilt
> from committed Dockerfiles.
>
> — `docs/adr/0028-publishable-reproduction-via-spacl-public-repo.md`

Claim boundaries travel with the evidence. The two performance lanes never
gain correctness wording. Verbatim from ADR 0024: "Its published claims are
execution performance and completability only: per-engine times, timeouts,
and completed-run counts on the pinned corpus." And: "ORE rows keep
`oracle_unavailable` as their permanent, honest correctness status." Verbatim
from ADR 0026: "The sweep is a performance lane under the ADR 0024 terms:
execution performance and completability claims only, no correctness claims —
no oracle exists for OWL2Bench consistency."

## Prerequisites

- Host tools: `git`, `docker`, `python3` (3.11+), `jq`, `curl`, `unzip`,
  `md5sum`/`sha256sum`, `awk`, `timeout`, GNU `time` (`/usr/bin/time`, for
  generator RSS actuals), and a Java runtime for the OWL2Bench generator.
- A Rust toolchain to build the Tableauxx reasoner binary on the host
  (`cargo build --release --bin owl2-reasoner`) — the W3C lane and the query
  verdict run it directly; the comparative lanes run it inside Docker.
- Run everything from the repository root. Run directories land under
  gitignored history; only manifests, metadata, hashes, and rendered tables
  are meant to be committed.

Steps executed while drafting this guide are marked **verified here**; the
heavy regenerations (engine sweeps, verdict rerun, dataset generation, image
builds) are **documented, not executed here** — their landing evidence is the
exported bundle and the run directories it cites.

## Step 1 — corpus fetch (official pins)

### 1a. ORE 2015 consistency sample (Zenodo pin + md5)

Official pin: [ORE 2015 Reasoner Competition Corpus
v1](https://zenodo.org/records/18578), DOI `10.5281/zenodo.18578`, archive
`ore2015_sample.zip` (≈725 MB), md5 `109f04cf8f124eb551d33c100e549730`. The
pin lives in `benchmarks/external/ore/lib.sh`
(`ORE2015_CORPUS_URL`/`ORE2015_CORPUS_MD5`); `prepare.sh` refuses the archive
on md5 mismatch.

```bash
ORE2015_AUTO_DOWNLOAD=1 benchmarks/external/ore/prepare.sh
```

Equivalent manual form: place `ore2015_sample.zip` under
`benchmarks/ontologies/external/ore/source/` and rerun `prepare.sh` (it
unzips a found archive, or set `ORE2015_SOURCE_DIR` to an extracted tree).

- Env: `ORE2015_AUTO_DOWNLOAD=1` (fetch); `ORE2015_GROUPS` (default
  `pure_dl,el`); `ORE2015_STAGED_DIR` (default
  `benchmarks/ontologies/external/ore/staged`); `ORE2015_LINK_MODE`
  (`link`|`copy`).
- Expected artifacts: `<staged>/manifest.tsv` (604 rows: 306 `pure_dl` + 298
  `el`), `<staged>/corpus_metadata.json` (records the DOI, source URL,
  archive md5, and the `oracle_unavailable` policy), and
  `<staged>/oracle_provenance.tsv`. Log line: `Prepared 604 ORE 2015
  ontologies`.
- **Verified here**: re-staging from the pinned source produced exactly
  306 + 298 = 604 rows with the corpus pin recorded in
  `corpus_metadata.json`; the retained archive's md5 equals the pin.

### 1b. W3C OWL 2 approved test manifest (official manifest + sha256)

Official pin: the W3C OWL 2 approved aggregate manifest
`https://www.w3.org/2009/11/owl-test/approved/all.rdf`, sha256
`6236dab71c2264903131042808e79e4aa41afab528f7cf6e633d2e3926b89b04` (the
sha256 recorded by the lane's earlier runs; treat a mismatch as a changed
source and stop).

```bash
mkdir -p benchmarks/ontologies/external/w3c-owl2/source
curl -fsSL "https://www.w3.org/2009/11/owl-test/approved/all.rdf" \
  -o benchmarks/ontologies/external/w3c-owl2/source/all.rdf
sha256sum benchmarks/ontologies/external/w3c-owl2/source/all.rdf
# expect 6236dab71c2264903131042808e79e4aa41afab528f7cf6e633d2e3926b89b04

W3C_OWL2_SOURCE_DIR=benchmarks/ontologies/external/w3c-owl2/source \
  benchmarks/external/w3c-owl2/prepare.sh
```

- Env: `W3C_OWL2_SOURCE_DIR` (directory holding `all.rdf`; the harness scans
  it for W3C test manifests), `W3C_OWL2_STAGED_DIR`,
  `W3C_OWL2_MANIFEST`, `W3C_OWL2_CLEAN_STAGE`.
- Expected artifacts: `<staged>/manifest.tsv` and
  `<staged>/prepare_summary.json`. Log lines: `discovered 507 manifest
  case(s)` and `baseline=329 unsupported=178` (the 178 are lane-filtered by
  design — non-Direct semantics, non-approved, or no normative RDF/XML — and
  are recorded with reasons, never silently dropped).
- **Verified here**: fetching the official manifest today hashes to the pin
  above, and `prepare.sh` stages 507 cases (329 baseline / 178
  lane-filtered), matching the published run's shape.

### 1c. OWL2Bench generator and query corpus (GitHub pin + jar sha256 / Zenodo)

Official generator pin: [kracr/owl2bench](https://github.com/kracr/owl2bench)
at revision `5a27536c8e7f7621b34c61c8a57ed9eb63e7829a`; the repo-tracked
`OWL2Bench.jar` at that revision has sha256
`c2bacf1d80e91ec05625b4c458f899a626ef92b85c60c6bac1f0a519060839ff`. Both
pins are enforced fail-closed by `benchmarks/external/owl2bench/ladder_dataset.py`
(`PINNED_REVISION`, `PINNED_JAR_SHA256`, ADR 0015/0017).

```bash
git clone https://github.com/kracr/owl2bench.git <owl2bench-clone>
git -C <owl2bench-clone> checkout 5a27536c8e7f7621b34c61c8a57ed9eb63e7829a
sha256sum <owl2bench-clone>/OWL2Bench.jar
# expect c2bacf1d80e91ec05625b4c458f899a626ef92b85c60c6bac1f0a519060839ff
```

The query lane's 22 official SPARQL query blocks come from the OWL2Bench
release on Zenodo, record
[3838735](https://zenodo.org/records/3838735) (`OWL2BENCH_SPARQL_Queries.txt`;
source revision recorded by the query staging as
`87af702049661ae9c2979dd59281d83985aed8900bbcc682f5556058c0dbdb56`).

- **Verified here**: a preserved clone of the official repo at the pinned
  revision carries `OWL2Bench.jar` hashing to the pin (the generator script
  re-checks both on every invocation); both Zenodo records (18578, 3838735)
  resolve today.

## Step 2 — dataset ladder generation (double-run determinism)

`benchmarks/external/owl2bench/ladder_dataset.py` generates the official low
ladder — universities 2, 5, 10, 20, 50 for each profile DL, EL, QL, RL, seed
1 — reusing the one-university correctness anchors by pinned sha256 instead of
regenerating them (ADR 0015/0017). Every generated point runs the generator
twice in isolated workspaces and stages nothing unless the two outputs are
byte-identical.

```bash
python3 benchmarks/external/owl2bench/ladder_dataset.py generate \
  --source-dir <owl2bench-clone>

python3 benchmarks/external/owl2bench/ladder_dataset.py validate \
  --manifest benchmarks/ontologies/external/owl2bench/scalability/ladder-manifest.json \
  --shape-only
```

- Env/flags: `--profiles` (default `DL,EL,QL,RL`), `--sizes` (default
  `2,5,10,20,50`), `--seed` (default 1), `--timeout-seconds` per generator
  run (default 3600, or `OWL2BENCH_LADDER_TIMEOUT_SECONDS`), `--time-bin`
  (default `auto` → `/usr/bin/time`), `--anchors-manifest` (default
  `benchmarks/ontologies/external/owl2bench/staged/manifest.tsv`), `--force`
  to regenerate instead of resuming verified points.
- Expected artifacts: datasets
  `benchmarks/ontologies/external/owl2bench/scalability/UNIV-BENCH-OWL2<profile>-seed1-universities<n>.owl`
  (gitignored), generation logs under `scalability/logs/`, and the committed
  `scalability/ladder-manifest.json` with 24 points (4 profiles × 6 sizes),
  each recording generator revision, jar sha256, invocation, output sha256,
  and the two-run determinism record. `validate --shape-only` must print
  `"status": "complete"`, `"point_count": 24`, `"errors": []`.
- **Verified here**: `validate --shape-only` over the committed manifest
  returns complete/24/empty-errors. Generation itself is **documented, not
  executed here**; its landing evidence is the committed
  `ladder-manifest.json` (in the bundle's tracked tree) whose per-point
  hashes `ladder_run.sh` re-verifies before every sweep invocation.

The anchor side of the ladder (u1 datasets for the query lane) is frozen by
the same pinned generator via `benchmarks/external/owl2bench/profile_dataset.py
prepare` (step 3).

## Step 3 — query-lane inputs: staged queries, dataset anchors, the derived oracle

### 3a. Stage the official queries

```bash
OWL2BENCH_QUERY_SOURCE_DIR=<dir containing OWL2BENCH_SPARQL_Queries.txt> \
  benchmarks/external/owl2bench/query/prepare.sh
```

Expected: `benchmarks/ontologies/external/owl2bench/query-staged/manifest.tsv`
with the 60 staged query/profile rows (22 query blocks expanded per profile),
source corpus `zenodo:3838735`, and the source revision and per-query hashes
recorded per row.

### 3b. Freeze the seed-1 profile datasets (anchors)

```bash
python3 benchmarks/external/owl2bench/profile_dataset.py prepare \
  --source-dir <owl2bench-clone> \
  --output-dir benchmarks/ontologies/external/owl2bench/owl2bench-profile-datasets \
  --profiles DL,EL,QL,RL --universities 1 --seed 1 \
  --query-manifest benchmarks/ontologies/external/owl2bench/query-staged/manifest.tsv

python3 benchmarks/external/owl2bench/profile_dataset.py validate \
  --manifest benchmarks/ontologies/external/owl2bench/owl2bench-profile-datasets/dataset-manifest.json
```

(`--query-manifest` must already exist — hence 3a first; it records the
official query provenance inside the frozen dataset manifest.)

Expected: `dataset-manifest.json` whose per-profile dataset sha256 equal the
anchor pins also carried by the ladder manifest and the oracle's
`dataset-metadata.json`: DL
`fce9e2c1fa58ffd8bd5deb7de224c1f4a7d8c0eec0c49dce0d42491e1854ed77`, EL
`49ac036c92153c0995f009ffea03d9843372eef5ffa29fd93f6533f6165f62c1`, QL
`d2c5037f49f8a71488b3e2e2b8d82057b18bda931319eb4b7cf3d09a41e17d9c`, RL
`78d7a0780e8cf7bf5d375d2a13b30c6bad8fa5c0763854770c6ae7f6fb9104f3`. The same
freezer/staging tooling writes the staged anchors manifest
(`benchmarks/ontologies/external/owl2bench/staged/manifest.tsv`) whose
seed-1 university-1 rows — invocation, revision, stage `correctness` — are
what `ladder_dataset.py` pins by hash in step 2.

### 3c. Regenerate the derived oracle (justification-backed, ADR 0013)

The governing procedure is the committed
[docs/benchmarking/OWL2BENCH_RL_DERIVED_ORACLE_PROCEDURE.md](../benchmarking/OWL2BENCH_RL_DERIVED_ORACLE_PROCEDURE.md).
Entry points:

```bash
# Derive one case's candidate answer set (per query x profile; repeat per case):
python3 benchmarks/external/owl2bench/query/derived_oracle_pipeline.py derive \
  --dataset <anchor.owl> --query <staged.rq> \
  --output-dir <oracle-dir> \
  --case-id owl2bench_query_001:DL --query-id owl2bench_query_001 \
  --profile DL --track shipped \
  --dataset-id owl2bench-dl:seed1-u1:sha256-fce9e2c1fa58ffd8bd5deb7de224c1f4a7d8c0eec0c49dce0d42491e1854ed77 \
  --dataset-sha256 fce9e2c1fa58ffd8bd5deb7de224c1f4a7d8c0eec0c49dce0d42491e1854ed77 \
  --source-query-sha256 <sha256 of the staged query> \
  --source-corpus zenodo:3838735 \
  --source-revision 87af702049661ae9c2979dd59281d83985aed8900bbcc682f5556058c0dbdb56

# Independent audit flips candidates to accepted / independently_audited:
python3 benchmarks/external/owl2bench/query/derived_oracle_audit.py \
  --oracle-dir <oracle-dir>
```

- Fail-closed identity: case identity is corpus + revision + query + profile
  + dataset sha256 + canonicalizer version; the shared canonicalizer
  (`benchmarks/external/owl2bench/query/canonicalize_result.py`,
  `owl2bench-query-canonicalizer/2`) must match the oracle manifest's pin or
  the verdict wrapper refuses the run.
- Expected artifacts per case: `answers/`, `proof/` (closure + derivations +
  per-row justification chains), `provenance/`, and a
  `derived-oracle-manifest.json` row with
  `acceptance_status: accepted`/`independently_audited` after the audit. The
  accepted oracle the published verdict consumed is
  `benchmarks/ontologies/external/owl2bench/derived-oracle-shipped-combined-v2/`
  (60 accepted rows).
- **Documented, not executed here** (60 derivations plus audits); the landing
  evidence is the accepted oracle manifest and the verdict run in the bundle.

## Step 4 — engine image rebuilds from the committed Dockerfiles

The ADR 0025 engine set is Tableauxx, Konclude, HermiT (EL track adds ELK;
ELK runs EL only). Build context is the repository root; the Tableauxx image
compiles the code under test in-container:

```bash
docker build -f benchmarks/competitors/docker/Dockerfile.tableauxx -t owl-reasoner-tableauxx .
docker build -f benchmarks/competitors/docker/Dockerfile.konclude   -t owl-reasoner-konclude .
docker build -f benchmarks/competitors/docker/Dockerfile.hermit     -t owl-reasoner-hermit .
docker build -f benchmarks/competitors/docker/Dockerfile.elk        -t owl-reasoner-elk .
```

- `Dockerfile.tableauxx` builds `cargo build --release --bin owl2-reasoner
  --features experimental-xml-parser` inside a `rust:bookworm` stage — no
  host binary precondition. Konclude pulls its official v0.7.0-1138 static
  release; HermiT and ELK build from the committed Maven poms
  (`hermit-pom.xml`, `elk-pom.xml`, `OwlapiConsistencyRunner.java`).
- Equivalent harness form (same tags, same context):
  `REASONERS_OVERRIDE=tableauxx,hermit,konclude,elk
  benchmarks/competitors/scripts/run_benchmarks.sh build`.
- Expected artifacts: the four local images. On every sweep run the harness
  records each engine's docker image digest and self-reported version into
  the run directory's `run_metadata.json` (ADR 0025: engine identities are
  pinned per run, and the Tableauxx image is rebuilt from the code under
  test before pinning).
- **Documented, not executed here**; the recorded digests live in each pass
  run's `run_metadata.json` (monorepo run history), referenced by the bundle's
  run ids.

## Step 5 — sweeps

### 5a. W3C conformance lane (correctness)

```bash
cargo build --release --bin owl2-reasoner   # or set W3C_OWL2_REASONER_BIN
RUN_ID=w3c_owl2_approved_full_<date>_r<rev> \
TIMEOUT_SECONDS=60 \
  benchmarks/external/w3c-owl2/run.sh
benchmarks/external/w3c-owl2/report.sh w3c_owl2_approved_full_<date>_r<rev>
```

- Env: `RUN_ID` (default timestamped), `TIMEOUT_SECONDS` (default 60; the
  published r69 run used 60), `W3C_OWL2_MANIFEST`, `W3C_OWL2_REASONER_BIN`,
  `W3C_OWL2_RESULTS_ROOT`.
- Expected artifacts under
  `benchmarks/external/w3c-owl2/results/history/<RUN_ID>/`: 507 per-case
  JSON results, `raw/`, `results.csv`, `summary.json` (status counts; the
  published run: 486 passed / 21 unsupported of 507), `report.md`,
  `run_metadata.json` (manifest path, source, reasoner binary, timeout).
- Lane claim (ADR 0002, verbatim): "Each lane report records its own source
  hash, semantics-specific oracle hash, and status counts; a lane passes only
  with zero wrong, failed, and unsupported rows, while timeout or unavailable
  oracle evidence blocks the lane."
- **Documented, not executed here**; staging (step 1b) was verified, the full
  507-case run was not repeated.

### 5b. OWL2Bench query verdict (correctness, split stack)

```bash
benchmarks/external/owl2bench/query/spacl_stack_verdict.sh
```

- Env: `SPACL_VERDICT_TIMEOUT_SECONDS` (default 3600), and the pinned-input
  overrides `SPACL_VERDICT_ROOT`, `SPACL_VERDICT_DATA_DIR`,
  `SPACL_VERDICT_ORACLE_DIR` (default
  `benchmarks/ontologies/external/owl2bench/derived-oracle-shipped-combined-v2`),
  `SPACL_VERDICT_QUERY_MANIFEST`, `SPACL_VERDICT_DATASET_METADATA`,
  `SPACL_VERDICT_RUN_ID` (default `spacl-stack-shipped-verdict-seed1-split`).
- The wrapper fails closed on: dataset anchors not hashing to the pinned
  metadata; canonicalizer version drifting from the oracle pin; split-stack
  cross-contract verification; materialization nondeterminism between the
  declaration load and the query load; any profile gate not passed; any
  oracle match not accepted; and result-status changes versus the prior
  accepted run (ADR 0027: "The re-run verdict must reproduce the accepted
  oracle matches; any mismatch is a blocker, not a re-try in place.").
- Expected artifacts:
  `benchmarks/external/owl2bench/verdict-stack-shipped-seed1/results/<RUN_ID>/`
  with `summary.json` (`query_count: 60`, `correctness_gate: passed`,
  `result_status_counts {"passed": 60}`), `report.md`, per-case JSONs, and
  the stack declaration under `verdict-stack-shipped-seed1/stack/`.
- **Documented, not executed here**; landing evidence is the exported verdict
  summary and report.

### 5c. ORE 2015 comparative sweep (performance; three serial passes per track)

Engine sets per ADR 0025 — pure_dl: `tableauxx,hermit,konclude`; el: those
three plus `elk`. One pass = one serial execution of the whole track under a
uniform 300-second per-case timeout. The first pass doubles as the smoke pass
(fix wiring problems before launching passes two and three).

```bash
# DL track (306 cases):
RUN_ID=ore2015_cmp_p1_pure_dl REASONERS_OVERRIDE=tableauxx,hermit,konclude \
TIMEOUT_SECONDS=300 SKIP_BUILD=1 benchmarks/external/ore/run.sh pure_dl run
RUN_ID=ore2015_cmp_p2_pure_dl ... benchmarks/external/ore/run.sh pure_dl run
RUN_ID=ore2015_cmp_p3_pure_dl ... benchmarks/external/ore/run.sh pure_dl run

# EL track (298 cases) — same three invocations with:
RUN_ID=ore2015_cmp_p1_el REASONERS_OVERRIDE=tableauxx,hermit,konclude,elk \
TIMEOUT_SECONDS=300 SKIP_BUILD=1 benchmarks/external/ore/run.sh el run
```

- The wrapper (`benchmarks/external/ore/run.sh`) pins the lane env for you:
  `OPERATION=consistency`, `TIMEOUT_SECONDS=300`, `SKIP_BUILD=1` (images from
  step 4; set `SKIP_BUILD=0` to rebuild inline), per-group `ONTOLOGY_REGEX`,
  and the honest oracle stance (`ORACLE_STATUS=unavailable`,
  `CORRECTNESS_STATUS=oracle_unavailable`). `ORE2015_DRY_RUN=1` prints the
  plan without executing. **Verified here**: the dry run resolves the staged
  corpus, track regex, engine set, and timeout as documented.
- `OWL2_REASONER_MAX_FILE_SIZE=<bytes>` (and the other
  `OWL2_REASONER_*` flags) are forwarded by
  `benchmarks/competitors/scripts/run_benchmarks.sh` into the Tableauxx
  container; it overrides the parser's max-file-size cap, which the ORE
  corpus's very large ontologies can exceed (`OWL2_REASONER_LARGE_PARSE=1`
  lifts the cap with arena allocation). Set it explicitly when replicating
  if cases refuse with a file-size guard.
- Expected artifacts per pass:
  `benchmarks/competitors/results/history/<RUN_ID>/` with per-case JSONs
  (execution status + container-inclusive `wall_time_ms`), `run_metadata.json`
  (engine image digests and versions per engine), `ore_metadata.json`
  (corpus pin, oracle policy), `results.csv`, and report tables.
- Performance-lane wording travels with every artifact: execution
  performance and completability only; `oracle_unavailable` is permanent on
  this lane.
- **Documented, not executed here**; landing evidence is the exported
  aggregation artifacts citing pass ids `ore2015_cmp_p1b_pure_dl`,
  `ore2015_cmp_p2_pure_dl`, `ore2015_cmp_p3_pure_dl` and
  `ore2015_cmp_p1c_el`, `ore2015_cmp_p2_el`, `ore2015_cmp_p3_el`.

### 5d. OWL2Bench scalability ladder sweep (performance; one point per invocation)

```bash
benchmarks/external/owl2bench/ladder_run.sh <PROFILE> <UNIVERSITIES> <PASS> [ACTION]
# e.g. all three passes of one point:
benchmarks/external/owl2bench/ladder_run.sh DL 5 1 run
benchmarks/external/owl2bench/ladder_run.sh DL 5 2 run
benchmarks/external/owl2bench/ladder_run.sh DL 5 3 run
```

- PROFILE `DL|EL|QL|RL`, UNIVERSITIES one of `1|2|5|10|20|50` (must exist in
  the ladder manifest), PASS `1|2|3`. Run id is forced to
  `owl2bench_cmp_p<pass>_<profile>_u<universities>`; engine sets follow ADR
  0025 per profile; `TIMEOUT_SECONDS` (default 300) is the per-case timeout;
  the dataset's sha256 is verified against the ladder manifest before
  anything runs; run ids are one-shot (an existing run dir aborts).
  `OWL2BENCH_LADDER_DRY_RUN=1` resolves and prints the plan only —
  **verified here** for `EL 50 3`.
- Expected artifacts per invocation:
  `benchmarks/competitors/results/history/owl2bench_cmp_p<pass>_<profile>_u<u>/`
  with the per-case JSON, `owl2bench_ladder_metadata.json` (dataset identity
  from the ladder manifest, engines, timeout, statistic),
  `run_metadata.json` (engine digests), and the harness report. 24 points ×
  3 passes = 72 invocations for the full sweep.
- **Documented, not executed here**; landing evidence is the per-point
  aggregations and ladder tables in the bundle.

## Step 6 — aggregation and ladder rendering

### 6a. Per-track comparative aggregation

```bash
AGGREGATION_OUTPUT_DIR=benchmarks/competitors/results/history/ore2015_cmp_agg_pure_dl \
benchmarks/competitors/scripts/run_comparative_aggregation.sh \
  benchmarks/competitors/results/history/ore2015_cmp_p1b_pure_dl \
  benchmarks/competitors/results/history/ore2015_cmp_p2_pure_dl \
  benchmarks/competitors/results/history/ore2015_cmp_p3_pure_dl
```

- Env: `AGGREGATION_OUTPUT_DIR` (default: an `aggregate_<timestamp>` sibling).
- Expected artifacts: `comparative_median_times.tsv`,
  `comparative_paper_table.md`, `comparative_engine_summary.tsv`,
  `comparative_engine_summary.md`, `aggregation_metadata.json` (pass run ids,
  engines, the ADR 0025 statistic sentence).
- **Verified here**: rerunning the aggregation over the three recorded ORE
  pass directories (both tracks) and one ladder point (`el_u50`) reproduced
  all four tables and the metadata byte-identically to the exported
  artifacts. The same command shape with each `agg/<profile>_u<n>` point's
  three pass dirs regenerates the 24 ladder aggregations.

### 6b. Ladder rendering

```bash
cd benchmarks/competitors/results/history
OWL2BENCH_LADDER_OUTPUT_DIR=<out> bash ../../../external/owl2bench/ladder_view.sh \
  owl2bench_cmp_agg_dl_u1 owl2bench_cmp_agg_dl_u2 owl2bench_cmp_agg_dl_u5 \
  owl2bench_cmp_agg_dl_u10 owl2bench_cmp_agg_dl_u20 owl2bench_cmp_agg_dl_u50 \
  owl2bench_cmp_agg_el_u1 owl2bench_cmp_agg_el_u2 owl2bench_cmp_agg_el_u5 \
  owl2bench_cmp_agg_el_u10 owl2bench_cmp_agg_el_u20 owl2bench_cmp_agg_el_u50 \
  owl2bench_cmp_agg_ql_u1 owl2bench_cmp_agg_ql_u2 owl2bench_cmp_agg_ql_u5 \
  owl2bench_cmp_agg_ql_u10 owl2bench_cmp_agg_ql_u20 owl2bench_cmp_agg_ql_u50 \
  owl2bench_cmp_agg_rl_u1 owl2bench_cmp_agg_rl_u2 owl2bench_cmp_agg_rl_u5 \
  owl2bench_cmp_agg_rl_u10 owl2bench_cmp_agg_rl_u20 owl2bench_cmp_agg_rl_u50
```

- Env: `OWL2BENCH_LADDER_PROFILES` (default `DL,EL,QL,RL`),
  `OWL2BENCH_LADDER_SIZES` (default `1,2,5,10,20,50`),
  `OWL2BENCH_LADDER_OUTPUT_DIR` (default a fresh
  `owl2bench_ladder_<timestamp>` in the history dir). Pass the aggregation
  directories by their run-id names so `ladder_metadata.json` records them
  portably.
- Expected artifacts: `ladder_dl.md`, `ladder_el.md`, `ladder_ql.md`,
  `ladder_rl.md`, `ladder_metadata.json` (run ids per point). Every expected
  profile × size point must be present; missing or malformed inputs fail
  closed.
- **Verified here**: rerunning the view over the 24 recorded aggregations
  reproduced `ladder_dl/el/ql/rl.md` and `ladder_metadata.json`
  byte-identically to the exported `ladder_final` artifacts.

### 6c. Campaign layer (optional cross-lane summary)

```bash
W3C_RUN_DIR=<w3c run dir> \
ORE_RUN_DIR=<ore aggregation dir> \
OWL2BENCH_RUN_DIR=<owl2bench stage run dir> \
OWL2BENCH_QUERY_RUN_DIR=<verdict run dir> \
CAMPAIGN_OUTPUT_DIR=<out> \
  benchmarks/campaign/report.sh
```

The campaign conjunction is W3C AND owl2bench-query; the ORE and OWL2Bench
stage suites are informational and can never block or pass the gate (ADR
0024/0027). Fixture test: `bash benchmarks/campaign/test_campaign_report.sh`.
**Verified here**: the report runs over the four published run directories.

## Step 7 — export

```bash
python3 benchmarks/repro/export_bundle.py            # builds benchmarks/repro/export/spacl-public-companion
python3 benchmarks/repro/export_bundle.py --force    # replace a previously generated bundle
python3 benchmarks/repro/export_bundle.py --self-check benchmarks/repro/export/spacl-public-companion
bash benchmarks/repro/test_export_bundle.sh          # fixture test (issue #203)
```

- Env/flags: `--config` (default `benchmarks/repro/export_selection.json`),
  `--repo-root`, `--output`. The export fails closed on vanished selection
  entries, absent run directories or headline artifacts, missing run-identity
  fields, ladder metadata that disagrees with the manifest, claim sentences
  not verbatim in their governing ADRs, and pointers-not-blobs violations
  (forbidden suffixes, size cap).
- Expected artifacts: the bundle tree with `hash_inventory.json` (one sha256
  entry per file), per-lane `evidence/<lane>/README.md` claim headers, and
  the self-check's `[ok] self-check passed: <N> inventoried files verified`.
- **Verified here**: `--self-check` over the current bundle (509 files) and
  the fixture test both pass. Regenerating the bundle itself re-copies the
  current run directories and is routine republication, not a one-off.

## Environment variable reference

| Variable | Used by | Meaning |
|---|---|---|
| `RUN_ID` | all sweep wrappers | run identity; becomes the run directory name (one-shot on ladder points) |
| `REASONERS_OVERRIDE` | `run_benchmarks.sh` via wrappers | comma list of engines for the pass (track engine sets above) |
| `TIMEOUT_SECONDS` | `run_benchmarks.sh`, `ore/run.sh`, `ladder_run.sh`, W3C/verdict wrappers | per-case timeout; 300 for comparative lanes (ADR 0025), 60 for the W3C lane, 3600 for the verdict |
| `SKIP_BUILD` | `run_benchmarks.sh` via wrappers | `1` = use images from step 4; `0` = rebuild inline |
| `OWL2_REASONER_MAX_FILE_SIZE` | forwarded into the Tableauxx container | parser max-file-size cap in bytes (large ORE ontologies) |
| `OWL2_REASONER_LARGE_PARSE` | forwarded into the Tableauxx container | lifts the cap with arena allocation |
| `ORE2015_AUTO_DOWNLOAD` / `ORE2015_GROUPS` / `ORE2015_STAGED_DIR` / `ORE2015_DRY_RUN` | `ore/prepare.sh`, `ore/run.sh` | corpus fetch, groups (`pure_dl,el`), staging dir, plan-only mode |
| `W3C_OWL2_SOURCE_DIR` / `W3C_OWL2_STAGED_DIR` / `W3C_OWL2_REASONER_BIN` | W3C wrapper | manifest source, staging, reasoner binary |
| `OWL2BENCH_LADDER_MANIFEST` / `OWL2BENCH_LADDER_TIMEOUT_SECONDS` / `OWL2BENCH_LADDER_DRY_RUN` | `ladder_dataset.py`, `ladder_run.sh` | ladder manifest path, per-generator-run budget, plan-only mode |
| `SPACL_VERDICT_ORACLE_DIR` / `SPACL_VERDICT_TIMEOUT_SECONDS` / `SPACL_VERDICT_RUN_ID` | `query/spacl_stack_verdict.sh` | oracle pin, per-step timeout, verdict run id |
| `AGGREGATION_OUTPUT_DIR` / `OWL2BENCH_LADDER_OUTPUT_DIR` | aggregation / ladder view | output directories for step 6 |

The full set of reasoner flags the harness forwards into the Tableauxx
container is listed in `FORWARDED_OWL_ENV_VARS` inside
`benchmarks/competitors/scripts/run_benchmarks.sh`.

## Reproduction status of this guide

Executed while drafting (2026-10-13): corpus re-staging and pin checks
(1a/1b/1c), ladder manifest validation (2), comparative aggregation and
ladder rendering over the recorded run directories (6a/6b), the campaign
report (6c), the dry forms of `ore/run.sh` and `ladder_run.sh` (5c/5d), and
the export self-check plus fixture tests (7). Documented, not executed:
engine image builds (4), the full W3C lane (5a), the verdict rerun (5b), the
sweep passes (5c/5d), and dataset/oracle regeneration (2/3) — each step above
names its committed entry point and its landing evidence in the exported
bundle.
