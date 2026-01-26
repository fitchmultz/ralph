//! Contract tests for `ralph run` CLI override behavior.

use anyhow::Result;

mod test_support;

#[test]
fn run_one_accepts_runner_and_model_overrides_without_todo_tasks() -> Result<()> {
    let dir = test_support::temp_dir_outside_repo();
    test_support::git_init(dir.path())?;

    let (status, stdout, stderr) = test_support::run_in_dir(dir.path(), &["init", "--force"]);
    anyhow::ensure!(
        status.success(),
        "ralph init failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    // With an empty queue, `run one` should return success (NoTodo), but still parse flags.
    let (status, stdout, stderr) = test_support::run_in_dir(
        dir.path(),
        &["run", "one", "--runner", "opencode", "--model", "gpt-5.2"],
    );
    anyhow::ensure!(
        status.success(),
        "expected success (NoTodo) with valid overrides\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    anyhow::ensure!(
        stdout.contains("No todo tasks found") || stderr.contains("No todo tasks found"),
        "expected NoTodo message\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    let (status, stdout, stderr) = test_support::run_in_dir(
        dir.path(),
        &[
            "run",
            "one",
            "--runner",
            "codex",
            "--model",
            "gpt-5.2-codex",
            "--effort",
            "high",
        ],
    );
    anyhow::ensure!(
        status.success(),
        "expected success (NoTodo) with valid codex overrides\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    // `--effort` is accepted even when runner is opencode (codex-only semantics),
    // and is expected to be ignored at execution time.
    let (status, stdout, stderr) = test_support::run_in_dir(
        dir.path(),
        &[
            "run", "one", "--runner", "opencode", "--model", "gpt-5.2", "--effort", "high",
        ],
    );
    anyhow::ensure!(
        status.success(),
        "expected success (NoTodo) when effort is provided with opencode\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    let (status, stdout, stderr) = test_support::run_in_dir(
        dir.path(),
        &[
            "run",
            "one",
            "--runner",
            "gemini",
            "--model",
            "gemini-3-flash-preview",
        ],
    );
    anyhow::ensure!(
        status.success(),
        "expected success (NoTodo) with valid gemini overrides\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    anyhow::ensure!(
        stdout.contains("No todo tasks found") || stderr.contains("No todo tasks found"),
        "expected NoTodo message\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    let (status, stdout, stderr) = test_support::run_in_dir(
        dir.path(),
        &["run", "one", "--runner", "claude", "--model", "sonnet"],
    );
    anyhow::ensure!(
        status.success(),
        "expected success (NoTodo) with valid claude overrides\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    anyhow::ensure!(
        stdout.contains("No todo tasks found") || stderr.contains("No todo tasks found"),
        "expected NoTodo message\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    Ok(())
}

#[test]
fn run_one_rejects_invalid_runner_flag() -> Result<()> {
    let dir = test_support::temp_dir_outside_repo();
    test_support::git_init(dir.path())?;

    let (status, stdout, stderr) = test_support::run_in_dir(
        dir.path(),
        &["run", "one", "--runner", "nope", "--model", "gpt-5.2"],
    );

    anyhow::ensure!(
        !status.success(),
        "expected failure\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    anyhow::ensure!(
        stderr.contains("--runner must be 'codex', 'opencode', 'gemini', 'claude', or 'cursor'"),
        "expected helpful runner error\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    Ok(())
}

#[test]
fn run_one_rejects_invalid_model_flag() -> Result<()> {
    let dir = test_support::temp_dir_outside_repo();
    test_support::git_init(dir.path())?;

    let (status, stdout, stderr) = test_support::run_in_dir(dir.path(), &["init", "--force"]);
    anyhow::ensure!(
        status.success(),
        "ralph init failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    let (status, stdout, stderr) = test_support::run_in_dir(
        dir.path(),
        &[
            "run",
            "one",
            "--runner",
            "codex",
            "--model",
            "definitely-not-a-model",
        ],
    );

    anyhow::ensure!(
        !status.success(),
        "expected failure\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    anyhow::ensure!(
        stderr.contains("not supported for codex runner"),
        "expected helpful model error\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    Ok(())
}

#[test]
fn run_one_accepts_custom_model_for_opencode() -> Result<()> {
    let dir = test_support::temp_dir_outside_repo();
    test_support::git_init(dir.path())?;

    let (status, stdout, stderr) = test_support::run_in_dir(dir.path(), &["init", "--force"]);
    anyhow::ensure!(
        status.success(),
        "ralph init failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    let (status, stdout, stderr) = test_support::run_in_dir(
        dir.path(),
        &[
            "run",
            "one",
            "--runner",
            "opencode",
            "--model",
            "gemini-3-pro-preview",
        ],
    );
    anyhow::ensure!(
        status.success(),
        "expected success (NoTodo) with custom opencode model\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    anyhow::ensure!(
        stdout.contains("No todo tasks found") || stderr.contains("No todo tasks found"),
        "expected NoTodo message\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    Ok(())
}

#[test]
fn run_one_rejects_invalid_effort_flag() -> Result<()> {
    let dir = test_support::temp_dir_outside_repo();
    test_support::git_init(dir.path())?;

    let (status, stdout, stderr) = test_support::run_in_dir(
        dir.path(),
        &[
            "run",
            "one",
            "--runner",
            "codex",
            "--model",
            "gpt-5.2-codex",
            "--effort",
            "extreme",
        ],
    );

    anyhow::ensure!(
        !status.success(),
        "expected failure\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    anyhow::ensure!(
        stderr.contains("unsupported reasoning effort"),
        "expected helpful effort error\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    Ok(())
}
