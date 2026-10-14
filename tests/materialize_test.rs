//! Contract tests for the `materialize` CLI command (#83).
//!
//! The command applies RL-profile reasoning to an RDF/XML ontology and
//! serializes the closure as N-Triples. The mini fixture encodes a known
//! closure over the documented rule fragment: subclass transitivity and
//! propagation, equivalent-class expansion, sub-property propagation,
//! domain/range typing, and declared symmetric/transitive properties.

use std::collections::BTreeSet;
use std::process::Command;

const BIN: &str = env!("CARGO_BIN_EXE_owl2-reasoner");
const FIXTURE: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/data/materialize_mini.owl");

fn materialize_to(output: &str) -> (std::process::Output, String) {
    let result = Command::new(BIN)
        .arg("materialize")
        .arg(FIXTURE)
        .arg(output)
        .output()
        .expect("failed to spawn owl2-reasoner");
    let stdout = String::from_utf8_lossy(&result.stdout).into_owned();
    (result, stdout)
}

fn read_lines(path: &str) -> BTreeSet<String> {
    let text = std::fs::read_to_string(path).expect("output file readable");
    text.lines()
        .map(|line| line.trim().to_string())
        .filter(|line| !line.is_empty())
        .collect()
}

fn ex(local: &str) -> String {
    format!("<http://example.org/m#{local}>")
}

#[test]
fn materialize_mini_closure_matches_expected_triples() {
    let out = std::env::temp_dir().join("materialize_mini_a.nt");
    let out = out.to_str().unwrap();
    let (result, stdout) = materialize_to(out);
    assert!(
        result.status.success(),
        "materialize failed:\nstdout: {stdout}\nstderr: {}",
        String::from_utf8_lossy(&result.stderr)
    );

    let rdf_type = "<http://www.w3.org/1999/02/22-rdf-syntax-ns#type>";
    let sub_class = "<http://www.w3.org/2000/01/rdf-schema#subClassOf>";
    let domain = "<http://www.w3.org/2000/01/rdf-schema#domain>";
    let range = "<http://www.w3.org/2000/01/rdf-schema#range>";
    let sub_prop = "<http://www.w3.org/2000/01/rdf-schema#subPropertyOf>";
    let equiv = "<http://www.w3.org/2002/07/owl#equivalentClass>";
    let sym = "<http://www.w3.org/2002/07/owl#SymmetricProperty>";
    let trans = "<http://www.w3.org/2002/07/owl#TransitiveProperty>";

    let expected: BTreeSet<String> = [
        // Typed node elements are rdf:type triples: every declared
        // individual carries owl:NamedIndividual (the patched-track
        // reflexive universe).
        format!("{} {} <http://www.w3.org/2002/07/owl#NamedIndividual> .", ex("acme"), rdf_type),
        format!("{} {} <http://www.w3.org/2002/07/owl#NamedIndividual> .", ex("alice"), rdf_type),
        format!("{} {} <http://www.w3.org/2002/07/owl#NamedIndividual> .", ex("berlin"), rdf_type),
        format!("{} {} <http://www.w3.org/2002/07/owl#NamedIndividual> .", ex("bob"), rdf_type),
        format!("{} {} <http://www.w3.org/2002/07/owl#NamedIndividual> .", ex("carol"), rdf_type),
        format!("{} {} <http://www.w3.org/2002/07/owl#NamedIndividual> .", ex("dave"), rdf_type),
        format!("{} {} <http://www.w3.org/2002/07/owl#NamedIndividual> .", ex("erin"), rdf_type),
        format!("{} {} <http://www.w3.org/2002/07/owl#NamedIndividual> .", ex("germany"), rdf_type),
        format!("{} {} <http://www.w3.org/2002/07/owl#NamedIndividual> .", ex("profX"), rdf_type),
        // TBox base
        format!("{} {} {} .", ex("Man"), sub_class, ex("Person")),
        format!("{} {} {} .", ex("Student"), sub_class, ex("Person")),
        format!("{} {} {} .", ex("GradStudent"), sub_class, ex("Student")),
        format!("{} {} {} .", ex("Foo"), equiv, ex("Bar")),
        format!("{} {} {} .", ex("knows"), rdf_type, sym),
        format!("{} {} {} .", ex("knows"), domain, ex("Person")),
        format!("{} {} {} .", ex("worksFor"), domain, ex("Person")),
        format!("{} {} {} .", ex("worksFor"), range, ex("Organization")),
        format!("{} {} {} .", ex("worksFor"), sub_prop, ex("affiliatedWith")),
        format!("{} {} {} .", ex("locatedIn"), rdf_type, trans),
        format!(
            "{} <http://www.w3.org/2002/07/owl#inverseOf> {} .",
            ex("evaluatedBy"),
            ex("evaluates")
        ),
        format!("{} {} {} .", ex("evaluatedBy"), domain, ex("Student")),
        // Metamodel base from the ontology header declaration
        format!(
            "<http://example.org/m> {} <http://www.w3.org/2002/07/owl#Ontology> .",
            rdf_type
        ),
        // ABox base
        format!("{} {} {} .", ex("bob"), rdf_type, ex("Man")),
        format!("{} {} {} .", ex("bob"), ex("knows"), ex("alice")),
        format!("{} {} {} .", ex("bob"), ex("worksFor"), ex("acme")),
        format!("{} {} {} .", ex("alice"), rdf_type, ex("GradStudent")),
        format!("{} {} {} .", ex("carol"), rdf_type, ex("Foo")),
        format!(
            "{} {} \"42\"^^<http://www.w3.org/2001/XMLSchema#integer> .",
            ex("dave"),
            ex("hasAge")
        ),
        format!("{} {} {} .", ex("acme"), ex("locatedIn"), ex("berlin")),
        // Anonymous individual (rdf:nodeID) serializes as a blank node label
        // (the parser's internal label convention, deterministic)
        format!("{} {} _:__place1 .", ex("erin"), ex("locatedIn")),
        format!("{} {} {} .", ex("profX"), ex("evaluates"), ex("alice")),
        format!("{} {} {} .", ex("berlin"), ex("locatedIn"), ex("germany")),
        // Derived: class hierarchy (equivalence cycles imply reflexive subsumption)
        format!("{} {} {} .", ex("GradStudent"), sub_class, ex("Person")),
        format!("{} {} {} .", ex("Foo"), sub_class, ex("Bar")),
        format!("{} {} {} .", ex("Bar"), sub_class, ex("Foo")),
        format!("{} {} {} .", ex("Foo"), sub_class, ex("Foo")),
        format!("{} {} {} .", ex("Bar"), sub_class, ex("Bar")),
        // Derived: instance typing
        format!("{} {} {} .", ex("bob"), rdf_type, ex("Person")),
        format!("{} {} {} .", ex("alice"), rdf_type, ex("Student")),
        format!("{} {} {} .", ex("alice"), rdf_type, ex("Person")),
        format!("{} {} {} .", ex("carol"), rdf_type, ex("Bar")),
        format!("{} {} {} .", ex("acme"), rdf_type, ex("Organization")),
        // Derived: property closure
        format!("{} {} {} .", ex("alice"), ex("knows"), ex("bob")),
        format!("{} {} {} .", ex("bob"), ex("affiliatedWith"), ex("acme")),
        format!("{} {} {} .", ex("acme"), ex("locatedIn"), ex("germany")),
        // Derived: inverse property and its domain typing (prp-inv + prp-dom)
        format!("{} {} {} .", ex("alice"), ex("evaluatedBy"), ex("profX")),
        format!("{} {} {} .", ex("alice"), rdf_type, ex("Student")),
    ]
    .into_iter()
    .collect();

    let actual = read_lines(out);
    let missing: Vec<_> = expected.difference(&actual).collect();
    let extra: Vec<_> = actual.difference(&expected).collect();
    assert!(
        missing.is_empty() && extra.is_empty(),
        "closure mismatch\nmissing (expected, not produced):\n  {}\nextra (produced, not expected):\n  {}",
        missing.iter().map(|s| s.as_str()).collect::<Vec<_>>().join("\n  "),
        extra.iter().map(|s| s.as_str()).collect::<Vec<_>>().join("\n  ")
    );

    // The stats line must report the closure triple count.
    assert!(stdout.contains("closure_triples"), "stats line missing closure_triples: {stdout}");
}

