# OWL2Bench Query Lane

This lane stages and evaluates the official OWL2Bench SPARQL query files without inventing query answers.

## Prepare

```bash
OWL2BENCH_QUERY_SOURCE_DIR=/path/to/owl2bench/Experiments/Queries \
benchmarks/external/owl2bench/query/prepare.sh
```

The source tree is scanned for `.rq`, `.sparql`, and `.spql` files. Profile labels are inferred from the path or filename and preserved in `manifest.tsv`.

The official Zenodo release also provides
`OWL2BENCH_SPARQL_Queries.txt`. When that file is present, the prepare step
extracts its 22 query blocks verbatim into staged `.rq` files, preserves the
source hash and query provenance, and records all profiles listed by each
query (for example `EL,QL,DL`).

## Run

```bash
OWL2BENCH_QUERY_ENGINE_BIN=/path/to/query-engine \
OWL2BENCH_QUERY_CAPABILITIES_FILE=/path/to/capabilities.json \
OWL2BENCH_QUERY_DATA_DIR=/path/to/generated/data \
OWL2BENCH_QUERY_ORACLE_DIR=/path/to/answer-keys \
OWL2BENCH_QUERY_ORACLE_MANIFEST=/path/to/answer-keys/manifest.json \
OWL2BENCH_QUERY_DATASET_METADATA=/path/to/dataset-metadata.json \
benchmarks/external/owl2bench/query/run.sh
```

The capability manifest must declare `supports_query: true`, a `profiles` array, and the engine data mode. Without an engine or capability manifest the case is `not_available`; without an answer key a completed engine run is `oracle_unavailable`. Neither state is a correctness pass.

When only `OWL2BENCH_QUERY_ORACLE_DIR` is set, the manifest resolves from the
directory itself: `manifest.json` (official answer-key layout) first, then
`derived-oracle-manifest.json` (derived-oracle layout, e.g.
`benchmarks/ontologies/external/owl2bench/derived-oracle-rl-seed1`) — the
official layout wins when both exist. Every
lookup still fails closed unless the case identity — corpus, revision, query,
profile, dataset sha256, canonicalizer — matches the manifest exactly.

For `data_mode: directory`, `OWL2BENCH_QUERY_DATA_DIR` is a context root with
one directory for each profile, named exactly `DL`, `EL`, `QL`, and `RL`:

```text
generated-data/
├── DL/
├── EL/
├── QL/
└── RL/
```

The runner invokes `<engine> load <data-root>/<PROFILE>` once for each
available profile used by a query case and invokes `<engine> query <query-file>
<data-root>/<PROFILE>` for each matching query/profile case. It never loads the
profile directories into one shared directory context. A missing profile
directory marks only that profile's cases `not_available`; other profiles
continue independently. Each result records its selected `profile_context`
directory, identity, status, load status, and load time, while
`run_metadata.json` records the same context map for all four profiles.
Query-file-only adapters keep their existing interface and do not require these
directories.

The repository includes an Oxigraph Docker adapter for a reproducible local
engine:

```bash
OWL2BENCH_QUERY_ENGINE_BIN=benchmarks/external/owl2bench/query/oxigraph_engine.sh \
OWL2BENCH_QUERY_CAPABILITIES_FILE=benchmarks/external/owl2bench/query/oxigraph-capabilities.json \
OWL2BENCH_QUERY_DATA_DIR=/path/to/generated/owl2bench-data \
OWL2BENCH_QUERY_ORACLE_DIR=/path/to/versioned/answer-keys \
benchmarks/external/owl2bench/query/run.sh
```

The adapter pins `oxigraph/oxigraph:0.5.9` by tag, loads RDF/XML files into a
data-directory-owned store, and injects the namespace declarations that the
official query text assumes. Record the image digest in run metadata before
publishing results; this adapter is an execution engine, not an official
OWL2Bench answer oracle.

Manifest rows point to `.out` or `.ans` answer artifacts. Raw answer files
without an accepted manifest row never produce a correctness pass. Successful
engine and oracle outputs are converted to the versioned typed canonical result
format by `canonicalize_result.py`; the canonical UTF-8 JSON payload is hashed
with SHA-256. Result records expose the canonicalizer version, output format,
canonical result file, and result hash. Malformed or unsupported engine result
documents are `failed` rather than an empty successful result. Load time and
per-query latency are retained as separate fields.

