//! Contract tests for the merge-probe size gate (#168): corpora above
//! the gate skip the Θ(gcis × edges)-per-node walk, the verdict stays
//! fail-open consistent, and the skip is observable both on the
//! reasoner and in the check-auto report JSON.

use std::process::Command;
use std::sync::Arc;

use owl2_reasoner::core::iri::IRI;
use owl2_reasoner::core::ontology::Ontology;
use owl2_reasoner::logic::axioms::{Axiom, ClassAssertionAxiom, ClassExpression, FunctionalPropertyAxiom};
use owl2_reasoner::reasoner::simple::SimpleReasoner;

const BIN: &str = env!("CARGO_BIN_EXE_owl2-reasoner");
const FIXTURE: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/data/functional_role_ab.owl"
);

/// A functional role plus two typed individuals: enough to enter the
/// merge probe (a functional property is a merge rule) with more than
/// one expression node.
fn gated_ontology() -> Arc<Ontology> {
    let mut ontology = Ontology::new();
    let property = || {
        Arc::new(IRI::new("http://example.org/gate#p").expect("test IRI is valid"))
    };
    ontology
        .add_axiom(Axiom::FunctionalProperty(Box::new(
            FunctionalPropertyAxiom::new(property()),
        )))
        .expect("functional axiom accepted");
    // Three typed individuals: three expression nodes for the walk.
    let class = || {
        ClassExpression::Class(
            owl2_reasoner::core::entities::Class::new(
                IRI::new("http://example.org/gate#c").expect("test IRI is valid"),
            ),
        )
    };
    for local in ["a", "b", "c"] {
        let individual = Arc::new(
            IRI::new(&format!("http://example.org/gate#{local}")).expect("test IRI is valid"),
        );
        ontology
            .add_axiom(Axiom::ClassAssertion(Box::new(ClassAssertionAxiom::new(
                individual,
                class(),
            ))))
            .expect("class assertion accepted");
    }
    Arc::new(ontology)
}

#[test]
fn gate_below_node_count_skips_probe_fail_open() {
    // The fixture has three expression nodes; a gate of two skips the
    // walk and the verdict stays fail-open consistent.
    let reasoner = SimpleReasoner::from_arc(gated_ontology()).with_merge_probe_max_nodes(2);
    let verdict = reasoner.is_consistent();
    assert!(verdict.unwrap_or(false), "the gate is fail-open, not fail-closed");
    assert!(
        reasoner.merge_probe_skipped(),
        "a corpus above the gate must record the skip"
    );
}

#[test]
fn gate_above_node_count_or_disabled_runs_probe() {
    let roomy = SimpleReasoner::from_arc(gated_ontology()).with_merge_probe_max_nodes(10);
    let _ = roomy.is_consistent();
    assert!(
        !roomy.merge_probe_skipped(),
        "a corpus below the gate runs the probe as before"
    );

    let disabled = SimpleReasoner::from_arc(gated_ontology()).with_merge_probe_max_nodes(0);
    let _ = disabled.is_consistent();
    assert!(
        !disabled.merge_probe_skipped(),
        "0 explicitly disables the gate (probe always runs)"
    );
}

#[test]
fn check_auto_report_marks_the_skip() {
    let report_path = std::env::temp_dir().join("merge_probe_gate_report.json");
    let _ = std::fs::remove_file(&report_path);
    let output = Command::new(BIN)
        .arg("check-auto")
        .arg(FIXTURE)
        .arg("--report-json")
        .arg(report_path.to_str().unwrap())
        .env("OWL2_REASONER_MERGE_PROBE_MAX_NODES", "1")
        .output()
        .expect("failed to spawn owl2-reasoner");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "check-auto must succeed under the gate:\n{stdout}\n{stderr}"
    );
    assert!(
        stderr.contains("merge_probe_skipped"),
        "the skip must be visible on the stage log channel:\n{stderr}"
    );
    let report: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&report_path).expect("report written"))
            .expect("report parses");
    assert_eq!(report["status"], "success", "report:\n{report}");
    assert_eq!(report["reasoning_result"], "consistent");
    assert_eq!(
        report["merge_probe"], "skipped_size_gate",
        "the report must mark the fail-open skip:\n{report}"
    );
}

#[test]
fn check_auto_report_is_clean_without_the_gate() {
    let report_path = std::env::temp_dir().join("merge_probe_gate_report_off.json");
    let _ = std::fs::remove_file(&report_path);
    let output = Command::new(BIN)
        .arg("check-auto")
        .arg(FIXTURE)
        .arg("--report-json")
        .arg(report_path.to_str().unwrap())
        .env("OWL2_REASONER_MERGE_PROBE_MAX_NODES", "0")
        .output()
        .expect("failed to spawn owl2-reasoner");
    assert!(
        output.status.success(),
        "check-auto must succeed with the probe running:\n{}",
        String::from_utf8_lossy(&output.stdout)
    );
    let report: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&report_path).expect("report written"))
            .expect("report parses");
    assert_eq!(report["status"], "success");
    assert!(
        report.get("merge_probe").is_none(),
        "no gate, no marker — the field must stay absent:\n{report}"
    );
}
