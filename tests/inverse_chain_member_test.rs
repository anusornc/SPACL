//! Contract tests for inverse property expressions as property-chain
//! members (#165): an anonymous `owl:inverseOf` blank node inside an
//! `owl:propertyChainAxiom` collection must parse and must compose in
//! the materialization closure, in any member position.

use std::collections::BTreeSet;
use std::process::Command;

const BIN: &str = env!("CARGO_BIN_EXE_owl2-reasoner");
const FIXTURE: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/data/inverse_chain_member.owl"
);

fn p(local: &str) -> String {
    format!("<http://example.org/ic#{local}>")
}

fn materialized_closure(tag: &str) -> BTreeSet<String> {
    // Unique output per test: materialize exits 0 even after printing an
    // error, so a shared path can serve a stale file from a sibling test.
    let out = std::env::temp_dir().join(format!("inverse_chain_member_{tag}.nt"));
    let _ = std::fs::remove_file(&out);
    let out = out.to_str().unwrap();
    let result = Command::new(BIN)
        .arg("materialize")
        .arg(FIXTURE)
        .arg(out)
        .output()
        .expect("failed to spawn owl2-reasoner");
    assert!(
        result.status.success(),
        "materialize failed (inverse chain member must parse, not fail closed):\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&result.stdout),
        String::from_utf8_lossy(&result.stderr)
    );
    assert!(
        std::path::Path::new(out).exists(),
        "materialize printed success but wrote no closure file:\nstdout: {}\nstderr: {}",
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
fn leading_inverse_member_composes() {
    let closure = materialized_closure("leading");
    // q(b,a) and r(b,c) must compose through inv(q) o r ⊑ s into s(a,c):
    // the chain reads inv(q) forwards from a, so the derived subject is
    // q's object, not q's subject.
    let derived = format!("{} {} {} .", p("a"), p("s"), p("c"));
    assert!(
        closure.contains(&derived),
        "leading inverse member must compose q(b,a) and r(b,c) into s(a,c)"
    );
}

#[test]
fn trailing_inverse_member_composes() {
    let closure = materialized_closure("trailing");
    // u(a,b) and v(c,b) must compose through u o inv(v) ⊑ w into w(a,c):
    // the trailing inverse member joins on v's object and produces v's
    // subject as the derived object.
    let derived = format!("{} {} {} .", p("a"), p("w"), p("c"));
    assert!(
        closure.contains(&derived),
        "trailing inverse member must compose u(a,b) and v(c,b) into w(a,c)"
    );
}

#[test]
fn blank_member_without_inverse_fails_closed() {
    // The unsupported shape must error, not silently drop the chain:
    // materialize exits 0 after printing errors, so the contract is
    // "no closure file written" plus the error on stderr.
    let malformed = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/data/inverse_chain_member_malformed.owl"
    );
    let out = std::env::temp_dir().join("inverse_chain_member_malformed.nt");
    let _ = std::fs::remove_file(&out);
    let result = Command::new(BIN)
        .arg("materialize")
        .arg(malformed)
        .arg(out.to_str().unwrap())
        .output()
        .expect("failed to spawn owl2-reasoner");
    let stderr = String::from_utf8_lossy(&result.stderr);
    assert!(
        stderr.contains("Malformed owl:propertyChainAxiom"),
        "expected the fail-closed parse error, stderr:\n{stderr}"
    );
    assert!(
        !out.exists(),
        "no closure may be written for a malformed chain member"
    );
}