The runner also persists `source_query_sha256` in every result record. Running
`report.sh` creates exactly five Markdown reports: the overall `report.md`
and `profiles/{DL,EL,QL,RL}/report.md`. Each profile has its own
`summary.json` with result, execution, correctness, and oracle status counts,
source/oracle hashes, dataset and engine identities, canonicalizer identity,
and timing provenance. The profile gate requires exactly 22 DL, 10 EL, 10 QL,
and 18 RL rows; every row must have a passed result with an accepted official
or independently audited derived oracle and complete source, dataset,
canonicalization, and oracle identity. The overall query gate is exactly
`DL AND EL AND QL AND RL`; it is never inferred from an aggregate status
count. Missing or invalid rows, profiles, hashes, or provenance block the
corresponding profile and the overall query gate. Performance/scalability
claims remain suppressed while that gate is blocked.

When split-stack mode is requested, each profile summary additionally exposes
component and materialization status counts, component/materialization
identities, and profile-context identities. A profile report blocks if a row
does not retain the available component manifest and complete materialization
identity for that profile; the overall query report preserves the same
four-profile AND gate. These fields are execution provenance only and do not
turn a component, materialized graph, or differential engine result into an
answer oracle.

No official answer-key bundle is assumed. The lane only reports `passed` or `wrong` when an explicit oracle directory is supplied.

### Split-stack component manifest

The provisional split baseline accepts an additional profile-qualified runtime
manifest through `OWL2BENCH_QUERY_COMPONENT_MANIFEST`. The example contract is
`split-stack-component-manifest.example.json`; its empty pins are intentional
and validate as `not_available` until an operator supplies exact values.
Set `OWL2BENCH_QUERY_SPLIT_STACK=1` for a split-stack run; if its component
manifest is absent, every affected profile case is recorded as
`not_available` instead of using the legacy generic engine path.

Validate the component manifest and expand the official query manifest into the
60 independent query/profile cases with:

```bash
python3 benchmarks/external/owl2bench/query/split_stack_manifest.py validate \
  --component-manifest /path/to/split-stack-component-manifest.json \
  --query-manifest /path/to/query-staged/manifest.tsv \
  --output /path/to/manifest-validation.json
```

The top-level matrix is fixed at `DL=22`, `EL=10`, `QL=10`, and `RL=18`.
Each profile must pin engine, materializer, and query components (name,
version, revision, and SHA-256 artifact digest), configuration identity,
runtime/dependency identity, credential status, capability, resource limits,
license status plus secret-free identity/hash when available, dataset identity,
generator revision/seed, and profile-context identity. Each
expanded case carries its official query source, profile dataset,
generator/seed, and profile context. Duplicate, unknown, missing, or
cross-profile rows make the matrix `failed`; an incomplete component pin is
`not_available`; a declared capability gap is `unsupported`. The runner never
falls back to another profile component. The validator emits only execution
provenance and explicitly rejects answer-key fields; credentials are reduced to
status and are never copied into validation or result artifacts.

### Profile materialization contract

The query runner can enforce a profile materialization manifest through
`OWL2BENCH_QUERY_MATERIALIZATION_MANIFEST` (or automatically in explicit split
mode). Validate it with:

```bash
python3 benchmarks/external/owl2bench/query/materialization_manifest.py validate \
  --materialization-manifest /path/to/materialization-manifest.json \
  --output /path/to/materialization-validation.json
```

Each profile record contains `profile`, `source_ontology`, `component`, `configuration`,
`output_graph`, `derivation`, `resource_limits`, `input_sha256`,
`output_sha256`, and `profile_context`. A `complete` record must point to
existing source/output artifacts whose hashes match both the nested identities
and the top-level input/output hashes. `partial`, `failed`, `not_available`,
and `unsupported` remain explicit; only `complete` permits query execution.
Shared output-graph or profile-context identities fail the affected profiles,
and malformed or answer-key-bearing manifests fail closed. The runner retains
the validated record in each result and never uses a different profile graph as
a fallback.

