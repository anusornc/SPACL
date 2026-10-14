# ORE 2015 consistency oracle recovery

Date: 2026-08-26

This note resolves the research question in
[Wayfinder: Recover ORE 2015 consistency oracle for the 604-file sample](https://github.com/anusornc/tableauxx/issues/11).
It records what can be reproduced from the official corpus, framework, public
result tables, and published ORE protocol. It does not turn a local or
reference-reasoner consensus into a gold oracle.

## Decision

The official corpus and public competition result tables provide an exact
604-file identity mapping and reproducible provenance, but they do not provide
a claim-safe semantic oracle for the 604 consistency cases.

The ORE protocol's majority vote is retained as differential/provenance
evidence only. Under the repository's official-oracle-first contract, it is
not an acceptable correctness oracle for the campaign unless the original
normalized response/expectation artifacts and their deterministic generation
context are recovered. The ORE lane must therefore continue to report
'oracle_unavailable' for cases without an independently attributable expected
result. Successful Tableauxx execution does not change that status.

This is a research resolution, not an implementation or benchmark-pass claim.

## Corpus and lane mapping

The official corpus is the Zenodo v1 archive
[ORE 2015 Reasoner Competition Corpus](https://zenodo.org/records/18578).
The downloaded archive has MD5
109f04cf8f124eb551d33c100e549730.

The ORE 2015 report defines the live consistency tracks as 306 OWL DL
problems and 298 OWL EL problems. The archive's consistency metadata and
file-order files reproduce those two sets:

| Local lane | Official track | Metadata rows | Unique IDs | File-order matches metadata |
|---|---|---:|---:|---|
| 'pure_dl' | OWL DL Consistency | 306 | 306 | yes |
| 'el' | OWL EL Consistency | 298 | 298 | yes |
| total | both consistency tracks | 604 | 604 | yes |

The identity key is the archive's 'ore2015_filename' basename, resolved under
'pool_sample/files/ore_ont_*.owl'. The public result tables use the legacy
'ore2014/files/' prefix, but every one of their basenames matches the
corresponding archive metadata row. The prefix difference is therefore a
display/path convention, not an identity match.

The archive is a corpus source, not an expected-result package. The ORE report
also explains that the full 1,920-ontology pool was preprocessed and sampled
into the 306/298 live tracks; the 604 rows are the selected track inputs, not
an independently labeled test set.

## Reproducible provenance

The following hashes were calculated from the local copy of the official
archive and the staged lane:

| Artifact | Count | SHA-256 or digest |
|---|---:|---|
| 'ore2015_sample.zip' | 1 | MD5 '109f04cf8f124eb551d33c100e549730' |
| 'pure_dl' consistency metadata | 306 | '52d0f2fb95c6015da3346adda1fc7b3a547e60a8c23ad7ab4caba0231c674467' |
| 'pure_dl' consistency file order | 306 | '4427065762f8eefec2dd27ae4c358e58daf35d439580010e1e4913cb0b9da9fe' |
| 'el' consistency metadata | 298 | '6a2500b43d85b18b56cbc0b5874d30372649a1466fc715126c4d32f08cf1fe35' |
| 'el' consistency file order | 298 | 'bee9c21dd20263cf2f238a09b5fcaf93b588b27b9d6b791b43f3fe96d5c47874' |
| sorted per-file source-hash manifest, 'pure_dl' | 306 | 'f674d802c888e947f5ebeac67c275d63dd9aa31e288287cacd425d67e3f93c3f' |
| sorted per-file source-hash manifest, 'el' | 298 | '2bb9e517296356451b055ecd7419f8bc766415a13f162fb2a7401f94f948945d' |
| sorted per-file source-hash manifest, all lanes | 604 | 'd052b8b4d198998f9252fb9085c7ac8c847dd0510594550b7b009173c92d4168' |
| staged 'manifest.tsv' | 604 | 'be587a374b14361937f2c6a93175a6087a48d3cfc4412a0cc0604396845d0114' |
| staged 'oracle_provenance.tsv' placeholder | 604 | '69ae0704a97fc64dc6d0f94810411f788ceab6c3843c3630aaa686114f5a815d' |

The sorted source-hash manifest rows use:

    local_group<TAB>ore2015_filename<TAB>sha256(source file)<NEWLINE>

The aggregate hash is not an oracle hash. It proves which source bytes were
used for the identity audit.

## What the official result artifacts contain

The ORE 2015 public result mirror
[publishes the final competition results](https://dl.kr.org/ore2015/).
Its downloadable consistency result tables cover:

- 10 OWL DL reasoners × 306 rows = 3,060 result rows;
- 13 OWL EL reasoners × 298 rows = 3,874 result rows;
- 23 reasoner tables and 6,934 table rows in total.

Every downloaded table had the exact expected ID set for its track: zero
missing IDs, zero extra IDs, and the expected row count. A sorted manifest of
the 23 downloaded table names, row counts, ID-set hashes, and table SHA-256
values has aggregate SHA-256
'e392db1b0576519719aa745f0dd76d2c0e3537d100422f865bc421bc52003bfb'.

Those public tables expose fields such as 'Processed',
'Correctly-Processed', 'Unexpected', 'Timeout', and execution/error flags.
They do not expose the normalized consistency value ('true' or 'false') or
the expected result hash for each ontology. 'Correctly-Processed=true' only
says that the published run matched the competition's expected normalized
result; it does not reveal that expected result to a new evaluator.

The [ORE 2015 reasoning-systems package](https://zenodo.org/records/50738)
contains executable participant systems. It is not a result/expectation
archive. The official GitHub framework at
[ykazakov/ore-2015-competition-framework](https://github.com/ykazakov/ore-2015-competition-framework)
is available at commit '9f535f7b2fd50ed32a867a180c8b4cd58f2da7d1'; its checked
in expectation tree contains only the small framework tests, not a 604-row
ORE 2015 consistency expectation set. The separate
[ORE 2015 user-submitted ontology package](https://zenodo.org/records/50737)
also contains ontologies, not the missing live-track expectations.

The surveyed official artifacts therefore establish coverage and provenance,
but not a semantic answer vector that can be attached to the 604 current
source files.

## Normalization and consensus protocol

The official [ORE 2015 reasoner specification](https://github.com/ykazakov/ore-2015-competition-framework/blob/master/SPECIFICATION.md)
requires a consistency reasoner to write 'true' for a consistent ontology and
'false' for an inconsistent ontology. The framework then normalizes reasoner
output and compares result hashes, rather than comparing a public boolean
column. For consistency output, the framework's normalizer represents
'true' as a bottom-to-top axiom and 'false' as a top-to-bottom axiom before
packing the result for hashing.

The framework contains three expected-result generators:

1. Verified-reasoner agreement writes an expected hash only when all designated
   verified reasoners agree.
2. Safe majority voting counts valid, completed normalized-result hashes and
   intends to write an expected hash only when the leading hash has at least a
   one-vote lead over the runner-up. The checked-in implementation's second
   pass compares a non-leading count with the leading count instead of with
   the current second-place count. Because the first pass uses a HashMap and
   '>' for maximum selection, the selected first non-leading entry can remain
   the second count. Thus tie and near-tie behavior is iteration-order
   dependent; it is not safe to describe this code as a deterministic
   runner-up policy without the exact execution context.
3. Random majority voting selects uniformly from the co-maximal hashes after
   shuffling the tied list. Without the original random seed and selected hash,
   this is not a deterministic replay contract.

The relevant implementation is
[ExpectedResultByMajorityVotingGenerationEvaluator.java](https://github.com/ykazakov/ore-2015-competition-framework/blob/master/src/org/semanticweb/ore/evaluation/ExpectedResultByMajorityVotingGenerationEvaluator.java)
and its companion
[ExpectedResultByMajorityRandomVotingGenerationEvaluator.java](https://github.com/ykazakov/ore-2015-competition-framework/blob/master/src/org/semanticweb/ore/evaluation/ExpectedResultByMajorityRandomVotingGenerationEvaluator.java).

The published ORE 2015 report independently describes result validation as a
majority vote with random tie-breaking fallback and calls the dispute
resolution mechanism unsatisfactory. It specifically notes that incomplete
reasoners can outweigh complete ones and that multiple submissions based on
the same underlying reasoner can effectively cast correlated votes. The
report therefore does not justify treating the majority result as a semantic
gold label.

## Oracle classification

| Evidence | Reproducible now | Allowed as campaign gold oracle |
|---|---:|---:|
| Zenodo corpus bytes and 604-row metadata mapping | yes | no; it contains inputs only |
| Public per-reasoner processed/correctness flags | yes | no; the semantic value is hidden |
| Fresh local reference-reasoner result | yes, subject to execution | no; differential evidence |
| Reconstructed majority from fresh reasoner runs | possible | no, absent an explicitly accepted protocol exception and complete deterministic provenance |
| Original normalized responses plus expected hashes and generation context | not recovered | yes, after exact identity and policy verification |

There is consequently no recovered ORE oracle hash:

    oracle_hash = NONE (not recovered)

The hash of 'oracle_provenance.tsv' above is intentionally reported as a
placeholder-file hash, not as an oracle hash. It must not be used to mark a
case as passed.

## Gate impact

The ORE lane remains split into 'pure_dl' and 'el' reports. It must preserve
the following statuses until an authoritative expectation artifact is
available:

- 'oracle_unavailable' for completed executions without an attributable
  expected result;
- 'timeout' or execution/error statuses for runs that do not terminate;
- 'wrong' only when a verified oracle comparison is actually performed.

The campaign completion gate remains an AND over the separate W3C, ORE, and
OWL2Bench lanes. ORE protocol consensus may be shown in a provenance or
differential appendix, but it cannot make either ORE lane pass the
correctness gate by itself.

## Recovery needed to unblock the ORE correctness gate

One of the following claim-safe inputs is still required:

1. the original ORE 2015 normalized response/expectation archive, including
   the expected hashes or semantic booleans and its file mapping;
2. an organizer-provided authoritative 604-row result vector with source
   identity and provenance; or
3. an explicitly human-approved change to the campaign contract that labels a
   fully reconstructed, deterministic ORE majority as 'ore_protocol_consensus'
   rather than pretending it is an independent gold oracle.

Until then, do not populate 'ORE2015_ORACLE_TSV' from Tableauxx, a single
reference reasoner, or an unseeded majority rerun.

## Sources

- [ORE 2015 Reasoner Competition Corpus, Zenodo record 18578](https://zenodo.org/records/18578)
- [ORE 2015 Reasoning Systems, Zenodo record 50738](https://zenodo.org/records/50738)
- [ORE 2015 User Submitted Ontologies, Zenodo record 50737](https://zenodo.org/records/50737)
- [ORE 2015 competition framework](https://github.com/ykazakov/ore-2015-competition-framework)
- [ORE 2015 framework specification](https://github.com/ykazakov/ore-2015-competition-framework/blob/master/SPECIFICATION.md)
- [Safe majority-result generator](https://github.com/ykazakov/ore-2015-competition-framework/blob/master/src/org/semanticweb/ore/evaluation/ExpectedResultByMajorityVotingGenerationEvaluator.java)
- [Random majority-result generator](https://github.com/ykazakov/ore-2015-competition-framework/blob/master/src/org/semanticweb/ore/evaluation/ExpectedResultByMajorityRandomVotingGenerationEvaluator.java)
- [ORE 2015 public result mirror](https://dl.kr.org/ore2015/)
- [The OWL Reasoner Evaluation (ORE) 2015 Competition Report](https://pmc.ncbi.nlm.nih.gov/articles/PMC6044265/)
- Local corpus staging: 'benchmarks/external/ore/prepare.sh' and
  'benchmarks/external/ore/report.sh'
- Local ORE run evidence:
  'unretained-artifact:competitors/ore2015_full_tableauxx_20260826_sharded_full/'
- Local benchmark contract:
  'docs/adr/0001-official-oracle-first-benchmark-contract.md'
