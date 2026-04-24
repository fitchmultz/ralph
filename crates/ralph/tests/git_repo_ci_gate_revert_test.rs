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
//! Scenario: CI gate revert behavior.
//!
//! Usage:
//! - Used through the crate module tree or integration test harness.
//!
//! Invariants/Assumptions:
//! - Keep behavior aligned with Ralph's canonical CLI, machine-contract, and queue semantics.

use anyhow::{Context, Result};
mod test_support;

#[test]
fn run_one_reverts_changes_when_ci_fails() -> Result<()> {
    let dir = test_support::temp_dir_outside_repo();
    test_support::git_init(dir.path())?;

    // Ensure ralph runtime files exist.
    let (status, stdout, stderr) =
        test_support::run_in_dir(dir.path(), &["init", "--force", "--non-interactive"]);
    anyhow::ensure!(
        status.success(),
        "ralph init failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    // Add a task to the queue.
    test_support::write_valid_single_todo_queue(dir.path())?;

    // Create a Makefile with a failing `ci` target.
    let makefile_content = r#"ci:
	@echo 'CI failing'
	exit 1
"#;
    std::fs::write(dir.path().join("Makefile"), makefile_content).context("write Makefile")?;

    // Create a "dirty runner" that creates a file and exits 0.
    // Drain stdin first because Codex reads prompts from stdin and otherwise the
    // fake runner can exit before Ralph finishes writing, causing a flaky broken pipe.
    let dirty_file = dir.path().join("dirty-file.txt");
    let script = format!(
        "#!/bin/sh\ncat >/dev/null\necho 'creating dirty file' > {}\nexit 0\n",
        dirty_file.display()
    );
    let runner_path = test_support::create_fake_runner(dir.path(), "codex", &script)
        .context("write runner script")?;
    test_support::configure_runner(dir.path(), "codex", "gpt-5.3-codex", Some(&runner_path))?;
    test_support::trust_project_commands(dir.path())?;

    // Commit the setup so the repo starts clean.
    test_support::git_add_all_commit(dir.path(), "setup test env")?;

    // Run `ralph run one`.
    let (status, stdout, stderr) =
        test_support::run_in_dir(dir.path(), &["run", "one", "--git-revert-mode", "enabled"]);

    // Assert: execution fails.
    anyhow::ensure!(
        !status.success(),
        "expected run one to fail due to CI\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    // Assert: stderr mentions CI failure.
    anyhow::ensure!(
        stderr.contains("CI gate failed") || stderr.contains("CI failed"),
        "expected CI failure message in stderr\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    // Assert: dirty file does NOT exist (changes were reverted).
    anyhow::ensure!(
        !dirty_file.exists(),
        "dirty file should not exist after CI failure and rollback"
    );

    // Assert: repo is clean (no uncommitted changes).
    let (status, stdout, stderr) =
        test_support::run_in_dir_raw(dir.path(), "git", &["status", "--porcelain"]);
    anyhow::ensure!(
        status.success(),
        "git status failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    anyhow::ensure!(
        stdout.trim().is_empty(),
        "repo should be clean after rollback, but git status showed:\n{stdout}"
    );

    Ok(())
}

#[test]
fn run_one_keeps_changes_when_ci_fails_and_git_revert_mode_disabled() -> Result<()> {
    let dir = test_support::temp_dir_outside_repo();
    test_support::git_init(dir.path())?;

    let (status, stdout, stderr) =
        test_support::run_in_dir(dir.path(), &["init", "--force", "--non-interactive"]);
    anyhow::ensure!(
        status.success(),
        "ralph init failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    test_support::write_valid_single_todo_queue(dir.path())?;

    let makefile_content = r#"ci:
	@echo 'CI failing'
	exit 1
"#;
    std::fs::write(dir.path().join("Makefile"), makefile_content).context("write Makefile")?;

    let dirty_file = dir.path().join("dirty-file.txt");
    let script = format!(
        "#!/bin/sh\ncat >/dev/null\necho 'creating dirty file' > {}\nexit 0\n",
        dirty_file.display()
    );
    let runner_path = test_support::create_fake_runner(dir.path(), "codex", &script)
        .context("write runner script")?;
    test_support::configure_runner(dir.path(), "codex", "gpt-5.3-codex", Some(&runner_path))?;
    test_support::trust_project_commands(dir.path())?;

    test_support::git_add_all_commit(dir.path(), "setup test env")?;

    let (status, stdout, stderr) =
        test_support::run_in_dir(dir.path(), &["run", "one", "--git-revert-mode", "disabled"]);

    anyhow::ensure!(
        !status.success(),
        "expected run one to fail due to CI\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    anyhow::ensure!(
        stderr.contains("CI gate failed") || stderr.contains("CI failed"),
        "expected CI failure message in stderr\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    anyhow::ensure!(
        dirty_file.exists(),
        "dirty file should remain when git revert is disabled"
    );

    Ok(())
}

#[test]
fn run_one_keeps_changes_when_ci_fails_and_git_revert_mode_ask_non_tty() -> Result<()> {
    let dir = test_support::temp_dir_outside_repo();
    test_support::git_init(dir.path())?;

    let (status, stdout, stderr) =
        test_support::run_in_dir(dir.path(), &["init", "--force", "--non-interactive"]);
    anyhow::ensure!(
        status.success(),
        "ralph init failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    test_support::write_valid_single_todo_queue(dir.path())?;

    let makefile_content = r#"ci:
	@echo 'CI failing'
	exit 1
"#;
    std::fs::write(dir.path().join("Makefile"), makefile_content).context("write Makefile")?;

    let dirty_file = dir.path().join("dirty-file.txt");
    let script = format!(
        "#!/bin/sh\ncat >/dev/null\necho 'creating dirty file' > {}\nexit 0\n",
        dirty_file.display()
    );
    let runner_path = test_support::create_fake_runner(dir.path(), "codex", &script)
        .context("write runner script")?;
    test_support::configure_runner(dir.path(), "codex", "gpt-5.3-codex", Some(&runner_path))?;
    test_support::trust_project_commands(dir.path())?;

    test_support::git_add_all_commit(dir.path(), "setup test env")?;

    let (status, stdout, stderr) =
        test_support::run_in_dir(dir.path(), &["run", "one", "--git-revert-mode", "ask"]);

    anyhow::ensure!(
        !status.success(),
        "expected run one to fail due to CI\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    anyhow::ensure!(
        stderr.contains("CI gate failed") || stderr.contains("CI failed"),
        "expected CI failure message in stderr\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    anyhow::ensure!(
        dirty_file.exists(),
        "dirty file should remain when ask mode runs non-interactively"
    );

    Ok(())
}