Before a split-stack run, verify that the component and materialization
manifests agree on the materializer, configuration, profile context, and
resource envelope:

```bash
python3 benchmarks/external/owl2bench/query/materialization_manifest.py verify-stack \
  --component-manifest /path/to/split-stack-component-manifest.json \
  --materialization-manifest /path/to/materialization-manifest.json \
  --query-manifest /path/to/query-staged/manifest.tsv \
  --output /path/to/materialization-stack-validation.json
```

This command verifies supplied source/output artifacts and emits secret-free
provenance; it never generates an inferred graph or an answer key. A nonzero
exit status preserves `not_available`, `unsupported`, `partial`, or `failed`
instead of allowing an incomplete stack to enter the query lane.

### Fake profile materialization seam

The contract has a dependency-free fake profile path for testing the complete
materialization handoff without an external reasoner or query service:

```bash
python3 benchmarks/external/owl2bench/query/materialization_pipeline.py run-fake \
  --profile DL \
  --source /path/to/source.nq \
  --exporter /path/to/fake-exporter \
  --identity /path/to/profile-identity.json \
  --output-dir /path/to/materialization-run

python3 benchmarks/external/owl2bench/query/materialization_validator.py validate \
  --profile DL \
  --source /path/to/source.nq \
  --candidate-graph /path/to/materialization-run/candidate_graph.nq \
  --graph /path/to/materialization-run/graph.nq \
  --manifest /path/to/materialization-run/materialization.json \
  --output /path/to/materialization-run/validation.json
```

The fake exporter is a process boundary that writes candidate N-Quads to
stdout. The pipeline writes the candidate and URDNA2015 canonical graph; the
separate validator checks source inclusion, finite-term coverage, assertion
families, exact component digests, canonical round-trip, and profile/resource
identity. A successful validation emits a read-only query handoff containing
the exact graph digest. Exporter failure, an exporter-declared unsupported
capability, empty output, timeout, malformed graphs, missing artifacts, and
generated anonymous witnesses remain explicit status outcomes. This path
never creates or consumes an answer key. A successful materialization summary
records `execution_status=complete` but reports `status=oracle_unavailable`
until an accepted oracle is supplied.

### ELK, QL, and RL profile materialization

`profile_materialization.py` is the production-shaped handoff for the
non-DL profiles. It accepts one exact identity manifest and a profile-bound
exporter whose only data operation is:

```text
export <source-nquads>
```

The exporter is run twice. The second canonical graph must have the same
URDNA2015 bytes and SHA-256 as the first before `validation.json` can produce
a read-only `handoff.json`. ELK is bound to `EL`; the QL and RL paths also
require a ruleset identity and conformance boundary. Missing external
materializers return `not_available`, exporter exit `123` is `unsupported`,
and malformed/incomplete or nondeterministic output is `failed`. The manifest
retains first-run diagnostics plus separate export, determinism, validation,
and total materialization timings; timing is provenance and does not alter the
graph identity.

Example:

```bash
python3 benchmarks/external/owl2bench/query/profile_materialization.py run \
  --profile EL \
  --source /path/to/el-source.nq \
  --exporter /path/to/elk_el_materializer.sh \
  --identity /path/to/el-profile-identity.json \
  --output-dir /path/to/el-materialization
```

The profile-specific bridge scripts
`elk_el_materializer.sh`, `ql_ruleset_materializer.sh`, and
`rl_ruleset_materializer.sh` delegate only to an explicitly provisioned
external executable (`ELK_EL_MATERIALIZER_BIN`, `QL_RULESET_MATERIALIZER_BIN`,
or `RL_RULESET_MATERIALIZER_BIN`, respectively). They do not discover a
latest engine or synthesize a closure. A complete handoff is execution
evidence and remains `oracle_unavailable`; it never creates or approves an
answer key.

The current runtime-backed boundaries are also explicit:

- `elk_el_export_boundary.py` pins ELK 0.6.0 and reports `not_available` when
  only the official taxonomy/classification output is present, because that
  output is not a finite query-complete RDF graph.
