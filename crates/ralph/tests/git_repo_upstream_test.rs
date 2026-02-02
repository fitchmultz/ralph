//! Integration tests for ralph CLI behavior against real git repositories.
//! Scenario: Upstream (push) behavior.

use anyhow::Result;
mod test_support;

#[test]
fn run_one_succeeds_without_upstream_and_warns() -> Result<()> {
    let dir = test_support::temp_dir_outside_repo();
    test_support::git_init(dir.path())?;

    // 1. Setup Ralph
    let (status, stdout, stderr) =
        test_support::run_in_dir(dir.path(), &["init", "--force", "--non-interactive"]);
    anyhow::ensure!(
        status.success(),
        "ralph init failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    // 2. Add a task
    test_support::write_valid_single_todo_queue(dir.path())?;

    // Create a dummy Makefile for post_run_supervise.
    std::fs::write(dir.path().join("Makefile"), "ci:\n\t@echo 'CI passed'\n")?;

    let runner_path = test_support::create_fake_runner(dir.path(), "codex", "#!/bin/sh\nexit 0\n")?;
    test_support::configure_runner(dir.path(), "codex", "gpt-5.2-codex", Some(&runner_path))?;

    // 4. Run `ralph run one` with the fake runner
    test_support::git_add_all_commit(dir.path(), "setup test env")?;

    let (status, stdout, stderr) = test_support::run_in_dir(dir.path(), &["run", "one"]);

    anyhow::ensure!(
        status.success(),
        "run one failed but should have succeeded (soft push failure)\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    anyhow::ensure!(
        stderr.contains("skipping push (no upstream configured)"),
        "expected warning about skipping push\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    // Verify task was actually marked done and archived (supervisor logic)
    let done_content = std::fs::read_to_string(dir.path().join(".ralph/done.json"))?;
    anyhow::ensure!(
        done_content.contains("RQ-0001"),
        "task should be moved to done"
    );

    Ok(())
}
