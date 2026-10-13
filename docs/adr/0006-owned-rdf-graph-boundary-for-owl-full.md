# Owned RDF graph boundary for OWL 2 Full

Status: accepted

The 157 FULL/RDFXML W3C rows use a dedicated RDF-Based Semantics lane with an
owned RDF graph as its semantic input. The graph preserves every IRI, blank
node, literal, list edge, RDF/RDFS/OWL vocabulary triple, and document/import
boundary; it is not first lowered into the Direct Semantics structural
ontology. A separate RDF-Based evaluator applies RDF, RDFS, D-entailment, and
OWL 2 RDF-Based semantic conditions for consistency and entailment. The
official W3C approved manifest supplies the expected result and provenance.

## Considered Options

- Reusing the Direct structural ontology was rejected because it loses OWL Full
  metamodeling and unrecognized RDF vocabulary triples.
- Implementing only RDFS/OWL rule closure was rejected because the RDF-Based
  Recommendation defines additional model-theoretic conditions for
  restrictions, lists, equality, keys, negative assertions, and OWL
  vocabulary.
- Using a reference reasoner or majority result as an oracle was rejected
  because the W3C approved expected values own correctness.

## Consequences

The RDF-Based lane needs its own graph-preserving parser mode, evaluator,
provenance fields, and regression matrix. Results must remain separate from
Direct and Functional semantic views, and the lane gate cannot pass while any
graph preservation or semantic decision returns unknown, unsupported, timeout,
or unavailable oracle evidence.

## Amendment (2026-10-09): FULL-lane implementation record

The owner's standing directive for this arc was "complete every phase of
the plan," and the six design questions posted with the track's opening
grill were settled by their posted recommendations under that directive
(each is revisitable through this ADR): **staged delivery** toward the
end state above; the RDF-Based evaluator **extends the existing closure
engine** (`reasoner/materialize.rs`'s rule families are a subset of the
RDF-Based conditions — the fixpoint machinery is reused, not rebuilt);
the **owned graph exports from the structural parser's Phase A triple
stream** (it holds the document's complete RDF graph by construction,
before any Direct-semantics lowering); families land **easiest-first**
(prop-vocabulary → eqdis → ndis → char/bool → restrict), each with its
own landing, byte-identical-sweep gate, and formal r-run; the lane is
**separate** in the harness (species=FULL rows route to the RDF-based
evaluator via an explicit evaluate flag) with results kept distinct
from the baseline lane; and the **lane gate's denominator is the
FULL/RDFXML rows that carry a normative premise** — the 21
no-normative-RDF/XML rows stay correctly reported as harness-level
unsupported and are documented as part of the gate's definition.

V0 posture: the RDF-based evaluator answers what its landed rule
families cover and **falls back to the existing Direct evaluation
otherwise**, so routing the lane cannot regress currently-answered
rows. Family 1 (the built-in annotation-property vocabulary typings —
rdfs:label/comment/seeAlso/isDefinedBy and owl:versionInfo/
priorVersion/backwardCompatibleWith/incompatibleWith/deprecated are
owl:AnnotationProperty in every interpretation) rides a fixed builtin
vocabulary triple set merged into the owned graph's closure.
