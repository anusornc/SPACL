# OWL2Bench Core 3-Run Aggregate (Clean)

Runs: owl2bench_univ_core_clean_20260223, owl2bench_univ_core_clean_20260223_r2, owl2bench_univ_core_clean_20260223_r3

## Reasoner Summary

| Reasoner | Success | Total | Success Rate | Median Wall (ms, success only) | P95 Wall (ms, success only) |
|---|---:|---:|---:|---:|---:|
| tableauxx | 12 | 12 | 100.0% | 2107.5 | 2707 |
| pellet | 12 | 12 | 100.0% | 2886.0 | 3135 |
| elk | 12 | 12 | 100.0% | 2999.0 | 3370 |
| openllet | 12 | 12 | 100.0% | 3204.5 | 3836 |
| hermit | 12 | 12 | 100.0% | 3573.0 | 4505 |
| jfact | 3 | 12 | 25.0% | 3507 | 3762 |
| konclude | 0 | 12 | 0.0% | NA | NA |

## Ontology Winners by Median (requires 3/3 success)

| Ontology | Winner | Median Wall (ms) |
|---|---|---:|
| 0001_UNIV-BENCH-OWL2DL.owl | tableauxx | 2169 |
| 0002_UNIV-BENCH-OWL2EL.owl | tableauxx | 2064 |
| 0003_UNIV-BENCH-OWL2QL.owl | tableauxx | 2509 |
| 0004_UNIV-BENCH-OWL2RL.owl | tableauxx | 2070 |
