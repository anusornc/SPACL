# Head-to-Head Wall Time (ms)

- Run ID: `owl2bench_univ_core_clean_20260223_r3`
- Suite: `standard`
- Operation: `consistency`
- Metric: wall time in milliseconds (container-level; parse + reasoning + startup)

| Ontology | tableauxx | hermit | konclude | openllet | elk | jfact | pellet |
|---|---:|---:|---:|---:|---:|---:|---:|
| 0001_UNIV-BENCH-OWL2DL.owl | **2123** | 3756 | FAIL | 3209 | 3088 | TO | 2929 |
| 0002_UNIV-BENCH-OWL2EL.owl | **1907** | 3520 | FAIL | 3228 | 2875 | TO | 2893 |
| 0003_UNIV-BENCH-OWL2QL.owl | **2707** | 3373 | FAIL | 3150 | 2916 | 3507 | 2939 |
| 0004_UNIV-BENCH-OWL2RL.owl | **2042** | 3692 | FAIL | 3149 | 3021 | TO | 2785 |

Legend: **best wall time**, TO=timeout, NA=not available, KIL=interrupted cleanup kill, ORPH=container orphan cleanup, FAIL=runtime/parser failure.
