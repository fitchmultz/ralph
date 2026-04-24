//! Integration tests for ralph CLI behavior against real git repositories.
//!
//! Purpose:
//! - Integration tests for ralph CLI behavior against real git repositories.
//!
//! Responsibilities:
//! - Provide focused implementation or regression coverage for this file's owning feature.
//!
//! Scope:
//! - Limited to this file's owning feature boundary.
//!
//! Scenario: Init and validate in a fresh repo.
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/Assumptions:
//! - Keep behavior aligned with Ralph's canonical CLI, machine-contract, and queue semantics.

use anyhow::Result;
mod test_support;

#[test]
fn init_and_validate_work_in_fresh_git_repo() -> Result<()> {
    let dir = test_support::temp_dir_outside_repo();
    test_support::git_init(dir.path())?;

    let (status, stdout, stderr) =
        test_support::run_in_dir(dir.path(), &["init", "--force", "--non-interactive"]);
    anyhow::ensure!(
        status.success(),
        "ralph init failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    test_support::configure_runner(dir.path(), "codex", "gpt-5.3-codex", None)?;

    let (status, stdout, stderr) = test_support::run_in_dir(dir.path(), &["queue", "validate"]);
    anyhow::ensure!(
        status.success(),
        "ralph queue validate failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    Ok(())
}
