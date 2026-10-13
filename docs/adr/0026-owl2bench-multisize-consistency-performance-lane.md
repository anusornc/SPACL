# OWL2Bench multi-size consistency sweep as a performance lane

Status: accepted

The OWL2Bench multi-size leg is a consistency scalability sweep: the pinned
official generator (kracr/owl2bench `5a27536c…`, jar sha256 recorded in ADR
0015/0017, seed 1, shipped generator-native track) produces the official low
size ladder — universities 1, 2, 5, 10, 20, 50 — for each profile DL, EL, QL,
RL. The seed1 u1 correctness anchors are the first scalability point; the
larger sizes are generated with the same double-run byte-determinism check
the anchors used. Each dataset is executed under the full ADR 0025 contract
(three serial passes, uniform 300-second timeout, per-case repetition median
of container-inclusive wall time, engine image digests pinned, Tableauxx
image from the code under test). Engine sets per dataset: DL, QL, and RL
datasets run Tableauxx, Konclude, HermiT; the EL dataset adds ELK (ELK stays
EL-only per ADR 0025).

The sweep is a performance lane under the ADR 0024 terms: execution
performance and completability claims only, no correctness claims — no
oracle exists for OWL2Bench consistency. The query lane remains the
correctness face of OWL2Bench (its gate passed via the accepted SPACL
verdict). Query/materialization scalability of the SPACL stack is deferred
to a later leg.

## Considered Options

- The full official ladder (up to universities 200, ≈10M axioms) was
  rejected: dataset generation takes hours per point and engines would time
  out wholesale, producing mostly timeout rows.
- Query/materialization scalability first was deferred: it measures the
  project's pipeline rather than engine-versus-engine execution performance,
  which is what the adopted destination names.
- The patched-namespace track was rejected for this sweep: it exists so the
  TBox classifies the ABox for query correctness; the generator-native
  shipped datasets are the native consistency workload.
- Waiting for the stage-suite gate was rejected: it is blocked by the same
  oracle-less structure ORE had (ADR 0024), and per-suite scoping makes the
  sweep possible without it.

## Consequences

- Two follow-ups are recorded (separate from this leg): (1) the OWL2Bench
  stage-suite gate inside the campaign conjunction has the ADR 0024 oracle-
  less deadlock shape and needs its own re-typing decision; (2) the passing
  SPACL verdict run does not declare the split stack the campaign's
  owl2bench-query contract requires, and that reconciliation is owed before
  any campaign-level query claim.
- Generated datasets are large and stay in gitignored storage; manifests and
  hashes enter the repository.
- The scalability view is a ladder table per profile: one row per size, one
  column per engine, cells are repetition medians from the ADR 0025
  aggregation.
