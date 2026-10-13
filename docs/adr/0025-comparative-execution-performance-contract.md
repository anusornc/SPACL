# Comparative execution performance contract for ORE 2015

Status: accepted

The comparative performance leg on the ORE 2015 consistency sample runs a
pinned engine set against the staged 604-case corpus on one recorded machine:
DL track = Tableauxx, Konclude, HermiT; EL track = those three plus ELK (ELK
runs EL only, matching the 2015 track design — an EL reasoner on DL input is
not a meaningful workload). Each pass executes every case serially (no
parallelism, so no engine competes for CPU) with a uniform 300-second
timeout per case, and the campaign performs three passes as separate run
identifiers. The primary timing statistic is the per-case median
container-inclusive wall time across the three passes; engine-reported times
stay secondary diagnostics. Every performance claim names its metric
(median wall time including container startup), the hardware fingerprint
recorded in run metadata, and the pinned engine identities: docker image
digests and engine versions are recorded per run, and the Tableauxx image is
rebuilt from the code under test before pinning.

Reports: the run directory holds a paper-table comparison (per-case median
wall time per engine, best bolded, TO/NA/FAIL legend) plus a per-engine
completability and timing summary; the ORE report's differential appendix
receives its first content as an observational engine-output comparison
(which engine returned what), explicitly adjudicating no correctness. Run
directories stay in gitignored history; only metadata, hashes, and
references enter the repository. Publication-ready packaging is deferred to
the reproduction leg.

Sequencing: the ORE comparative matrix runs first; the OWL2Bench multi-size
sub-leg is specced after it lands, with the existing seed1 one-university
anchor as its first scalability point and the larger sizes generated then.

## Considered Options

- A single pass was rejected because a publication-facing timing claim must
  survive questions about run-to-run variance.
- Parallel or sharded execution was rejected because contention pollutes
  wall time; serial passes are slower but comparable.
- Engine-reported time as the primary metric was rejected because it is not
  comparable across engines (JVM-internal timers versus native process
  internals); container-inclusive wall time is uniform and honest about
  end-to-end latency, with startup cost disclosed in the claim wording.
- The full seven-engine harness set was rejected: the adopted destination
  names Konclude, HermiT, and ELK; extra engines can be added later as
  additional rows without changing the contract.
- Running ELK on the DL track was rejected because it measures a workload
  the EL reasoner is not designed for and the 2015 competition never ran.

## Consequences

- The benchmark harness records image digests and engine versions in run
  metadata (previously `latest` tags, unpinned).
- A comparative aggregation step computes per-case medians across the three
  passes and renders the paper table and per-engine summary.
- The ORE report differential appendix moves from contract-only to first
  content; it still holds no gate authority.
- The first pass doubles as the smoke pass: if it exposes input or engine
  wiring problems (for example Konclude preprocessing), they are fixed
  before passes two and three are launched.
- OWL2Bench multi-size performance remains unspecced until this leg lands.
