//! Baseline doctor contract tests.
//!
//! Purpose:
//! - Baseline doctor contract tests.
//!
//! Responsibilities:
//! - Verify core success, warning, and missing-queue doctor outcomes.
//! - Keep git-only and seeded-repo setup paths explicit for failure locality.
//!
//! Not handled here:
//! - JSON-format specifics or auto-fix behavior.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Missing-queue coverage intentionally uses a git-only repo with no seeded `.ralph/` files.
//! - Success-path coverage uses trusted seeded fixtures rather than real init.

use super::*;

#[test]
fn doctor_passes_in_clean_env() -> Result<()> {
    let dir = setup_trusted_doctor_repo()?;

    let output = ralph_cmd_in_dir(dir.path()).arg("doctor").output()?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{}\n{}", stdout, stderr);

    if !output.status.success() {
        println!("STDOUT:\n{stdout}");
        println!("STDERR:\n{stderr}");
    }

    assert!(output.status.success());
    assert!(combined.contains("OK") && combined.contains("git binary found"));
    assert!(combined.contains("OK") && combined.contains("queue valid"));
    assert!(combined.contains("WARN") && combined.contains("no upstream configured"));
    Ok(())
}

#[test]
fn doctor_fails_when_queue_missing() -> Result<()> {
    let dir = setup_git_repo()?;

    let output = ralph_cmd_in_dir(dir.path()).arg("doctor").output()?;

    assert!(!output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{}\n{}", stdout, stderr);
    assert!(combined.contains("FAIL") && combined.contains("queue file missing"));
    Ok(())
}

#[test]
fn doctor_warns_on_missing_upstream() -> Result<()> {
    let dir = setup_trusted_doctor_repo()?;

    let output = ralph_cmd_in_dir(dir.path()).arg("doctor").output()?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{}\n{}", stdout, stderr);

    if !output.status.success() {
        println!("STDOUT:\n{stdout}");
        println!("STDERR:\n{stderr}");
    }

    assert!(output.status.success());
    assert!(combined.contains("WARN") && combined.contains("no upstream configured"));
    Ok(())
}
