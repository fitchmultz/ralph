//! Integration tests for CI gate compliance message delivery to agent.
//!
//! Responsibilities:
//! - Verify CI failure compliance messages reach the agent via Continue session.
//! - Ensure strict enforcement language and CI output context are passed to retry.
//!
//! Not handled here:
//! - CI gate execution itself (see ci_gate_*_test.rs).
//! - Git revert behavior (see git_repo_ci_gate_revert_test.rs).
//!
//! Invariants/assumptions:
//! - CI gate command runs in subprocess with captured output.
//! - Compliance message is passed to resume_continue_session on failure.

use anyhow::{Context, Result};
mod test_support;

#[test]
fn ci_gate_compliance_message_passed_to_runner_on_failure() -> Result<()> {
    let dir = test_support::temp_dir_outside_repo();
    test_support::git_init(dir.path())?;

    let (status, stdout, stderr) =
        test_support::run_in_dir(dir.path(), &["init", "--force", "--non-interactive"]);
    anyhow::ensure!(
        status.success(),
        "ralph init failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    test_support::write_valid_single_todo_queue(dir.path())?;

    let ci_script = r#"#!/bin/sh
echo 'CI failing with TOML error'
echo 'ruff failed: TOML parse error at line 44: unknown variant `py314`' >&2
exit 2
"#;
    test_support::create_executable_script(dir.path(), "ci-gate.sh", ci_script)?;
    test_support::configure_ci_gate(dir.path(), Some("./ci-gate.sh"), Some(true))?;

    let resume_args_file = dir.path().join(".ralph/continue_resume_args.txt");

    let runner_script = format!(
        r#"#!/bin/sh
# Log all args to file for assertion
echo "$@" >> {args_file}
# Output JSON with session_id so resume can work
echo '{{"session_id":"test-session-123","stdout":"runner output","status":"success"}}'
exit 0
"#,
        args_file = resume_args_file.display()
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
        resume_args_file.exists(),
        "expected Continue resume args file at {}\nstdout:\n{stdout}\nstderr:\n{stderr}",
        resume_args_file.display()
    );

    let args_content =
        std::fs::read_to_string(&resume_args_file).context("read resume args file")?;

    anyhow::ensure!(
        args_content.contains("CI gate (./ci-gate.sh): CI failed with exit code 2")
            || args_content.contains("CI failed with exit code 2"),
        "expected compliance message with exit code in runner args\nargs:\n{args_content}"
    );

    anyhow::ensure!(
        args_content.contains("Fix the errors above before continuing."),
        "expected remediation instruction in runner args\nargs:\n{args_content}"
    );

    anyhow::ensure!(
        args_content.contains("COMMON PATTERNS"),
        "expected COMMON PATTERNS section in runner args\nargs:\n{args_content}"
    );

    anyhow::ensure!(
        args_content.contains("TOML parse error at line 44")
            || args_content.contains("ruff failed: TOML parse error"),
        "expected CI output snippet in runner args\nargs:\n{args_content}"
    );

    Ok(())
}

#[test]
fn ci_gate_custom_command_shown_in_compliance_message() -> Result<()> {
    let dir = test_support::temp_dir_outside_repo();
    test_support::git_init(dir.path())?;

    let (status, stdout, stderr) =
        test_support::run_in_dir(dir.path(), &["init", "--force", "--non-interactive"]);
    anyhow::ensure!(
        status.success(),
        "ralph init failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    test_support::write_valid_single_todo_queue(dir.path())?;

    let ci_script = r#"#!/bin/sh
echo 'custom CI failing'
exit 1
"#;
    test_support::create_executable_script(dir.path(), "custom-ci.sh", ci_script)?;
    test_support::configure_ci_gate(dir.path(), Some("./custom-ci.sh"), Some(true))?;

    let resume_args_file = dir.path().join(".ralph/custom_ci_resume_args.txt");
    let runner_script = format!(
        r#"#!/bin/sh
echo "$@" >> {args_file}
echo '{{"session_id":"test-session-custom-123","stdout":"runner output","status":"success"}}'
exit 0
"#,
        args_file = resume_args_file.display()
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
        resume_args_file.exists(),
        "expected Continue resume args file at {}\nstdout:\n{stdout}\nstderr:\n{stderr}",
        resume_args_file.display()
    );
    let args_content =
        std::fs::read_to_string(&resume_args_file).context("read resume args file")?;

    anyhow::ensure!(
        args_content.contains("CI gate (./custom-ci.sh): CI failed with exit code 1")
            || args_content.contains("CI gate (./custom-ci.sh)"),
        "expected custom CI command in compliance message passed to runner args\nargs:\n{args_content}"
    );

    Ok(())
}
