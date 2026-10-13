# Tableauxx Reasoning and Benchmark Context

This context defines the language used for Tableauxx reasoning behavior,
semantic correctness, and workload evaluation.

## SPACL scheduling language

**Static scheduling gate**:
A scheduling choice made from fixed configuration and deterministic input
features without learning from observed execution outcomes.
_Avoid_: adaptive controller, dynamic tuning.

**Feedback-trained threshold selector**:
An experimental selector that updates a threshold between completed
measurement blocks using an explicit feedback record.
_Avoid_: within-run adaptation, production scheduler.

**Within-run feedback controller**:
An experimental controller that may update future scheduling choices during
one reasoning invocation at deterministic logical epochs.
_Avoid_: feedback-trained threshold selector.

**Deterministic logical epoch**:
A controller update boundary derived from logical work progress rather than
elapsed time or worker completion order.
_Avoid_: timer interval, race-dependent checkpoint.

**Feedback tape**:
A versioned sequence of recorded observations supplied to reproduce controller
state transitions independently of new timing measurements.
_Avoid_: hidden process state, benchmark log.

**Deterministic replay**:
Re-execution with the same input identity, configuration, seed, and feedback
tape that yields the same threshold and scheduling-decision trace. It does not
require identical thread interleaving or wall-clock duration.
_Avoid_: verdict-only repeatability, identical timing.

**Semantic verdict invariance**:
The requirement that a scheduling experiment preserve the normalized public
result and error classification of its accepted baseline.
_Avoid_: similar result, performance-compensated mismatch.

**Decision-flip coverage**:
The share of eligible evaluations for which a candidate threshold changes the
static gate's scheduling choice.
_Avoid_: workload coverage, branch coverage.

## Benchmark language

**Benchmark suite**:
A named corpus plus its execution protocol, semantic oracle, and result vocabulary.
_Avoid_: dataset (a dataset is only the input files).

**Full approved corpus**:
Every approved operation row materialized from the official W3C approved manifest, including rows assigned to different semantics or concrete syntaxes.
_Avoid_: RDF/XML-only subset, physical-case count.

**Lane membership**:
The single correctness lane assigned to a test case by its official semantics, species, and normative syntax metadata.
_Avoid_: inferred lane, mixed lane.

**Test case**:
One suite-defined input and its expected semantic or operational outcome.
_Avoid_: benchmark run (a run executes test cases).

**Semantic oracle**:
The authoritative expected result against which Tableauxx is judged. A reference reasoner may help diagnose a mismatch, but it is not automatically the oracle.
_Avoid_: comparator.

**Derived oracle**:
An expected result produced by a deterministic formal derivation whose proof or
checking procedure is independently auditable. It may satisfy the correctness
contract only when the derivation, input identity, and review evidence are
recorded; it is not an official answer key.

**Protocol consensus**:
An answer selected by agreement among reasoning engines under a documented
benchmark protocol. It is provenance or differential evidence, not a semantic
oracle by default.

**Derived-oracle acceptance**:
The condition under which a formally derived answer may serve as a correctness
oracle: deterministic derivation, exact input/query identity, checkable proof
or certificate, and independent audit. Engine agreement alone does not meet
this condition.

**Proposer engine**:
An engine whose output may suggest candidate answer rows for a derived
oracle. Its output, and its agreement with other engines, confers no
correctness authority.
_Avoid_: oracle engine, reference answer.

**Justification evidence**:
The per-row checkable entailment justification, as a proof trace or
rule-derivation chain, that turns a proposed answer row into auditable
derivation evidence.

**Independent audit session**:
A verification session that re-derives or verifies justification evidence
without access to the producing session's conclusions.
_Avoid_: self-review by the producing session.

**Oracle independence**:
The accepted answer derivation must be independently attributable from the
implementation under test. A result emitted by the same query engine or
reasoner being evaluated cannot certify its own correctness.

**Semantics-specific oracle**:
The authoritative expected result interpreted under the semantics declared by a correctness lane; the same input representation is not reused across semantics without an explicit mapping.
_Avoid_: cross-semantics oracle.

**Semantic view**:
One interpretation of an official test case under one declared semantics, with its own structural input meaning and result status.
_Avoid_: one result for multiple semantics.

**RDF graph view**:
The complete RDF graph interpretation of an official input, including RDF vocabulary, metamodeling, blank nodes, literals, and list triples.
_Avoid_: Direct structural ontology view.

**Unsupported**:
A case that the current implementation cannot execute under the declared contract. It is neither a pass nor an engine failure.
_Avoid_: success, and avoid conflating it with failed.

