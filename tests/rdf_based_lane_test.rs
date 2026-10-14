//! Contract tests for the FULL lane v0 (#181, Track B, ADR 0006 and
//! its 2026-10-09 amendment): the RDF-based evaluator answers the
//! annotation-vocabulary family through `evaluate --rdf-based`, and
//! shapes it does not cover fall back to the Direct evaluation with
//! identical answers — routing the lane cannot regress rows.

use std::process::Command;

const BIN: &str = env!("CARGO_BIN_EXE_owl2-reasoner");
const DATA: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/data/rdf_based");

fn evaluate_entailment(rdf_based: bool, premise: &str, conclusion: &str) -> serde_json::Value {
    let mut command = Command::new(BIN);
    command.arg("evaluate").arg("entailment");
    if rdf_based {
        command.arg("--rdf-based");
    }
    let output = command
        .arg(premise)
        .arg(conclusion)
        .output()
        .expect("failed to spawn owl2-reasoner");
    let stdout = String::from_utf8_lossy(&output.stdout);
    serde_json::from_str(stdout.trim()).expect("one JSON response on stdout")
}

/// The family-1 shape: an empty premise graph and the conclusion
/// typing rdfs:label as an owl:AnnotationProperty — a semantic
/// condition on the vocabulary, entailed by the built-in triples.
#[test]
fn annotation_vocabulary_typing_is_entailed_from_the_empty_graph() {
    let case = DATA;
    let response = evaluate_entailment(
        true,
        &format!("{case}/label_type_premise.owl"),
        &format!("{case}/label_type_conclusion.owl"),
    );
    assert_eq!(
        response["semantic_result"], "true",
        "the label typing is a vocabulary semantic condition: {response}"
    );
    assert_eq!(
        response["supported_scope"], "owl2_rdf_based_graph_closure",
        "the answer must be attributable to the RDF-based lane: {response}"
    );
}

/// A typing the vocabulary does not fix is NOT entailed — the lane
/// answers false, not vacuously true.
#[test]
fn non_builtin_annotation_typing_is_not_entailed() {
    let dir = std::env::temp_dir().join("rdf_based_lane_control");
    std::fs::create_dir_all(&dir).unwrap();
    let premise = dir.join("empty.owl");
    let conclusion = dir.join("custom.owl");
    std::fs::write(
        &premise,
        r#"<rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#"></rdf:RDF>"#,
    )
    .unwrap();
    std::fs::write(
        &conclusion,
        r#"<rdf:RDF xmlns:rdf="http://www.w3.org/1999/02/22-rdf-syntax-ns#" xmlns:owl="http://www.w3.org/2002/07/owl#" xmlns:ex="http://example.org#">
  <owl:AnnotationProperty rdf:about="http://example.org#custom"/>
</rdf:RDF>"#,
    )
    .unwrap();
    // The uncovered ground shape falls back to the Direct lane, which
    // answers false from the empty premise — the attribution records
    // the fallback rather than a lane-claimed refutation.
    let response = evaluate_entailment(
        true,
        premise.to_str().unwrap(),
        conclusion.to_str().unwrap(),
    );
    assert_eq!(
        response["semantic_result"], "false",
        "ex:custom is not a built-in annotation property: {response}"
    );
    assert_eq!(
        response["supported_scope"], "owl2_entailment_consistency_reduction",
        "v0 claims positives only; the false comes from the Direct fallback: {response}"
    );
}

/// The fallback contract: a Direct-decided entailment with a blank-node
/// conclusion (outside v0's families) answers identically with and
/// without the flag.
#[test]
fn uncovered_shapes_fall_back_to_the_direct_evaluation() {
    let case = DATA;
    let premise = format!("{case}/bnode2svf_premise.owl");
    let conclusion = format!("{case}/bnode2svf_conclusion.owl");
    let direct = evaluate_entailment(false, &premise, &conclusion);
    let flagged = evaluate_entailment(true, &premise, &conclusion);
    assert_eq!(
        direct["semantic_result"], flagged["semantic_result"],
        "the flag must not change uncovered shapes"
    );
    assert_eq!(
        direct["semantic_result"], "true",
        "control premise: the Direct lane answers true (bnode2someValuesFrom)"
    );
    assert_eq!(
        flagged["supported_scope"], "owl2_entailment_consistency_reduction",
        "the fallback answer is attributable to the Direct lane: {flagged}"
    );
}
