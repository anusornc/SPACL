# Stage Benchmark Summary

- Run set: `stages_go_basic_20260219_130710`
- Ontology: `go-basic.owl`
- Suite: `large`
- Warm repeats: `1`

## Results (ms)

| Mode | Value (ms) | Notes |
|---|---:|---|
| E2E cold wall | 7409 | Full wall time (cold text path) |
| Parse-only (stage) | 3472 | Parse stage extracted from E2E cold |
| Reason-only warm (median stage) | 40 | Reason stage median from bin-only warm runs |
| Warm wall median | 130287 | Wall median in warm bin-only runs |
| Warm parse median | 126854 | Parse stage median in warm bin-only runs |

## Raw Run IDs

- Warm-up: `stages_go_basic_20260219_130710_warmup`
- E2E cold: `stages_go_basic_20260219_130710_e2e_cold`
- Warm mode bin-only: `1`
- Warm run 1: `stages_go_basic_20260219_130710_warm_1`
