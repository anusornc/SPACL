# Stage Benchmark Summary

- Run set: `stages_chebi_20260301_091935`
- Ontology: `chebi.owl`
- Suite: `large`
- Bin-cache profiling: `0`
- Diagnostic repeats: `3`

## Results (ms)

| Mode | Value (ms) | Notes |
|---|---:|---|
| E2E cold wall | 36472 | Full wall time (cold text path) |
| Parse-only (stage) | 23740 | Parse stage extracted from E2E cold |
| Reason-only stage | 243 | Reason stage extracted from the same E2E cold run |
| Bin-cache wall median | -1 | Diagnostic wall median from optional bin-only runs |
| Bin-cache load median | -1 | Diagnostic load-stage median from optional bin-only runs |

## Raw Run IDs

- E2E cold: `stages_chebi_20260301_091935_e2e_cold`
- Bin-cache mode: `0`
