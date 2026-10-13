# Verification guide (Tier 1): checking the published evidence bundle

Date: 2026-10-13. Status: active (issue [#204](https://github.com/anusornc/tableauxx/issues/204),
spec [#202](https://github.com/anusornc/tableauxx/issues/202), [ADR 0028](../adr/0028-publishable-reproduction-via-spacl-public-repo.md)).

## Audience and scope

You hold the exported bundle — the content tree of the public companion
repository assembled by `benchmarks/repro/export_bundle.py` from the selection
manifest `benchmarks/repro/export_selection.json`. This guide walks you through
Tier 1 verification per ADR 0028: checking every published number's hash
provenance, lane by lane, with commands you run yourself. All commands below
run from the bundle root and need only `python3`, `jq`, `sha256sum`, `awk`,
`sort`, and `grep`.

Tier 1 verifies identity and internal consistency. It does not re-execute
anything: a matching sha256 proves the published artifact is the recorded
artifact, not that the run was performed correctly. Rerunning the evidence from
official pins is Tier 2 and is covered by the
[reproduction guide](REPRODUCTION_GUIDE.md).

Redistribution boundary, verbatim from ADR 0028:

> Redistribution is pointers, not blobs: no engine binaries, no corpora, and no
> generated datasets are redistributed; manifests record identities and hashes,
> sources are fetched from their official pins, and engine images are rebuilt
> from committed Dockerfiles.
>
> — `docs/adr/0028-publishable-reproduction-via-spacl-public-repo.md`

Consequently the bundle contains no corpora, no generated datasets, and no
engine binaries to inspect; verification works from manifests, hashes, and the
exported tables themselves.

## Bundle anatomy

- `hash_inventory.json` — the manifest that travels. One entry per file:
  `path`, `sha256`, `source_kind` (`tracked_selection` | `run_dir` |
  `generated`), `source_path` (the run directory or tracked file it was copied
  from), and `run_ids`. Also records `source_commit` (the monorepo commit the
  bundle was exported from), `generated_at`, per-kind counts, and the four
  lane records with their verbatim claim sentences and run ids.
- `evidence/<lane>/` — the four evidence lanes:
  `w3c-conformance/`, `owl2bench-query-verdict/`, `ore-comparative/`,
  `owl2bench-ladder/`. Each carries a generated `README.md` header with the
  lane's run identity table and its verbatim ADR claim sentences.
- Everything else (`src/`, `benchmarks/`, `docs/adr/`, …) is the tracked
  repository selection, copied byte-identically and inventoried like the
  evidence.

## Step 0 — whole-bundle integrity

The committed export script re-hashes every file against the inventory,
verifies completeness (no unlisted files), checks each lane README carries its
verbatim claim sentences, and enforces the pointers-not-blobs guards:

```bash
cd <bundle-root>
python3 benchmarks/repro/export_bundle.py --self-check .
```

Expected output ends with a line of the form
`[ok] self-check passed: <N> inventoried files verified, <N> files on disk, counts={...}`.

If you prefer not to trust the script, check the hashes yourself with
stock tools only:

```bash
jq -r '.entries[] | "\(.sha256)  \(.path)"' hash_inventory.json | sha256sum -c -
```

`sha256sum -c` reads `sha256  path` lines and prints `: OK` per file; any
mismatch or missing file prints `FAILED` and a nonzero exit.

Confirm the export identity recorded in the inventory:

```bash
jq '{source_commit, generated_at, counts, lanes: [.lanes[].name]}' hash_inventory.json
```

The `source_commit` is the monorepo commit the tracked tree (engine source
included) was copied from; every `tracked_selection` entry's `source_path` is
its path in that commit.

## Step 1 — W3C OWL 2 conformance lane (`evidence/w3c-conformance/`)

### What this lane claims

Verbatim from ADR 0002:

> Each lane report records its own source hash, semantics-specific oracle hash,
> and status counts; a lane passes only with zero wrong, failed, and
> unsupported rows, while timeout or unavailable oracle evidence blocks the
> lane.
>
> — `docs/adr/0002-three-lane-w3c-full-corpus-gate.md`

This is a correctness lane: the W3C's own expected values are the oracle, and
the claim is about status counts under that oracle.

### What this lane does NOT claim

- It does not claim performance. No timing number in this lane is a claim.
- Per ADR 0002: "Rows outside a lane are reported as not applicable to that
  lane, never as passed." The `unsupported` rows are lane-filtered cases
  without runnable normative RDF/XML premises, recorded with reasons — not
  silently counted as passes.

### Artifacts and hash check

`summary.json` and `report.md`, exported from run directory
`benchmarks/external/w3c-owl2/results/history/w3c_owl2_approved_full_20261009_r69`
(run id `w3c_owl2_approved_full_20261009_r69`).

```bash
for f in evidence/w3c-conformance/summary.json evidence/w3c-conformance/report.md; do
  jq -r --arg p "$f" '.entries[] | select(.path == $p) | "\(.sha256)  \(.path)"' hash_inventory.json \
    | sha256sum -c -
done
```

Check that both entries carry the run id and the recorded source run directory:

```bash
jq -r '.entries[] | select(.path | startswith("evidence/w3c-conformance/")) | "\(.path)\t\(.source_path)\t\(.run_ids | join(","))"' hash_inventory.json
```

### Number cross-checks

Read the headline counts and confirm they are internally consistent (the
status counts sum to the total, and `report.md` carries the same numbers as
`summary.json`):

```bash
jq '{total: .total_results, statuses: .result_status_counts, sum: ([.result_status_counts[]] | add)}' \
  evidence/w3c-conformance/summary.json
grep -A4 '^## Result Status' evidence/w3c-conformance/report.md
```

`sum` must equal `total`; the report's bullet counts must equal the JSON's.
The operation counts (`consistency`/`entailment`/`inconsistency`) and the W3C
expected-value counts (`true`/`false`) are checked the same way. Zero `wrong`
and zero `failed` rows is the lane's gate condition under the sentence quoted
above.

### How run identity traces to provenance

`summary.json`'s `run_dir` basename, the lane README's run identity table, and
the inventory's `run_ids` must all name the same run id. The run's own
`run_metadata.json` (which records the manifest, source, reasoner binary, and
timeout) stays in the monorepo's gitignored run history; the bundle carries
the identity pointer, per ADR 0028's pointers-not-blobs rule. Regenerating the
run is Tier 2 (reproduction guide, step 1 and step 5).

## Step 2 — OWL2Bench query correctness verdict lane (`evidence/owl2bench-query-verdict/`)

### What this lane claims

Verbatim from ADR 0027:

> The accepted SPACL query verdict (lane gate passed, 60/60 oracle matches
> accepted)
>
> — `docs/adr/0027-stage-suite-retyping-and-spacl-split-stack-reconciliation.md`

This is a correctness lane. The oracle is the accepted derived oracle
(justification-backed per ADR 0013), and the claim is the split-stack verdict:
all four profile gates passed with every oracle match accepted.

### What this lane does NOT claim

- It does not claim query performance or scalability; those are deferred
  lanes. The `performance_claims_allowed` field is a campaign-level permission
  flag, not a published performance result.
- The verdict claim is scoped to the split stack (per-profile SPACL adapters
  over the pinned query backend) on the seed-1 university-1 anchor datasets.
  It is not a claim about other stacks, datasets, or seeds.

### Artifacts and hash check

`summary.json` and `report.md`, exported from run directory
`benchmarks/external/owl2bench/verdict-stack-shipped-seed1/results/spacl-stack-shipped-verdict-seed1-split`
(run id `spacl-stack-shipped-verdict-seed1-split`).

```bash
for f in evidence/owl2bench-query-verdict/summary.json evidence/owl2bench-query-verdict/report.md; do
  jq -r --arg p "$f" '.entries[] | select(.path == $p) | "\(.sha256)  \(.path)"' hash_inventory.json \
    | sha256sum -c -
done
jq -r '.entries[] | select(.path | startswith("evidence/owl2bench-query-verdict/")) | "\(.path)\t\(.run_ids | join(","))"' hash_inventory.json
```

### Number cross-checks

Re-derive the gate from the summary the way the verdict wrapper does:

```bash
jq '{
  query_count, split_stack_requested, query_gate, correctness_gate,
  profile_gates, result_status_counts,
  component_status_counts, materialization_status_counts
}' evidence/owl2bench-query-verdict/summary.json
```

The accepted-verdict shape is: `query_count == 60`, `split_stack_requested ==
true`, every profile gate in `profile_gates` (DL, EL, QL, RL) `passed`,
`query_gate == "passed"`, `correctness_gate == "passed"`,
`result_status_counts == {"passed": 60}`. The report's conjunctive profile
table (per-profile rows and required counts) must agree with the summary's
profile gates; the four profile row counts sum to 60.

### How run identity traces to provenance

The oracle identity the verdict matched against is pinned in the query lane's
committed oracle manifest (canonicalizer version, dataset sha256 anchors per
profile, source corpus and revision); the dataset anchors are the same pinned
sha256 values as the ladder's one-university points (step 4). The verdict
wrapper that enforces all of this on rerun is
`benchmarks/external/owl2bench/query/spacl_stack_verdict.sh` (in the bundle's
tracked tree); rerunning it is Tier 2.

## Step 3 — ORE 2015 comparative lane (`evidence/ore-comparative/`)

### What this lane claims

Verbatim from ADR 0024 and ADR 0025:

> Its published claims are execution performance and completability only:
> per-engine times, timeouts, and completed-run counts on the pinned corpus.
>
> — `docs/adr/0024-ore-2015-performance-only-lane.md`

> The primary timing statistic is the per-case median container-inclusive wall
> time across the three passes; engine-reported times stay secondary
> diagnostics.
>
> — `docs/adr/0025-comparative-execution-performance-contract.md`

### What this lane does NOT claim

Verbatim from ADR 0024:

> ORE rows keep `oracle_unavailable` as their permanent, honest correctness
> status.
>
> — `docs/adr/0024-ore-2015-performance-only-lane.md`

> Engine-output comparisons from comparative runs appear only as a clearly
> labelled differential appendix: agreement is observational evidence, attaches
> to no gate, and is never reported as correctness.
>
> — `docs/adr/0024-ore-2015-performance-only-lane.md`

No table in this lane says any engine answered correctly or incorrectly.
Faster is not "better at ORE"; it is a wall-time measurement only. The corpus
is the pinned ORE 2015 consistency sample (604 cases: 306 pure_dl, 298 el;
see step 1 of the reproduction guide for the pin).

### Artifacts and hash check

Per track (`pure_dl/`, `el/`): `aggregation_metadata.json`,
`comparative_paper_table.md`, `comparative_engine_summary.md`,
`comparative_engine_summary.tsv`, `comparative_median_times.tsv`.

```bash
jq -r '.entries[] | select(.path | startswith("evidence/ore-comparative/")) | "\(.sha256)  \(.path)"' \
  hash_inventory.json | sha256sum -c -
```

Run identity: each track's `aggregation_metadata.json` names its three pass
run ids; the paper table's "Pass runs" line must repeat them, and the
inventory's `run_ids` for every artifact in that track must be the aggregation
run id plus exactly those three pass ids:

```bash
jq -r '.pass_run_ids' evidence/ore-comparative/pure_dl/aggregation_metadata.json
head -5 evidence/ore-comparative/pure_dl/comparative_paper_table.md
jq -r '.entries[] | select(.path == "evidence/ore-comparative/pure_dl/comparative_paper_table.md") | .run_ids' hash_inventory.json
```

### Number cross-checks

Everything in the rendered tables is recomputable from
`comparative_median_times.tsv`, which is in the bundle. Recompute an engine's
completability counts, median, and total, and compare against
`comparative_engine_summary.tsv`:

```bash
cd evidence/ore-comparative/pure_dl
awk -F'\t' 'NR>1 && $3=="tableauxx" && $7!="" {print $7}' comparative_median_times.tsv \
  | sort -n \
  | awk '{v[NR]=$1; s+=$1} END {if(NR%2==1) m=v[(NR+1)/2]; else m=(v[NR/2]+v[NR/2+1])/2;
        printf "n=%d median=%s total=%.1f\n", NR, m, s}'
awk -F'\t' 'NR==1 || $1=="tableauxx"' comparative_engine_summary.tsv
cd -
```

(The `n`, `median`, and `total` must equal the engine summary's `completed`,
`median_wall_time_ms`, and `total_wall_time_ms` columns; `cases` is the count
of all rows for the engine including non-completed ones.) Pick any paper-table
row and confirm the bolded cell is the minimum median among engines that
completed that case, read from the TSV:

```bash
cd evidence/ore-comparative/pure_dl
CASE=pure_dl_0001_pool_sample_files_ore_ont_12901.owl
awk -F'\t' -v c="$CASE" 'NR>1 && $2==c && $7!="" {print $3, $7}' comparative_median_times.tsv | sort -k2 -n
grep "$CASE" comparative_paper_table.md
cd -
```

The engine sets differ by track per ADR 0025 (EL adds ELK; ELK runs EL only) —
visible as the column sets of the two paper tables.

### How run identity traces to provenance

Each pass run id is a separate serial execution of the whole track; the
aggregation run id holds the per-case medians across the three passes. The
per-pass run directories (which record docker image digests and engine
versions per engine in their `run_metadata.json`) stay in the monorepo's
gitignored history; the bundle carries the aggregation artifacts and the pass
identities. Regenerating passes is Tier 2 (reproduction guide, step 5).

## Step 4 — OWL2Bench scalability ladder lane (`evidence/owl2bench-ladder/`)

### What this lane claims

Verbatim from ADR 0026:

> The sweep is a performance lane under the ADR 0024 terms: execution
> performance and completability claims only, no correctness claims — no
> oracle exists for OWL2Bench consistency.
>
> — `docs/adr/0026-owl2bench-multisize-consistency-performance-lane.md`

### What this lane does NOT claim

- No correctness claim of any kind: no oracle exists for OWL2Bench
  consistency, and none of these tables says an engine answered correctly.
- The claim is scoped to the pinned generator (kracr/owl2bench at the pinned
  revision and jar sha256), seed 1, the shipped generator-native datasets, and
  the ADR 0025 engine sets per profile. It is not a claim about other sizes,
  seeds, or datasets.

### Artifacts and hash check

`ladder_final/` holds `ladder_metadata.json` and one ladder table per profile
(`ladder_dl.md`, `ladder_el.md`, `ladder_ql.md`, `ladder_rl.md`). `agg/` holds
one comparative aggregation per ladder point (`<profile>_u<universities>` for
universities 1, 2, 5, 10, 20, 50 across DL, EL, QL, RL — 24 points).

```bash
jq -r '.entries[] | select(.path | startswith("evidence/owl2bench-ladder/")) | "\(.sha256)  \(.path)"' \
  hash_inventory.json | sha256sum -c -
```

Structural cross-checks that need only the bundle:

1. Every ladder point the metadata references exists in the bundle, and every
   aggregation directory the bundle carries is referenced by the metadata:

```bash
comm -3 \
  <(jq -r '.profiles[] .points[] .aggregation_dir' evidence/owl2bench-ladder/ladder_final/ladder_metadata.json | sed 's/^owl2bench_cmp_agg_//' | sort) \
  <(ls evidence/owl2bench-ladder/agg | sort)
```

Empty output means the two sides agree. (The export itself fails closed on any
disagreement; this is your independent confirmation.)

2. Each ladder point's per-pass run ids are consistent between the ladder
   metadata's aggregation reference and that aggregation's own
   `aggregation_metadata.json` (three passes named
   `owl2bench_cmp_p{1,2,3}_<profile>_u<universities>`):

```bash
jq -r '.pass_run_ids' evidence/owl2bench-ladder/agg/dl_u5/aggregation_metadata.json
```

3. Ladder table cells trace to the per-point engine summaries: for any cell
   `<median>ms (completed/cases)`, the values appear in that point's
   `comparative_engine_summary.tsv`:

```bash
P=evidence/owl2bench-ladder/agg/dl_u5
awk -F'\t' 'NR==1 || $1=="hermit" || $1=="konclude" || $1=="tableauxx"' $P/comparative_engine_summary.tsv
grep '^| 5 ' evidence/owl2bench-ladder/ladder_final/ladder_dl.md
```

### How run identity traces to provenance

The dataset identity for every ladder point — generator revision, jar sha256,
invocation, per-point output sha256, and the double-run byte-determinism
record — lives in the committed ladder manifest
`benchmarks/ontologies/external/owl2bench/scalability/ladder-manifest.json`
(in the bundle's tracked tree). Its one-university points are the same pinned
dataset anchors the query verdict matched against (step 2). Validate the
manifest's shape from inside the bundle:

```bash
python3 benchmarks/external/owl2bench/ladder_dataset.py validate \
  --manifest benchmarks/ontologies/external/owl2bench/scalability/ladder-manifest.json \
  --shape-only
```

Expected: `"status": "complete"`, `"point_count": 24`, `"errors": []`.

## Claim boundaries at a glance

| Lane | Kind | Governing ADR | Claims | Never claims |
|---|---|---|---|---|
| W3C conformance | correctness | 0002 | status counts under the W3C oracle; lane gate with zero wrong/failed/unsupported | performance |
| OWL2Bench query verdict | correctness | 0027 | 60/60 oracle matches accepted; profile and query gates passed | query performance/scalability |
| ORE comparative | performance | 0024, 0025 | per-engine times, timeouts, completed-run counts; per-case median container-inclusive wall time | any correctness; `oracle_unavailable` is permanent |
| OWL2Bench ladder | performance | 0026 (under 0024 terms) | execution performance and completability across the ladder | any correctness — no oracle exists |

The lane README headers inside `evidence/<lane>/README.md` carry these
sentences verbatim, and the bundle self-check fails if any header or bundled
ADR copy drifts from them. Performance lanes never gain correctness wording in
export (ADR 0028).

## What Tier 1 verification does not establish

- It does not re-execute any engine or oracle: hashes bind published numbers
  to recorded artifacts, and the cross-checks above bind rendered tables to
  their TSV sources inside the bundle.
- It does not vouch for the monorepo's gitignored run history beyond the
  identities recorded here; that is what Tier 2 regeneration is for.
- The campaign-level conjunction (W3C AND owl2bench-query, with ORE and the
  OWL2Bench stage suite informational per ADR 0024/0027) is computed by
  `benchmarks/campaign/report.sh` from lane run directories; its fixture test
  is `benchmarks/campaign/test_campaign_report.sh`. The exported bundle
  publishes lane evidence, not the campaign report.
