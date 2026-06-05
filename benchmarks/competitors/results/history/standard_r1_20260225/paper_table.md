# Head-to-Head Wall Time (ms)

- Run ID: `standard_r1_20260225`
- Suite: `standard`
- Operation: `consistency`
- Metric: wall time in milliseconds (container-level; parse + reasoning + startup)

| Ontology | tableauxx | hermit | konclude | openllet | elk | jfact | pellet |
|---|---:|---:|---:|---:|---:|---:|---:|
| disjunctive_simple.owl | **2240** | 3446 | FAIL | 3383 | 3718 | 3350 | 3019 |
| disjunctive_test.owl | **1951** | 3245 | FAIL | 3007 | 3012 | 3362 | 2843 |
| hierarchy_100.owl | **2384** | 3442 | FAIL | 3070 | 2910 | 3271 | 2883 |
| hierarchy_1000.owl | **2330** | 3559 | FAIL | 3937 | 3188 | 3954 | 2921 |
| hierarchy_10000.owl | 8007 | 4083 | FAIL | 4084 | 3643 | FAIL | **3527** |
| hierarchy_100000.owl | 40210 | 7706 | FAIL | 8881 | **7035** | FAIL | 11485 |
| univ-bench.owl | **2282** | 3338 | FAIL | 3135 | 2866 | 3317 | 2742 |

Legend: **best wall time**, TO=timeout, NA=not available, KIL=interrupted cleanup kill, ORPH=container orphan cleanup, FAIL=runtime/parser failure.