#[test]
fn materialize_output_is_byte_deterministic() {
    let dir = std::env::temp_dir();
    let out_a = dir.join("materialize_mini_d1.nt");
    let out_b = dir.join("materialize_mini_d2.nt");
    let (result, stdout) = materialize_to(out_a.to_str().unwrap());
    assert!(result.status.success(), "first run failed: {stdout}");
    let (result, stdout) = materialize_to(out_b.to_str().unwrap());
    assert!(result.status.success(), "second run failed: {stdout}");

    let a = std::fs::read(out_a).expect("first output readable");
    let b = std::fs::read(out_b).expect("second output readable");
    assert_eq!(a, b, "materialize output must be byte-identical across runs");
}

#[test]
#[ignore = "expensive: runs the full RL closure over the seed-1 anchor"]
fn materialize_seed1_anchor_smoke() {
    let anchor = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/benchmarks/ontologies/external/owl2bench/staged/0005_UNIV-BENCH-OWL2RL-seed1-u1.owl"
    );
    if !std::path::Path::new(anchor).is_file() {
        panic!("seed-1 anchor is missing from the repository tree: {anchor}");
    }
    let out = std::env::temp_dir().join("materialize_seed1_smoke.nt");
    let started = std::time::Instant::now();
    let result = Command::new(BIN)
        .arg("materialize")
        .arg(anchor)
        .arg(out.to_str().unwrap())
        .output()
        .expect("failed to spawn owl2-reasoner");
    let elapsed = started.elapsed();
    assert!(
        result.status.success(),
        "seed-1 materialize failed:\n{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(
        elapsed.as_secs() < 120,
        "seed-1 smoke exceeded the wall-clock bound: {elapsed:?}"
    );
    let stdout = String::from_utf8_lossy(&result.stdout).into_owned();
    eprintln!("seed-1 smoke stats: {stdout}");
    eprintln!("seed-1 smoke wall time: {elapsed:?}");
    let count = read_lines(out.to_str().unwrap()).len();
    eprintln!("seed-1 smoke distinct triples: {count}");
    assert!(count > 40_000, "seed-1 closure suspiciously small: {count}");
}
