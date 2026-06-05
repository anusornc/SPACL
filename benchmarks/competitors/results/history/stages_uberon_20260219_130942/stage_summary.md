# Stage Benchmark Summary

- Run set: `stages_uberon_20260219_130942`
- Ontology: `uberon.owl`
- Suite: `large`
- Warm repeats: `1`

## Results (ms)

| Mode | Value (ms) | Notes |
|---|---:|---|
| E2E cold wall | 6632 | Full wall time (cold text path) |
| Parse-only (stage) | 2952 | Parse stage extracted from E2E cold |
| Reason-only warm (median stage) | 23 | Reason stage median from bin-only warm runs |
| Warm wall median | 114513 | Wall median in warm bin-only runs |
| Warm parse median | 111201 | Parse stage median in warm bin-only runs |

## Raw Run IDs

- Warm-up: `stages_uberon_20260219_130942_warmup`
- E2E cold: `stages_uberon_20260219_130942_e2e_cold`
- Warm mode bin-only: `1`
- Warm run 1: `stages_uberon_20260219_130942_warm_1`
