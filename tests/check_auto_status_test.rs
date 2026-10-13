use std::path::Path;
use std::process::{Command, Output};

use serde_json::Value;

const ENV_KEYS: &[&str] = &[
    "OWL2_REASONER_FORCE_TEXT",
    "OWL2_REASONER_AUTO_CACHE",
    "OWL2_REASONER_BIN_ONLY",
    "SPACL_SCHED_MODE",
    "SPACL_BRANCH_POLICY",
    "SPACL_NOGOOD",
];

fn fixture(name: &str) -> String {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/data/benchmark_status")
        .join(name)
        .to_string_lossy()
        .into_owned()
}

fn run_check_auto(input: &str, report: &Path) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_owl2-reasoner"));
    command.args(["check-auto", input, "--report-json"]);
    command.arg(report);
    for key in ENV_KEYS {
        command.env_remove(key);
    }
    command.output().unwrap()
}

fn read_report(path: &Path) -> Value {
    let bytes = std::fs::read(path).unwrap();
    serde_json::from_slice(&bytes).unwrap()
}

fn assert_common_report(report: &Value, input: &str) {
    assert_eq!(report["schema_version"], 1);
    assert_eq!(report["report_type"], "tableauxx_check_auto");
    assert_eq!(report["operation"], "consistency");
    assert_eq!(report["input_file"], input);
    assert_eq!(report["correctness_status"], "not_evaluated");
    assert!(report["duration_ms"].as_u64().is_some());
}

#[test]
fn help_documents_check_auto_report_and_exit_contract() {
    let output = Command::new(env!("CARGO_BIN_EXE_owl2-reasoner"))
        .arg("help")
        .output()
        .unwrap();
    assert!(output.status.success(), "help failed: {:?}", output);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("--report-json <new-file>"));
    assert!(stdout.contains("create-new JSON report"));
    assert!(stdout.contains("0 success, 1 failed, 2 unsupported, 124 timeout"));
}

