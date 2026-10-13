# OWL2Bench stage suite re-typed and SPACL verdict reconciled

Status: accepted

Two campaign-contract reconciliations close the follow-ups recorded by ADR
0026 (issue #199):

1. **Stage suite re-typing.** The OWL2Bench stage suite is re-typed as an
   execution-informational suite under the ADR 0024 pattern: always
   reported, never blocking and never passing the campaign gate. Its
   correctness rows keep `oracle_unavailable` permanently — no oracle exists
   for OWL2Bench consistency, and the suite's execution evidence (loads,
   consistency executions, timing) remains visible as performance-lane
   data. The campaign correctness conjunction becomes W3C AND
   owl2bench-query.
2. **SPACL verdict reconciliation by declaration.** The accepted SPACL
   query verdict (lane gate passed, 60/60 oracle matches accepted) is
   reconciled with the campaign's owl2bench-query contract by making the
   SPACL stack emit the split-stack declaration and the component/
   materialization identities the contract requires — per-profile adapter
   identity, the pinned query-backend digest, dataset identity, and
   canonicalizer identity — and re-running the verdict. The campaign's
   strict contract is unchanged.

## Considered Options

- Redefining the stage gate as execution-success-only was rejected: it
  would let a correctness gate pass without correctness semantics.
- Relaxing the campaign's query contract to accept alternative declaration
  shapes was rejected: it weakens the strict split-stack evidence contract
  for no gain, since the SPACL stack can emit the required declaration.
- Leaving both blocked was rejected: the campaign conjunction could never
  complete, and a lane-level pass would stay invisible at campaign level.

## Consequences

- The campaign conjunction is W3C AND owl2bench-query; after the SPACL
  verdict re-run, campaign completion becomes reachable when W3C passes.
- The stage suite's report keeps flowing as execution evidence under
  performance-lane wording; no correctness claims are added anywhere.
- The re-run verdict must reproduce the accepted oracle matches; any
  mismatch is a blocker, not a re-try in place.
