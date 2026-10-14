# ORE 2015 consistency oracle recovery

Date: 2026-09-01

Scope: official ORE 2015 corpus/result packages, the official ORE 2015 framework repository/spec, and the published competition report. This note records whether an independently attributable 604-file consistency expectation artifact can be recovered for issue #48. It does not create, infer, approve, or copy an answer vector.

## Request type

Comprehensive research.

## Bottom line

I did not recover a claim-safe ORE 2015 consistency oracle for the official 604-file sample. The official sources recover input identity, sample ordering, and protocol-consensus hash generation, but not a separately published row-by-row normalized `true`/`false` expectation archive with enough provenance to treat it as independently audited gold.

For implementation purposes, the correct status remains `oracle_unavailable`.

## Official source set

| Source | Exact identity | What it establishes | What it does not establish |
|---|---|---|---|
| Zenodo corpus record | [ORE 2015 Reasoner Competition Corpus](https://zenodo.org/records/18578) | The corpus archive is `ore2015_sample.zip` with Zenodo checksum `md5:109f04cf8f124eb551d33c100e549730`. The record is the official corpus anchor. | It is an input corpus, not a published gold oracle. |
| Downloaded corpus archive | Zenodo record `18578`, file `ore2015_sample.zip` | MD5 `109f04cf8f124eb551d33c100e549730`, SHA-256 `775064eb6dfee404d8f0b0e8500974eb420006b656f5cf413e05a9b33c2252e7`, 725,398,299 bytes. The archive contains both broader `pool_sample/dl/` folders and the narrower `pool_sample/pure_dl/` sample used by the issue. | The archive alone does not publish a normalized answer vector. |
| Competition framework repo | [ykazakov/ore-2015-competition-framework](https://github.com/ykazakov/ore-2015-competition-framework) at commit `9f535f7b2fd50ed32a867a180c8b4cd58f2da7d1` (tree `4d58aeeaee0f44350f6fad05de54c54bde7b6de9`) | The framework reads expected results from `data/expectations/` and can generate replacement hashes from competition outputs. | It does not ship a full 604-row independently attributable oracle. |
| Framework README | [README](https://github.com/ykazakov/ore-2015-competition-framework/blob/9f535f7b2fd50ed32a867a180c8b4cd58f2da7d1/README.md#L176-L181) and [README](https://github.com/ykazakov/ore-2015-competition-framework/blob/9f535f7b2fd50ed32a867a180c8b4cd58f2da7d1/README.md#L377-L381) | The framework compares parsed/normalized results against expected hashes in `data/expectations/`, and the README says the `random-majority-voting` hashes can be copied into that directory to create new expectations. | This is protocol machinery, not a public gold vector. |
| Framework specification | [SPECIFICATION.md](https://github.com/ykazakov/ore-2015-competition-framework/blob/9f535f7b2fd50ed32a867a180c8b4cd58f2da7d1/SPECIFICATION.md#L110-L115) | Consistency output is defined as `true` for consistent and `false` for inconsistent. | It defines output format, not the missing 604-row expectation archive. |
| Public competition report | [The OWL Reasoner Evaluation (ORE) 2015 Competition Report](https://pmc.ncbi.nlm.nih.gov/articles/PMC6044265/) | The report says the live competition had 14 reasoners, six tracks, and 306 OWL DL consistency/classification cases plus 298 OWL EL consistency/classification cases in the live corpus sample. It also says the majority-vote/random-tie fallback is unsatisfactory. | It does not publish a reusable, independently audited 604-row expectation vector. |
| Public results mirror | [ORE 2015 results mirror](https://dl.kr.org/ore2015/) | The 23 consistency result TSV identities (10 DL, 13 EL) are recorded in [the source manifest](ORE_2015_CONSISTENCY_RESULT_SOURCE_MANIFEST_20260901.tsv), SHA-256 `06b6815893ce0b79d8ba186a2cf407ac4264f7b0fb91b02173d1dade9d4c6ef0`. Each manifest row records the lane, exact filename, parsed row count, and full downloaded TSV SHA-256. The TSVs expose `Processed`, `Correctly-Processed`, `Timeout`, `Execution-Error`, `Unexpected`, and `Not-Processed` flags for each reasoner run. | These flags show protocol outcome, not the hidden normalized truth value for each ontology. |
| Reasoner binaries package | [ORE 2015 Reasoning Systems](https://zenodo.org/records/50738) | The package contains participant executables. | It is not an expectations archive. |
| User-submitted ontologies package | [ORE 2015 User Submitted Ontologies](https://zenodo.org/records/50737) | The package contains ontology inputs. | It is not an expectations archive. |

## Corpus mapping evidence

The issue's 604-file sample matches the archive's `pure_dl` consistency subset plus the EL consistency subset, not the broader `dl` folders.

| Lane | Archive path inside `ore2015_sample.zip` | Rows | `metadata.csv` SHA-256 | `fileorder.txt` SHA-256 | First `ore2015_filename` | Last `ore2015_filename` | Exact order match |
|---|---|---:|---|---|---|---|---|
| pure_dl consistency | `pool_sample/pure_dl/consistency` | 306 | `52d0f2fb95c6015da3346adda1fc7b3a547e60a8c23ad7ab4caba0231c674467` | `4427065762f8eefec2dd27ae4c358e58daf35d439580010e1e4913cb0b9da9fe` | `ore_ont_12901.owl` | `ore_ont_16822.owl` | yes |
| EL consistency | `pool_sample/el/consistency` | 298 | `6a2500b43d85b18b56cbc0b5874d30372649a1466fc715126c4d32f08cf1fe35` | `bee9c21dd20263cf2f238a09b5fcaf93b588b27b9d6b791b43f3fe96d5c47874` | `ore_ont_9809.owl` | `ore_ont_4261.owl` | yes |

The archive also contains a broader `pool_sample/dl/consistency` folder, but that is not the target sample for this issue. The target 604-file sample is the `pure_dl` subset plus EL.

The repository-staged 604-row input manifest has SHA-256
`135e3ecab6f5b4d1ca8f6cff359cc6390a150508bf2c0eeab2d9b14e32d3c96a`.
This hash locks the normalized file mapping only; every row remains
`oracle_unavailable` because the mapping contains no semantic expectation.

## Protocol consensus versus gold

The framework contains three distinct expectation-generation paths:

| Path | What it does | Handling of missing, malformed, or incomplete results | Why it is not an independent gold oracle |
|---|---|---|---|
| Verified reasoners | `ExpectedResultByVerifiedReasonersGenerationEvaluator` writes a hash only when every designated verified reasoner yields the same valid hash. | It rejects items with timeout, execution error, incomplete execution, reasoner errors, over-time processing, missing response, or missing verification report. | This is consensus among chosen reasoners, not an audited semantic gold. |
| Majority voting | `ExpectedResultByMajorityVotingGenerationEvaluator` counts valid completed hashes and writes the leading hash only when it leads the runner-up by at least one vote. | Invalid items are skipped. Missing or malformed responses do not contribute a vote. | The second-pass runner-up tracking is flawed in the checked code, so tie/near-tie behavior is order dependent. Even if corrected, this is still protocol consensus. |
| Random majority voting | `ExpectedResultByMajorityRandomVotingGenerationEvaluator` selects randomly among the co-maximal hashes after shuffling. | Invalid items are skipped. Missing or malformed responses do not contribute a vote. | Without the original random seed and selected hash, the result is not reproducible as a gold oracle. |

Relevant source lines:

- [Verified-reasoners generator](https://github.com/ykazakov/ore-2015-competition-framework/blob/9f535f7b2fd50ed32a867a180c8b4cd58f2da7d1/src/org/semanticweb/ore/evaluation/ExpectedResultByVerifiedReasonersGenerationEvaluator.java#L75-L118)
- [Majority-voting generator](https://github.com/ykazakov/ore-2015-competition-framework/blob/9f535f7b2fd50ed32a867a180c8b4cd58f2da7d1/src/org/semanticweb/ore/evaluation/ExpectedResultByMajorityVotingGenerationEvaluator.java#L63-L125)
- [Random-majority generator](https://github.com/ykazakov/ore-2015-competition-framework/blob/9f535f7b2fd50ed32a867a180c8b4cd58f2da7d1/src/org/semanticweb/ore/evaluation/ExpectedResultByMajorityRandomVotingGenerationEvaluator.java#L62-L115)

The competition report independently says the majority-vote / random-tie fallback is unsatisfactory. That matches the code-level distinction above: the protocol can produce comparison hashes, but it does not recover an independently audited gold vector.

## Public result packages

The mirrored public consistency TSVs show why they are not enough to recover a gold oracle:

- they use a legacy `ore2014/files/ore_ont_*.owl` path prefix, so the identity key is the basename;
- they expose run-status columns such as `Processed`, `Correctly-Processed`, `Timeout`, `Execution-Error`, `Unexpected`, and `Not-Processed`;
- `Correctly-Processed=true` means the published run matched the competition's hidden expectation, not that the hidden expectation has been published;
- they do not expose a 604-row `true`/`false` oracle vector that can be independently audited from the package alone.

## Checked paths and remaining gap

I checked the following official or first-party sources:

- Zenodo record 18578 and the downloaded `ore2015_sample.zip`
- the official ORE 2015 framework repository at commit `9f535f7b2fd50ed32a867a180c8b4cd58f2da7d1`
- the framework README, specification, and evaluation code
- the public ORE 2015 competition report
- the public results mirror and local copies of the DL/EL consistency TSVs
- the content-addressed 23-file consistency result source manifest stored next
  to this report
- Zenodo record 50738 (reasoner binaries)
- Zenodo record 50737 (user-submitted ontologies)

What is still missing:

- a normalized 604-row expectation archive with row-by-row `true`/`false` values;
- provenance that ties each expected row to an independently attributable oracle artifact rather than to competition consensus;
- deterministic seed / replay data for any random majority selection that would otherwise be used as a fallback.

Audit disposition: `oracle_unavailable`. The corpus archive, normalized
306+298 mapping, framework code, and public result-package identities are
reproducible, but none supplies an independently attributable 604-row semantic
truth vector. No consistency answer row is generated or copied by this audit.

## Handoff for `$implement #48`

Use the `pure_dl/consistency` and `el/consistency` archive mappings above as the canonical sample identity map for the issue. Keep every row at `oracle_unavailable` until a separate expectation archive or organizer-provided oracle vector is recovered and verified end to end.

Do not promote `Correctly-Processed`, majority hashes, or a single reference reasoner run to gold status.
