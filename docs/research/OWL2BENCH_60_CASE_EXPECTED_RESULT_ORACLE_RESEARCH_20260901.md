# OWL2Bench 60-case expected-result oracle research

Date: 2026-09-01

Scope: primary-source verification of the official OWL2Bench repository, its
official release artifact, the Zenodo query record, and the published OWL2Bench
paper/README materials. This note does not create, copy, approve, or infer an
answer key. Engine agreement remains non-oracle.

## Audited source identities

The following immutable identities define the source trail checked on
2026-09-01. They identify the benchmark workload and release surfaces; none is
an expected-result artifact.

| Source | Revision / record | Content identity | Audit result |
|---|---|---|---|
| Official `kracr/owl2bench` repository | `master` commit `5a27536c8e7f7621b34c61c8a57ed9eb63e7829a` (tree `763debac5378265b32dfb3ca6e68fc822f943776`) | Recursive official tree and experiment scripts | Workload/build/execution sources present; no answer-key bundle recovered. |
| Official GitHub release `1` | tag commit `133989caee06d17633ec5537a478902bb583a5ef`; asset API id `25404283` | `OWL2Bench.jar`: SHA-256 `1f2d6013cfdc679550cb5c5c11ccca40849f63ac595c887b13c55c76ff06370d`, MD5 `976741c16feec3afe67f97fb34def758`, 66,110,579 bytes | Executable benchmark artifact; no expected-result archive. |
| Zenodo query record `3838735` | DOI `10.5281/zenodo.3838735` | `OWL2BENCH_SPARQL_Queries.txt`: MD5 `9f762c5f4ce46e9579f26b7834314d32`, SHA-256 `87af702049661ae9c2979dd59281d83985aed8900bbcc682f5556058c0dbdb56`, 6,457 bytes | Official 22-query workload with profile annotations; no expected answers. |
| Repository-staged 60-case expansion | DL=22, EL=10, QL=10, RL=18 | Manifest SHA-256 `dae8f0a57e74a61690cce432915edeadba929ccf7a84c91a2a59e8a841d8f7a7` | Direct workload expansion only; not an oracle. |

Audit disposition: `oracle_unavailable`. The missing evidence is an externally
attributable per-case expected-result bundle, or a deterministic derived bundle
with proof artifacts and an independent audit tied to these exact workload and
dataset identities. No such artifact was recovered, so no answer row is
created or promoted by this report.

## Bottom line

| Question | Result | Why |
|---|---|---|
| Is there an independently attributable authoritative OWL2Bench expected-result artifact in the official sources I checked? | `not recovered` | The official repo, release, Zenodo query record, README, and experiment scripts publish workload definitions and execution instructions, but no official expected-result bundle. |
| Can the official 22-query workload be expanded to 60 profile cases from the source annotations alone? | `yes, by direct count` | The official query file lists per-query applicable OWL 2 profiles; counting those annotations yields DL 22, EL 10, QL 10, RL 18, for 60 cases total. This is a derived count, not an answer key. |
| Is a claim-safe audited-derived oracle recoverable from the recovered official sources alone? | `not yet` | The missing pieces are exact per-case expected outputs, provenance for the concrete run that produced them, and an audit trail that ties the outputs to the official query/dataset identities. |

## Official source trail

### 1) OWL2Bench repository

