# Per-case engine routing for the blocked query 006 DL/EL executions

Status: accepted

The OWL2Bench query lane may execute an individual query/profile case on a
profile-capable alternative adapter when the pinned konclude-unified engine
cannot produce a protocol-valid result for that case. Query 006 in DL and EL
is the currently known blocked pair: Konclude v0.7.0-1138 emits a SPARQL
Results document whose head projection does not match the SELECT clause, and
no upstream release fixes it.

Routing is constrained. The official case matrix is unchanged: 22 DL, 10 EL,
10 QL, 18 RL. The routed case keeps its profile-qualified identity and flows
through the same canonicalizer and the same oracle pipeline as every other
case. The result record must carry the per-case engine identity, version,
and invocation provenance, and lane reports must present a routed case so it
cannot be mistaken for a konclude-unified execution. The substitute adapter
must declare capability for the target profile; for query 006 this is the
split-stack HermiT DL adapter for DL and the ELK EL adapter for EL.

Routing changes execution availability, never correctness. A routed case
still reaches a correctness verdict only through the ADR 0007 oracle ladder
and the ADR 0013 acceptance procedure.

## Considered Options

- Keeping the two cases permanently failed was rejected because it leaves the
  query gate permanently red through an upstream serializer defect, not a
  semantic finding, and blocks the campaign destination indefinitely.
- Patching or rebuilding Konclude was rejected because it changes the pinned
  engine identity, carries an unbounded maintenance burden, and the patched
  serializer would still have no independent fix authority.

## Consequences

The two blocked cases can complete execution and become oracle-evaluable
under the same acceptance procedure as the rest of the matrix. Lane reports
gain per-case engine identity as a reported field. The konclude-unified
variant remains the default execution path; routing is per-case, evidenced,
and reversible by withdrawing the routing entry without touching the matrix.
