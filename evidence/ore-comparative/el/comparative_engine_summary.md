# Comparative Engine Completability and Timing Summary

- Pass runs: `ore2015_cmp_p1c_el`, `ore2015_cmp_p2_el`, `ore2015_cmp_p3_el`
- Suite: `standard`
- Operation: `consistency`
- Statistic: per-engine counts over aggregated per-case outcomes; median and total over the recorded per-case repetition medians.

| Engine | Cases | Completed | Timeout | Failed | Not Available | Other Execution | Median Wall (ms) | Total Wall (ms) |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| elk | 298 | 298 | 0 | 0 | 0 | 0 | 3195.5 | 2031720 |
| hermit | 298 | 291 | 7 | 0 | 0 | 0 | 3910 | 2982918 |
| konclude | 298 | 298 | 0 | 0 | 0 | 0 | 5412 | 3699797 |
| tableauxx | 298 | 280 | 0 | 18 | 0 | 0 | 2081 | 985544 |
