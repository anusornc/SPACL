# ORE 2015 as a performance-only lane

Status: accepted

The ORE 2015 consistency sample (306 pure_dl + 298 el cases) is re-typed from
a correctness lane to a performance lane. Its published claims are execution
performance and completability only: per-engine times, timeouts, and
completed-run counts on the pinned corpus. It is removed from the campaign
correctness conjunction, which becomes W3C AND OWL2Bench; the ORE suite is
reported as informational and can never block or pass the campaign gate. ORE
rows keep `oracle_unavailable` as their permanent, honest correctness status.

Why: no public answer key exists for the sample (recovered-source audits in
docs/research, issue #48); the competition's expected results are an internal
artifact of an event this project did not compete in; the competition's own
scoring used majority voting with random tie-breaks, which ADR 0007 forbids
treating as gold; and a conjunctive gate over an unreachable oracle would
block campaign completion and the adopted comparative-performance destination
leg forever. Owner decision (2026-10-10 grill): we did not compete, so we
have no claim on the internal key.

ADR 0001's performance-after-correctness rule is scoped per suite rather
than carved with an exception: a correctness lane may publish performance
only after its own correctness gate passes; a declared performance lane may
publish execution performance and must make no correctness claims. The ORE
lane report becomes performance-first (its header states that the lane
adjudicates no correctness; correctness columns stay, every row
oracle_unavailable). Engine-output comparisons from comparative runs appear
only as a clearly labelled differential appendix: agreement is observational
evidence, attaches to no gate, and is never reported as correctness.

## Not taken (recorded so they are not re-derived)

- Flag-anchored reconstruction of the 2015 expected values. The published
  per-reasoner tables (23 TSVs, hash-verified against the 20260901 source
  manifest) flag, per case, which engines matched the hidden expected result.
  Re-running pinned corroborating binaries would cover 603/604 cases at two
  or more corroborators (577/604 within a modern-runnable core of
  Konclude/HermiT/JFact/FaCT++/ELK/TrOWL/jcel/Chainsaw);
  `ore_ont_7192.owl` has zero corroborators because every reasoner timed out
  in 2015. Rule sketch: two or more agreeing corroborators yield a
  reconstructed value, anything else fails closed. Not adopted: correctness
  ambition on ORE is not required by the destination, and the build cost is
  disproportionate now.
- Organizer contact for the original expectation archive: not pursued. The
  archive is competition-internal and this project did not compete. It can be
  reopened before publication without changing this ADR, mirroring the
  recorded option in ADR 0013.
- ADR 0013-style proof-backed derived oracle for consistency: rejected for
  now. Coverage is asymmetric (inconsistency has checkable certificates,
  consistency lacks general finite model witnesses) and the independent
  checker build is disproportionate to the claim it would support.

## Considered Options

- Keeping ORE in the correctness conjunction with permanent
  `oracle_unavailable` was rejected because it permanently blocks campaign
  completion and the adopted destination sequence (comparative performance,
  then publishable reproduction).
- Promoting the competition's majority-vote pseudo-gold to an oracle was
  rejected by ADR 0007; the ORE 2015 report itself calls the mechanism
  unsatisfactory.
- Deleting the ORE lane entirely was rejected because it is the project's
  only real-world corpus and the comparative-performance leg of the
  destination runs on it.

## Consequences

- `campaign_report.py`: the ORE suite becomes informational; the campaign
  correctness conjunction is W3C AND OWL2Bench; the ORE report and its
  tests are reshaped performance-first.
- `wrong` and `passed` are unreachable on this lane by design;
  `oracle_unavailable` is the standing record, not a defect to fix.
- Comparative performance on ORE may proceed without correctness-gate
  precedence; performance claims for other suites still wait on their own
  correctness gates.
- Publication wording is pinned: execution performance on the ORE 2015
  consistency sample (306 DL / 298 EL), with an explicit no-correctness
  note because no public oracle exists.