**Engine defect**:
Behavior that contradicts the applicable semantic oracle for a supported input. Every confirmed defect requires a regression test.

**Differential diagnosis**:
Comparison with another reasoner to isolate a mismatch or implementation fault; agreement between reasoners does not establish semantic correctness.

**Differential appendix**:
A report section that places engines' outputs side by side as observational evidence from comparative runs. It adjudicates no correctness and holds no gate authority.
_Avoid_: consensus result, correctness column.

## Reasoning capabilities

**Consistency**:
Whether an ontology has a model under the semantics specified by the test case.

**Entailment**:
Whether a premise ontology semantically entails a conclusion ontology or does not entail a non-conclusion ontology.

**Entailment verdict**:
The semantic answer True, False, Unknown, or Error for one premise/conclusion pair.
_Avoid_: pass/fail as a semantic answer.

**Countermodel**:
A model that satisfies the premise ontology but does not satisfy the conclusion ontology.
_Avoid_: reference-reasoner disagreement.

**Qualified cardinality**:
A cardinality restriction whose counted fillers must also belong to a class expression or data range; an absent qualifier means every filler is counted.
_Avoid_: cardinality with an ignored filler.

**Expression-valued domain/range**:
A property domain or range constraint whose value is a class expression rather than only a named class.
_Avoid_: synthetic named-class domain/range.

**Union choice**:
The commitment of one individual to one disjunct of a union of classes it must belong to.
_Avoid_: union expansion, case split.

**Branch-and-settle search**:
The refutation contract for a choice: every branch dying refutes (the ontology is inconsistent), any branch surviving settles (it stays consistent); no branch is refuted by default.
_Avoid_: enumeration, guessing.

**Free union**:
A union constraint whose disjuncts no other constraint presses on; no refutation can pass through it, so a bounded search leaves it undecided.
_Avoid_: unconstrained disjunction, ignorable union.

**Fail-open budget**:
The exhaustion contract of a bounded search: spending the whole budget reads as "no refutation found", never as a positive verdict in either direction.
_Avoid_: budget timeout, search failure.

**Merge probe**:
The bounded node-merging search behind consistency and entailment verdicts for cardinality, nominal, and identification constraints.
_Avoid_: tableaux fallback, SAT search.

**Size gate**:
An entry decision that skips the merge probe before any search work when the input exceeds a node-count threshold.
_Avoid_: budget exhaustion, timeout.

**Work unit**:
One edge inspection inside an existential-GCI candidate check; the deterministic unit a work ceiling counts.
_Avoid_: node visits, elapsed time.

**Exhaustion marker**:
The report signal separating a probe that entered and spent its work ceiling (`budget_exhausted`) from one skipped at entry (`skipped_size_gate`); the verdict itself stays fail-open.
_Avoid_: error status, incomplete verdict.

**Cardinality complement pair**:
Two named classes defined over one property — one at most zero values, its companion at least one — which full OWL semantics makes exact complements (X.comp ≡ ¬X).
_Avoid_: declared disjointness, complement axiom.

**Disjointness half**:
The instance-level entailment of a cardinality complement pair (X ⊓ X.comp ⊑ ⊥), consumed as a cardinality clash on one individual.
_Avoid_: disjoint classes axiom.

**Covering half**:
The class-level entailment of a cardinality complement pair (⊤ ⊑ X ⊔ X.comp), consumed through contrapositive and negation readers — never materialized as per-individual branching.
_Avoid_: union introduction, disjunction axiom.

**Pinned data witness**:
A data-existential whose value the premise forces to exactly one lattice value, turning the witness into a definite positive data assertion.
_Avoid_: skolemized constant.

**Materialized data positive**:
A data property assertion the reasoner derives from a restriction the premise states — the value is given, not chosen — rather than reads from the ABox; it is spent only where a conflict check can consume it, never exported as an asserted fact.
_Avoid_: inferred source-graph triple.

**Query workload**:
An official set of queries executed over generated or staged ontology data. Query-result correctness and query timing are separate measurements.

**Canonical query result**:
A deterministic semantic representation of a query answer that preserves RDF
term types, projected variable order, and solution multiplicity. Row order is
significant only when required by the query semantics, such as an explicit
ordering clause.

**Query/profile case**:
One official query evaluated against one applicable OWL2Bench profile dataset.
The source query text may therefore produce multiple correctness cases when
its metadata declares more than one profile.

**Split profile-specific query stack**:
An execution topology in which DL, EL, QL, and RL cases may use different
reasoning engines while preserving one profile-qualified query-case contract.
It is an execution baseline, not a semantic oracle and not a historical
Stardog reproduction.

