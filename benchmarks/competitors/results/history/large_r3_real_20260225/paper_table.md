# Head-to-Head Wall Time (ms)

- Run ID: `large_r3_real_20260225`
- Suite: `large`
- Operation: `consistency`
- Metric: wall time in milliseconds (container-level; parse + reasoning + startup)

| Ontology | tableauxx | hermit | konclude | openllet | elk | jfact | pellet |
|---|---:|---:|---:|---:|---:|---:|---:|
| doid.owl | **3701** | 19180 | FAIL | 14336 | 13406 | 60112 | 28771 |
| go-basic.owl | **7059** | 18039 | FAIL | 17747 | 15976 | 41847 | 82906 |
| uberon.owl | **6548** | 110560 | FAIL | TO | 15594 | FAIL | TO |

Legend: **best wall time**, TO=timeout, NA=not available, KIL=interrupted cleanup kill, ORPH=container orphan cleanup, FAIL=runtime/parser failure.
