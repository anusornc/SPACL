# Head-to-Head Wall Time (ms)

- Run ID: `large_r2_20260225`
- Suite: `large`
- Operation: `consistency`
- Metric: wall time in milliseconds (container-level; parse + reasoning + startup)

| Ontology | tableauxx | hermit | konclude | openllet | elk | jfact | pellet |
|---|---:|---:|---:|---:|---:|---:|---:|
| doid.owl | **3412** | 20295 | FAIL | 14266 | 13534 | 60443 | 28896 |
| go-basic.owl | **8000** | 17616 | FAIL | 17697 | 16456 | 41438 | 83238 |
| uberon.owl | **7363** | 108729 | FAIL | TO | 15318 | FAIL | TO |

Legend: **best wall time**, TO=timeout, NA=not available, KIL=interrupted cleanup kill, ORPH=container orphan cleanup, FAIL=runtime/parser failure.
