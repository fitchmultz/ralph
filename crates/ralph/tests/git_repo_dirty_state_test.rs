//! Integration tests for ralph CLI behavior against real git repositories.
//! Scenario: Dirty repo checks.

use anyhow::{Context, Result};
mod test_support;

#[test]
fn run_one_refuses_to_run_when_repo_is_dirty_and_a_todo_exists() -> Result<()> {
    let dir = test_support::temp_dir_outside_repo();
    test_support::git_init(dir.path())?;

    // Ensure ralph runtime files exist.
    let (status, stdout, stderr) =
        test_support::run_in_dir(dir.path(), &["init", "--force", "--non-interactive"]);
    anyhow::ensure!(
        status.success(),
        "ralph init failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    test_support::configure_runner(dir.path(), "codex", "gpt-5.2-codex", None)?;

    // Ensure there is a todo item so commands::run hits the clean-repo preflight.
    test_support::write_valid_single_todo_queue(dir.path())?;

    // Make the repo dirty with an untracked file.
    std::fs::write(dir.path().join("untracked.txt"), "dirty").context("write dirty file")?;

    let (status, stdout, stderr) = test_support::run_in_dir(dir.path(), &["run", "one"]);
    anyhow::ensure!(
        !status.success(),
        "expected run one to fail on dirty repo\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    anyhow::ensure!(
        stderr.to_lowercase().contains("repo is dirty"),
        "expected dirty repo error\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    Ok(())
}

#[test]
fn run_one_succeeds_when_repo_is_dirty_and_force_is_used() -> Result<()> {
    let dir = test_support::temp_dir_outside_repo();
    test_support::git_init(dir.path())?;

    // Ensure ralph runtime files exist.
    let (status, stdout, stderr) =
        test_support::run_in_dir(dir.path(), &["init", "--force", "--non-interactive"]);
    anyhow::ensure!(
        status.success(),
        "ralph init failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    // Ensure there is a todo item.
    test_support::write_valid_single_todo_queue(dir.path())?;

    // Make the repo dirty with an untracked file.
    std::fs::write(dir.path().join("untracked.txt"), "dirty").context("write dirty file")?;

    // Create a dummy Makefile for post_run_supervise
    std::fs::write(dir.path().join("Makefile"), "ci:\n\t@echo 'CI passed'\n")
        .context("write Makefile")?;

    let runner_path = test_support::create_fake_runner(dir.path(), "codex", "#!/bin/sh\nexit 0\n")?;
    test_support::configure_runner(dir.path(), "codex", "gpt-5.2-codex", Some(&runner_path))?;

    // Use --force to bypass the dirty repo check.
    let (status, stdout, stderr) = test_support::run_in_dir(dir.path(), &["--force", "run", "one"]);

    anyhow::ensure!(
        status.success(),
        "run one failed with --force on dirty repo\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    Ok(())
}

#[test]
fn scan_refuses_to_run_when_repo_is_dirty() -> Result<()> {
    let dir = test_support::temp_dir_outside_repo();
    test_support::git_init(dir.path())?;

    // Ensure ralph runtime files exist.
    let (status, stdout, stderr) =
        test_support::run_in_dir(dir.path(), &["init", "--force", "--non-interactive"]);
    anyhow::ensure!(
        status.success(),
        "ralph init failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    // Make the repo dirty with an untracked file.
    std::fs::write(dir.path().join("untracked.txt"), "dirty").context("write dirty file")?;

    let (status, stdout, stderr) =
        test_support::run_in_dir(dir.path(), &["scan", "--focus", "security"]);
    anyhow::ensure!(
        !status.success(),
        "expected scan to fail on dirty repo\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    anyhow::ensure!(
        stderr.to_lowercase().contains("repo is dirty"),
        "expected dirty repo error\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    Ok(())
}
