# Unified structural OWL expression model

Status: accepted

The structural ontology model will use one canonical representation for OWL
class expressions. Object and data cardinality restrictions carry an optional
class-expression or data-range qualifier, while property domain and range
constraints carry a ClassExpression value rather than only an IRI. This
preserves the semantics required by the W3C RDF/XML mapping and prevents
parser, reasoner, and entailment code from maintaining parallel
representations.

This is an intentional public API breaking change. Existing internal callers,
tests, and documentation must migrate to the canonical model, with convenience
constructors retained where they do not create a second semantic form. Binary
serialization receives a new format version; deserializers continue reading
the existing versions, but old versions must not silently encode or decode
new semantics. Acceptance tests use the public ontology-loading seam, the
evaluator contract, and binary round-trips.

## Considered Options

- Adding qualified-only variants beside the current unqualified variants was
  rejected because it duplicates parser and reasoner branches.
- Keeping qualifiers in an opaque side channel was rejected because the
  structural ontology would no longer be the complete semantic model.
- Reusing an old binary version was rejected because serde-compatible bytes
  could still carry an incompatible meaning.

## Consequences

The model migration must update exhaustive expression consumers and binary
format handling before downstream W3C satisfiability and entailment decisions
can be implemented. The compatibility boundary is explicit and auditable,
while unqualified OWL restrictions remain convenient through constructors and
normal accessors.
