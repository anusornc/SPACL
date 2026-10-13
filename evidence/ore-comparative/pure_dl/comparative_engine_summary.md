# Comparative Engine Completability and Timing Summary

- Pass runs: `ore2015_cmp_p1b_pure_dl`, `ore2015_cmp_p2_pure_dl`, `ore2015_cmp_p3_pure_dl`
- Suite: `standard`
- Operation: `consistency`
- Statistic: per-engine counts over aggregated per-case outcomes; median and total over the recorded per-case repetition medians.

| Engine | Cases | Completed | Timeout | Failed | Not Available | Other Execution | Median Wall (ms) | Total Wall (ms) |
|---|---:|---:|---:|---:|---:|---:|---:|---:|
| hermit | 306 | 287 | 5 | 14 | 0 | 0 | 4336 | 2535300 |
| konclude | 306 | 303 | 2 | 0 | 1 | 0 | 5846 | 3668346 |
| tableauxx | 306 | 305 | 0 | 1 | 0 | 0 | 2330 | 2111536 |
