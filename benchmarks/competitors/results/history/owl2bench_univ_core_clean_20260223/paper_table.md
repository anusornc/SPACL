# Head-to-Head Wall Time (ms)

- Run ID: `owl2bench_univ_core_clean_20260223`
- Suite: `standard`
- Operation: `consistency`
- Metric: wall time in milliseconds (container-level; parse + reasoning + startup)

| Ontology | tableauxx | hermit | konclude | openllet | elk | jfact | pellet |
|---|---:|---:|---:|---:|---:|---:|---:|
| 0001_UNIV-BENCH-OWL2DL.owl | **2169** | 3703 | FAIL | 3142 | 3012 | TO | 2957 |
| 0002_UNIV-BENCH-OWL2EL.owl | **2064** | 3545 | FAIL | 3200 | 2890 | TO | 2818 |
| 0003_UNIV-BENCH-OWL2QL.owl | **2076** | 3314 | FAIL | 3261 | 2820 | 3308 | 2866 |
| 0004_UNIV-BENCH-OWL2RL.owl | **2070** | 3566 | FAIL | 3157 | 2954 | TO | 2864 |

Legend: **best wall time**, TO=timeout, NA=not available, KIL=interrupted cleanup kill, ORPH=container orphan cleanup, FAIL=runtime/parser failure.