**Provisional execution baseline**:
A reproducible run path whose purpose is to execute and preserve evidence, not
to establish semantic correctness. It may expose `failed`, `unsupported`,
`not_available`, or `oracle_unavailable` outcomes, and it cannot unlock a
correctness or performance gate without accepted oracle evidence.

**Materialization/query layer**:
The boundary that turns profile-specific reasoning output into a queryable RDF
graph and then evaluates the official SPARQL text. Materialized graphs require
their own identity and completeness evidence; a query store does not become an
oracle merely because it contains inferred triples.

**Finite query-complete closure**:
The source graph plus every query-relevant entailed class, object-property, and
data-property triple over the finite named-term and input-blank-node domain.
The closure does not invent new anonymous witnesses; if an official query
requires one, the materialization is incomplete and reports `not_available`.

**Anonymous witness**:
A reasoner-generated individual introduced to satisfy an existential condition.
It is not exported or skolemized by the OWL2Bench materialization contract,
because doing so would create an engine-dependent identity and change the
reproducibility boundary.

**Materialization completeness**:
An execution-level claim that one profile graph passed structural coverage,
round-trip, deterministic-canonical-hash, capability, and provenance checks.
It does not assert semantic correctness or replace the profile's semantic
oracle.

**Profile materialization artifact**:
An immutable canonical graph and its profile-qualified manifest for exactly one
of DL, EL, QL, or RL. Its source identity, exporter identity, graph hash,
resource envelope, and profile context are independent of the other profiles;
no shared graph may satisfy multiple profile records.

**Materialization completeness validator**:
An independent structural checker that validates a profile materialization
artifact without deciding semantic correctness. It verifies source inclusion,
finite-term coverage, assertion categories, canonical round-trip, graph hash,
profile identity, and required provenance; engine agreement is not one of its
proof obligations.

**Query case matrix**:
The complete set of query/profile cases obtained by expanding every official
query over every profile declared applicable to it. A source query count is
not the correctness-case count.

**Correctness anchor dataset**:
The one deterministic generated dataset selected per profile for the query
correctness gate. Other generated sizes belong to scalability evidence and do
not silently expand the correctness gate.

For the OWL2Bench query gate, the anchor uses the pinned official generator,
seed 1, and one university per profile. Its identity includes the generator
revision, profile TBox, generated ABox, and generation configuration hashes.

**Query answer key**:
An artifact containing the expected canonical result for one query/profile
case, identified by the exact query, generated dataset, canonicalizer, and
oracle provenance. A derived key is not labelled official.

**Canonical result (v1/v2)**:
The typed, versioned canonical form of one engine's query answer. v2 applies
RDF 1.1 simple-literal normalization: a literal without a language tag whose
datatype is `xsd:string` canonicalizes to the simple-literal form, because
RDF 1.1 defines the two as one literal. v1 preserved the serialized datatype
exactly; the canonicalizer version is part of answer-key identity, so a
version bump retires every key pinned to the older version.
_Avoid_: whitespace-normalized digest, engine raw output.

**Serialization delta**:
A canonical mismatch caused only by serialization representation — identical
rows and values after RDF 1.1 normalization — with no disagreement about the
entailed answers.
_Avoid_: semantic disagreement, wrong answer.

**Semantic disagreement**:
A canonical mismatch where the compared answers differ in entailed content
(missing, extra, or different bindings), not merely in representation.
_Avoid_: serialization delta, formatting noise.

**Oracle pipeline**:
The independently attributable process that produces or checks an accepted
query answer key. It is separate from the system under test and records its
proof, certificate, review, and versioned inputs.

**Profile-specific reasoning configuration**:
The reasoning mode selected by the applicable OWL2Bench profile. A query case
cannot silently use another profile's reasoning mode or a default fallback.

**Historical reproduction**:
A benchmark run using the original version, configuration, inputs, and
execution protocol identified by the published experiment. If an original
artifact cannot be obtained, the claim is historical-unavailable rather than
silently replaced by a newer run.

**Current engine variant**:
A separately named benchmark run using a currently obtainable engine and
explicitly pinned provenance. It is comparable evidence, but it is not a
historical reproduction unless every historical input and runtime condition
matches.

**Current Stardog variant**:
The current-release Stardog execution view for OWL2Bench. It covers EL, QL, and
RL through their named reasoning modes; DL remains a separate capability
boundary because current Stardog no longer exposes the historical DL mode.

**Historical-unavailable baseline**:
An original benchmark baseline whose required software artifact cannot be
obtained or verified. It remains a recorded unavailable claim and is never
replaced by a current-release result.

