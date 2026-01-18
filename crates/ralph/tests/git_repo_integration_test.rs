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

fn write_valid_single_todo_queue(dir: &Path) -> Result<()> {
    let ralph_dir = dir.join(".ralph");
    std::fs::create_dir_all(&ralph_dir).context("create .ralph dir")?;
    let queue_path = ralph_dir.join("queue.yaml");
    let done_path = ralph_dir.join("done.yaml");

    let queue = r#"version: 1
tasks:
  - id: RQ-0001
    status: todo
    title: Test task
    tags:
      - rust
    scope:
      - crates/ralph
    evidence:
      - integration test fixture
    plan:
      - run preflight
"#;

    let done = r#"version: 1
tasks: []
"#;

    std::fs::write(&queue_path, queue).context("write queue.yaml")?;
    std::fs::write(&done_path, done).context("write done.yaml")?;
    Ok(())
}

#[test]
fn init_and_validate_work_in_fresh_git_repo() -> Result<()> {
    let dir = TempDir::new().context("create temp dir")?;
    git_init(dir.path())?;

    let (status, stdout, stderr) = run_in_dir(dir.path(), &["init", "--force"]);
    anyhow::ensure!(
        status.success(),
        "ralph init failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    let (status, stdout, stderr) = run_in_dir(dir.path(), &["queue", "validate"]);
    anyhow::ensure!(
        status.success(),
        "ralph queue validate failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    Ok(())
}

#[test]
fn run_one_refuses_to_run_when_repo_is_dirty_and_a_todo_exists() -> Result<()> {
    let dir = TempDir::new().context("create temp dir")?;
    git_init(dir.path())?;

    // Ensure ralph runtime files exist.
    let (status, stdout, stderr) = run_in_dir(dir.path(), &["init", "--force"]);
    anyhow::ensure!(
        status.success(),
        "ralph init failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    // Ensure there is a todo item so run_cmd hits the clean-repo preflight.
    write_valid_single_todo_queue(dir.path())?;

    // Make the repo dirty with an untracked file.
    std::fs::write(dir.path().join("untracked.txt"), "dirty").context("write dirty file")?;

    let (status, stdout, stderr) = run_in_dir(dir.path(), &["run", "one"]);
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
fn scan_refuses_to_run_when_repo_is_dirty() -> Result<()> {
    let dir = TempDir::new().context("create temp dir")?;
    git_init(dir.path())?;

    // Ensure ralph runtime files exist.
    let (status, stdout, stderr) = run_in_dir(dir.path(), &["init", "--force"]);
    anyhow::ensure!(
        status.success(),
        "ralph init failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );

    // Make the repo dirty with an untracked file.
    std::fs::write(dir.path().join("untracked.txt"), "dirty").context("write dirty file")?;

    let (status, stdout, stderr) = run_in_dir(dir.path(), &["scan", "--focus", "security"]);
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
