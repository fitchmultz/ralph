//! CLI argument validation coverage for queue sorting commands.
//!
//! Purpose:
//! - CLI argument validation coverage for queue sorting commands.
//!
//! Responsibilities:
//! - Verify `queue list` and `queue sort` reject unsupported `--sort-by` values.
//!
//! Non-scope:
//! - Successful ordering semantics or dry-run behavior.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions callers must respect:
//! - Clap errors should mention the invalid value and representative valid choices.

use super::queue_sorting_cli_test_support::{run_in_dir, setup_repo};
use anyhow::Result;

#[test]
fn queue_list_rejects_invalid_sort_by() -> Result<()> {
    let dir = setup_repo()?;
    let (status, _stdout, stderr) = run_in_dir(dir.path(), &["queue", "list", "--sort-by", "nope"]);
    anyhow::ensure!(
        !status.success(),
        "expected non-zero exit for invalid sort-by"
    );
    anyhow::ensure!(stderr.contains("nope"));
    anyhow::ensure!(stderr.contains("priority"));
    anyhow::ensure!(stderr.contains("created_at"));
    anyhow::ensure!(stderr.contains("scheduled_start"));
    Ok(())
}

#[test]
fn queue_sort_rejects_invalid_sort_by() -> Result<()> {
    let dir = setup_repo()?;
    let (status, _stdout, stderr) = run_in_dir(dir.path(), &["queue", "sort", "--sort-by", "nope"]);
    anyhow::ensure!(
        !status.success(),
        "expected non-zero exit for invalid sort-by"
    );
    anyhow::ensure!(stderr.contains("nope") && stderr.contains("priority"));
    Ok(())
}
