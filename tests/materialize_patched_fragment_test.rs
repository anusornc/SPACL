//! Contract tests for the patched-fragment materialization (#101):
//! reflexive roles over declared individuals, hasValue membership,
//! binary intersection introduction, hasSelf, and the qualified
//! cardinality rules, mirroring the derivation core's v2 rule table.

use std::collections::BTreeSet;
use std::process::Command;

const BIN: &str = env!("CARGO_BIN_EXE_owl2-reasoner");
const FIXTURE: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/data/materialize_patched_fragment.owl"
);

fn p(local: &str) -> String {
    format!("<http://example.org/p#{local}>")
}

fn rdf_type() -> &'static str {
    "<http://www.w3.org/1999/02/22-rdf-syntax-ns#type>"
}

fn materialized_closure(tag: &str) -> BTreeSet<String> {
    // One output file per test: the tests run in parallel and would
    // otherwise overwrite each other's materialization mid-read.
    let out = std::env::temp_dir().join(format!("materialize_patched_fragment_{tag}.nt"));
    let out = out.to_str().unwrap();
    let result = Command::new(BIN)
        .arg("materialize")
        .arg(FIXTURE)
        .arg(out)
        .output()
        .expect("failed to spawn owl2-reasoner");
    assert!(
        result.status.success(),
        "materialize failed:\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    );
    std::fs::read_to_string(out)
        .expect("output file readable")
        .lines()
        .map(|line| line.trim().to_string())
        .filter(|line| !line.is_empty())
        .collect()
}

#[test]
fn reflexive_roles_add_declared_individual_self_pairs() {
    let closure = materialized_closure("reflexive");
    let named_individual = "<http://www.w3.org/2002/07/owl#NamedIndividual>";
    let knows = p("knows");

    // Declared individuals carry their owl:NamedIndividual type triple.
    for local in ["alice", "bob", "carol", "dave", "student1", "prog1", "T20", "hobbyA"] {
        let declaration = format!("{} {} {} .", p(local), rdf_type(), named_individual);
        assert!(
            closure.contains(&declaration),
            "missing declaration triple: {declaration}"
        );
    }

    // prp-ref: every declared individual knows itself, and only declared
    // individuals (the TBox class IRIs never appear).
    for local in ["alice", "bob", "carol", "dave", "student1", "prog1", "T20", "hobbyA", "hobbyB", "hobbyC"] {
        let self_pair = format!("{} {} {} .", p(local), knows, p(local));
        assert!(closure.contains(&self_pair), "missing self-pair: {self_pair}");
    }
    let self_pair_count = closure
        .iter()
        .filter(|line| line.contains(&knows) && {
            let parts: Vec<&str> = line.split_whitespace().collect();
            parts.len() >= 3 && parts[0] == parts[2]
        })
        .count();
    assert_eq!(self_pair_count, 15, "exactly the declared individuals");
}

#[test]
fn construct_memberships_match_the_v2_rule_table() {
    let closure = materialized_closure("constructs");

    // cls-hv2: bob is crazy about T20, so bob is a Fan.
    assert!(closure.contains(&format!("{} {} {} .", p("bob"), rdf_type(), p("Fan"))));
    assert!(!closure.contains(&format!("{} {} {} .", p("alice"), rdf_type(), p("Fan"))));

    // cls-hs + cls-intro over the reflexive role: every Person is self-aware.
    for local in ["alice", "bob", "carol", "dave"] {
        assert!(
            closure.contains(&format!("{} {} {} .", p(local), rdf_type(), p("SelfAwarePerson"))),
            "{local} must be a SelfAwarePerson"
        );
    }
    assert!(!closure.contains(&format!("{} {} {} .", p("student1"), rdf_type(), p("SelfAwarePerson"))));

    // cls-qc1 + cls-intro: student1 (Student via enrollFor's domain) has
    // exactly one qualified enrollment.
    assert!(closure.contains(&format!("{} {} {} .", p("student1"), rdf_type(), p("UGStudent"))));

    // cls-mqc3: only carol likes three distinct Interests.
    assert!(closure.contains(&format!("{} {} {} .", p("carol"), rdf_type(), p("PeopleWithManyHobbies"))));
    assert!(!closure.contains(&format!("{} {} {} .", p("bob"), rdf_type(), p("PeopleWithManyHobbies"))));
}

#[test]
fn chains_compose_and_sameas_replaces() {
    let closure = materialized_closure("chains");

    // prp-spo2-2: enrollIn ∘ isPartOf ⊑ isMemberOf, with isPartOf's
    // transitivity feeding the composition.
    let member_of = format!("{} {} {} .", p("student1"), p("isMemberOf"), p("univ0"));
    assert!(
        closure.contains(&member_of),
        "chain must compose student1's enrollment into university membership"
    );

    // owl:equivalentProperty: isPartOf ≡ locatedIn propagates assertions in
    // both directions (scm-eqp1/2 + prp-eqp1/2).
    let via_equivalence = format!("{} {} {} .", p("deptA"), p("locatedIn"), p("univ0"));
    assert!(
        closure.contains(&via_equivalence),
        "equivalent properties must propagate assertions across the pair"
    );

    // eq-sym/eq-rep: football sameAs soccer propagates the likes assertion
    // to the merged individual.
    let replaced = format!("{} {} {} .", p("erin"), p("likes"), p("soccer"));
    assert!(
        closure.contains(&replaced),
        "sameAs must replace the liked individual across assertions"
    );
    let same_as = format!(
        "{} <http://www.w3.org/2002/07/owl#sameAs> {} .",
        p("soccer"),
        p("football")
    );
    assert!(closure.contains(&same_as), "eq-sym must add the reverse sameAs");
}
