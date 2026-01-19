use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use std::process::{Command, ExitStatus};
use tempfile::TempDir;

fn ralph_bin() -> PathBuf {
    if let Some(path) = std::env::var_os("CARGO_BIN_EXE_ralph") {
        return PathBuf::from(path);
    }

    let exe = std::env::current_exe().expect("resolve current test executable path");
    let exe_dir = exe
        .parent()
        .expect("test executable should have a parent directory");
    let profile_dir = if exe_dir.file_name() == Some(std::ffi::OsStr::new("deps")) {
        exe_dir
            .parent()
            .expect("deps directory should have a parent directory")
    } else {
        exe_dir
    };

    let bin_name = if cfg!(windows) { "ralph.exe" } else { "ralph" };
    let candidate = profile_dir.join(bin_name);
    if candidate.exists() {
        return candidate;
    }

    panic!(
        "CARGO_BIN_EXE_ralph was not set and fallback binary path does not exist: {}",
        candidate.display()
    );
}

fn run_in_dir(dir: &Path, args: &[&str]) -> (ExitStatus, String, String) {
    let output = Command::new(ralph_bin())
        .current_dir(dir)
        .args(args)
        .output()
        .expect("failed to execute ralph binary");
    (
        output.status,
        String::from_utf8_lossy(&output.stdout).to_string(),
        String::from_utf8_lossy(&output.stderr).to_string(),
    )
}

fn git_init(dir: &Path) -> Result<()> {
    let status = Command::new("git")
        .current_dir(dir)
        .args(["init"])
        .status()
        .context("run git init")?;
    anyhow::ensure!(status.success(), "git init failed");
    Ok(())
}

fn assert_failure(status: ExitStatus, stdout: &str, stderr: &str) {
    assert!(
        !status.success(),
        "expected failure\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
}

#[test]
fn run_one_accepts_runner_and_model_overrides_without_todo_tasks() -> Result<()> {
    let dir = TempDir::new().context("create temp dir")?;
    git_init(dir.path())?;

    let (status, stdout, stderr) = run_in_dir(dir.path(), &["init", "--force"]);
    anyhow::ensure!(
        status.success(),
        "ralph init failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    // With an empty queue, `run one` should return success (NoTodo), but still parse flags.
    let (status, stdout, stderr) = run_in_dir(
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

    let (status, stdout, stderr) = run_in_dir(
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
    let (status, stdout, stderr) = run_in_dir(
        dir.path(),
        &[
            "run", "one", "--runner", "opencode", "--model", "gpt-5.2", "--effort", "high",
        ],
    );
    anyhow::ensure!(
        status.success(),
        "expected success (NoTodo) when effort is provided with opencode\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    let (status, stdout, stderr) = run_in_dir(
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

    Ok(())
}

#[test]
fn run_one_rejects_invalid_runner_flag() -> Result<()> {
    let dir = TempDir::new().context("create temp dir")?;
    git_init(dir.path())?;

    let (status, stdout, stderr) = run_in_dir(
        dir.path(),
        &["run", "one", "--runner", "nope", "--model", "gpt-5.2"],
    );

    assert_failure(status, &stdout, &stderr);
    anyhow::ensure!(
        stderr.contains("--runner must be codex, opencode, or gemini"),
        "expected helpful runner error\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    Ok(())
}

#[test]
fn run_one_rejects_invalid_model_flag() -> Result<()> {
    let dir = TempDir::new().context("create temp dir")?;
    git_init(dir.path())?;

    let (status, stdout, stderr) = run_in_dir(dir.path(), &["init", "--force"]);
    anyhow::ensure!(
        status.success(),
        "ralph init failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    let (status, stdout, stderr) = run_in_dir(
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

    assert_failure(status, &stdout, &stderr);
    anyhow::ensure!(
        stderr.contains("not supported for codex runner"),
        "expected helpful model error\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    Ok(())
}

#[test]
fn run_one_accepts_custom_model_for_opencode() -> Result<()> {
    let dir = TempDir::new().context("create temp dir")?;
    git_init(dir.path())?;

    let (status, stdout, stderr) = run_in_dir(dir.path(), &["init", "--force"]);
    anyhow::ensure!(
        status.success(),
        "ralph init failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    let (status, stdout, stderr) = run_in_dir(
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
    let dir = TempDir::new().context("create temp dir")?;
    git_init(dir.path())?;

    let (status, stdout, stderr) = run_in_dir(
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

    assert_failure(status, &stdout, &stderr);
    anyhow::ensure!(
        stderr.contains("unsupported reasoning effort"),
        "expected helpful effort error\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    Ok(())
}
