# Model-based Direct Semantics entailment

Status: accepted

Direct Semantics entailment will be exposed as one public structural operation
over a premise ontology and a conclusion ontology. It returns the W3C
entailment verdict True, False, Unknown, or Error: True requires that every
premise model satisfy every conclusion axiom, while False requires a verified
countermodel. Positive and non-entailment tests use the same operation and
distinguish themselves only through the official expected result. Complex
conclusions are not reduced to named-class subsumption or to merely adding the
conclusion to the premise, because consistency of that union is not an
entailment proof.

The evaluator canonicalizes both documents into structural ontologies,
standardizes anonymous individuals apart, preserves datatype and import
semantics, and treats a multi-axiom conclusion as a conjunction. Unknown,
Error, timeout, and unsupported remain distinct non-passing outcomes. The
operation may reuse sound reductions to the shared satisfiability service only
where their proof obligations are explicit; otherwise it needs a
model-based proof or countermodel path.

## Considered Options

- Extending the existing named-class-subsumption closure was rejected because
  the W3C baseline includes assertions, property axioms, equality/difference,
  restrictions, datatypes, and multi-axiom conclusions.
- Checking only premise-plus-conclusion consistency was rejected because a
  consistent union does not establish entailment.
- Treating Unknown or unsupported as False was rejected because it would turn
  inability to decide into a semantic claim.

## Consequences

The Direct Semantics entailment lane depends on the unified structural model
and the sound model-finding contract. The current 83 baseline rows must be
covered by representative conclusion-shape tests before their correctness
gate can be enabled.
