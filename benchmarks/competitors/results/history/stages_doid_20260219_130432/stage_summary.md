# Stage Benchmark Summary

- Run set: `stages_doid_20260219_130432`
- Ontology: `doid.owl`
- Suite: `large`
- Warm repeats: `1`

## Results (ms)

| Mode | Value (ms) | Notes |
|---|---:|---|
| E2E cold wall | 4073 | Full wall time (cold text path) |
| Parse-only (stage) | 830 | Parse stage extracted from E2E cold |
| Reason-only warm (median stage) | 9 | Reason stage median from bin-only warm runs |
| Warm wall median | 26686 | Wall median in warm bin-only runs |
| Warm parse median | 23967 | Parse stage median in warm bin-only runs |

## Raw Run IDs

- Warm-up: `stages_doid_20260219_130432_warmup`
- E2E cold: `stages_doid_20260219_130432_e2e_cold`
- Warm mode bin-only: `1`
- Warm run 1: `stages_doid_20260219_130432_warm_1`
