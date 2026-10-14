use std::io::Write;
use std::process::{Command, Stdio};

use serde_json::{json, Value};

fn run_probe(request: Value) -> Value {
    let mut child = Command::new(env!("CARGO_BIN_EXE_prototype_spacl_probe"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .expect("prototype probe should start");

    child
        .stdin
        .as_mut()
        .expect("stdin should be available")
        .write_all(request.to_string().as_bytes())
        .expect("request should be written");

    let output = child
        .wait_with_output()
        .expect("prototype probe should finish");
    assert!(output.status.success(), "probe failed: {output:?}");
    serde_json::from_slice(&output.stdout).expect("probe stdout should be one JSON response")
}

fn request(conflict: bool, mode: &str) -> Value {
    json!({
        "input": {
            "kind": "synthetic",
            "unions": 4,
            "operands": 2,
            "padding": 200,
            "conflict": conflict
        },
        "mode": mode,
        "workers": 2,
        "parallel_threshold": 1,
        "cost_threshold_us": 1,
        "timeout_ms": 2000
    })
}

#[test]
fn synthetic_sat_matches_sequential_and_forced_parallel() {
    let sequential = run_probe(request(false, "sequential"));
    let parallel = run_probe(request(false, "always_parallel"));

    assert_eq!(sequential["status"], "success");
    assert_eq!(parallel["status"], "success");
    assert_eq!(sequential["verdict"]["result"], "consistent");
    assert_eq!(
        parallel["verdict"]["result"],
        sequential["verdict"]["result"]
    );
    assert_eq!(parallel["used_parallel"], false);
    assert_eq!(parallel["boolean_certificate"], true);
    assert_eq!(parallel["branches_created"], 0);
    assert_eq!(parallel["ontology_sha256"], sequential["ontology_sha256"]);
}

#[test]
fn synthetic_unsat_matches_sequential_and_forced_parallel() {
    let sequential = run_probe(request(true, "sequential"));
    let parallel = run_probe(request(true, "always_parallel"));

    assert_eq!(sequential["status"], "success");
    assert_eq!(parallel["status"], "success");
    assert_eq!(sequential["verdict"]["result"], "inconsistent");
    assert_eq!(
        parallel["verdict"]["result"],
        sequential["verdict"]["result"]
    );
    assert_eq!(parallel["used_parallel"], false);
    assert_eq!(parallel["boolean_certificate"], false);
    assert_eq!(parallel["branches_created"], 0);
    assert_eq!(parallel["ontology_sha256"], sequential["ontology_sha256"]);
}

#[test]
fn invalid_workers_return_stable_error_envelope() {
    let response = run_probe(json!({
        "input": {
            "kind": "synthetic",
            "unions": 1,
            "operands": 2,
            "padding": 0,
            "conflict": false
        },
        "mode": "adaptive",
        "workers": 0,
        "parallel_threshold": 2000,
        "cost_threshold_us": 500
    }));

    assert_eq!(response["status"], "error");
    assert_eq!(response["verdict"]["result"], "error");
    assert_eq!(response["verdict"]["error_class"], "InvalidWorkers");
}

#[test]
fn file_input_hash_is_stable_across_processes() {
    let root = env!("CARGO_MANIFEST_DIR");
    let path = format!("{root}/tests/data/consistent_subclass_cycle.owl");
    let request = json!({
        "input": {
            "kind": "file",
            "path": path
        },
        "mode": "sequential",
        "workers": 2,
        "parallel_threshold": 2000,
        "cost_threshold_us": 500,
        "timeout_ms": 2000
    });

    let first = run_probe(request.clone());
    let second = run_probe(request);

    assert_eq!(first["status"], "success");
    assert_eq!(second["status"], "success");
    assert_eq!(first["ontology_sha256"], second["ontology_sha256"]);
    assert!(first["ontology_sha256"].as_str().unwrap().len() == 64);
}

#[test]
fn synthetic_padding_changes_actual_hash_and_scheduler_stats() {
    let mut unpadded_request = request(false, "sequential");
    unpadded_request["input"]["padding"] = json!(0);

    let mut padded_request = request(false, "sequential");
    padded_request["input"]["padding"] = json!(200);

    let unpadded = run_probe(unpadded_request);
    let padded = run_probe(padded_request);

    assert_eq!(unpadded["status"], "success");
    assert_eq!(padded["status"], "success");
    assert_ne!(unpadded["ontology_sha256"], padded["ontology_sha256"]);
    assert_eq!(
        unpadded["features"]["disjunctions"],
        padded["features"]["disjunctions"]
    );
    assert!(
        padded["features"]["branches"].as_u64().unwrap()
            > unpadded["features"]["branches"].as_u64().unwrap()
    );
}
