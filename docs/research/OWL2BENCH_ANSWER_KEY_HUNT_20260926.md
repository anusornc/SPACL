# OWL2Bench answer-key hunt

Date: 2026-09-26

Scope: exhaustive public search for any official or third-party expected-results
artifact for the 22 OWL2Bench SPARQL queries, going beyond the 2026-08-28
research by inspecting artifacts at file level. No answer keys were created,
approved, or copied.

## Verdict

No public answer key for the 22 OWL2Bench SPARQL queries exists. The 2026-08-28
conclusion stands after a deeper, artifact-level pass.

## Sources checked

- Official repository [kracr/owl2bench](https://github.com/kracr/owl2bench):
  full tree of 3,959 files scanned for result/answer/expected/gold artifacts.
  Only HermiT source internals and npm test fixtures match; no result data.
- All 8 forks compared against upstream `master` (GunjanSingh1, akshala, devrb,
  pake97, alexander-mart, ahmadalfaishal, zhouaimin, anusornc): no fork adds
  answer-like artifacts. GunjanSingh1's fork only predates an upstream cleanup
  of the bundled HermiT experiment sources.
- All 22 issues in the official repository scanned: dependency bumps and code
  patches only; no discussion of expected results.
- Zenodo search for "owl2bench" returns exactly four records:
  - [3838735](https://zenodo.org/records/3838735): the 22 query texts (known).
  - [3838285](https://zenodo.org/records/3838285): generator v2 archive (known).
  - [4764368](https://zenodo.org/records/4764368) (new finding): SemREC
    challenge dataset — 32 pre-generated ontologies across EL/QL/RL/DL at
    scales 1–200. Input ontologies only, no expected results.
  - [22959957](https://zenodo.org/records/22959957) (new finding):
    [CORE-LLM-Bench](https://openreview.net/forum?id=core-llm-bench) (published
    2026-09-25), an ontology-grounded LLM reasoning benchmark derived from
    OWL2Bench. It ships ground-truth answers plus explanation structures for
    its own 9,048 QA instances — not for the 22 SPARQL queries — but it is a
    direct precedent for justification-backed ground truth.
- [SemREC challenge site](https://semrec.github.io/): the organizers (the
  OWL2Bench authors' group — KRaCR Lab / IBM) scored subsumption correctness
  via labels in provided datasets plus an unpublished organizer-held hidden
  test set. That hidden ground truth is for subsumption tasks, not the query
  suite, and was never published.

## Conclusions for the campaign

- The OWL2Bench authors' own evaluation practice derives expected answers with
  reference reasoners (HermiT, Pellet) rather than a published gold file, so
  the ADR-0013 justification-backed derived-oracle path is consistent with —
  and stricter than — the benchmark owners' methodology.
- The authors demonstrably construct ground truth internally (SemREC hidden
  sets), so an outbound request remains the only route to any official key or
  endorsement; the request should cite SemREC explicitly.
- CORE-LLM-Bench's ground-truth-with-explanations artifact is a methodological
  precedent for justification evidence and its authors are an additional
  potential audit contact.
