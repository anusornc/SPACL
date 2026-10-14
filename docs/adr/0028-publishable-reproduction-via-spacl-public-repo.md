# Publishable reproduction via the public companion repository

Status: accepted

The reproduction leg publishes the project's claim-safe evidence through
the existing public companion repository wired as the `public` remote
(`anusornc/SPACL`), using its established curated-PR flow (June 2026
precedent: `public-repro-package`, `public-spacl-curated`). The project
structure is one private monorepo (`tableauxx`) holding all work, plus two
public companions — one per planned paper: `SPACL` for the
engine/reasoning paper, `SPACL-RRN` for the RRN policy/scheduling paper.
This leg refreshes only `SPACL`; the public repo carries only what third
parties need to verify and reproduce results, and is the artifact of
record cited by paper #1.

Form is two-tier. Tier 1 (verification): the published tables, reports,
summaries, manifests, and hash inventories, with a verification guide that
walks a third party through checking every published number's provenance.
Tier 2 (reproduction): the pinned procedures that regenerate the evidence
from official sources — corpus fetch instructions, the dataset ladder
generator run, derived-oracle regeneration, engine image builds from the
committed Dockerfiles, and the comparative sweep invocations — so a reader
may rerun end to end instead of trusting hashes.

Scope is all four evidence lanes (W3C conformance, OWL2Bench query
correctness with the accepted derived oracle, the ORE 2015 comparative
matrix, and the OWL2Bench scalability ladders) under their ADR 0024-0027
claim wording, carried verbatim. Redistribution is pointers, not blobs: no
engine binaries, no corpora, and no generated datasets are redistributed;
manifests record identities and hashes, sources are fetched from their
official pins, and engine images are rebuilt from committed Dockerfiles.

## Considered Options

- Creating a new public repository was superseded by the discovery that
  the channel already exists and is wired as the `public` remote with a
  proven curation flow; a second new repo would duplicate it.
- Making the main repository public was rejected: it carries in-progress
  private work and internal notes, and the two-paper split requires
  curated companions anyway.
- Publishing bundles as release artifacts on the private repository was
  rejected: unverifiable by third parties and awkward to cite.
- A Zenodo DOI now was deferred: once the public repository is refreshed,
  a DOI can be minted from it at paper-submission time without changing
  this decision.

## Consequences

- The main repository stays private; the companion is export-only, fed by
  a committed export/sync script so republication is a repeatable action.
- Every exported number must trace to a hash-identified artifact or a
  pinned regeneration procedure; the verification guide is part of the
  deliverable, not an afterthought.
- Claim wording travels verbatim from ADRs 0024-0027; performance lanes
  never gain correctness wording in export.
