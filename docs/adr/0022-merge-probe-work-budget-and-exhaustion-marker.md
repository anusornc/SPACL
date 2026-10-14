# Merge-probe work budget with an explicit exhaustion marker

The merge probe's exhaustion verdict stays fail-open exactly as ADR 0020
records it — spending a budget reads as "no refutation found", never a
positive verdict — but it stops being silent: a probe that enters and
spends its work ceiling reports `merge_probe: budget_exhausted`, kept
distinct from the entry size gate's `skipped_size_gate` (#168), and an
absent field means the probe ran to completion. Probe budgets are
denominated in **work units** — one edge inspection inside an
existential-GCI candidate check — with the existing per-phase caps kept
and one global work ceiling added on top; its default is calibrated to
roughly thirty seconds of probe work on uberon-scale corpora (measured
in the landing, not guessed), overridable by
`OWL2_REASONER_MERGE_PROBE_WORK_BUDGET` with `0` meaning unlimited for
D2 comparisons. The #168 size gate (default 20,000 expression nodes)
stays in force during the transition until the role-closure edge index
proves the per-node cost collapse; revisiting its default is recorded
follow-up work with the new numbers, not a precondition.

## Considered Options

- Fail-closed exhaustion (error / undecided verdict) was rejected for
  the same reason ADR 0020 records: it silently changes the meaning of
  already-passing cases, and an exhausted search is incompleteness the
  lane has always accepted, not unsoundness.
- One merged "probe incomplete" marker value was rejected because
  skipping at entry and entering-but-not-finishing are different
  evidence; the transition of D2 moves uberon between them and merged
  reads would be unrecoverable after the fact.
- A wall-clock deadline was rejected because the sweep and stage
  contract need deterministic reproducibility; a counted unit does not
  vary with machine load.
- Counting walk-node visits (today's scheme) was rejected because the
  cost per node shifts when the edge index lands, decoupling the budget
  from the work it claims to bound.

## Consequences

Honest reporting requires the stage harness to project the marker into
the outer stage JSON (folded into sub-landing 1 together with the
marker). Sub-landing 1 must also measure and record whether any current
lane (go-basic, the W3C corpus) has been silently exhausting the walk
budget — the #166 side-question becomes observable for the first time.
The glossary terms (merge probe, size gate, work unit, exhaustion
marker) live in `CONTEXT.md`.

## Amendment (#173, 2026-10-08): calibration target and gate default

Two decisions recorded at design time were revised by the sub-landing-3
measurements, and this ADR records the revision rather than leaving the
code contradicting it:

- **Calibration**: the "~30 s of probe work on uberon-scale corpora"
  target is superseded by the measured correctness floor. The W3C cases
  the probe decides spend up to 2,511,166 work units
  (description-logic-626/627), and a ceiling that binds them flips
  refutations to fail-open — a 250,000-unit draft lost five corpus
  cases before the measurement corrected it. The landed default is
  **4,000,000** (60% above the corpus floor; uberon's
  1,173,140-unit/142.6 s ≈ 8,230 units/s profile fits inside, with the
  walk-node budget remaining the practical time binder there).
- **Gate default**: the #168 size gate rises 20,000 → **1,000,000**
  expression nodes. With the role-closure index (#172) collapsing the
  time wall and the work ceiling bounding search cost deterministically,
  the gate's remaining job is a memory backstop; 1,000,000 sits an
  order of magnitude above the largest suite corpus (uberon: 113,191
  nodes) while still capping pathological inputs. Consequence, accepted
  deliberately: the default doid and uberon lanes now run the probe —
  doid completes clean (~4.8 s reason), uberon completes marked
  `budget_exhausted` (~147 s e2e, under the 156 s Feb-2026 reference)
  where both previously skipped in ~0.3–1 s.
