# Comparative Repetition Median Wall Time (ms)

- Pass runs: `owl2bench_cmp_p1_dl_u1`, `owl2bench_cmp_p2_dl_u1`, `owl2bench_cmp_p3_dl_u1`
- Passes aggregated: 3
- Suite: `standard`
- Operation: `consistency`
- Statistic: per-case median of wall time in milliseconds across passes (container-level; parse + reasoning + startup; recorded passes only)

| Ontology | hermit | konclude | tableauxx |
|---|---:|---:|---:|
| 0006_UNIV-BENCH-OWL2DL-seed1-u1.owl | 17779 | 10068 | **7217** |

Legend: **best median wall time among engines that completed**, TO=timeout, NA=not available, NC=non-comparable parser/runtime incompatibility, KIL=interrupted cleanup kill, ORPH=container orphan cleanup, FAIL=hard runtime failure.
