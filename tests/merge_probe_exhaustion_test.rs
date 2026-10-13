//! Contract tests for the merge-probe budget-exhaustion marker (#171,
//! D2 sub-landing 1, ADR 0022): a probe that enters and spends its walk
//! budget keeps the fail-open consistent verdict (ADR 0020's contract,
//! unchanged) but the exhaustion is observable on the reasoner and in
//! the check-auto report JSON as `merge_probe: budget_exhausted` —
//! distinct from the #168 entry gate's `skipped_size_gate`, and absent
//! when the probe runs to completion.

use std::process::Command;
use std::sync::Arc;

use owl2_reasoner::core::iri::IRI;
use owl2_reasoner::core::ontology::Ontology;
use owl2_reasoner::logic::axioms::{
    Axiom, ClassAssertionAxiom, ClassExpression, FunctionalPropertyAxiom, ObjectPropertyExpression,
    PropertyAssertionAxiom, SubClassOfAxiom,
};
use owl2_reasoner::reasoner::simple::SimpleReasoner;

const BIN: &str = env!("CARGO_BIN_EXE_owl2-reasoner");
const FIXTURE: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/data/functional_role_ab.owl"
);

/// A functional role plus typed individuals: enough to enter the merge
/// probe (a functional property is a merge rule) with real walk work;
/// the existential GCI over the same role spends candidate-edge work
/// (#173's work-unit lever).
fn probed_ontology() -> Arc<Ontology> {
    let mut ontology = Ontology::new();
    let property = || {
        Arc::new(IRI::new("http://example.org/exhaustion#p").expect("test IRI is valid"))
    };
    ontology
        .add_axiom(Axiom::FunctionalProperty(Box::new(
            FunctionalPropertyAxiom::new(property()),
        )))
        .expect("functional axiom accepted");
    let class = || {
        ClassExpression::Class(owl2_reasoner::core::entities::Class::new(
            IRI::new("http://example.org/exhaustion#c").expect("test IRI is valid"),
        ))
    };
    let existential = ClassExpression::ObjectSomeValuesFrom(
        Box::new(ObjectPropertyExpression::ObjectProperty(Box::new(
            owl2_reasoner::core::entities::ObjectProperty::new(
                IRI::new("http://example.org/exhaustion#p").expect("test IRI is valid"),
            ),
        ))),
        Box::new(ClassExpression::Class(owl2_reasoner::core::entities::Class::new(
            IRI::new("http://www.w3.org/2002/07/owl#Thing").expect("test IRI is valid"),
        ))),
    );
    ontology
        .add_axiom(Axiom::SubClassOf(Box::new(SubClassOfAxiom::new(
            existential,
            class(),
        ))))
        .expect("existential GCI accepted");
    let individual = |local: &str| {
        Arc::new(
            IRI::new(&format!("http://example.org/exhaustion#{local}")).expect("test IRI is valid"),
        )
    };
    for local in ["a", "b", "c"] {
        ontology
            .add_axiom(Axiom::ClassAssertion(Box::new(ClassAssertionAxiom::new(
                individual(local),
                class(),
            ))))
            .expect("class assertion accepted");
    }
    // A concrete successor edge: the existential GCI's candidate check
    // has edges to inspect (and work to charge, #173).
    ontology
        .add_property_assertion(PropertyAssertionAxiom::new(
            individual("a"),
            property(),
            individual("b"),
        ))
        .expect("property assertion accepted");
    Arc::new(ontology)
}

#[test]
fn exhausted_walk_budget_marks_the_probe_fail_open() {
    // One walk-node budget: the probe enters, spends it immediately,
    // and the verdict stays fail-open consistent — but recorded.
    let reasoner = SimpleReasoner::from_arc(probed_ontology()).with_merge_probe_walk_budget(1);
    let verdict = reasoner.is_consistent();
    assert!(
        verdict.unwrap_or(false),
        "exhaustion is fail-open, not fail-closed"
    );
    assert!(
        reasoner.merge_probe_exhausted(),
        "a probe that spent its walk budget must record the exhaustion"
    );
}

#[test]
fn completing_probe_stays_unmarked() {
    // The same ontology with the default-scale budget completes the
    // walk: neither marker may appear.
    let roomy = SimpleReasoner::from_arc(probed_ontology()).with_merge_probe_walk_budget(30_000);
    let _ = roomy.is_consistent();
    assert!(
        !roomy.merge_probe_exhausted(),
        "a completing probe must not claim exhaustion"
    );
    assert!(
        !roomy.merge_probe_skipped(),
        "a running probe must not claim the entry gate fired"
    );
}

