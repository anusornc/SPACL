# One branch-and-settle search with a fail-open budget for probe choices

Disjunctive commitments in the consistency probe (a node belonging to a
union of classes) are decided by the same search contract that decides
identification choices (#115's oneOf branching): clone the probe,
commit the choice, settle; every branch dying is the refutation
(inconsistent), any surviving branch settles the ontology as
consistent. When the clone budget is exhausted mid-search the result
reads as "no refutation found" — the consistent verdict of the layers
below — never as a positive verdict in either direction.

We deliberately kept the exhaustion semantics fail-open rather than
declining with UnsupportedFeature: flipping it would silently change
the meaning of already-passing identification-search cases, and an
exhausted search is incompleteness of the same class the lane has
always accepted, not unsoundness. Branch selection therefore carries
the soundness-adjacent weight: free unions (no other constraint
presses on their disjuncts) must never be branched, or a bounded budget
dies on combinatorial noise (W3C description-logic-040 carries 32 free
unions alongside the one pressed choice that refutes the ontology).
