//! Integration tests for `ralph migrate` commands.
//!
//! Purpose:
//! - Integration tests for `ralph migrate` commands.
//!
//! Responsibilities:
//! - Verify `migrate --list` displays available migrations.
//! - Verify `migrate status` shows detailed migration status.
//! - Verify `migrate --apply` with `--force` works without interactive prompts.
//! - Verify `migrate --check` returns appropriate exit codes.
//! - Verify `migrate` (no args) shows current status.
//!
//! Scope:
//! - Limited to this file's owning feature boundary.
//!
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/assumptions:
//! - The migration `config_key_rename_parallel_worktree_root_2026_02` exists in the registry.
//! - `seed_ralph_dir()` creates a valid baseline config that may or may not trigger migrations.

use anyhow::Result;
use std::fs;

mod test_support;

fn write_legacy_project_config(dir: &std::path::Path, git_commit_push_enabled: bool) -> Result<()> {
    fs::write(
        dir.join(".ralph/config.jsonc"),
        format!(
            r#"{{
  "version": 1,
  "agent": {{
    "runner": "codex",
    "model": "gpt-5.3-codex",
    "git_commit_push_enabled": {}
  }}
}}
"#,
            git_commit_push_enabled
        ),
    )?;
    Ok(())
}

#[test]
fn migrate_list_shows_all_migrations() -> Result<()> {
    let dir = test_support::temp_dir_outside_repo();
    test_support::git_init(dir.path())?;
    test_support::seed_ralph_dir(dir.path())?;

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
    test_support::seed_ralph_dir(dir.path())?;

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
    test_support::seed_ralph_dir(dir.path())?;

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
    test_support::seed_ralph_dir(dir.path())?;

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
    test_support::seed_ralph_dir(dir.path())?;

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
    test_support::seed_ralph_dir(dir.path())?;

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

#[test]
fn migrate_check_detects_legacy_config_without_parse_failure() -> Result<()> {
    let dir = test_support::temp_dir_outside_repo();
    test_support::git_init(dir.path())?;
    test_support::seed_ralph_dir(dir.path())?;
    write_legacy_project_config(dir.path(), true)?;

    let (status, stdout, stderr) = test_support::run_in_dir(dir.path(), &["migrate", "--check"]);

    anyhow::ensure!(
        !stderr.contains("resolve configuration"),
        "migrate should not fail before checking migrations\nstderr:\n{stderr}"
    );
    anyhow::ensure!(
        !stderr.contains("unknown field `git_commit_push_enabled`"),
        "migrate should not surface config parse failure\nstderr:\n{stderr}"
    );
    anyhow::ensure!(
        !status.success(),
        "legacy config should produce a pending migration exit code"
    );
    anyhow::ensure!(
        stdout.contains("config_legacy_contract_upgrade_2026_03"),
        "expected legacy config migration to be reported, got:\n{stdout}"
    );

    Ok(())
}

#[test]
fn migrate_apply_upgrades_legacy_config_with_push_enabled_true() -> Result<()> {
    let dir = test_support::temp_dir_outside_repo();
    test_support::git_init(dir.path())?;
    test_support::seed_ralph_dir(dir.path())?;
    write_legacy_project_config(dir.path(), true)?;

    let (status, stdout, stderr) =
        test_support::run_in_dir(dir.path(), &["migrate", "--apply", "--force"]);
    anyhow::ensure!(status.success(), "migrate apply failed\nstderr:\n{stderr}");
    anyhow::ensure!(
        stdout.contains("config_legacy_contract_upgrade_2026_03"),
        "expected legacy migration to be applied, got:\n{stdout}"
    );

    let migrated = fs::read_to_string(dir.path().join(".ralph/config.jsonc"))?;
    anyhow::ensure!(
        migrated.contains("\"version\": 2"),
        "expected config version upgrade, got:\n{migrated}"
    );
    anyhow::ensure!(
        migrated.contains("\"git_publish_mode\": \"commit_and_push\""),
        "expected commit_and_push mapping, got:\n{migrated}"
    );
    anyhow::ensure!(
        !migrated.contains("git_commit_push_enabled"),
        "legacy key should be removed, got:\n{migrated}"
    );

    let (resolve_status, resolve_stdout, resolve_stderr) =
        test_support::run_in_dir(dir.path(), &["--no-color", "machine", "config", "resolve"]);
    anyhow::ensure!(
        resolve_status.success(),
        "machine config resolve should succeed after migration\nstdout:\n{resolve_stdout}\nstderr:\n{resolve_stderr}"
    );

    Ok(())
}

#[test]
fn migrate_apply_upgrades_legacy_config_with_push_enabled_false() -> Result<()> {
    let dir = test_support::temp_dir_outside_repo();
    test_support::git_init(dir.path())?;
    test_support::seed_ralph_dir(dir.path())?;
    write_legacy_project_config(dir.path(), false)?;

    let (status, _stdout, stderr) =
        test_support::run_in_dir(dir.path(), &["migrate", "--apply", "--force"]);
    anyhow::ensure!(status.success(), "migrate apply failed\nstderr:\n{stderr}");

    let migrated = fs::read_to_string(dir.path().join(".ralph/config.jsonc"))?;
    anyhow::ensure!(
        migrated.contains("\"git_publish_mode\": \"off\""),
        "expected off mapping, got:\n{migrated}"
    );

    Ok(())
}

#[test]
fn migrate_apply_preserves_existing_git_publish_mode() -> Result<()> {
    let dir = test_support::temp_dir_outside_repo();
    test_support::git_init(dir.path())?;
    test_support::seed_ralph_dir(dir.path())?;
    fs::write(
        dir.path().join(".ralph/config.jsonc"),
        r#"{
  "version": 1,
  "agent": {
    "runner": "codex",
    "git_commit_push_enabled": false,
    "git_publish_mode": "commit"
  }
}
"#,
    )?;

    let (status, _stdout, stderr) =
        test_support::run_in_dir(dir.path(), &["migrate", "--apply", "--force"]);
    anyhow::ensure!(status.success(), "migrate apply failed\nstderr:\n{stderr}");

    let migrated = fs::read_to_string(dir.path().join(".ralph/config.jsonc"))?;
    anyhow::ensure!(
        migrated.contains("\"git_publish_mode\": \"commit\""),
        "existing git_publish_mode should win, got:\n{migrated}"
    );
    anyhow::ensure!(
        !migrated.contains("git_commit_push_enabled"),
        "legacy key should be removed, got:\n{migrated}"
    );

    Ok(())
}
