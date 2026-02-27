//! Integration tests for EPCIS Reasoner Demo
//!
//! Tests the GS1 EPCIS supply chain demo functionality

use std::process::Command;

const EPCIS_BINARY: &str = "epcis-reasoner";

/// Test that the EPCIS binary exists and runs
#[test]
fn test_epcis_binary_exists() {
    let output = Command::new("cargo")
        .args(["run", "--bin", EPCIS_BINARY, "--", "help"])
        .current_dir(".")
        .output()
        .expect("Failed to execute EPCIS reasoner");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    // Check that help output contains expected content
    assert!(
        stdout.contains("GS1 EPCIS") || stderr.contains("GS1 EPCIS"),
        "Expected GS1 EPCIS in output. stdout: {}, stderr: {}",
        stdout,
        stderr
    );
}

/// Test the generate-demo command produces valid output
#[test]
fn test_generate_demo_command() {
    let output = Command::new("cargo")
        .args(["run", "--bin", EPCIS_BINARY, "--", "generate-demo"])
        .current_dir(".")
        .output()
        .expect("Failed to execute generate-demo command");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{} {}", stdout, stderr);

    // Check for key elements in the demo output
    assert!(
        combined.contains("SUPPLY CHAIN TRACE IS VALID"),
        "Expected valid supply chain trace. Output: {}",
        combined
    );

    // Check GS1 EPCIS vocabulary is used
    assert!(
        combined.contains("https://ref.gs1.org/epcis/") || combined.contains("GS1 EPCIS"),
        "Expected GS1 EPCIS namespace. Output: {}",
        combined
    );

    // Check EPCIS event types
    assert!(
        combined.contains("ObjectEvent"),
        "Expected ObjectEvent in output. Output: {}",
        combined
    );

    // Check business steps
    assert!(
        combined.contains("commissioning")
            || combined.contains("shipping")
            || combined.contains("receiving"),
        "Expected business steps in output. Output: {}",
        combined
    );

    // Check product EPC
    assert!(
        combined.contains("urn:epc:id:sgtin:"),
        "Expected SGTIN EPC in output. Output: {}",
        combined
    );
}

/// Test the stats command
#[test]
fn test_stats_command() {
    let output = Command::new("cargo")
        .args(["run", "--bin", EPCIS_BINARY, "--", "stats"])
        .current_dir(".")
        .output()
        .expect("Failed to execute stats command");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{} {}", stdout, stderr);

    // Check for statistics output
    assert!(
        combined.contains("GS1 EPCIS") || combined.contains("EPCIS Core Classes"),
        "Expected EPCIS statistics header. Output: {}",
        combined
    );

    // Check for event types listed
    assert!(
        combined.contains("ObjectEvent") && combined.contains("AggregationEvent"),
        "Expected event types in stats. Output: {}",
        combined
    );
}

/// Test the check-consistency command
#[test]
fn test_check_consistency_command() {
    let output = Command::new("cargo")
        .args(["run", "--bin", EPCIS_BINARY, "--", "check-consistency"])
        .current_dir(".")
        .output()
        .expect("Failed to execute check-consistency command");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{} {}", stdout, stderr);

    // Check for consistency check output
    assert!(
        combined.contains("CONSISTENT") || combined.contains("consistent"),
        "Expected consistency result. Output: {}",
        combined
    );

    // Check GS1 namespace is mentioned
    assert!(
        combined.contains("https://ref.gs1.org/epcis/") || combined.contains("GS1 EPCIS"),
        "Expected GS1 namespace in consistency check. Output: {}",
        combined
    );
}

/// Test that unknown commands are handled gracefully
#[test]
fn test_unknown_command() {
    let output = Command::new("cargo")
        .args(["run", "--bin", EPCIS_BINARY, "--", "unknown-command"])
        .current_dir(".")
        .output()
        .expect("Failed to execute command");

    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let combined = format!("{} {}", stdout, stderr);

    // Should show error or usage
    assert!(
        combined.contains("Error") || combined.contains("Unknown") || combined.contains("Usage"),
        "Expected error message for unknown command. Output: {}",
        combined
    );
}

/// Test EPCIS vocabulary correctness - verify the demo uses correct URIs
#[test]
fn test_epcis_vocabulary_correctness() {
    let output = Command::new("cargo")
        .args(["run", "--bin", EPCIS_BINARY, "--", "generate-demo"])
        .current_dir(".")
        .output()
        .expect("Failed to execute generate-demo");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{} {}", stdout, stderr);

    // Verify no placeholder URLs are used
    assert!(
        !combined.contains("http://example.org/epcis#"),
        "Should not use placeholder URLs. Output: {}",
        combined
    );

    // Verify real GS1 namespace is used
    assert!(
        combined.contains("https://ref.gs1.org/epcis/") || combined.contains("GS1 EPCIS 2.0"),
        "Should use real GS1 EPCIS namespace. Output: {}",
        combined
    );

    // Verify CBV vocabulary reference
    assert!(
        combined.contains("https://ref.gs1.org/cbv/") || combined.contains("GS1 CBV"),
        "Should reference GS1 CBV vocabulary. Output: {}",
        combined
    );
}
