//! Test that .env load errors (except NotFound) emit warnings.
//!
//! Purpose:
//! - Test that .env load errors (except NotFound) emit warnings.
//!
//! Responsibilities:
//! - Verify that missing .env files do NOT produce warnings.
//! - Verify that invalid .env files DO produce warnings while still allowing success.
//!
//! Not handled here:
//! - Testing the actual content loading from valid .env files.
//! - Testing environment variable propagation.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - The Ralph binary is built and discoverable by the test harness.

mod test_support;

use std::process::Command;

/// Test that running ralph without a .env file produces no warning.
#[test]
fn no_warning_when_env_file_missing() {
    let temp_dir = test_support::temp_dir_outside_repo();

    // Initialize a minimal ralph project
    test_support::seed_ralph_dir(temp_dir.path()).expect("init should succeed");

    // Run ralph --help from the temp dir (no .env file exists)
    let output = Command::new(test_support::ralph_bin())
        .current_dir(temp_dir.path())
        .args(["--help"])
        .output()
        .expect("failed to execute ralph binary");

    let stderr = String::from_utf8_lossy(&output.stderr);

    // Should succeed
    assert!(output.status.success(), "expected success, got: {stderr}");

    // Should NOT contain any .env warning
    assert!(
        !stderr.contains(".env") && !stderr.to_lowercase().contains("failed to load"),
        "expected no .env warning when file is missing, but stderr was: {stderr}"
    );
}

/// Test that running ralph with an invalid .env file produces a warning but still succeeds.
#[test]
fn warning_when_env_file_invalid() {
    let temp_dir = test_support::temp_dir_outside_repo();

    // Initialize a minimal ralph project
    test_support::seed_ralph_dir(temp_dir.path()).expect("init should succeed");

    // Create an invalid .env file (bad syntax)
    let env_content = r#"INVALID LINE WITHOUT EQUALS SIGN
VALID_KEY=valid_value
"#;
    std::fs::write(temp_dir.path().join(".env"), env_content).expect("write invalid .env file");

    // Run ralph --help from the temp dir (invalid .env file exists)
    let output = Command::new(test_support::ralph_bin())
        .current_dir(temp_dir.path())
        .args(["--help"])
        .output()
        .expect("failed to execute ralph binary");

    let stderr = String::from_utf8_lossy(&output.stderr);

    // Should still succeed (exit 0)
    assert!(
        output.status.success(),
        "expected success even with invalid .env, got: {stderr}"
    );

    // Should contain a warning about .env file
    assert!(
        stderr.to_lowercase().contains(".env") || stderr.to_lowercase().contains("warning"),
        "expected .env warning when file is invalid, but stderr was: {stderr}"
    );
}

/// Test that running ralph with an empty .env file produces no warning.
#[test]
fn no_warning_when_env_file_empty() {
    let temp_dir = test_support::temp_dir_outside_repo();

    // Initialize a minimal ralph project
    test_support::seed_ralph_dir(temp_dir.path()).expect("init should succeed");

    // Create an empty .env file
    std::fs::write(temp_dir.path().join(".env"), "").expect("write empty .env file");

    // Run ralph --help from the temp dir
    let output = Command::new(test_support::ralph_bin())
        .current_dir(temp_dir.path())
        .args(["--help"])
        .output()
        .expect("failed to execute ralph binary");

    let stderr = String::from_utf8_lossy(&output.stderr);

    // Should succeed
    assert!(output.status.success(), "expected success, got: {stderr}");

    // Should NOT contain any .env warning (empty file is valid)
    assert!(
        !stderr.to_lowercase().contains("failed to load"),
        "expected no warning for empty .env file, but stderr was: {stderr}"
    );
}

/// Test that running ralph with a valid .env file produces no warning.
#[test]
fn no_warning_when_env_file_valid() {
    let temp_dir = test_support::temp_dir_outside_repo();

    // Initialize a minimal ralph project
    test_support::seed_ralph_dir(temp_dir.path()).expect("init should succeed");

    // Create a valid .env file
    let env_content = r#"RUST_LOG=info
SOME_VAR=some_value
# This is a comment
ANOTHER_VAR=another_value
"#;
    std::fs::write(temp_dir.path().join(".env"), env_content).expect("write valid .env file");

    // Run ralph --help from the temp dir
    let output = Command::new(test_support::ralph_bin())
        .current_dir(temp_dir.path())
        .args(["--help"])
        .output()
        .expect("failed to execute ralph binary");

    let stderr = String::from_utf8_lossy(&output.stderr);

    // Should succeed
    assert!(output.status.success(), "expected success, got: {stderr}");

    // Should NOT contain any .env warning
    assert!(
        !stderr.to_lowercase().contains("failed to load"),
        "expected no warning for valid .env file, but stderr was: {stderr}"
    );
}
