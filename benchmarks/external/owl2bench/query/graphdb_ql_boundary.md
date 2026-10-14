# GraphDB QL Materialization Boundary

This variant is a current GraphDB QL execution boundary for OWL2Bench. It does
not use a historical GraphDB benchmark path and does not create or approve
answer keys.

- Image: `ontotext/graphdb:11.5.0`
- Platform: `linux/amd64`
- Digest: `sha256:d688879c4e5751a0c323f996d86bf1b2189191290711b1936e211d4510b53485`
- Runtime reference:
  `ontotext/graphdb@sha256:d688879c4e5751a0c323f996d86bf1b2189191290711b1936e211d4510b53485`
- Repository ruleset: `owl2-ql`
- Exporter contract: `export <source.nq>` builds the repository offline with
  `importrdf load` (the free runtime gates the repository-creation REST
  endpoint behind a license while the offline import tool and the statements
  export are served openly — the route the RL variant proved end to end),
  then boots the server against that data directory and writes RDF4J
  `/repositories/{repository}/statements?includeInferred=true` as
  `application/n-quads` to stdout.

The profile identity produced by `graphdb_ql_materializer.py identity` is
compatible with `profile_materialization.py run --profile QL`. The materializer
component remains the existing QL seam identity, `QL-ruleset-engine`, so the
shared validator and verify-stack contracts do not need to change. GraphDB
version, image digest, repository ID, generated repository config hash,
ruleset, dataset freezer identity, query workload, and resource envelope are
recorded in the identity manifest.

GraphDB documentation records an OWL2-QL conformance limitation for its
predefined ruleset. A complete export is therefore execution and differential
evidence only. The profile summary remains `oracle_unavailable` until a
separately accepted oracle exists.

Runtime absence is truthful:

- missing Docker executable: `not_available` (`125`)
- missing pinned image without `GRAPHDB_QL_ALLOW_PULL=1`: `not_available`
- container/license startup failure: `not_available`
- unsupported product capability: `unsupported` (`123`) if declared by the
  exporter boundary
- malformed, empty, or nondeterministic graph export: `failed`

The generated anonymous-witness policy is `not_exported`; the GraphDB QL
boundary cannot enumerate required anonymous witnesses. If such a required
witness family is observed, the export must not be marked complete.
