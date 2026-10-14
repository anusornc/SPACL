# Three-lane W3C full-corpus correctness gate

Status: accepted

The full approved W3C corpus remains the destination, but its operation rows
are evaluated through three separate correctness lanes: OWL 2 Direct
Semantics/RDF/XML, Functional-Style Syntax, and RDF-Based Semantics/OWL 2
Full. Each row belongs to exactly one lane according to official metadata.
Each lane report records its own source hash, semantics-specific oracle hash,
and status counts; a lane passes only with zero wrong, failed, and unsupported
rows, while timeout or unavailable oracle evidence blocks the lane. Rows
outside a lane are reported as not applicable to that lane, never as passed.
The campaign completion gate is the conjunction of all three lane gates, so
research may proceed independently but no cross-suite correctness claim is
made until every lane passes.

## Considered Options

- A single mixed W3C gate was rejected because Direct and RDF-Based Semantics
  have different model-theoretic contracts.
- Limiting the destination to the Direct Semantics/RDF/XML subset was rejected
  because it would abandon the full approved corpus requirement.

## Consequences

The Direct Semantics/RDF/XML lane is implemented and verified first, followed
by the Functional-Style Syntax lane and then the RDF-Based/FULL lane. Aggregate
reports may show lane totals, but they must not interpret mixed-semantic
status counts as one correctness result.
