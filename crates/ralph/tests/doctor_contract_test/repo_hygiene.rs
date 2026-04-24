//! Doctor repo-hygiene auto-fix tests.
//!
//! Purpose:
//! - Doctor repo-hygiene auto-fix tests.
//!
//! Responsibilities:
//! - Verify doctor detects missing `.ralph/logs/` gitignore entries.
//! - Verify doctor auto-fix repairs repo hygiene issues in place.
//!
//! Not handled here:
//! - Queue or runner-binary diagnostics.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - Tests start from seeded doctor fixtures, then intentionally corrupt `.gitignore`.
//! - JSON output remains the assertion surface for hygiene diagnostics.

use super::*;

#[test]
fn doctor_detects_missing_ralph_logs_gitignore() -> Result<()> {
    let dir = setup_doctor_repo()?;

    std::fs::write(
        dir.path().join(".gitignore"),
        ".ralph/lock\n.ralph/cache/\n",
    )?;

    let output = ralph_cmd_in_dir(dir.path())
        .args(["doctor", "--format", "json"])
        .output()?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|_| panic!("JSON should be valid. Got stdout: {}", stdout));

    assert_eq!(
        json["success"], false,
        "doctor should fail when .ralph/logs/ is not gitignored"
    );

    let checks = json["checks"].as_array().unwrap();
    let logs_check = checks
        .iter()
        .find(|c| c["category"] == "project" && c["check"] == "gitignore_ralph_logs");

    assert!(
        logs_check.is_some(),
        "should have a gitignore_ralph_logs check. Checks: {:?}",
        checks
    );
    let logs_check = logs_check.unwrap();

    assert_eq!(logs_check["severity"], "Error", "should be Error severity");
    assert_eq!(
        logs_check["fix_available"], true,
        "should have fix_available=true"
    );
    assert!(
        logs_check["suggested_fix"]
            .as_str()
            .unwrap_or("")
            .contains(".ralph/logs/"),
        "suggested_fix should mention .ralph/logs/"
    );

    Ok(())
}

#[test]
fn doctor_auto_fix_adds_ralph_logs_gitignore() -> Result<()> {
    let dir = setup_doctor_repo()?;

    std::fs::write(
        dir.path().join(".gitignore"),
        ".ralph/lock\n.ralph/cache/\n",
    )?;

    let output = ralph_cmd_in_dir(dir.path())
        .args(["doctor", "--format", "json", "--auto-fix"])
        .output()?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let json: serde_json::Value = serde_json::from_str(&stdout)
        .unwrap_or_else(|_| panic!("JSON should be valid. Got stdout: {}", stdout));

    let checks = json["checks"].as_array().unwrap();
    let logs_check = checks
        .iter()
        .find(|c| c["category"] == "project" && c["check"] == "gitignore_ralph_logs");

    assert!(
        logs_check.is_some(),
        "should have a gitignore_ralph_logs check"
    );
    let logs_check = logs_check.unwrap();

    assert_eq!(
        logs_check["fix_applied"], true,
        "fix_applied should be true after auto-fix"
    );

    let gitignore_content = std::fs::read_to_string(dir.path().join(".gitignore"))?;
    assert!(
        gitignore_content.contains(".ralph/logs/"),
        ".gitignore should now contain .ralph/logs/. Content: {}",
        gitignore_content
    );

    Ok(())
}
