# Head-to-Head Wall Time (ms)

- Run ID: `standard_r3_20260225`
- Suite: `standard`
- Operation: `consistency`
- Metric: wall time in milliseconds (container-level; parse + reasoning + startup)

| Ontology | tableauxx | hermit | konclude | openllet | elk | jfact | pellet |
|---|---:|---:|---:|---:|---:|---:|---:|
| disjunctive_simple.owl | **2273** | 3275 | FAIL | 2811 | 2695 | 3466 | 3310 |
| disjunctive_test.owl | **2113** | 3323 | FAIL | 3020 | 2923 | 3189 | 2627 |
| hierarchy_100.owl | **2032** | 3391 | FAIL | 2849 | 2985 | 3178 | 2838 |
| hierarchy_1000.owl | **2083** | 3498 | FAIL | 3204 | 3025 | 4110 | 3222 |
| hierarchy_10000.owl | 7903 | 4275 | FAIL | 4383 | **3579** | FAIL | 3644 |
| hierarchy_100000.owl | 39494 | 7636 | FAIL | 8967 | **5687** | FAIL | 10648 |
| univ-bench.owl | 3096 | 3345 | FAIL | 2908 | 2717 | 3196 | **2492** |

Legend: **best wall time**, TO=timeout, NA=not available, KIL=interrupted cleanup kill, ORPH=container orphan cleanup, FAIL=runtime/parser failure.
