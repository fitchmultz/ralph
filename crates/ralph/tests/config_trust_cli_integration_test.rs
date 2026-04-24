//! Integration tests for repo execution trust CLI (`ralph config trust init`, `ralph init --trust-project-commands`).
//!
//! Purpose:
//! - Integration tests for repo execution trust CLI (`ralph config trust init`, `ralph init --trust-project-commands`).
//!
//! Responsibilities:
//! - Provide focused implementation or regression coverage for this file's owning feature.
//!
//! Scope:
//! - Limited to this file's owning feature boundary.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/Assumptions:
//! - Keep behavior aligned with Ralph's canonical CLI, machine-contract, and queue semantics.

use anyhow::Result;
mod test_support;

const SENSITIVE_CONFIG: &str = r#"{
  "version": 2,
  "agent": {
    "runner": "codex",
    "model": "gpt-5.3-codex",
    "codex_bin": "codex"
  }
}"#;

#[test]
fn config_trust_init_unlocks_sensitive_project_config() -> Result<()> {
    let dir = test_support::temp_dir_outside_repo();
    test_support::git_init(dir.path())?;
    test_support::ralph_init(dir.path())?;

    std::fs::write(dir.path().join(".ralph/config.jsonc"), SENSITIVE_CONFIG)?;

    let (status, _stdout, stderr) = test_support::run_in_dir(dir.path(), &["config", "show"]);
    assert!(
        !status.success(),
        "expected config show to fail without trust\nstderr:\n{stderr}"
    );
    assert!(
        stderr.contains("not trusted"),
        "expected trust error in stderr, got:\n{stderr}"
    );

    let (status, _stdout, stderr) =
        test_support::run_in_dir(dir.path(), &["config", "trust", "init"]);
    assert!(
        status.success(),
        "ralph config trust init failed\nstderr:\n{stderr}"
    );

    let (status, _stdout, stderr) = test_support::run_in_dir(dir.path(), &["config", "show"]);
    assert!(
        status.success(),
        "config show should succeed after trust init\nstderr:\n{stderr}"
    );

    let trust_path = dir.path().join(".ralph/trust.jsonc");
    let first = std::fs::read_to_string(&trust_path)?;
    let (status, _stdout, stderr) =
        test_support::run_in_dir(dir.path(), &["config", "trust", "init"]);
    assert!(
        status.success(),
        "second trust init failed\nstderr:\n{stderr}"
    );
    let second = std::fs::read_to_string(&trust_path)?;
    assert_eq!(
        first, second,
        "idempotent trust init must not rewrite trust file bytes"
    );

    Ok(())
}

#[test]
fn init_trust_project_commands_allows_later_sensitive_config() -> Result<()> {
    let dir = test_support::temp_dir_outside_repo();
    test_support::git_init(dir.path())?;
    let (status, _stdout, stderr) = test_support::run_in_dir(
        dir.path(),
        &[
            "init",
            "--force",
            "--non-interactive",
            "--trust-project-commands",
        ],
    );
    assert!(
        status.success(),
        "ralph init --trust-project-commands failed\nstderr:\n{stderr}"
    );

    std::fs::write(dir.path().join(".ralph/config.jsonc"), SENSITIVE_CONFIG)?;

    let (status, _stdout, stderr) = test_support::run_in_dir(dir.path(), &["config", "show"]);
    assert!(
        status.success(),
        "config show should succeed when trust was created at init\nstderr:\n{stderr}"
    );

    Ok(())
}
