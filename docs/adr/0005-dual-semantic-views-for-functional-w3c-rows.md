# Dual semantic views for Functional W3C rows

Status: accepted

The Functional-Style Syntax lane remains one top-level correctness lane with
two semantic views because its 21 official payloads declare both Direct and
RDF-Based applicability. The Direct view parses the payload into the canonical
structural ontology; the RDF-Based view starts from that same canonical
structure and applies the normative OWL mapping to RDF before evaluating RDF
semantics. Each result is identified by the operation row and semantic view,
uses the official expected result and provenance, and is reported separately.
The Functional lane passes only when both semantic views pass; the campaign
completion gate still combines the three top-level lanes by AND.

## Considered Options

- Evaluating only the Direct view was rejected because it would silently drop
  an applicability declared by the official test metadata.
- Creating a fourth top-level lane was rejected because the campaign already
  defines the Functional lane as a syntax-specific lane with semantic
  subviews.
- Translating the payload through a locally authored RDF/XML representation was
  rejected because the W3C mapping, not a custom serialization, owns the
  semantic transformation.

## Consequences

The Functional report has per-view counts and provenance; its two views must
not be summed as if they were independent corpus rows. The lane implementation
depends on both the unified structural model and the official Functional
parser, while RDF-Based view work must remain distinct from Direct view
reasoning.
