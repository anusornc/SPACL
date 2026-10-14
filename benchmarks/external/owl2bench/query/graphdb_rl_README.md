# GraphDB RL Materialization Boundary

This variant is only the current GraphDB RL materialization/export boundary for
OWL2Bench. It is not the historical GraphDB 9.0.0 experiment and it is not an
answer oracle.

Pinned runtime:

- Image: `ontotext/graphdb@sha256:d688879c4e5751a0c323f996d86bf1b2189191290711b1936e211d4510b53485`
- Tag identity: `ontotext/graphdb:11.5.0`
- Platform: `linux/amd64`
- Repository ID: `owl2bench-graphdb-rl`
- Ruleset: `owl2-rl`

The exporter contract is compatible with `profile_materialization.py`:

```bash
benchmarks/external/owl2bench/query/graphdb_rl_materializer.sh export /path/to/source.nq
```

The command writes candidate N-Quads to stdout. The existing
`profile_materialization.py` runner owns the second export, URDNA2015
canonicalization, materialization manifest, independent validator handoff, and
`oracle_unavailable` policy.

Build an identity manifest from frozen RL dataset outputs and staged RL query
files:

```bash
python3 benchmarks/external/owl2bench/query/graphdb_rl_identity.py build \
  --profile-identity /tmp/.../profiles/RL/profile-identity.json \
  --query-dir /path/to/staged/rl-queries \
  --output /tmp/graphdb-rl-identity.json
```

If Docker, the pinned image, licensing, repository creation, import, or RDF4J
export is unavailable, the exporter exits `125` (`not_available`) or `123`
(`unsupported`) and no complete materialization is claimed. GraphDB output and
agreement with any other engine remain differential evidence only.
