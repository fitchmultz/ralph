//! Integration tests for `ralph migrate` commands.
//!
//! Responsibilities:
//! - Verify `migrate --list` displays available migrations.
//! - Verify `migrate status` shows detailed migration status.
//! - Verify `migrate --apply` with `--force` works without interactive prompts.
//! - Verify `migrate --check` returns appropriate exit codes.
//! - Verify `migrate` (no args) shows current status.
//!
//! Not handled here:
//! - Testing migrations that require legacy config keys (config validation rejects unknown
//!   fields before migration can run; such migrations are tested at the unit level).
//!
//! Invariants/assumptions:
//! - The migration `config_key_rename_parallel_worktree_root_2026_02` exists in the registry.
//! - `ralph init` creates a valid config that may or may not trigger migrations.

use anyhow::Result;

mod test_support;

#[test]
fn migrate_list_shows_all_migrations() -> Result<()> {
    let dir = test_support::temp_dir_outside_repo();
    test_support::git_init(dir.path())?;
    test_support::ralph_init(dir.path())?;

    let (status, stdout, stderr) = test_support::run_in_dir(dir.path(), &["migrate", "--list"]);
    anyhow::ensure!(status.success(), "migrate list failed\nstderr:\n{stderr}");

    // Verify the migration is listed
    anyhow::ensure!(
        stdout.contains("config_key_rename_parallel_worktree_root_2026_02"),
        "expected migration to be listed, got:\n{stdout}"
    );

    // Verify status indicators are present
    anyhow::ensure!(
        stdout.contains("applied")
            || stdout.contains("pending")
            || stdout.contains("not applicable"),
        "expected status indicators, got:\n{stdout}"
    );

    Ok(())
}

#[test]
fn migrate_status_shows_detailed_info() -> Result<()> {
    let dir = test_support::temp_dir_outside_repo();
    test_support::git_init(dir.path())?;
    test_support::ralph_init(dir.path())?;

    let (status, stdout, stderr) = test_support::run_in_dir(dir.path(), &["migrate", "status"]);
    anyhow::ensure!(status.success(), "migrate status failed\nstderr:\n{stderr}");

    // Verify status output contains expected sections
    anyhow::ensure!(
        stdout.contains("History:") || stdout.contains("Pending migrations:"),
        "expected status sections, got:\n{stdout}"
    );

    // Verify migration history path is shown
    anyhow::ensure!(
        stdout.contains("migrations.json"),
        "expected migrations.json path, got:\n{stdout}"
    );

    Ok(())
}

#[test]
fn migrate_shows_current_status() -> Result<()> {
    let dir = test_support::temp_dir_outside_repo();
    test_support::git_init(dir.path())?;
    test_support::ralph_init(dir.path())?;

    // Run migrate without args - should show current status
    let (status, stdout, _stderr) = test_support::run_in_dir(dir.path(), &["migrate"]);
    anyhow::ensure!(status.success(), "migrate without args should succeed");

    // Should mention pending or up-to-date
    anyhow::ensure!(
        stdout.to_lowercase().contains("no pending")
            || stdout.to_lowercase().contains("up to date")
            || stdout.to_lowercase().contains("pending")
            || stdout.contains('✓'),
        "expected status message, got:\n{stdout}"
    );

    Ok(())
}

#[test]
fn migrate_apply_runs_without_error() -> Result<()> {
    let dir = test_support::temp_dir_outside_repo();
    test_support::git_init(dir.path())?;
    test_support::ralph_init(dir.path())?;

    // Apply migrations with --force to skip confirmation prompt
    let (status, stdout, stderr) =
        test_support::run_in_dir(dir.path(), &["migrate", "--apply", "--force"]);
    anyhow::ensure!(status.success(), "migrate apply failed\nstderr:\n{stderr}");

    // Verify appropriate message (may be "no pending" or "successfully applied")
    anyhow::ensure!(
        stdout.contains("Successfully applied")
            || stdout.contains("No pending migrations")
            || stdout.contains("No migrations were applied"),
        "expected completion message, got:\n{stdout}"
    );

    Ok(())
}

#[test]
fn migrate_check_returns_appropriate_exit_code() -> Result<()> {
    let dir = test_support::temp_dir_outside_repo();
    test_support::git_init(dir.path())?;
    test_support::ralph_init(dir.path())?;

    // Run check - should either succeed (if no pending) or fail with code 1 (if pending)
    let (status, stdout, _stderr) = test_support::run_in_dir(dir.path(), &["migrate", "--check"]);

    if status.success() {
        // No pending migrations
        anyhow::ensure!(
            stdout.to_lowercase().contains("no pending") || stdout.contains('✓'),
            "expected 'no pending' message on success, got:\n{stdout}"
        );
    } else {
        // Pending migrations exist - verify exit code 1
        let code = status.code().unwrap_or(-1);
        anyhow::ensure!(
            code == 1,
            "expected exit code 1 for pending migrations, got {code}"
        );
    }

    Ok(())
}

#[test]
fn migrate_subcommand_status_works() -> Result<()> {
    let dir = test_support::temp_dir_outside_repo();
    test_support::git_init(dir.path())?;
    test_support::ralph_init(dir.path())?;

    // Run migrate status subcommand
    let (status, stdout, stderr) = test_support::run_in_dir(dir.path(), &["migrate", "status"]);
    anyhow::ensure!(status.success(), "migrate status failed\nstderr:\n{stderr}");

    // Should show migration info
    anyhow::ensure!(
        stdout.contains("History:") || stdout.contains("migrations.json"),
        "expected history info, got:\n{stdout}"
    );

    Ok(())
}
