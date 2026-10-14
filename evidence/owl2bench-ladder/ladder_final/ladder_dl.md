# OWL2Bench DL Consistency Scalability Ladder

- Lane: execution performance and completability evidence only (ADR 0024/0026).
- Statistic: repetition median of container-inclusive wall time per engine across three serial passes; cells show `<median>ms (completed/cases)` (ADR 0025).
- Rows: universities ascending. Columns: ADR 0025 engine set for profile DL.
- Inputs: per-dataset comparative aggregations; run ids per point are recorded in ladder_metadata.json.

| Universities | tableauxx | hermit | konclude |
|---|---:|---:|---:|
| 1 | **7217ms (1/1)** | 17779ms (1/1) | 10068ms (1/1) |
| 2 | 15976ms (1/1) | 47284ms (1/1) | **13890ms (1/1)** |
| 5 | **25295ms (1/1)** | 294092ms (1/1) | 25948ms (1/1) |
| 10 | 146176ms (1/1) | TO (0/1) | **51325ms (1/1)** |
| 20 | TO (0/1) | TO (0/1) | **96992ms (1/1)** |
| 50 | TO (0/1) | TO (0/1) | **238486ms (1/1)** |

Legend: `<median>ms (completed/cases)`; **bold** = best median among engines that completed; TO=timeout, NA=not available, NC=non-comparable parser/runtime incompatibility, KIL=interrupted cleanup kill, ORPH=container orphan cleanup, FAIL=hard runtime failure.
