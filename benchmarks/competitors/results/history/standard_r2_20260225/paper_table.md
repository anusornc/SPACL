# Head-to-Head Wall Time (ms)

- Run ID: `standard_r2_20260225`
- Suite: `standard`
- Operation: `consistency`
- Metric: wall time in milliseconds (container-level; parse + reasoning + startup)

| Ontology | tableauxx | hermit | konclude | openllet | elk | jfact | pellet |
|---|---:|---:|---:|---:|---:|---:|---:|
| disjunctive_simple.owl | **2337** | 3783 | FAIL | 4049 | 3920 | 4309 | 3592 |
| disjunctive_test.owl | **2318** | 3917 | FAIL | 2941 | 2979 | 3572 | 2659 |
| hierarchy_100.owl | **2096** | 3238 | FAIL | 3101 | 2816 | 3344 | 2730 |
| hierarchy_1000.owl | **2190** | 3341 | FAIL | 3220 | 3055 | 4318 | 3682 |
| hierarchy_10000.owl | 8290 | 4131 | FAIL | 3796 | **3507** | FAIL | 3727 |
| hierarchy_100000.owl | 40273 | 8038 | FAIL | 8862 | **5816** | FAIL | 10867 |
| univ-bench.owl | 2951 | 3345 | FAIL | 2930 | 3065 | 3093 | **2827** |

Legend: **best wall time**, TO=timeout, NA=not available, KIL=interrupted cleanup kill, ORPH=container orphan cleanup, FAIL=runtime/parser failure.