**Profile coverage boundary**:
The set of profile cases for which a selected engine has a verified reasoning
configuration. Current Stardog covers EL, QL, and RL through their named modes;
the absence of a DL mode leaves DL outside that variant's correctness coverage.

**DL query path**:
An independently selected, query-capable execution path for OWL2Bench DL
cases. It is distinct from current Stardog's EL/QL/RL variant and must preserve
the same query, dataset, canonical-result, oracle, and provenance contracts.

**Capability gap**:
A profile or operation that an engine cannot run under its declared reasoning
contract. It is reported as unsupported and is not interchangeable with a
missing answer oracle.

**Current-variant gate**:
The per-profile and overall correctness condition for a current engine
variant. A profile may report execution evidence independently, but the
overall variant remains blocked while any required profile is unsupported,
unavailable, or lacks an accepted oracle.

**Profile**:
An OWL 2 language restriction such as EL, QL, or RL. OWL2Bench's DL family is a benchmark variant, not a W3C OWL 2 profile.

**Scalability point**:
A reproducible workload identified by profile, generator seed, and generated ABox size or university count.

**Scalability ladder**:
The ordered set of scalability points from one pinned generator and seed, used as the size axis of a performance lane's scalability view.
_Avoid_: size list, dataset series.

## Result language

**Correctness lane**:
A benchmark-scope partition with one declared semantics or syntax, input contract, semantic oracle, and result gate.
_Avoid_: mixed lane, aggregate result.

**Correctness gate**:
The semantic result must match the suite oracle before performance or scalability claims are interpreted.

**Campaign completion gate**:
The final cross-suite condition that is satisfied only when every in-scope correctness lane has passed its own gate.
_Avoid_: one mixed W3C gate.

**Performance lane**:
A benchmark lane that declares no semantic oracle and therefore adjudicates no correctness; its published claims are execution performance and completability on a pinned workload.
_Avoid_: correctness lane without an oracle, failed correctness lane.

**Execution performance claim**:
A claim about runtime, timeouts, or completed-run counts on pinned inputs and hardware; it asserts nothing about answer correctness.
_Avoid_: correctness-gated performance, speed as correctness.

**Comparative execution run**:
One serial pass of a pinned engine set over a pinned workload on recorded hardware, producing per-case timing and completability evidence for every engine.
_Avoid_: correctness run, consensus vote.

**Repetition median**:
The per-case median container-inclusive wall time across a comparative execution run's declared repetitions; the primary timing statistic for performance claims.
_Avoid_: best-of-N time, single-shot time.

**Lane report**:
An evidence record for one correctness lane containing its source provenance, oracle provenance, and status counts; it is not an aggregate across semantics.
_Avoid_: mixed-semantics summary.

**Benchmark status**:
The execution state of a test case, kept distinct from its semantic result:
passed, wrong, unsupported, not available, failed, timeout, or
oracle_unavailable.

**Query lane gate**:
The correctness condition for the OWL2Bench query workload. Each applicable
profile subview must pass its own query/profile cases, and the overall query
lane passes only when the DL, EL, QL, and RL subviews all pass.

**Oracle-unavailable rule**:
If query execution succeeds but no official answer key or independently
audited derived oracle is available, the case remains oracle_unavailable. It
may report timing, but it cannot be marked passed or contribute to a passing
correctness gate.

**Query status precedence**:
Mutually exclusive classification of a query case: unavailable engine/data,
unsupported capability, failed execution or output parsing, timeout, missing
accepted oracle, wrong oracle comparison, or passed oracle comparison. Timing
does not override correctness status.

**Query evidence completeness**:
The condition that every required query/profile case has a source identity,
generated-dataset identity, engine/config provenance, canonical result hash,
and accepted oracle provenance before a profile or overall query gate may pass.

**Shipped-bytes track**:
The correctness track measured on the OWL2Bench anchors exactly as the
generator emits them, where the ABox namespace differs from the TBox
namespace and the TBox is inert for the query vocabulary.
_Avoid_: the default benchmark, the only benchmark.

**Patched-namespace track**:
The parallel correctness track measured on anchors whose ABox namespace
has been deterministically re-based onto the TBox namespace, so the TBox
classifies the ABox and answers depend on real reasoning.
_Avoid_: the fixed benchmark (the re-base is our recorded transformation,
not an upstream fix).

**External answer key**:
A cross-reasoner answer set assembled outside this project. It is
differential evidence recorded in provenance and a source of diagnosis
tickets; it is never an oracle input and holds no veto over a gate.
_Avoid_: official answer key (authority ladder), oracle.