- `graphdb_ql_materializer.py` provisions the current GraphDB 11.5.0 QL
  variant with a platform-specific image digest and records the documented QL
  conformance boundary. A missing license/runtime is `not_available`.
- `graphdb_rl_materializer.sh` provisions the analogous current RL variant.
  Its export is merged with the pinned source graph and filtered to the finite
  source term domain before independent validation, so GraphDB implementation
  vocabulary and generated witnesses cannot silently enter the graph.

`openllet_ql_materialization.py` is the production Openllet QL bridge. It
live-attests the exact `openllet-ql-runtime-v1` image and 78-jar classpath,
requires every profile/query artifact to match
`openllet_ql_acceptance_lock.json`, builds a QL-only `openllet-ql` component
manifest from that unchanged frozen identity, and runs
`openllet_ql_materializer.sh` twice through the shared profile materialization
seam. The acceptance lock has no runtime override. Runtime containers use the
digest-pinned image, `--network none`, a read-only root filesystem, and no
service endpoint. The exporter merges the complete source with named
source-domain entailments; query-relevant dependencies that require a
generated witness are `not_available`.

The same command retains the accepted entailment-sensitive fixture under
`openllet_ql_diagnostic/` as a separately hashed, non-oracle regression. The
shared validator independently derives its missing class and object-property
obligations, and the existing read-only query backend must observe both before
the diagnostic is complete. Diagnostic identity/result artifacts contain no
answer-key or semantic-oracle fields and never enter official result storage.

```bash
python3 benchmarks/external/owl2bench/query/openllet_ql_materialization.py run \
  --dataset-root /path/to/frozen-profile-datasets \
  --query-manifest /path/to/query-staged/manifest.tsv \
  --output-dir /path/to/openllet-ql-materialization
```

A successful run writes `official/handoff.json`,
`diagnostic/handoff.json`, `component-manifest.json`, and
`openllet_ql_materialization_summary.json`. Official status remains
`oracle_unavailable`; QL/overall correctness and performance claims remain
blocked. No Docker archive is created or published, and AGPL review plus a
second-host rebuild remain pre-release requirements.

The hermetic test always covers runtime identity, finite-witness rejection,
and source-inclusive export. Supply the accepted external corpus to also run
the locked generated-manifest path through the complete 60-case runner:

```bash
OWL2BENCH_PROFILE_DATASET_ROOT=/path/to/frozen-profile-datasets \
OWL2BENCH_QUERY_MANIFEST=/path/to/query-staged/manifest.tsv \
  benchmarks/external/owl2bench/query/test_openllet_ql_materialization.sh
```

For split-stack execution, select `openllet-ql` explicitly, set
`OWL2BENCH_OPENLLET_QL_ENGINE` to `openllet_ql_engine.sh`, and use
`split-openllet-ql-capabilities.json`. The adapter reuses the existing
profile-engine core to validate the generated component/materialization
manifests, probes the exact attested Openllet runtime, and delegates queries to
`openllet_ql_query_engine.sh`, which accepts only the graph bound to the
read-only handoff. GraphDB remains a separate explicit QL variant and is never
used as fallback.

For a licensed GraphDB runtime, set `GRAPHDB_LICENSE_ID` and the secret-free
`GRAPHDB_LICENSE_DIGEST` before building the QL/RL identity. The digest is
recorded as provenance; license contents are never read or copied into a
benchmark artifact. Without it, the GraphDB component remains
`not_available` even if a runtime probe happens to start.

Build profile identities from the frozen dataset package and official query
manifest before running a materializer. The identity commands record exact
component, configuration, dataset, query workload, and resource provenance;
they do not create answer keys.

After profile materialization runs, assemble and verify the stack:

```bash
python3 profile_stack_assembly.py assemble \
  --dataset-manifest /path/to/dataset-manifest.json \
  --query-manifest /path/to/query-staged/manifest.tsv \
  --output-dir /path/to/stack \
  --profile-identity DL=/path/to/dl-identity.json \
  --profile-identity EL=/path/to/el-identity.json \
  --profile-identity QL=/path/to/ql-identity.json \
  --profile-identity RL=/path/to/rl-identity.json \
  --profile-materialization DL=/path/to/dl-materialization.json \
  --profile-materialization EL=/path/to/el-materialization.json \
  --profile-materialization QL=/path/to/ql-materialization.json \
  --profile-materialization RL=/path/to/rl-materialization.json
```

