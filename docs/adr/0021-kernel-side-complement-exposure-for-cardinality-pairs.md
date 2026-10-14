# Kernel-side complement exposure for cardinality complement pairs

The W3C oiled family encodes class complements as cardinality
complement pairs over one property — X ≡ (p at most 0 values) with
X.comp ≡ (p at least 1 value) jointly entailing X.comp ≡ ¬X — and the
kernel consumed only the disjointness half (the instance-level clash),
leaving every contrapositive proof through the covering half
undecided (W3C description-logic-601 answered consistent). We decided
to expose the relation kernel-side: registered when the consistency
lane builds its indexes, consumed through the #142 contrapositive/
negation readers plus a scoped normal-form assist for the closure this
shape needs — one landing, exposure and consumption together.

Rejected alternatives: parser-side lowering of pairs into explicit
disjointness/covering axioms (changes every consumer lane's view and
forces the parser A/B discipline for a consistency-lane-only need);
materializing covering GCIs (⊤ ⊑ X ⊔ X.comp branching per individual
re-invites the free-union explosion ADR 0020 guards against); a pure
special-cased emptiness check without index registration (starves the
compositional readers of the relation). Scope guard: named classes,
one shared property, the 0-vs-≥1 polarity only — the generator's
entire corpus vocabulary; anything broader waits for a case that
needs it.
