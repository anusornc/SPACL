# Comparative Repetition Median Wall Time (ms)

- Pass runs: `owl2bench_cmp_p1_el_u20`, `owl2bench_cmp_p2_el_u20`, `owl2bench_cmp_p3_el_u20`
- Passes aggregated: 3
- Suite: `standard`
- Operation: `consistency`
- Statistic: per-case median of wall time in milliseconds across passes (container-level; parse + reasoning + startup; recorded passes only)

| Ontology | elk | hermit | konclude | tableauxx |
|---|---:|---:|---:|---:|
| UNIV-BENCH-OWL2EL-seed1-universities20.owl | **26646** | 51249 | 85183 | TO |

Legend: **best median wall time among engines that completed**, TO=timeout, NA=not available, NC=non-comparable parser/runtime incompatibility, KIL=interrupted cleanup kill, ORPH=container orphan cleanup, FAIL=hard runtime failure.