The assembler writes separate component/materialization manifests and invokes
`verify-stack`. A profile may be structurally complete while its query result
remains `oracle_unavailable`; the four-profile correctness gate is still
`DL AND EL AND QL AND RL`.

For an execution-only query smoke that consumes this handoff, set
`OWL2BENCH_QUERY_EXECUTION_ONLY=1` when invoking `run.sh`. The result contract
then requires a ready read-only handoff and omits the `oracle` object entirely;
a successful query remains `oracle_unavailable` until an independently
accepted oracle is supplied. The normal oracle-aware query path remains the
default when the flag is `0`.

### DL HermiT materialization

The DL-specific materializer uses an explicitly supplied HermiT jar and
configuration; no reasoner artifact is bundled or discovered implicitly. It
first renders the original RDF/XML ontology to an immutable source N-Quads
graph, then runs the HermiT/OWLAPI exporter over that same graph and the
profile's query directory:

```bash
python3 benchmarks/external/owl2bench/query/hermit_dl_materialization.py run \
  --ontology /path/to/generated-OWL2DL-1.owl \
  --query-dir /path/to/query-staged \
  --query-manifest /path/to/query-staged/manifest.tsv \
  --hermit-jar /path/to/hermit.jar \
  --hermit-classpath '/path/to/hermit-dependencies/*' \
  --config /path/to/hermit-dl.properties \
  --query-backend /path/to/read-only-query-backend \
  --query-backend-manifest /path/to/query-backend-identity.json \
  --query-backend-name backend \
  --query-backend-version 1.0.0 \
  --query-backend-revision backend-revision \
  --profile-context /path/to/profile-context/DL \
  --profile-tbox /path/to/UNIV-BENCH-OWL2DL.owl \
  --profile-tbox-revision <tbox-revision> \
  --profile-abox /path/to/generated-OWL2DL-1.owl \
  --profile-abox-revision <abox-revision> \
  --generator-artifact /path/to/OWL2Bench.jar \
  --generator-revision <generator-revision> \
  --generator-config /path/to/config.properties \
  --generator-config-revision <generator-config-revision> \
  --generator-seed 1 \
  --university-count 1 \
  --dataset-id owl2bench-dl-seed1-u1 \
  --dataset-artifact /path/to/generated-OWL2DL-1.owl \
  --dataset-artifact-revision <dataset-artifact-revision> \
  --hermit-revision <hermit-revision> \
  --output-dir /path/to/hermit-dl-materialization
```

The configuration must declare `engine.id=HermiT`, an exact
`hermit.version`, and `materialization.mode=finite_query_complete`. The run
records source-document, generator, query-workload, component, runtime,
configuration, exporter implementation, profile-context, separate TBox/ABox,
generator configuration, generated dataset artifact, backend, and graph hash
identities. The dataset artifact must be the exact ontology consumed by HermiT.
It also emits an
obligation graph/manifest for the query-scoped entailments; the independent
validator derives structural finite-domain obligations and requires every
obligation quad to be present in the candidate graph with matching identity
and hash. The production path reruns source export and materialization in an
isolated directory and blocks the handoff if the canonical hashes differ.
HermiT-derived anonymous tableau witnesses are never exported. Missing
artifacts remain `not_available`; exporter capability exit 123 is
`unsupported`; malformed or incomplete graphs are `failed`; and a deadline is
`timeout`. A complete run only proves an execution/materialization handoff; it
does not create or approve an answer key.

### DL HermiT profile adapter

`hermit_dl_engine.sh` is the DL-only adapter for the split stack. It refuses
non-DL context directories, requires the DL component and complete
materialization records, probes the manifest-pinned HermiT executable, and
requires a separately pinned query backend through `HERMIT_DL_QUERY_BIN`.
HermiT itself is not treated as a SPARQL engine: the backend interface is
`query <query-file> <materialized-rdf-graph>`. The adapter writes its
secret-free provenance under `<DL-context>/.hermit-dl/provenance.json`; the
shared runner retains raw output, diagnostics, canonical results, and timings.

