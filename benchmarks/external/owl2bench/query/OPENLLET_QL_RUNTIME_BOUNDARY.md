# Openllet QL Runtime Boundary

`openllet_ql_*` files define the production runtime identity for the Openllet
QL benchmark variant. The runtime is credential-free, benchmark-only, and not a
published service artifact.

- Build resolution uses Docker build `--network host` only to let Maven resolve
  the pinned artifact graph.
- Runtime execution uses Docker `--network none`.
- The Dockerfile has no `EXPOSE` instruction and no reusable endpoint.
- The Maven base image is digest-pinned.
- The Openllet 2.6.5 / OWLAPI 5.5.1 classpath is locked by 78 individual jar
  SHA-256 values.
- Local image archives are operator-local evidence only and must not be
  committed or published.
- Openllet is open source and credential-free; AGPL distribution review remains
  pending before any redistribution.
- `openllet_ql_license_identity.json` records the available AGPL-3.0-only
  identity separately from the still-pending distribution review. Runtime
  execution does not require a vendor license, but redistribution approval is
  never reported as complete.

## Materialization and handoff

`openllet_ql_materialization.py run` binds this runtime to the existing QL
profile materialization contract. It requires the frozen QL profile package
and the official staged query manifest, live-attests the exact local image,
runs the exporter twice with networking disabled, and delegates canonical
graph validation plus read-only handoff creation to the shared materialization
pipeline.

`openllet_ql_acceptance_lock.json` is mandatory and has no CLI or environment
override. It pins the accepted profile identity, source, generated dataset,
TBox, ABox, generator/configuration, query manifest, and all 10 QL query
hashes. A mismatch is `not_available` before Docker execution. Changing the
lock requires a new materialization revision and fresh diagnostic plus
10-query evidence.

```bash
python3 benchmarks/external/owl2bench/query/openllet_ql_materialization.py run \
  --dataset-root /path/to/frozen-profile-datasets \
  --query-manifest /path/to/query-staged/manifest.tsv \
  --output-dir /path/to/openllet-ql-materialization
```

The output contains:

- `runtime/attestation.json` and the local image inspection used to prove the
  exact pinned runtime;
- `component-manifest.json`, whose QL execution variant is `openllet-ql` and
  whose derivation boundary is “Openllet Direct Semantics over OWL 2 QL
  input”;
- `official/`, containing the two-run canonical graph, independent validation,
  and read-only handoff for the unchanged frozen QL source; and
- `diagnostic/`, containing the separately hashed accepted extension, its own
  materialization and validation, and non-oracle class/object-property query
  observations.

`openllet_ql_engine.sh` connects those generated manifests to the existing
split-profile adapter core. Select it only with
`OWL2BENCH_SPLIT_QL_VARIANT=openllet-ql`, set
`OWL2BENCH_OPENLLET_QL_ENGINE` to that script, and use
`split-openllet-ql-capabilities.json`. The adapter records
`.openllet-ql/provenance.json`; `openllet_ql_query_engine.sh` then accepts only
the exact graph in the read-only handoff. No GraphDB or generic QL fallback is
defined.

The adapter emits only source terms and merges the complete source into every
candidate graph. A query-relevant existential dependency that has no finite
source-domain value is `not_available`; generated anonymous witnesses are
never exported or skolemized. Missing runtime evidence is `not_available`, a
deadline is `timeout`, and malformed, nondeterministic, or hash-mismatched
evidence is `failed`.

Successful official execution remains `oracle_unavailable`; the diagnostic is
regression evidence only. Neither artifact can pass QL correctness, unblock
the four-profile gate, or authorize a performance claim. The command never
creates a Docker archive. AGPL review and a matching second-host rebuild remain
release gates.

## Variant-control status namespace

Variant-control reports use release-gate states, not query result statuses.
Public fields such as implementation completeness, release readiness,
execution availability, and semantic correctness use `complete`, `blocked`,
`not_available`, or `failed`. These values describe pre-release control state
only. They must not be interpreted as query result vocabulary such as
`passed`, `oracle_unavailable`, `timeout`, or `success`.

`openllet_ql_runtime_identity.json` declares the immutable variant revision in
`variant_control.revision_id` and `variant_control.category_sha256`. The
validator recomputes the category SHA from the engine, base image, image,
dependency identities, classpath, exporter, configuration, resource envelope, and graph contract
stored in the identity. Any identity edit must declare a distinct revision id
and the newly computed category SHA, register that pair in
`openllet_ql_variant_revision_lock.json`, then rerun diagnostic, materialization,
all 10 official QL query artifacts, and profile-result-contract validation
before core evidence can become `complete`.

The variant selector consumes only immutable runner-produced artifacts:
runtime attestation, component manifest, diagnostic output, materialization
summary, the profile-result contract, and all 10 official QL result records.
It derives the official-status digest from those records. Report evidence uses
a two-pass fixed point: a preflight validation with complete core evidence
provides stable projected statuses; `report.sh` renders a report containing
those statuses plus the revision id and category SHA; final validation then
hash-checks that report. Re-rendering from final validation must preserve the
same report hash. This keeps report evidence mandatory without making the
validator hash depend recursively on itself.

## Accepted evidence lineage

The production bridge selectively carries forward the decisions from issues
`#67`, `#68`, and parent spec `#69`; it does not copy prototype orchestration or
reports. The accepted prototype inputs were commits
`d5b76ace04949add3dba1606ad6491c04a465325` and
`f1d98d9bb448dc530fe980efc73c0ddfd03f9714`, with portable image
`sha256:71437ce1c3098651013cf982b1ca27f7b793eb8107b908364218c61677a2d625`
and diagnostic graph
`sha256:6e25b500c61b710962e420079b38e6542024e881a95810fd5e12ac04ac765928`.
The promoted diagnostic extension itself is locked at
`sha256:76d45d5e66810fe2484e7f3250f82ec7205a2c62562c58ea11a9332682acd849`.
Runtime issue `#71` supersedes the prototype image with the source-controlled
production identity; old prototype results are evidence lineage, not reusable
production results.
