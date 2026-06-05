# Head-to-Head Wall Time (ms)

- Run ID: `owl2bench_univ_core_clean_20260223_r2`
- Suite: `standard`
- Operation: `consistency`
- Metric: wall time in milliseconds (container-level; parse + reasoning + startup)

| Ontology | tableauxx | hermit | konclude | openllet | elk | jfact | pellet |
|---|---:|---:|---:|---:|---:|---:|---:|
| 0001_UNIV-BENCH-OWL2DL.owl | **2243** | 4505 | FAIL | 3836 | 3124 | TO | 3002 |
| 0002_UNIV-BENCH-OWL2EL.owl | **2092** | 3580 | FAIL | 3479 | 3239 | TO | 2879 |
| 0003_UNIV-BENCH-OWL2QL.owl | **2509** | 3459 | FAIL | 3071 | 3370 | 3762 | 2858 |
| 0004_UNIV-BENCH-OWL2RL.owl | **2211** | 3941 | FAIL | 3420 | 2986 | TO | 3135 |

Legend: **best wall time**, TO=timeout, NA=not available, KIL=interrupted cleanup kill, ORPH=container orphan cleanup, FAIL=runtime/parser failure.
