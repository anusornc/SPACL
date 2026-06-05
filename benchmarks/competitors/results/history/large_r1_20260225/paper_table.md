# Head-to-Head Wall Time (ms)

- Run ID: `large_r1_20260225`
- Suite: `large`
- Operation: `consistency`
- Metric: wall time in milliseconds (container-level; parse + reasoning + startup)

| Ontology | tableauxx | hermit | konclude | openllet | elk | jfact | pellet |
|---|---:|---:|---:|---:|---:|---:|---:|
| doid.owl | **3447** | 20576 | FAIL | 13699 | 13357 | 61015 | 28610 |
| go-basic.owl | **7329** | 17549 | FAIL | 18847 | 15711 | 44232 | 83780 |
| uberon.owl | **6677** | 119284 | FAIL | TO | 16406 | FAIL | TO |

Legend: **best wall time**, TO=timeout, NA=not available, KIL=interrupted cleanup kill, ORPH=container orphan cleanup, FAIL=runtime/parser failure.
