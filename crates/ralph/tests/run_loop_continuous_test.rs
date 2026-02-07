//! Integration tests for continuous execution mode (`--wait-when-empty` / `--continuous`).
//!
//! Responsibilities:
//! - Test CLI argument parsing for continuous mode flags
//!
//! Not handled here:
//! - Full lifecycle tests (requires mock runner)
//! - Parallel mode tests

use std::process::Command;

mod test_support;

/// Test that --wait-when-empty flag parses correctly.
#[test]
fn run_loop_wait_when_empty_flag_parses() {
    let ralph = test_support::ralph_bin();

    // Test that the flag is recognized (will fail for other reasons, not unknown flag)
    let output = Command::new(&ralph)
        .arg("run")
        .arg("loop")
        .arg("--wait-when-empty")
        .arg("--help")
        .output()
        .expect("Failed to run ralph");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    // Should succeed (help works)
    assert!(
        output.status.success(),
        "--wait-when-empty flag should be recognized. stdout: {}, stderr: {}",
        stdout,
        stderr
    );

    // Help should mention the flag
    assert!(
        stdout.contains("wait-when-empty") || stdout.contains("continuous"),
        "Help should mention continuous mode flags"
    );
}

/// Test that --continuous alias parses correctly.
#[test]
fn run_loop_continuous_alias_parses() {
    let ralph = test_support::ralph_bin();

    // Test that the alias is recognized
    let output = Command::new(&ralph)
        .arg("run")
        .arg("loop")
        .arg("--continuous")
        .arg("--help")
        .output()
        .expect("Failed to run ralph");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    // Should succeed (help works)
    assert!(
        output.status.success(),
        "--continuous flag should be recognized. stdout: {}, stderr: {}",
        stdout,
        stderr
    );
}

/// Test that --empty-poll-ms flag parses correctly.
#[test]
fn run_loop_empty_poll_ms_flag_parses() {
    let ralph = test_support::ralph_bin();

    // Test that the flag is recognized
    let output = Command::new(&ralph)
        .arg("run")
        .arg("loop")
        .arg("--empty-poll-ms")
        .arg("5000")
        .arg("--help")
        .output()
        .expect("Failed to run ralph");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    // Should succeed (help works)
    assert!(
        output.status.success(),
        "--empty-poll-ms flag should be recognized. stdout: {}, stderr: {}",
        stdout,
        stderr
    );

    // Help should mention the flag
    assert!(
        stdout.contains("empty-poll-ms"),
        "Help should mention empty-poll-ms flag"
    );
}

/// Test that --wait-when-empty conflicts with --parallel.
#[test]
fn run_loop_wait_when_empty_conflicts_with_parallel() {
    let ralph = test_support::ralph_bin();

    let output = Command::new(&ralph)
        .arg("run")
        .arg("loop")
        .arg("--wait-when-empty")
        .arg("--parallel")
        .output()
        .expect("Failed to run ralph");

    let stderr = String::from_utf8_lossy(&output.stderr);

    // Should fail due to conflict
    assert!(
        !output.status.success(),
        "--wait-when-empty should conflict with --parallel"
    );
    assert!(
        stderr.contains("cannot be used with") || stderr.contains("conflict"),
        "Error should mention conflict: {}",
        stderr
    );
}

/// Test that --wait-when-empty conflicts with --interactive.
#[test]
fn run_loop_wait_when_empty_conflicts_with_interactive() {
    let ralph = test_support::ralph_bin();

    let output = Command::new(&ralph)
        .arg("run")
        .arg("loop")
        .arg("--wait-when-empty")
        .arg("--interactive")
        .output()
        .expect("Failed to run ralph");

    let stderr = String::from_utf8_lossy(&output.stderr);

    // Should fail due to conflict
    assert!(
        !output.status.success(),
        "--wait-when-empty should conflict with --interactive"
    );
    assert!(
        stderr.contains("cannot be used with") || stderr.contains("conflict"),
        "Error should mention conflict: {}",
        stderr
    );
}
