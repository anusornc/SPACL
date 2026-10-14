# RDF 1.1 simple-literal normalization in the query canonicalizer (v2)

The OWL2Bench query canonicalizer (v1) preserves each literal's datatype
exactly as the engine serialized it, so an engine that emits
`"x"^^xsd:string` and an oracle that records a plain literal compare as
different terms and the case is reported `wrong` even when the answers are
semantically identical. The first `konclude-unified` execution+oracle run
(2026-10-09, run `konclude-unified-verdict-seed1`) showed this concretely:
query 004 in DL/EL/RL returns the same 2,486 rows on every profile and is
byte-identical after normalization, yet reports `wrong`. We decided to adopt
RDF 1.1 simple-literal normalization in a canonicalizer v2 — a literal with
no language tag and datatype `xsd:string` canonicalizes to the simple-literal
form — because RDF 1.1 Concepts defines the two as one literal, so a typed
comparison stricter than the semantics destroys the meaning of `wrong` as a
signal for every future engine comparison.

## Consequences

- Bumping the canonicalizer to v2 invalidates, by the fail-closed identity
  contract, every accepted derived oracle whose manifest pins
  `owl2bench-query-canonicalizer/1`: all 60 combined-oracle cases must be
  re-derived and re-audited with v2 before any `passed` verdict is available
  again. This cost was accepted deliberately.
- Re-derivation is answer-preserving by construction (the verdict run proved
  the v1 row sets identical modulo normalization), so the existing
  `derived_oracle_pipeline` + `derived_oracle_audit` procedures re-run
  mechanically; no third engine is added to the audit. The re-derived set
  lives at `benchmarks/ontologies/external/owl2bench/derived-oracle-shipped-combined-v2`
  (60/60 rows accepted and independently audited; untracked by policy). The
  v1-keyed sets, including the per-profile `derived-oracle-{dl,el,ql,rl}-seed1`
  variants, are retained as historical evidence for the runs that consumed
  them; any later v2 runner fails closed on their version identity.
- After v2, `wrong` means a semantic disagreement again; the konclude-unified
  differential record settled at 55 passed / 3 wrong (query 020 DL/EL/QL,
  one missing explicitly-asserted row — Konclude v0.7.0-1138 incompleteness)
  and 2 failed (query 006 DL/EL, upstream serializer projection defect,
  fail-closed), with the RL profile gate passing (run
  `konclude-unified-verdict-seed1-v2` under
  `benchmarks/external/owl2bench/query/results/history/`).

## Considered Options

- Keep v1 and annotate 004-class mismatches as "normalized-identical": keeps
  60 accepted oracles but leaves false `wrong` signals in every future
  comparison; rejected.
- Defer until a second comparison engine exists: the defect is in shared
  comparison machinery, not in any engine; rejected.
