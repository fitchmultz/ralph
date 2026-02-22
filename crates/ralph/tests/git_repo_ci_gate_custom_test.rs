//! Integration tests for ralph CLI behavior against real git repositories.
//! Scenario: Custom CI gate command behavior.

use anyhow::{Context, Result};
mod test_support;

#[test]
fn run_one_fails_when_custom_ci_gate_command_fails() -> Result<()> {
    let dir = test_support::temp_dir_outside_repo();
    test_support::git_init(dir.path())?;

    let (status, stdout, stderr) =
        test_support::run_in_dir(dir.path(), &["init", "--force", "--non-interactive"]);
    anyhow::ensure!(
        status.success(),
        "ralph init failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    test_support::write_valid_single_todo_queue(dir.path())?;

    let script = "#!/bin/sh\necho 'CI failing'\nexit 2\n";
    test_support::create_executable_script(dir.path(), "ci-gate.sh", script)?;
    test_support::configure_ci_gate(dir.path(), Some("./ci-gate.sh"), Some(true))?;

    let dirty_file = dir.path().join("dirty-file.txt");
    let runner_script = format!(
        "#!/bin/sh\necho 'creating dirty file' > {}\nexit 0\n",
        dirty_file.display()
    );
    let runner_path = test_support::create_fake_runner(dir.path(), "codex", &runner_script)
        .context("write runner script")?;
    test_support::configure_runner(dir.path(), "codex", "gpt-5.2-codex", Some(&runner_path))?;

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
        stderr.contains("./ci-gate.sh"),
        "expected CI gate command in stderr\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    Ok(())
}

#[test]
fn run_one_succeeds_when_ci_gate_disabled() -> Result<()> {
    let dir = test_support::temp_dir_outside_repo();
    test_support::git_init(dir.path())?;

    let (status, stdout, stderr) =
        test_support::run_in_dir(dir.path(), &["init", "--force", "--non-interactive"]);
    anyhow::ensure!(
        status.success(),
        "ralph init failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    test_support::write_valid_single_todo_queue(dir.path())?;

    let script = "#!/bin/sh\necho 'CI failing'\nexit 2\n";
    test_support::create_executable_script(dir.path(), "ci-gate.sh", script)?;
    test_support::configure_ci_gate(dir.path(), Some("./ci-gate.sh"), Some(false))?;

    let dirty_file = dir.path().join("dirty-file.txt");
    let runner_script = format!(
        "#!/bin/sh\necho 'creating dirty file' > {}\nexit 0\n",
        dirty_file.display()
    );
    let runner_path = test_support::create_fake_runner(dir.path(), "codex", &runner_script)
        .context("write runner script")?;
    test_support::configure_runner(dir.path(), "codex", "gpt-5.2-codex", Some(&runner_path))?;

    test_support::git_add_all_commit(dir.path(), "setup test env")?;

    let (status, stdout, stderr) =
        test_support::run_in_dir(dir.path(), &["run", "one", "--git-revert-mode", "disabled"]);

    anyhow::ensure!(
        status.success(),
        "expected run one to succeed with CI gate disabled\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    let done_content = std::fs::read_to_string(dir.path().join(".ralph/done.jsonc"))?;
    anyhow::ensure!(
        done_content.contains("RQ-0001"),
        "task should be moved to done when CI gate is disabled"
    );

    let (status, stdout, stderr) =
        test_support::run_in_dir_raw(dir.path(), "git", &["status", "--porcelain"]);
    anyhow::ensure!(
        status.success(),
        "git status failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    anyhow::ensure!(
        stdout.trim().is_empty(),
        "repo should be clean after successful run, but git status showed:\n{stdout}"
    );

    Ok(())
}
