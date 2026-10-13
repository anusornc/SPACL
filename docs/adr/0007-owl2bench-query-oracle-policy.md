# OWL2Bench query oracle policy

Status: accepted; historical engine baseline superseded by ADR 0008

The OWL2Bench query workload is one correctness lane with profile subviews.
The 22 official query texts expand into 60 query/profile cases, and the
correctness anchor is one deterministic generated dataset per profile
(official generator revision, seed 1, one university). An official answer key
has highest authority; an independently audited formal derivation may serve as
a derived oracle; agreement between engines is differential evidence only.
Without an accepted key, a case is oracle_unavailable and cannot pass its
profile or the overall query gate. This preserves query correctness separately
from loading and latency while keeping the final profile and campaign gates
conjunctive.

## Considered Options

- Promoting agreement between two or more engines was rejected because
  correlated implementations and shared assumptions do not establish
  semantic correctness.
- Treating a successful query execution as a pass was rejected because
  execution proves availability, not the answer.
- Using raw engine output bytes was rejected because serialization order,
  lexical formatting, RDF term types, and blank-node labels are not a stable
  semantic equality contract.
- Requiring every scalability dataset to be a correctness case was rejected
  because it conflates the correctness anchor with the separate scalability
  dimension.

## Consequences

The query lane needs an independently attributable oracle pipeline, typed
canonical result representation, exact generated-dataset identity, and
per-profile reports. Stardog 7.0.2 with the profile-specific reasoning mode is
the pinned historical execution baseline when its runtime is available;
GraphDB or Oxigraph runs remain differential unless separately accepted under
the oracle policy. Missing credentials or runtime access produces an
availability blocker and does not lower the correctness standard.