The official repository is [kracr/owl2bench](https://github.com/kracr/owl2bench).

The repository README states that OWL2Bench is a benchmark for OWL 2 reasoners,
that it contains fixed TBoxes, ABox generation, and a separate set of SPARQL
queries, and that the queries are published via Zenodo:

- [Repository README at audited commit](https://github.com/kracr/owl2bench/blob/5a27536c8e7f7621b34c61c8a57ed9eb63e7829a/README.md)
- [Raw README at audited commit](https://raw.githubusercontent.com/kracr/owl2bench/5a27536c8e7f7621b34c61c8a57ed9eb63e7829a/README.md)

The repository’s `Experiments/README.md` says the experiment scripts cover
dataset generation, reasoning tasks, and query execution, and that the query
set is available at Zenodo:

- [Experiments README at audited commit](https://raw.githubusercontent.com/kracr/owl2bench/5a27536c8e7f7621b34c61c8a57ed9eb63e7829a/Experiments/README.md)

The query runner script is only an execution harness; it invokes Stardog query
commands and prints completion markers. It does not publish, compare, or store
expected results:

- [Experiments/QueryResponse.sh at audited commit](https://raw.githubusercontent.com/kracr/owl2bench/5a27536c8e7f7621b34c61c8a57ed9eb63e7829a/Experiments/QueryResponse.sh)

I also searched the official repository tree recursively through the GitHub API
for `expected`, `oracle`, `answer`, `gold`, `result`, `query`, and `sparql`.
The tree contains the benchmark sources, experiment scripts, and supporting
code, but no official benchmark answer-key file. The search path was:

- [GitHub tree API at audited tree](https://api.github.com/repos/kracr/owl2bench/git/trees/763debac5378265b32dfb3ca6e68fc822f943776?recursive=1)

### 2) Official release artifact

The official GitHub release for the repository is tag `1`, and the release API
exposes a single asset:

- [GitHub release tag 1](https://github.com/kracr/owl2bench/releases/tag/1)

The corresponding release API shows the asset list contains only
`OWL2Bench.jar` (asset id `25404283`). The downloaded asset identity is
SHA-256 `1f2d6013cfdc679550cb5c5c11ccca40849f63ac595c887b13c55c76ff06370d`
and MD5 `976741c16feec3afe67f97fb34def758` at 66,110,579 bytes:

- [Release API for tag 1](https://api.github.com/repos/kracr/owl2bench/releases/tags/1)
- [Immutable asset API record](https://api.github.com/repos/kracr/owl2bench/releases/assets/25404283)

That is an executable benchmark artifact, not an expected-result archive.

### 3) Zenodo query record

The official Zenodo query record is [10.5281/zenodo.3838735](https://zenodo.org/records/3838735).
Its API exposes exactly one file:

- `OWL2BENCH_SPARQL_Queries.txt`
- checksum `md5:9f762c5f4ce46e9579f26b7834314d32`
- size `6457`

Source:

- [Zenodo record API 3838735](https://zenodo.org/api/records/3838735)
- [Zenodo query file content](https://zenodo.org/api/records/3838735/files/OWL2BENCH_SPARQL_Queries.txt/content)

The query file explicitly says:

- there are twenty-two SPARQL queries;
- each query includes a profile annotation;
- the profile annotation lists the OWL 2 profile(s) to which that query applies.

That means the 60-case expansion is a derived count from the official file’s
profile annotations:

| Profile | Count |
|---|---:|
| DL | 22 |
| EL | 10 |
| QL | 10 |
| RL | 18 |
| Total | 60 |

This derived count is useful for coverage accounting, but it is still not an
expected-result oracle.

### 4) Official OWL2Bench paper

The paper describes OWL2Bench as an extension of UOBM with four profile TBoxes,
an ABox generator, and a fixed set of 22 SPARQL queries:

- [ISWC 2020 paper PDF](https://ceur-ws.org/Vol-2721/paper587.pdf)

The paper and README both support the same conclusion: the benchmark workload is
officially defined, but the public artifacts I checked do not publish a gold
answer set.

### 5) Official source archive

The Zenodo source archive for the benchmark repository is:

- [Zenodo record 3838285](https://zenodo.org/records/3838285)
- [Zenodo record API 3838285](https://zenodo.org/api/records/3838285)

Its API exposes one file:

- `GunjanSingh1/OWL2Bench-v2.zip`
- checksum `md5:a0fa2f25919ac5850d6c420f5d47d19d`
- size `121590242`

This is the benchmark source archive, not an expected-result package.

## What I did not find

I did not recover any of the following from the official sources above:

- a published per-query/per-profile expected-result file;
- a maintainer-signed oracle bundle;
- a machine-readable gold vector for the 60 profile cases;
- a release asset or Zenodo file that clearly functions as an answer key;
- an audit record tying a candidate answer file to exact dataset/query hashes.

In particular, the official materials I checked never move from workload
definition to authoritative answers. They stop at generation, execution, and
experiment description.

## What evidence would be required for a claim-safe audited-derived oracle

To promote a derived result set to claim-safe status, I would need all of the
following, or an equivalent evidence bundle from the benchmark owners:

1. Exact workload identity.
   - Official query file hash or record identifier.
   - A clear mapping from each of the 22 queries to the 60 profile cases.

2. Exact data identity.
   - The dataset generation inputs for the specific run: profile, university
     count, seed, configuration, and TBox/ABox identities.
   - Hashes or immutable identities for the generated ontologies used to
     produce the outputs.

3. Exact execution identity.
   - Reasoner/SPARQL engine name, version, container or binary digest, and
     runtime flags.
   - Canonicalization rules for result ordering, blank nodes, and serialization.

4. Result provenance.
   - The raw outputs for every case.
   - The normalized expected-result files.
   - A trace showing how each expected file was produced from the exact inputs.

5. Independent audit trail.
   - A second-party review or reproducible verification pass.
   - A statement that the artifact is an approved oracle, not just a run log or
     an engine-agreement derivative.

Without those pieces, a derived bundle is still only execution evidence.

## Handoff for later `$implement #47`

The safe next step is not to invent answers. It is to acquire or publish a
first-party expected-result archive with immutable identities for:

- the 22-query workload,
- the exact dataset used to derive each of the 60 cases,
- the engine/runtime that produced the outputs,
- and the canonicalized result files for each case.

If that archive cannot be recovered from the OWL2Bench maintainers or their
official releases, then the repository should keep treating OWL2Bench query
results as `oracle_unavailable` and use only workload-level validation.

## Reusable takeaway

The official OWL2Bench sources recover the workload, not an oracle. The 22
queries expand to 60 profile cases by profile annotation count, but no
authoritative expected-result artifact was recovered from the official repo,
release, Zenodo, or paper materials.