Required runtime inputs are `HERMIT_DL_BIN`, `HERMIT_DL_CONFIG`,
`HERMIT_DL_QUERY_BIN`, `OWL2BENCH_QUERY_COMPONENT_MANIFEST`,
`OWL2BENCH_QUERY_MATERIALIZATION_MANIFEST`, and the official query manifest.
Missing or changed executable/configuration/backend/graph artifacts return
`not_available`; query protocol or process failures remain `failed`, while a
declared capability gap remains `unsupported`. The adapter passes the staged
query file unchanged to the query backend; any backend-specific prefix/config
requirement must be part of that pinned backend contract. Use
`hermit-dl-capabilities.json` as the DL capability declaration. The adapter is
execution evidence only and does not create or approve an OWL2Bench oracle.

### ELK profile adapter

`elk_el_engine.sh` uses the same profile-qualified adapter core with defaults
for `profile=EL`, `engine=ELK`, and `.elk-el` provenance. Its capability
declaration is `elk-el-capabilities.json`; the runner therefore executes only
the 10 EL cases through this path and leaves DL, QL, and RL outside its
profile coverage. Exact ELK, query-backend, configuration, runtime,
materialization, dataset, and context identities remain manifest inputs, and a
successful run without an accepted oracle remains `oracle_unavailable`.

### QL ruleset profile adapter

`ql_ruleset_engine.sh` is the QL-only ruleset-backed adapter. It defaults to
`profile=QL`, `engine=QL-ruleset-engine`, and `.ql-ruleset` provenance, and
uses `QL_RULESET_BIN`, `QL_RULESET_CONFIG`, and `QL_RULESET_QUERY_BIN` for the
manifest-pinned runtime. Its capability declaration is
`ql-ruleset-capabilities.json`; missing credentials/runtime/configuration are
`not_available`, a declared QL capability gap is `unsupported`, and known
ruleset conformance limitations remain execution evidence rather than a
correctness pass.

### RL ruleset profile adapter

`rl_ruleset_engine.sh` is the RL-only ruleset-backed adapter. It defaults to
`profile=RL`, `engine=RL-ruleset-engine`, and `.rl-ruleset` provenance, with
`RL_RULESET_BIN`, `RL_RULESET_CONFIG`, and `RL_RULESET_QUERY_BIN` supplying the
manifest-pinned runtime. Its capability declaration is
`rl-ruleset-capabilities.json`; missing credentials/runtime/configuration are
`not_available`, declared capability gaps are `unsupported`, and completed
execution without an accepted oracle remains `oracle_unavailable`.

### SPACL RL engine adapter

`spacl_rl_engine.sh` puts the project's own reasoner in the lane. `load`
materializes the RL profile context's ontology with
`owl2-reasoner materialize` (override the binary with `SPACL_RL_REASONER_BIN`)
and writes a materialization handoff plus `.spacl-rl/provenance.json` recording
the reasoner executable hash, the source ontology hash, the pinned Oxigraph
image digest, and the materialization stats. `query` re-validates the handoff
graph hash and delegates to the pinned materialized-graph Oxigraph backend,
which injects the `:` namespace prefix the official query text assumes.
`spacl-rl-capabilities.json` declares `profiles: ["RL"]` in directory mode, so
other profiles fail closed. The docker-free contract tests use a fake reasoner
and a fake query backend (`test_spacl_rl_engine.sh`); set
`SPACL_RL_TEST_DOCKER=1` to exercise the real pinned backend.

### One 60-case split-profile runner

`split_profile_engine.sh` is the query-side multiplexer for a complete
profile-qualified campaign. It routes `DL`, `EL`, `QL`, and `RL` contexts to
their own adapters (`OWL2BENCH_SPLIT_DL_ENGINE`,
`OWL2BENCH_SPLIT_EL_ENGINE`, `OWL2BENCH_SPLIT_QL_ENGINE`, and
`OWL2BENCH_SPLIT_RL_ENGINE`). `split-profile-capabilities.json` declares the
four routing identities and their separate provenance locations. An unknown
or missing profile adapter is `not_available`; there is no cross-profile
fallback. The existing runner then expands exactly `22 + 10 + 10 + 18 = 60`
cases, preserves four profile reports, and computes the query gate only as
`DL AND EL AND QL AND RL`.

