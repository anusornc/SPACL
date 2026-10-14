# Second benchmark track over namespace-patched anchors

Status: accepted

> **Correction after primary-source verification (same day).** The external
> key's README frames the split as a current-master generator bug; research
> against the upstream repository history
> (`docs/research/OWL2BENCH_GENERATOR_NAMESPACE_BUG.md`) refutes that
> framing — current upstream TBoxes are kracr-namespace throughout, the
> official queries carry no namespace at all (a bare `:` prefix; the
> namespace is a deployment choice), and the split in our pinned anchor
> bytes traces to the pinned jar bundling a 2020-era benchmark-namespace
> TBox alongside its kracr-writing generator. The decision below is
> unchanged: it never rested on the README's framing, only on the verified
> fact that the pinned anchor bytes are split and on what a repaired
> dataset answers. A useful consequence of the verification: the external
> key's datasets are byte-identical to our pinned anchors after the same
> re-base, so the cross-check compares like with like.

The campaign's correctness verdicts (RL 18/18, QL 10/10, EL 10/10, DL 22/22)
run on the OWL2Bench generator's shipped bytes, where the ABox is written
under `https://kracr.iiitd.edu.in/OWL2Bench#` and the TBox under
`http://benchmark/OWL2Bench#` (ADR 0016 records the split; ADR 0018 records
that it makes the TBox inert for the query vocabulary). An externally
assembled cross-reasoner answer key (HermiT 1.3.8 / Openllet 2.6 / Pellet
2.3, disagreements hand-resolved; staged at
`benchmarks/ontologies/external/owl2bench/external-answerkey/` with pinned
provenance) demonstrates what the same queries answer when the namespace
bug is repaired and the TBox actually classifies the ABox — e.g. Q19
Faculty 858 via `teachesCourse`'s domain, Q1 `knows` 5,108 with the
reflexive pairs, versus 0 and 1,427 on the shipped bytes.

## Decision

- A **second, parallel benchmark track** is opened: the patched-namespace
  correctness scale. Its anchors are produced by our own deterministic
  re-base of the ADR 0015/0017 anchors (kracr namespace → benchmark
  namespace), staged with manifest and byte-determinism verification; the
  external key's datasets are not adopted as anchors.
- All four profiles are in scope. The external key covers EL/QL/RL fully
  and DL partially (7/22, some single-reasoner); every cross-check
  expectation is set accordingly.
- The shipped-bytes track and this track **coexist**: each answers a
  different question — "correctness on the artifact as generated" versus
  "correctness once the dataset is repaired". Neither replaces the other;
  the existing verdicts and oracles stay final evidence.
- The external key is **differential evidence, never an oracle input**
  (ADR 0013). The track's correctness gate remains our audited derived
  oracle; per-case key agreement is recorded in provenance, and every
  disagreement becomes a diagnosis ticket (ours could be a rule-table gap,
  theirs a reasoner defect — their own key records Openllet returning
  unsound ObjectHasSelf rows). *(Judgment call recorded here because the
  round-2 question went unanswered: key-as-adviser, not key-as-veto.)*
- On patched data the queried IRIs appear in axioms, so the ADR 0018
  vocabulary-disjointness argument does not carry over: the construct
  decision point (implemented semantics or fail closed per case)
  reactivates for 006/017/018 under this track.

## Consequences

- The derivation core grows a second, versioned configuration for this
  track (correct wrapper-connection parsing, `prp-ref`, and the construct
  semantics decided per case), while the legacy configuration stays
  byte-identical so the accepted RL/QL/EL/DL artifacts keep reproducing.
- The reasoner's `materialize` needs the matching propagations (e.g.
  reflexive roles) before its verdict runs can meet the patched oracles.
- Published numbers must name their track: an unpatched-dataset number
  measures an ABox-disconnected TBox (the external key's README makes the
  same point about OWL2Bench results generally).