#[test]
fn check_auto_missing_input_does_not_exit_success() {
    let directory = tempfile::tempdir().unwrap();
    let missing = directory.path().join("missing.owl");
    let output = Command::new(env!("CARGO_BIN_EXE_owl2-reasoner"))
        .arg("check-auto")
        .arg(&missing)
        .output()
        .unwrap();
    assert!(
        !output.status.success(),
        "load failure exited successfully: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn check_auto_missing_input_writes_failed_report_when_report_path_is_distinct() {
    let directory = tempfile::tempdir().unwrap();
    let missing = directory.path().join("missing.owl");
    let input = missing.to_string_lossy().into_owned();
    let report_path = directory.path().join("missing-report.json");

    let output = run_check_auto(&input, &report_path);
    assert_eq!(output.status.code(), Some(1), "CLI output: {:?}", output);
    assert!(!missing.exists());

    let report = read_report(&report_path);
    assert_common_report(&report, &input);
    assert_eq!(report["status"], "failed");
    assert_eq!(report["reasoning_result"], "error");
    assert!(report["selected_reasoner"].is_null());
    assert!(report["parse_time_ms"].is_null());
    assert!(report["reason_time_ms"].is_null());
    assert!(report["error"].as_str().unwrap().contains("File not found"));
}

#[test]
fn check_auto_report_refuses_relative_alias_of_missing_input_before_creation() {
    let directory = tempfile::tempdir().unwrap();
    let mut command = Command::new(env!("CARGO_BIN_EXE_owl2-reasoner"));
    command.current_dir(directory.path());
    command.args([
        "check-auto",
        "missing.owl",
        "--report-json",
        "./missing.owl",
    ]);
    let output = command.output().unwrap();

    assert_eq!(output.status.code(), Some(1), "CLI output: {:?}", output);
    assert!(!directory.path().join("missing.owl").exists());
    assert!(String::from_utf8_lossy(&output.stderr).contains("must not target the input file"));
}

#[test]
fn check_auto_mixed_targeted_fragment_decides_consistent() {
    // #113: the bounded qualified checker declining its fragment used
    // to surface as unsupported (non-zero exit); the sequential layers
    // now decide, and this satisfiable oneOf definition is consistent.
    let input = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/data/uberon_oneof_minimal.owl");
    let output = Command::new(env!("CARGO_BIN_EXE_owl2-reasoner"))
        .arg("check-auto")
        .arg(&input)
        .env("OWL2_REASONER_FORCE_TEXT", "1")
        .env("OWL2_REASONER_AUTO_CACHE", "0")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "satisfiable mixed fragment should decide: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn check_auto_report_success_consistent() {
    let directory = tempfile::tempdir().unwrap();
    let input = fixture("status_consistent.owl");
    let report_path = directory.path().join("consistent.json");

    let output = run_check_auto(&input, &report_path);
    assert!(output.status.success(), "CLI failed: {:?}", output);

    let report = read_report(&report_path);
    assert_common_report(&report, &input);
    assert_eq!(report["status"], "success");
    assert_eq!(report["reasoning_result"], "consistent");
    assert_eq!(report["selected_reasoner"], "SimpleReasoner");
    assert!(report["parse_time_ms"].as_u64().is_some());
    assert!(report["reason_time_ms"].as_u64().is_some());
    assert!(report["error"].is_null());
}

#[test]
fn check_auto_report_success_inconsistent_is_not_correctness_pass() {
    let directory = tempfile::tempdir().unwrap();
    let input = fixture("status_inconsistent.owl");
    let report_path = directory.path().join("inconsistent.json");

    let output = run_check_auto(&input, &report_path);
    assert!(output.status.success(), "CLI failed: {:?}", output);

    let report = read_report(&report_path);
    assert_common_report(&report, &input);
    assert_eq!(report["status"], "success");
    assert_eq!(report["reasoning_result"], "inconsistent");
    assert_eq!(report["correctness_status"], "not_evaluated");
    assert_eq!(report["selected_reasoner"], "SimpleReasoner");
    assert!(report["parse_time_ms"].as_u64().is_some());
    assert!(report["reason_time_ms"].as_u64().is_some());
    assert!(report["error"].is_null());
}

#[test]
fn check_auto_report_parse_error_fails_without_stage_timings() {
    let directory = tempfile::tempdir().unwrap();
    let input = fixture("status_parse_error.owl");
    let report_path = directory.path().join("parse-error.json");

    let output = run_check_auto(&input, &report_path);
    assert_eq!(output.status.code(), Some(1), "CLI output: {:?}", output);

    let report = read_report(&report_path);
    assert_common_report(&report, &input);
    assert_eq!(report["status"], "failed");
    assert_eq!(report["reasoning_result"], "error");
    assert!(report["selected_reasoner"].is_null());
    assert!(report["parse_time_ms"].is_null());
    assert!(report["reason_time_ms"].is_null());
    assert!(report["error"].as_str().unwrap().contains("literal member"));
}

#[test]
fn check_auto_report_decides_status_unsupported_fixture() {
    // #113: this fixture (a targeted oneOf plus an ordinary TBox) used
    // to exercise the unsupported pathway; the sequential layers now
    // decide it, and it is consistent.
    let directory = tempfile::tempdir().unwrap();
    let input = fixture("status_unsupported.owl");
    let report_path = directory.path().join("decided.json");

    let output = run_check_auto(&input, &report_path);
    assert_eq!(output.status.code(), Some(0), "CLI output: {:?}", output);

    let report = read_report(&report_path);
    assert_common_report(&report, &input);
    assert_eq!(report["status"], "success");
    assert_eq!(report["reasoning_result"], "consistent");
    assert_eq!(report["selected_reasoner"], "SimpleReasoner");
    assert!(report["parse_time_ms"].as_u64().is_some());
}

#[test]
fn check_auto_existing_report_path_is_rejected_without_overwrite() {
    let directory = tempfile::tempdir().unwrap();
    let input = fixture("status_consistent.owl");
    let report_path = directory.path().join("stale.json");
    std::fs::write(&report_path, b"stale-report").unwrap();

    let output = run_check_auto(&input, &report_path);
    assert_eq!(output.status.code(), Some(1), "CLI output: {:?}", output);
    assert_eq!(std::fs::read(&report_path).unwrap(), b"stale-report");
}

#[test]
fn check_auto_invalid_spacl_config_is_ignored_by_simple_route() {
    let directory = tempfile::tempdir().unwrap();
    let input = fixture("status_consistent.owl");
    let report_path = directory.path().join("simple-ignores-config.json");

    let mut command = Command::new(env!("CARGO_BIN_EXE_owl2-reasoner"));
    command.args(["check-auto", &input, "--report-json"]);
    command.arg(&report_path);
    for key in ENV_KEYS {
        command.env_remove(key);
    }
    command.env("SPACL_BRANCH_POLICY", "not-a-policy");
    let output = command.output().unwrap();
    assert!(output.status.success(), "CLI failed: {:?}", output);

    let report = read_report(&report_path);
    assert_eq!(report["status"], "success");
    assert_eq!(report["selected_reasoner"], "SimpleReasoner");
}
