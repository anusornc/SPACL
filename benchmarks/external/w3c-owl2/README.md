# W3C OWL 2 Approved Conformance Lane

This lane stages W3C OWL 2 test-case metadata into a manifest, evaluates the
approved Direct Semantics semantic baseline through `owl2-reasoner evaluate`,
and reports with the same benchmark result vocabulary used by the existing
competitor harness.

The official approved archive exposes aggregate RDF manifests such as
`approved/all.rdf`. `prepare.sh` expands each embedded `test:TestCase`, and
emits one operation row for every semantic test type declared on that case;
therefore operation-row counts can be larger than physical test-case counts.

The lane keeps W3C semantic values (`true`, `false`, `unknown`, `error`) as
oracle data and keeps execution state (`success`, `failed`, `timeout`,
`unsupported`) separate.

## Layout

- `prepare.sh`: discover W3C-style manifest cases by metadata and write
  `manifest.tsv`
- `run.sh`: evaluate baseline cases and write one JSON artifact per case
- `report.sh`: regenerate `summary.json`, `results.csv`, and `report.md`
- `w3c_owl2_harness.py`: lane-local parser and reporter used by the shell
  entrypoints
- `tests/test_w3c_owl2_harness.sh`: shell regression test for parsing,
  classification, running, and reporting

## Quick Start

```bash
W3C_OWL2_SOURCE_DIR=/path/to/w3c/owl2/tests \
benchmarks/external/w3c-owl2/prepare.sh

RUN_ID=w3c_owl2_smoke \
TIMEOUT_SECONDS=60 \
benchmarks/external/w3c-owl2/run.sh

benchmarks/external/w3c-owl2/report.sh w3c_owl2_smoke
```

## Baseline Gate

Only cases with all of the following metadata enter the claim-bearing baseline:

- approved status
- semantic case type: consistency, inconsistency, positive entailment, or
  negative entailment
- Direct Semantics
- OWL 2 DL when species metadata is present
- normative RDF/XML input

Syntax/profile tests, RDF-based-only tests, non-approved tests, extra-credit
tests, and cases without runnable RDF/XML premise/conclusion inputs are recorded
as unsupported lane results rather than silently entering the baseline.
