# Query namespaces follow the dominant ABox vocabulary

Status: accepted

Derived-oracle query terms written with the empty prefix (`:isMemberOf`)
expand into the dominant ABox vocabulary of the parsed anchor: the most
frequent predicate namespace outside the standard RDF/RDFS/OWL/XSD
vocabularies, falling back to the document `xml:base` namespace when the
dataset asserts no non-standard predicate. The chosen namespace is recorded
in provenance (`query_namespace`, `vocabulary_split`), and the independent
audit recomputes it from the dataset and rejects any disagreement.

The seed-1 RL anchor (ADR 0015) made this load-bearing: the official
generator serializes the TBox under the document `xml:base`
(`http://benchmark/OWL2Bench#`) while every instance typing and property
assertion uses the generator's fixed prefix
(`https://kracr.iiitd.edu.in/OWL2Bench#`). One dataset, two disjoint
vocabularies, no bridging axioms. Resolving query terms from the `xml:base`
matched only the TBox side and certified 0/18 answer rows; resolving from the
assertion side yields 8/18 non-empty cases, and the remaining 10 empties are
correct literal entailments of the staged bytes (their vocabulary never
occurs in the ABox under any namespace), so engines loading the same file
must also answer empty.

Answers remain entailments of the staged bytes exactly as pinned by ADR 0015.

## Considered Options

- Resolving from the document `xml:base` (the e17d06f behavior) was rejected:
  on split-vocabulary generator output it selects the TBox side, which the
  queries never traverse, and silently certifies total emptiness.
- Rewriting the anchor (or the parsed graph) to unify the two vocabularies
  was rejected: it changes the bytes ADR 0015 pins by sha256, and the oracle
  would answer over a graph no engine ever loads, breaking the
  derived-oracle contract that answers are entailments of the staged file.
- Hardcoding the generator prefix was rejected: the namespace must be
  derived from the dataset so the procedure extends to other corpora without
  a per-corpus constant, and the derivation stays reproducible from inputs
  alone.

## Consequences

`dominant_namespace` lives in `rl_derivation_core.py` beside the parser so
derive and audit share one definition; ties break lexicographically to keep
derivation deterministic. On the old staged sample (0004), which is
TBox-dominant with a single vocabulary, the fallback reproduces the previous
audited behavior unchanged. For split-vocabulary datasets the proof artifact
carries the flag, and case-level emptiness must be read against
`vocabulary_split` before calling an empty oracle vacuous.
