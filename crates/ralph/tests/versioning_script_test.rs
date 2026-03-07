//! Versioning script contract tests.
//!
//! Responsibilities:
//! - Verify scripts/versioning.sh can print the canonical repo version.
//! - Verify scripts/versioning.sh check succeeds when metadata is synchronized.
//!
//! Not handled here:
//! - Full release flow behavior.
//! - Mutating version sync operations.
//!
//! Invariants/assumptions:
//! - The versioning.sh script exists at scripts/versioning.sh relative to repo root.
//! - The checked-in repo metadata is synchronized before tests run.

use std::path::PathBuf;
use std::process::{Command, ExitStatus};

fn repo_root() -> PathBuf {
    let exe = std::env::current_exe().expect("resolve current test executable path");
    let exe_dir = exe
        .parent()
        .expect("test executable should have a parent directory");

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

fn version_script() -> PathBuf {
    repo_root().join("scripts").join("versioning.sh")
}

fn canonical_version() -> String {
    std::fs::read_to_string(repo_root().join("VERSION"))
        .expect("read VERSION")
        .trim()
        .to_string()
}

fn run_script(args: &[&str]) -> (ExitStatus, String, String) {
    let output = Command::new("bash")
        .arg(version_script())
        .args(args)
        .output()
        .expect("failed to execute versioning.sh");
    (
        output.status,
        String::from_utf8_lossy(&output.stdout).to_string(),
        String::from_utf8_lossy(&output.stderr).to_string(),
    )
}

#[test]
fn versioning_script_current_matches_version_file() {
    let expected = canonical_version();
    let (status, stdout, stderr) = run_script(&["current"]);
    assert!(
        status.success(),
        "expected versioning.sh current to succeed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert_eq!(stdout.trim(), expected);
}

#[test]
fn versioning_script_check_succeeds_when_metadata_is_synced() {
    let (status, stdout, stderr) = run_script(&["check"]);
    assert!(
        status.success(),
        "expected versioning.sh check to succeed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(
        stdout.contains("versioning: OK"),
        "expected success marker in stdout\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
}

#[test]
fn versioning_script_sync_refreshes_lockfile() {
    let script = std::fs::read_to_string(version_script()).expect("read versioning.sh");
    assert!(
        script.contains("cargo update -w --offline"),
        "versioning.sh sync should refresh Cargo.lock for the workspace root package"
    );
}

#[test]
fn versioning_script_check_reports_lockfile_drift() {
    let script = std::fs::read_to_string(version_script()).expect("read versioning.sh");
    assert!(
        script.contains("Cargo.lock version drifted"),
        "versioning.sh check should fail explicitly when Cargo.lock is out of sync"
    );
}