### SPACL split-stack declaration (ADR 0027)

The SPACL stack verdict satisfies the campaign's owl2bench-query
split-stack contract by declaration. `spacl_stack_manifest.py build`
produces the component and materialization manifests from provenance that
already exists — each profile context's `.spacl-<profile>/provenance.json`
(adapter, reasoner binary digest, source ontology hash, pinned Oxigraph
image digest) and `handoff.json` (materialized graph hash), the dataset
anchor metadata, and the pinned capabilities file. No pin is invented; a
stale handoff, a drifted reasoner binary, or a mismatched dataset anchor
fails the build. `spacl-stack-capabilities.json` additionally declares
`adapter_managed_materialization`: the stack's directory adapter owns its
materialization handoff (load materializes and writes the graph plus a ready
handoff; query re-validates the graph hash before delegating), so the runner
verifies that handoff per profile instead of requiring a canonical export
graph contract. The manifest validator, the split-stack cross-contract
check, and the profile result contract still validate every record.

`spacl_stack_verdict.sh` re-runs the verdict end to end with the same
inputs and pins as the accepted run: it verifies the dataset anchors,
loads each profile context, builds and cross-verifies the two manifests,
runs `run.sh` in split-stack mode against the shipped derived oracle, and
fail-closes on any nondeterministic re-materialization, any profile gate
that is not passed, any oracle match that is not accepted, or any change in
the per-profile result-status counts or canonical oracle hashes against the
prior accepted run. The accepted oracle records its identity under
canonicalizer version `/1`, so the verdict pins
`canonicalize_result.v1.py` (the byte-frozen `/1` canonicalizer from
before the `/2` normalization change) via `OWL2BENCH_QUERY_CANONICALIZER`;
the wrapper refuses to start when that pin does not match the oracle
manifest. `test_spacl_stack_manifest.sh` covers the builder, the
validators, the declaration emission through a full fixture run of the real
adapters, and the fail-closed negatives.

### Profile result contract

Every query run validates its result records with
`profile_result_contract.py` before report generation and writes the validation
evidence under `provenance/profile_result_contract.json`. The contract keeps
raw output, diagnostics, materialization, canonical result, timing,
execution status, semantic result, correctness status, oracle status, and
profile provenance as separate fields. Status precedence is fail-closed:
`unsupported`, `not_available`, `failed`, `timeout`, `oracle_unavailable`,
`wrong`, and `passed` cannot be substituted for one another. A completed
result without an accepted official/audited oracle remains
`oracle_unavailable`; differential evidence never changes that status. Invalid
profile identity, canonical/oracle mismatch, or locally supplied answer-key
fields fail contract validation without repairing the result.

### Oracle metadata gate

An answer artifact is accepted only through
`OWL2BENCH_QUERY_ORACLE_MANIFEST` (or `ORACLE_DIR/manifest.json`). Each
accepted row must identify the exact `case_id`, `query_id`, profile, source
query SHA-256, source corpus/revision, dataset ID/SHA-256, canonicalizer
version, answer-artifact SHA-256, canonical result hash, and oracle provenance.
Dataset metadata is keyed by profile, for example
`{"EL":{"dataset_id":"anchor-el","dataset_sha256":"..."}}`.
The provenance artifact must exist under the oracle root and match its recorded
SHA-256. Derived rows additionally require existing proof and independent-audit
artifacts with matching SHA-256 values.
Rows with `oracle_class: official` take precedence over `derived`; derived
rows additionally require an independently audited deterministic,
sound/complete proof record. Missing, stale, malformed, unsafe, or mismatched
metadata remains `oracle_unavailable`. Differential/engine-agreement rows are
retained under `differential_evidence` and can never produce `passed`.

## Konclude unified adapter

The selected license-free `konclude-unified` adapter uses Konclude
`v0.7.0-1138` through its `sparqlfile` interface. Its pinned capabilities and
provenance fields are declared in
`konclude-unified-capabilities.json`.