#[test]
fn check_auto_report_marks_exhaustion() {
    let report_path = std::env::temp_dir().join("merge_probe_exhaustion_report.json");
    let _ = std::fs::remove_file(&report_path);
    let output = Command::new(BIN)
        .arg("check-auto")
        .arg(FIXTURE)
        .arg("--report-json")
        .arg(report_path.to_str().unwrap())
        .env("OWL2_REASONER_MERGE_PROBE_MAX_NODES", "0")
        .env("OWL2_REASONER_MERGE_PROBE_WALK_NODES", "1")
        .output()
        .expect("failed to spawn owl2-reasoner");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "check-auto must succeed under exhaustion:\n{stdout}\n{stderr}"
    );
    assert!(
        stderr.contains("merge_probe_budget_exhausted"),
        "the exhaustion must be visible on the stage log channel:\n{stderr}"
    );
    let report: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&report_path).expect("report written"))
            .expect("report parses");
    assert_eq!(report["status"], "success", "report:\n{report}");
    assert_eq!(report["reasoning_result"], "consistent");
    assert_eq!(
        report["merge_probe"], "budget_exhausted",
        "the report must mark the fail-open exhaustion:\n{report}"
    );
}

#[test]
fn check_auto_report_is_clean_without_exhaustion() {
    let report_path = std::env::temp_dir().join("merge_probe_exhaustion_report_off.json");
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
        "check-auto must succeed with a completing probe"
    );
    let report: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&report_path).expect("report written"))
            .expect("report parses");
    assert_eq!(report["status"], "success", "report:\n{report}");
    assert!(
        report.get("merge_probe").is_none(),
        "a completing probe leaves the field absent:\n{report}"
    );
}

/// #173 (ADR 0022): the global work ceiling — one unit per edge
/// inspection inside an existential-GCI candidate check. A tiny
/// ceiling exhausts the probe mid-search (fail-open, marked); `0`
/// means unlimited; the counted work is deterministic across runs.
const WORK_FIXTURE: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/data/existential_gci_probe.owl"
);

#[test]
fn work_ceiling_exhaustion_marks_the_probe_fail_open() {
    // The fixture's functional role enters the probe and its
    // existential GCI spends candidate work; a ceiling of one
    // inspection cannot finish the search.
    let reasoner = SimpleReasoner::from_arc(probed_ontology()).with_merge_probe_work_budget(1);
    let verdict = reasoner.is_consistent();
    assert!(
        verdict.unwrap_or(false),
        "work exhaustion is fail-open, not fail-closed"
    );
    assert!(
        reasoner.merge_probe_exhausted(),
        "a probe that spent its work ceiling must record the exhaustion"
    );
}

#[test]
fn unlimited_work_budget_runs_to_completion() {
    let reasoner = SimpleReasoner::from_arc(probed_ontology()).with_merge_probe_work_budget(0);
    let _ = reasoner.is_consistent();
    assert!(
        !reasoner.merge_probe_exhausted(),
        "0 means unlimited: the small fixture must complete"
    );
}

fn run_probe_cli(extra_env: &[(&str, &str)]) -> (String, String) {
    let mut command = Command::new(BIN);
    command
        .arg("check-auto")
        .arg(WORK_FIXTURE)
        .env("OWL2_REASONER_MERGE_PROBE_MAX_NODES", "0");
    for (key, value) in extra_env {
        command.env(key, value);
    }
    let output = command.output().expect("failed to spawn owl2-reasoner");
    (
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

#[test]
fn check_auto_report_marks_work_ceiling_exhaustion() {
    let report_path = std::env::temp_dir().join("merge_probe_work_report.json");
    let _ = std::fs::remove_file(&report_path);
    let output = Command::new(BIN)
        .arg("check-auto")
        .arg(WORK_FIXTURE)
        .arg("--report-json")
        .arg(report_path.to_str().unwrap())
        .env("OWL2_REASONER_MERGE_PROBE_MAX_NODES", "0")
        .env("OWL2_REASONER_MERGE_PROBE_WORK_BUDGET", "1")
        .output()
        .expect("failed to spawn owl2-reasoner");
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "check-auto must succeed under work exhaustion:\n{stdout}\n{stderr}"
    );
    let report: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&report_path).expect("report written"))
            .expect("report parses");
    assert_eq!(report["status"], "success", "report:\n{report}");
    assert_eq!(
        report["merge_probe"], "budget_exhausted",
        "the report must mark the work-ceiling exhaustion:\n{report}"
    );
}

#[test]
fn counted_work_is_deterministic_across_runs() {
    let extract = |stderr: &str| -> Option<String> {
        let line = stderr
            .lines()
            .rev()
            .find(|line| line.contains("merge_probe_done"))?;
        line.split_whitespace()
            .find(|field| field.starts_with("work="))
            .map(|field| field.to_string())
    };
    let (_, first) = run_probe_cli(&[]);
    let (_, second) = run_probe_cli(&[]);
    let first_work = extract(&first).expect("first run reports merge_probe_done work=");
    let second_work = extract(&second).expect("second run reports merge_probe_done work=");
    assert_eq!(
        first_work, second_work,
        "the same corpus must count the same work (no wall-clock coupling)"
    );
    assert!(
        !first_work.ends_with("work=0"),
        "the fixture must spend some candidate work: {first_work}"
    );
}
