//! Contract tests for CLI exit-status integrity (#174, Phase 4
//! hardening): a command that prints `Error:` must exit nonzero —
//! callers cannot detect failure by status alone otherwise. Only
//! genuinely successful completions (and bare usage/help) exit 0.

use std::process::Command;

const BIN: &str = env!("CARGO_BIN_EXE_owl2-reasoner");
const FIXTURE: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/data/existential_gci_probe.owl"
);
const MISSING: &str = "/tmp/tableauxx-definitely-missing-input.owl";

fn run(args: &[&str]) -> std::process::Output {
    Command::new(BIN)
        .args(args)
        .output()
        .expect("failed to spawn owl2-reasoner")
}

#[test]
fn missing_input_fails_nonzero_for_every_sibling_command() {
    let cases: &[&[&str]] = &[
        &["materialize", MISSING, "/tmp/tableauxx-w174-out.tsv"],
        &["stats", MISSING],
        &["check", MISSING],
        &["evaluate", "consistency", MISSING],
        &["convert", MISSING, "/tmp/tableauxx-w174-out.owl"],
    ];
    for args in cases {
        let output = run(args);
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(
            !output.status.success(),
            "{args:?} must exit nonzero on a missing input (got {:?})",
            output.status.code()
        );
        // evaluate reports its failure through the structured response
        // on stdout; the siblings print Error on stderr.
        let combined = format!("{stdout}{stderr}");
        assert!(
            combined.contains("Error")
                || combined.contains("not found")
                || combined.contains("No such")
                || combined.contains("\"status\":\"failed\""),
            "{args:?} must say why it failed: {combined}"
        );
    }
}

#[test]
fn bad_option_fails_nonzero() {
    let output = run(&["materialize", FIXTURE, "/tmp/tableauxx-w174-out.tsv", "--bogus"]);
    assert!(
        !output.status.success(),
        "an unrecognized option must exit nonzero"
    );
}

#[test]
fn successful_materialize_still_exits_zero() {
    let out = std::env::temp_dir().join("tableauxx_w174_ok.tsv");
    let _ = std::fs::remove_file(&out);
    let output = run(&["materialize", FIXTURE, out.to_str().unwrap()]);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success(),
        "a successful materialize must still exit 0:\n{stdout}\n{stderr}"
    );
    assert!(out.exists(), "the output file must exist on success");
}

#[test]
fn bare_usage_stays_zero() {
    let output = run(&[]);
    assert!(
        output.status.success(),
        "printing usage with no arguments stays exit 0"
    );
}
