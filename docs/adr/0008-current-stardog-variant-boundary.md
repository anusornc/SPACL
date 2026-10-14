# Current Stardog variant boundary for OWL2Bench

Status: accepted

The currently obtainable Stardog release is accepted only as a separately
named current-stardog execution variant. The historical OWL2Bench Stardog
7.0.2 reproduction remains historical-unavailable. Current Stardog may cover
EL, QL, and RL with their named reasoning modes, but it must not use SL as a
DL-equivalent mode; DL remains a separate query-path decision. The current
variant is pinned by exact release, image/archive digest, server/CLI version,
architecture, configuration, dataset, query, and resource provenance, and it
cannot pass correctness without an accepted official or audited derived oracle.

## Considered Options

- Replacing the historical claim with the current release was rejected because
  a newer runtime is not the same benchmark environment.
- Mapping DL to the current SL mode was rejected because SL combines RDFS, QL,
  RL, and EL reasoning and is not the historical DL contract.
- Dropping DL rows was rejected because the official query workload and
  profile-case matrix must remain complete.

## Consequences

The current-stardog variant can report EL/QL/RL execution and timing evidence
while keeping DL explicitly unsupported until a separate query-capable DL path
is selected. Provisioning uses the current official Stardog installation
route and an immutable artifact pin; historical and current results remain
separate in reports and claims.
