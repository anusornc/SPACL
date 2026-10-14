# Comparative Repetition Median Wall Time (ms)

- Pass runs: `owl2bench_cmp_p1_rl_u5`, `owl2bench_cmp_p2_rl_u5`, `owl2bench_cmp_p3_rl_u5`
- Passes aggregated: 3
- Suite: `standard`
- Operation: `consistency`
- Statistic: per-case median of wall time in milliseconds across passes (container-level; parse + reasoning + startup; recorded passes only)

| Ontology | hermit | konclude | tableauxx |
|---|---:|---:|---:|
| UNIV-BENCH-OWL2RL-seed1-universities5.owl | **16732** | 23302 | 20802 |

Legend: **best median wall time among engines that completed**, TO=timeout, NA=not available, NC=non-comparable parser/runtime incompatibility, KIL=interrupted cleanup kill, ORPH=container orphan cleanup, FAIL=hard runtime failure.
