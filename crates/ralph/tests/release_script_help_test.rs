//! Release script help output contract tests.
//!
//! Responsibilities:
//! - Assert that scripts/release.sh --help exits successfully.
//! - Verify help output contains expected sections (EXAMPLES, RELEASE_DRY_RUN).
//!
//! Not handled here:
//! - Full validation of release process behavior.
//! - Testing actual release creation (requires git/credentials).
//!
//! Invariants/assumptions:
//! - The release.sh script exists at scripts/release.sh relative to repo root.
//! - Bash is available to execute the script.

use std::path::PathBuf;
use std::process::{Command, ExitStatus};

fn repo_root() -> PathBuf {
    // Start from the test executable and find the repo root
    let exe = std::env::current_exe().expect("resolve current test executable path");
    let exe_dir = exe
        .parent()
        .expect("test executable should have a parent directory");

    // Navigate up from target/{profile}/deps or target/{profile} to repo root
    let profile_dir = if exe_dir.file_name() == Some(std::ffi::OsStr::new("deps")) {
        exe_dir
            .parent()
            .expect("deps directory should have a parent directory")
    } else {
        exe_dir
    };

    profile_dir
        .parent()
        .expect("profile directory should have a parent (target)")
        .parent()
        .expect("target directory should have a parent (repo root)")
        .to_path_buf()
}

fn release_script() -> PathBuf {
    repo_root().join("scripts").join("release.sh")
}

fn run_help(args: &[&str]) -> (ExitStatus, String, String) {
    let script = release_script();
    let output = Command::new("bash")
        .arg(&script)
        .args(args)
        .output()
        .expect("failed to execute release.sh script");
    (
        output.status,
        String::from_utf8_lossy(&output.stdout).to_string(),
        String::from_utf8_lossy(&output.stderr).to_string(),
    )
}

fn assert_contains(haystack: &str, needle: &str) {
    assert!(
        haystack.contains(needle),
        "expected output to contain {needle:?}\n--- output ---\n{haystack}\n--- end ---"
    );
}

#[test]
fn release_script_help_exits_successfully() {
    let (status, stdout, stderr) = run_help(&["--help"]);
    assert!(
        status.success(),
        "expected `release.sh --help` to succeed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    let combined = format!("{stdout}\n{stderr}");

    // Verify key sections are present
    assert_contains(&combined, "Examples:");
    assert_contains(&combined, "RELEASE_DRY_RUN");
    assert_contains(&combined, "Usage:");
    assert_contains(&combined, "Arguments:");
    assert_contains(&combined, "Environment Variables:");
    assert_contains(&combined, "Prerequisites:");
    assert_contains(&combined, "Release Process:");
}

#[test]
fn release_script_short_help_exits_successfully() {
    let (status, stdout, stderr) = run_help(&["-h"]);
    assert!(
        status.success(),
        "expected `release.sh -h` to succeed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    let combined = format!("{stdout}\n{stderr}");

    // Verify key sections are present
    assert_contains(&combined, "Examples:");
    assert_contains(&combined, "RELEASE_DRY_RUN");
}

#[test]
fn release_script_no_args_shows_usage_and_exits_error() {
    let (status, stdout, stderr) = run_help(&[]);
    assert!(!status.success(), "expected `release.sh` (no args) to fail");

    let combined = format!("{stdout}\n{stderr}");

    // Should show usage information
    assert_contains(&combined, "Usage:");
    assert_contains(&combined, "VERSION is required");
}

#[test]
fn release_script_help_contains_examples() {
    let (status, stdout, stderr) = run_help(&["--help"]);
    assert!(status.success(), "expected `release.sh --help` to succeed");

    let combined = format!("{stdout}\n{stderr}");

    // Verify specific examples are present
    assert_contains(&combined, "Full release");
    assert_contains(&combined, "scripts/release.sh 0.2.0");
    assert_contains(&combined, "Dry run mode");
    assert_contains(&combined, "RELEASE_DRY_RUN=1 scripts/release.sh 0.2.0");
    assert_contains(&combined, "Show this help");
}

#[test]
fn release_script_help_contains_version_format() {
    let (status, stdout, stderr) = run_help(&["--help"]);
    assert!(status.success(), "expected `release.sh --help` to succeed");

    let combined = format!("{stdout}\n{stderr}");

    // Verify version format is documented
    assert_contains(&combined, "semver format");
    assert_contains(&combined, "0.2.0");
    assert_contains(&combined, "1.0.0");
}
