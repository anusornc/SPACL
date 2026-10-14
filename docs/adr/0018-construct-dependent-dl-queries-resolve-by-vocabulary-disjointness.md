# Construct-dependent DL queries resolve by vocabulary disjointness

Status: accepted

The derived-oracle procedure pinned a decision point for the three
construct-dependent queries of the OWL2Bench matrix — 006 (`:SelfAwarePerson`,
ObjectHasSelf), 017 (`:UGStudent`, exact qualified cardinality), 018
(`:PeopleWithManyHobbies`, min qualified cardinality): they must either gain
implemented derivation semantics or the case fails closed with the construct
recorded in the uncovered-axiom inventory; never answered from an engine.

This ADR records a third, verified resolution for the seed-1 DL anchor (0006,
ADR 0017) and the EL anchor's 006: **neither branch is needed, because the
constructs cannot influence the answers.** The anchors define all three
classes in the benchmark TBox vocabulary (`http://benchmark/OWL2Bench#…`)
via `owl:hasSelf` / `owl:qualifiedCardinality` /
`owl:minQualifiedCardinality`, while the queries expand their terms into the
dominant ABox vocabulary (ADR 0016, `https://kracr.iiitd.edu.in/OWL2Bench#…`).
Occurrence-level verification of the anchor triples shows the queried IRIs
occur in zero triples — as subject, predicate, or object — so under OWL 2 DL
semantics no type assertion for them is entailed; the exact answer for each
case is the empty set, which the derivation produces under its rule table
while the constructs stay in the uncovered-axiom inventory. The same
verification covers every other class query in the EL/DL matrices (all
queried kracr class IRIs: zero occurrences) and every property query
(queried kracr property IRIs occur only as predicates of ABox assertions and
in no axiom, so no rule can derive new triples with those predicates), which
makes the derived answers exact for the whole matrix rather than merely
sound-for-the-table.

## Decision

- The construct-dependent cases are derived, not fail-closed: their
  empty answers are exact, and the hasSelf/cardinality axioms remain
  uncovered (never reasoned).
- The procedure doc's DL section records the occurrence-level verification
  numbers and the revisit condition: if a future anchor defines a queried
  class inside the query vocabulary, the original either/or decision point
  reactivates for that case (implement the construct semantics or fail
  closed).
- No engine output participates in any of this; acceptance still runs the
  neutral per-case audit (ADR 0013) as for RL and QL.

## Consequences

The DL profile gate can be decided by real per-case verdicts for all 22
cases instead of carrying three permanently fail-closed cases. The decision
is anchor-scoped, not a general license: it holds exactly as long as the
vocabulary-disjointness occurrence check holds for the anchor in use.