The default `konclude-unified` variant verifies the runtime version and the
binary/configuration/converter SHA-256 values against those pins. It declares
one processing thread and a 24 GiB comparable-run memory envelope; set
`KONCLUDE_UNIFIED_THREADS` and `KONCLUDE_UNIFIED_MEMORY_LIMIT` only when the
run is intentionally given different provenance.

```bash
KONCLUDE_UNIFIED_BIN=/path/to/Konclude \
KONCLUDE_UNIFIED_CONFIG=/path/to/Configs/querying-config.xml \
KONCLUDE_UNIFIED_CONVERTER=/path/to/pinned-converter-wrapper \
OWL2BENCH_QUERY_ENGINE_BIN=benchmarks/external/owl2bench/query/konclude_unified_engine.sh \
OWL2BENCH_QUERY_CAPABILITIES_FILE=benchmarks/external/owl2bench/query/konclude-unified-capabilities.json \
OWL2BENCH_QUERY_DATA_DIR=/path/to/generated-data \
benchmarks/external/owl2bench/query/run.sh
```

Each profile context must contain one unambiguous native Functional-Style
input (`.ofn`, `.omn`, `.fss`, or `.functional`) or one RDF/XML input. For
RDF/XML, the converter must implement the adapter contract
`converter <source> <destination>`; the converted file is kept under the
context's `.konclude-unified/` state directory. Source and converted hashes,
engine/configuration hashes, and the profile context identity are written to
`provenance.json` without changing the official query/source identity.

The adapter prepends only the approved RDF and OWL2Bench declarations to a
temporary query file. Konclude result XML is emitted on stdout for the shared
canonicalizer; diagnostics remain on stderr. Missing binary, config,
converter, runtime, or input returns protocol exit `125`, which the runner
records as `not_available`; process/output errors remain `failed` or `timeout`.
Adapter state and conversion provenance stay under each profile context's
`.konclude-unified/` directory so the runner can retain the actual provenance
path in every result.

## Real 60-case smoke campaign

For the production-shaped execution check, use the pinned Konclude adapter
with the official staged manifest and one generated seed-1/one-university
dataset directory per profile:

```bash
OWL2BENCH_QUERY_STAGED_DIR=/path/to/query-staged \
OWL2BENCH_QUERY_DATA_DIR=/path/to/generated-data \
OWL2BENCH_QUERY_COMPONENT_MANIFEST=/path/to/split-stack-component-manifest.json \
OWL2BENCH_QUERY_MATERIALIZATION_MANIFEST=/path/to/materialization-manifest.json \
KONCLUDE_UNIFIED_BIN=/path/to/Konclude \
KONCLUDE_UNIFIED_CONFIG=/path/to/Configs/querying-config.xml \
KONCLUDE_UNIFIED_CONVERTER=/path/to/convert.jar \
KONCLUDE_UNIFIED_RUNTIME_ARTIFACT=/path/to/runtime-package.deb \
LD_LIBRARY_PATH=/path/to/libpcre3/lib/x86_64-linux-gnu \
OWL2BENCH_SMOKE_GENERATOR_DIR=/path/to/owl2bench-generator \
OWL2BENCH_SMOKE_RESULTS_ROOT=/path/to/query-results \
benchmarks/external/owl2bench/query/run_real_smoke.sh
```

The smoke command forces split-stack mode and passes these two manifests to
both runs. If either manifest is absent or incomplete, the runner records the
affected cases as unavailable/failed and the smoke gate remains blocked.
Completed cases without an accepted oracle remain execution evidence only.

The command runs the exact 60-case matrix twice, writes run directories
`<run-id>_r1` and `<run-id>_r2`, and records a `<run-id>_smoke/summary.json`
and `report.md`. It captures the Konclude release/configuration/converter,
runtime library path, generator artifacts, query-manifest hash, per-profile
dataset provenance, raw output/diagnostic/canonical artifacts, timing fields,
and canonical-hash stability. It deliberately removes oracle environment
variables from both runs: completed cases therefore remain
`oracle_unavailable`, the query and campaign gates remain blocked, and no
Stardog historical-reproduction or official-oracle claim is emitted. A
successful command means the execution/stability smoke gate passed; it does
not mean semantic correctness passed.
