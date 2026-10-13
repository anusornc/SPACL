# SPACL

SPACL is a tableaux-based OWL 2 reasoner (binary `owl2-reasoner`) and its
correctness-gated benchmarking discipline. This repository is the public
artifact of record for the project's evaluation evidence: everything a
third party needs to verify the published numbers (Tier 1) or regenerate
them from official sources (Tier 2). It is the companion repository cited
by the paper.

## What is here

- `src/`, `Cargo.toml`, `benches/`, `tests/`, `examples/`, `scripts/` — the engine source and its test suite, exactly as evaluated (see `hash_inventory.json` for the source commit binding).
- `benchmarks/` — the benchmark harness: comparative execution infrastructure, engine Dockerfiles, lane runners, the dataset-ladder generator, and the export pipeline itself.
- `docs/` — architecture decision records (ADR 0001-0028), the domain glossary (`CONTEXT.md` is at the root), the reproduction research notes, and the two guides below.
- `evidence/` — the exported results for the four evidence lanes, each with its run identity and claim wording.
- `hash_inventory.json` — the sha256 inventory binding every file in this export to its source.

## Evidence at a glance

Four lanes, each with an explicit claim boundary (the exact sentences are
in each `evidence/<lane>/README.md`):

1. **W3C OWL 2 conformance** — full approved corpus, per-lane evaluation:
   486 passed / 0 wrong / 21 unsupported (r69). Correctness is judged
   against the suite's official expected results (ADR 0002).
2. **OWL2Bench query correctness** — 60/60 query/profile cases with
   oracle matches `accepted` against the independently audited derived
   oracle v2; the lane gate passed (ADR 0027).
3. **ORE 2015 comparative execution performance** — three serial passes
   over the 604-case consistency sample (306 DL / 298 EL): median
   container-inclusive wall time, 300s timeout, engines pinned by digest;
   no correctness claims, no public oracle exists (ADRs 0024/0025).
4. **OWL2Bench scalability ladders** — three serial passes over the
   24-point ladder (universities 1-50, seed 1, four profiles) under the
   same contract; no correctness claims (ADR 0026).

Headline numbers live in `evidence/` tables; their provenance is one
hash-check away.

## Verify (Tier 1)

Follow `docs/repro/VERIFICATION_GUIDE.md`: hash-check this tree against
`hash_inventory.json`, trace every published number to its run identity,
and recompute the aggregations from the exported TSVs.

## Reproduce (Tier 2)

Follow `docs/repro/REPRODUCTION_GUIDE.md`: fetch the pinned official
sources (ORE Zenodo corpus, W3C approved manifest, the OWL2Bench
generator), regenerate the dataset ladder, rebuild the engine images from
the committed Dockerfiles, and rerun the sweeps and the verdict.

## Provenance discipline

No corpora, datasets, or engine binaries are redistributed here. Every
external artifact is named by manifest identity (revision, sha256) and is
fetched from its official source; engines are rebuilt from the committed
Dockerfiles. This is the pointers-not-blobs rule (ADR 0028).

## License

See `LICENSE`.
