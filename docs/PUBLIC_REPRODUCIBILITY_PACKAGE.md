# Public SPACL Reproducibility Package

This checkout includes a curated metadata package for auditing the SPACL manuscript benchmark tables without publishing private paper workflow assets.

Key files:

- `benchmarks/competitors/docker/`
- `benchmarks/competitors/docker-compose.yml`
- `benchmarks/external/owl2bench/{prepare,run,report}.sh`
- `benchmarks/competitors/results/reproducibility/PUBLIC_REPRODUCIBILITY_MANIFEST_20260605.md`
- `benchmarks/competitors/results/reproducibility/semantic_agreement_20260605.csv`
- `benchmarks/competitors/results/reproducibility/spread_summary_20260605.csv`

The semantic audit is scoped to the primary displayed reasoners: SPACL (`tableauxx`), HermiT, Openllet, ELK, JFact, and Pellet. Konclude remains supplementary and is not included in that denominator.

For the large biomedical suite, the canonical third clean run is `large_r3_real_20260225`; `large_r3_20260225` is not the clean third run.

Ontology payloads and binary cache files are intentionally omitted from the curated git package. Use the benchmark download/generation scripts and record checksums before rerunning long campaigns.
